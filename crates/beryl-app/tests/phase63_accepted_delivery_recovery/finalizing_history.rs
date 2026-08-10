use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use beryl_app::cas_projection::test_faults::{
    TerminalHistoryBarrierStage, install_terminal_history_barrier,
};
use beryl_backend::ManagedBackendClientConnector;
use beryl_model::CasProcessGeneration;
use serde_json::json;
use syndic_storage::{
    AcceptedRouteEffectiveState, InputGateState, NextTurnReason, RecoveryProjectionRequest,
    SelectedPathProof, TurnIncompleteReason, TurnLifecycle,
};

use crate::{
    app_support::{point_limit, restart_service, restart_service_with},
    phase62_support::{
        AUTHORIZATION, NormalTerminalServer, SUBMITTED_TEXT, SessionSlot, TIMEOUT,
        accepted_route_state, ready_provider, wait_until,
    },
};

#[path = "finalizing_history/live_support.rs"]
mod live_support;
#[path = "finalizing_history/steering_owner.rs"]
mod steering_owner;
#[path = "finalizing_history/support.rs"]
mod support;
#[path = "finalizing_history/terminal_admission.rs"]
mod terminal_admission;

use support::{
    COMPLETED_PARENT_TEXT, CountingCheckoutProvider, CountingUnavailableProvider,
    assert_recovered_predecessor, finalizing_fixture,
};

#[test]
fn source_less_finalizing_history_restart_converges_without_replay_and_reopen_is_idempotent() {
    let fixture = finalizing_fixture(196, false);
    assert!(fixture.successor.is_none());
    let attempts = Arc::new(AtomicUsize::new(0));
    let (service, storage, _) = restart_service(
        fixture.directory.path(),
        Box::new(CountingUnavailableProvider {
            attempts: Arc::clone(&attempts),
        }),
    );
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_cases(), 1);
    assert_eq!(diagnostics.startup_active_convergences(), 0);
    assert_eq!(diagnostics.startup_terminal_convergences(), 1);
    assert_eq!(diagnostics.startup_pending_turns(), 0);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        0,
        "terminal-history recovery must not replay the interrupted predecessor"
    );
    let (revision, gate, state, head) = {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_recovered_predecessor(home, storage, &fixture);
        (
            storage.revision(home).unwrap(),
            storage
                .input_gate(home, fixture.thread, point_limit())
                .unwrap()
                .unwrap(),
            storage
                .turn_state(home, fixture.predecessor, point_limit())
                .unwrap()
                .unwrap(),
            storage
                .transcript_view_head(home, fixture.thread, point_limit())
                .unwrap()
                .unwrap(),
        )
    };
    service.close().unwrap();

    let reopened_attempts = Arc::new(AtomicUsize::new(0));
    let (service, storage, _) = restart_service(
        fixture.directory.path(),
        Box::new(CountingUnavailableProvider {
            attempts: Arc::clone(&reopened_attempts),
        }),
    );
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_cases(), 0);
    assert_eq!(diagnostics.startup_terminal_convergences(), 0);
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_eq!(storage.revision(home).unwrap(), revision);
        assert_eq!(
            storage
                .input_gate(home, fixture.thread, point_limit())
                .unwrap()
                .unwrap(),
            gate
        );
        assert_eq!(
            storage
                .turn_state(home, fixture.predecessor, point_limit())
                .unwrap()
                .unwrap(),
            state
        );
        assert_eq!(
            storage
                .transcript_view_head(home, fixture.thread, point_limit())
                .unwrap()
                .unwrap(),
            head
        );
    }
    thread::sleep(Duration::from_millis(100));
    assert_eq!(reopened_attempts.load(Ordering::SeqCst), 0);
    service.close().unwrap();
}

#[test]
fn queued_successor_waits_for_terminal_history_gate_then_executes_exactly_once() {
    let fixture = finalizing_fixture(197, true);
    let successor = fixture
        .successor
        .expect("finalizing-history fixture retains one queued successor");
    let attempts = Arc::new(AtomicUsize::new(0));
    let slot = SessionSlot::default();
    let barrier = install_terminal_history_barrier(
        fixture.thread,
        TerminalHistoryBarrierStage::BeforeGateRelease,
    );
    let paused_attempts = Arc::clone(&attempts);
    let release = thread::spawn(move || {
        barrier.wait();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(
            paused_attempts.load(Ordering::SeqCst),
            0,
            "queued work reached the execution provider before FinalizingHistory released"
        );
        barrier.release();
    });
    let provider_attempts = Arc::clone(&attempts);
    let provider_slot = slot.clone();
    let (service, storage, _) = restart_service_with(fixture.directory.path(), move |state| {
        Box::new(CountingCheckoutProvider {
            attempts: provider_attempts,
            checkout: ready_provider(provider_slot, state.assets()),
        })
    });
    release.join().unwrap();

    let startup = service.accepted_input_scheduler_diagnostics();
    assert!(startup.recovery_handed_off());
    assert_eq!(startup.startup_recovery_cases(), 1);
    assert_eq!(startup.startup_terminal_convergences(), 1);
    let parked = wait_until("one post-handoff unavailable successor attempt", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (attempts.load(Ordering::SeqCst) == 1
            && diagnostics.next_execution_unavailable() == 1
            && diagnostics.workers_active() == 0)
            .then_some(diagnostics)
    });
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "the parked successor must not self-retry"
    );
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_eq!(
            accepted_route_state(home, storage, &successor),
            AcceptedRouteEffectiveState::NextTurn(NextTurnReason::TerminalHistory)
        );
        assert_eq!(
            storage
                .turn_state(home, fixture.predecessor, point_limit())
                .unwrap()
                .unwrap()
                .lifecycle(),
            TurnLifecycle::Incomplete
        );
    }

    let server = NormalTerminalServer::spawn_recovery_terminal(vec![
        json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": COMPLETED_PARENT_TEXT}],
        }),
        json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": SUBMITTED_TEXT}],
        }),
    ]);
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            fixture.runtime_id,
            CasProcessGeneration::new(63_797).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    service.notify_scheduled_ordinary_execution_ready();
    let issued = wait_until("queued successor execution issue", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        if attempts.load(Ordering::SeqCst) >= 2 {
            return Some(Ok(diagnostics));
        }
        diagnostics.fatal().then_some(Err(diagnostics))
    });
    assert!(
        issued.is_ok(),
        "queued successor failed before CAS projection: {issued:?}"
    );
    let worker = wait_until("queued successor worker start", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        if diagnostics.fatal() {
            return Some(Err(diagnostics));
        }
        (diagnostics.workers_started() > parked.workers_started()).then_some(Ok(diagnostics))
    });
    assert!(
        worker.is_ok(),
        "queued successor did not start one worker: {worker:?}"
    );
    thread::sleep(Duration::from_millis(100));
    let before_projection = service.accepted_input_scheduler_diagnostics();
    if before_projection.fatal() {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        let thread = storage
            .thread(home, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        let selected_path = SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        );
        let preflight = storage.prepare_recovery_projection(
            home,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                fixture.thread,
                selected_path,
                Some(2_000_000),
            ),
        );
        panic!(
            "queued successor failed before sending its CAS projection: diagnostics={before_projection:?}, route={:?}, gate={:?}, predecessor={:?}, tail={:?}, preflight={preflight:?}",
            accepted_route_state(home, storage, &successor),
            storage
                .input_gate(home, fixture.thread, point_limit())
                .unwrap(),
            storage
                .turn_state(home, fixture.predecessor, point_limit())
                .unwrap(),
            thread.committed_tail(),
        );
    }
    server.wait_for_projection();
    server.wait_for_turn_start();

    let promoted_turn = wait_until("queued successor is durably promoted", || {
        let command_home = service.live_home_command().ok()?;
        let tail = storage
            .thread(command_home.home(), fixture.thread, point_limit())
            .ok()
            .flatten()?
            .committed_tail()?;
        (tail != fixture.predecessor).then_some(tail)
    });
    {
        let command_home = service.live_home_command().unwrap();
        assert_eq!(
            storage
                .turn(command_home.home(), promoted_turn, point_limit())
                .unwrap()
                .unwrap()
                .parent()
                .turn(),
            Some(fixture.predecessor)
        );
    }
    wait_until(
        "one queued successor reaches a terminal history fixed point",
        || {
            let command_home = service.live_home_command().ok()?;
            let home = command_home.home();
            let state = storage
                .turn_state(home, promoted_turn, point_limit())
                .ok()
                .flatten()?;
            let gate = storage
                .input_gate(home, fixture.thread, point_limit())
                .ok()
                .flatten()?;
            (state.lifecycle() == TurnLifecycle::Complete
                && gate.state() == &InputGateState::Idle
                && slot.is_ready())
            .then_some(())
        },
    );
    thread::sleep(Duration::from_millis(100));
    let settled = service.accepted_input_scheduler_diagnostics();
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(
        settled.workers_started(),
        parked.workers_started().saturating_add(1)
    );
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_eq!(
            accepted_route_state(home, storage, &successor),
            AcceptedRouteEffectiveState::Promoted
        );
        assert_eq!(
            storage
                .turn_state(home, fixture.predecessor, point_limit())
                .unwrap()
                .unwrap()
                .end_status()
                .unwrap()
                .incomplete_reason(),
            Some(TurnIncompleteReason::AuthorityLost)
        );
    }
    service.close().unwrap();
    server.join();
}
