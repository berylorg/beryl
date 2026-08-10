use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use beryl_app::cas_projection::{
    ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
};
use beryl_backend::ManagedBackendClientConnector;
use beryl_model::{CasProcessGeneration, RuntimeId};
use syndic_storage::{
    AcceptedRouteEffectiveState, BindingState, DeliveryRecoveryCase, InputGateState, TurnLifecycle,
};

use crate::{
    app_support::{
        close_seeded, point_limit, promote_installed_next, restart_service, restart_service_with,
        seeded_home, startup_source,
    },
    phase62_support::{
        AUTHORIZATION, NormalTerminalServer, SessionSlot, TIMEOUT, accepted_route_state,
        execution_binding, install_next_records, ready_provider, wait_until,
    },
    records::{activate_promoted_turn, cancel_activation},
};

struct CountingUnavailableProvider {
    attempts: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for CountingUnavailableProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

fn counting_provider(attempts: Arc<AtomicUsize>) -> Box<dyn ScheduledOrdinaryExecutionProvider> {
    Box::new(CountingUnavailableProvider { attempts })
}

#[test]
fn already_promoted_pending_turn_resumes_once_and_stays_terminal_after_restart() {
    let home = seeded_home();
    let ids = install_next_records(
        &home.store,
        home.storage,
        184,
        execution_binding(RuntimeId::from_bytes([244; 16])),
    );
    let promoted = promote_installed_next(&home.store, home.storage, &home.state, ids, 214);
    let active = activate_promoted_turn(
        &home.store,
        home.storage,
        ids.thread,
        promoted.turn,
        244,
        false,
    );
    cancel_activation(
        &home.store,
        home.storage,
        ids.thread,
        promoted.turn,
        active.snapshot,
    );
    let source = startup_source(&home.store, home.storage);
    assert!(matches!(
        home.storage.classify_delivery_recovery(
            &home.store,
            &source,
            point_limit(),
        ),
        Ok(DeliveryRecoveryCase::Pending {
            thread_id,
            turn_id,
            ..
        }) if thread_id == ids.thread && turn_id == promoted.turn
    ));
    let binding = home
        .storage
        .current_binding(&home.store, ids.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("activation cancellation must restore valid CAS authority");
    };
    let runtime_id = usable.execution().runtime_id();
    let gate_before = home
        .storage
        .input_gate(&home.store, ids.thread, point_limit())
        .unwrap()
        .unwrap();
    let state_before = home
        .storage
        .turn_state(&home.store, promoted.turn, point_limit())
        .unwrap()
        .unwrap();
    let directory = close_seeded(home);

    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let (service, storage, _) = restart_service_with(directory.path(), |state| {
        Box::new(ready_provider(provider_slot, state.assets()))
    });
    let parked_diagnostics = wait_until("initial recovered-pending provider park", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.recovered_pending_execution_unavailable() >= 1
            && diagnostics.recovered_pending_pass_count() >= 2
            && diagnostics.workers_active() == 0)
            .then_some(diagnostics)
    });
    let parked_workers_joined = parked_diagnostics.workers_joined();
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_eq!(
            storage
                .input_gate(home, ids.thread, point_limit())
                .unwrap()
                .unwrap(),
            gate_before
        );
        assert_eq!(
            storage
                .turn_state(home, promoted.turn, point_limit())
                .unwrap()
                .unwrap(),
            state_before
        );
        assert_eq!(
            accepted_route_state(home, storage, &ids),
            AcceptedRouteEffectiveState::Promoted
        );
    }

    let server = NormalTerminalServer::spawn_resume_terminal(active.cas_thread.as_str());
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(63_184).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    service.notify_scheduled_ordinary_execution_ready();
    let activation = wait_until("recovered pending durable activation", || {
        let command_home = service.live_home_command().ok()?;
        let gate = storage
            .input_gate(command_home.home(), ids.thread, point_limit())
            .ok()
            .flatten()?;
        if gate.state() != &InputGateState::PendingTurn(promoted.turn) {
            return Some(Ok(()));
        }
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.fatal() || diagnostics.recovered_pending_execution_unavailable() >= 2)
            .then_some(Err(diagnostics))
            .or_else(|| {
                (diagnostics.workers_joined() > parked_workers_joined
                    || diagnostics.recovered_pending_flight_waits() >= 1)
                    .then_some(Err(diagnostics))
            })
    });
    assert!(
        activation.is_ok(),
        "recovered pending execution parked or failed: {activation:?}"
    );
    server.wait_for_projection();
    let completed = wait_until("recovered exact pending turn terminal", || {
        let command_home = service.live_home_command().ok()?;
        let state = storage
            .turn_state(command_home.home(), promoted.turn, point_limit())
            .ok()
            .flatten()?;
        (state.lifecycle() == TurnLifecycle::Complete).then_some(state)
    });
    assert_eq!(completed.turn_id(), promoted.turn);
    wait_until("recovered scheduled session return", || {
        slot.is_ready().then_some(())
    });
    {
        let command_home = service.live_home_command().unwrap();
        let thread_record = storage
            .thread(command_home.home(), ids.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(thread_record.committed_tail(), Some(promoted.turn));
        assert_eq!(
            accepted_route_state(command_home.home(), storage, &ids),
            AcceptedRouteEffectiveState::Promoted
        );
    }
    service.close().unwrap();
    server.join();

    let attempts = Arc::new(AtomicUsize::new(0));
    let (service, storage, _) =
        restart_service(directory.path(), counting_provider(Arc::clone(&attempts)));
    thread::sleep(Duration::from_millis(100));
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_cases(), 0);
    assert_eq!(diagnostics.recovered_pending_execution_unavailable(), 0);
    assert_eq!(diagnostics.workers_started(), 0);
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_eq!(
            accepted_route_state(home, storage, &ids),
            AcceptedRouteEffectiveState::Promoted
        );
        assert_eq!(
            storage
                .thread(home, ids.thread, point_limit())
                .unwrap()
                .unwrap()
                .committed_tail(),
            Some(promoted.turn)
        );
        assert_eq!(
            storage
                .turn_state(home, promoted.turn, point_limit())
                .unwrap()
                .unwrap()
                .lifecycle(),
            TurnLifecycle::Complete
        );
    }
    service.close().unwrap();
}

#[test]
fn unavailable_provider_parks_until_an_explicit_execution_ready_wake() {
    let home = seeded_home();
    let ids = install_next_records(
        &home.store,
        home.storage,
        185,
        execution_binding(RuntimeId::from_bytes([185; 16])),
    );
    let promoted = promote_installed_next(&home.store, home.storage, &home.state, ids, 215);
    let source = startup_source(&home.store, home.storage);
    assert!(matches!(
        home.storage.classify_delivery_recovery(
            &home.store,
            &source,
            point_limit(),
        ),
        Ok(DeliveryRecoveryCase::Pending {
            thread_id,
            turn_id,
            ..
        }) if thread_id == ids.thread && turn_id == promoted.turn
    ));
    let revision_before = home.storage.revision(&home.store).unwrap();
    let gate_before = home
        .storage
        .input_gate(&home.store, ids.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate_before.state(),
        &InputGateState::PendingTurn(promoted.turn)
    );
    let state_before = home
        .storage
        .turn_state(&home.store, promoted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state_before.lifecycle(), TurnLifecycle::Pending);
    let directory = close_seeded(home);

    let attempts = Arc::new(AtomicUsize::new(0));
    let (service, storage, _) =
        restart_service(directory.path(), counting_provider(Arc::clone(&attempts)));
    wait_until("one recovery-owned unavailable provider attempt", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (attempts.load(Ordering::SeqCst) == 1
            && diagnostics.recovered_pending_pass_count() >= 2
            && diagnostics.workers_active() == 0)
            .then_some(())
    });
    let parked = service.accepted_input_scheduler_diagnostics();
    assert!(parked.recovery_handed_off());
    assert_eq!(parked.startup_pending_turns(), 1);
    assert_eq!(parked.recovered_pending_execution_unavailable(), 1);
    assert!(!parked.recovered_pending_retained_source_cursor());
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_eq!(storage.revision(home).unwrap(), revision_before);
        assert_eq!(
            storage
                .input_gate(home, ids.thread, point_limit())
                .unwrap()
                .unwrap(),
            gate_before
        );
        assert_eq!(
            storage
                .turn_state(home, promoted.turn, point_limit())
                .unwrap()
                .unwrap(),
            state_before
        );
    }

    thread::sleep(Duration::from_millis(100));
    let still_parked = service.accepted_input_scheduler_diagnostics();
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(still_parked.workers_started(), parked.workers_started());
    assert_eq!(
        still_parked.recovered_pending_execution_unavailable(),
        parked.recovered_pending_execution_unavailable()
    );

    service.notify_scheduled_ordinary_execution_ready();
    wait_until("explicit execution-ready retry", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (attempts.load(Ordering::SeqCst) == 2
            && diagnostics.workers_active() == 0
            && diagnostics.recovered_pending_pass_count()
                >= parked.recovered_pending_pass_count() + 2)
            .then_some(())
    });
    let retried = service.accepted_input_scheduler_diagnostics();
    assert_eq!(retried.recovered_pending_execution_unavailable(), 2);
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_eq!(storage.revision(home).unwrap(), revision_before);
        assert_eq!(
            accepted_route_state(home, storage, &ids),
            AcceptedRouteEffectiveState::Promoted
        );
        assert_eq!(
            storage
                .input_gate(home, ids.thread, point_limit())
                .unwrap()
                .unwrap(),
            gate_before
        );
        assert_eq!(
            storage
                .turn_state(home, promoted.turn, point_limit())
                .unwrap()
                .unwrap(),
            state_before
        );
    }
    service.close().unwrap();
}
