use std::{
    fmt::Write as _,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use beryl_app::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, OrdinaryDynamicToolHandlers,
    OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
    test_faults::{TerminalHistoryBarrierStage, install_terminal_history_barrier},
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions};
use beryl_model::{CasProcessGeneration, SyndicAcceptedInputId};
use syndic_storage::{AcceptedRouteEffectiveState, InputGateState, NextTurnReason};

use super::live_support::{NoopBranch, NoopLifecycle, TerminalHistoryReleaseGuard};
use super::support::{
    CountingUnavailableProvider, admit_and_wait_for_steering, admit_successor,
    prepare_steering_draft,
};
use crate::{
    app_support::point_limit,
    phase62_support::{
        AUTHORIZATION, NextRecordIds, NormalTerminalServer, SUBMITTED_TEXT, TIMEOUT,
        accepted_route_state, wait_until,
    },
};

#[test]
fn active_steering_loss_leaves_terminal_history_release_to_the_capture_flight_owner() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let provider_attempts = Arc::clone(&attempts);
    let mut fixture = crate::syndic::Fixture::new_with_scheduled_provider(198, move |_| {
        Box::new(CountingUnavailableProvider {
            attempts: provider_attempts,
        })
    });
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    prepare_steering_draft(&fixture);
    fixture.store.notify_scheduled_ordinary_execution_ready();
    wait_until("manual steering fixture scheduler becomes idle", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (attempts.load(Ordering::SeqCst) == 1
            && diagnostics.recovered_pending_execution_unavailable() == 1
            && diagnostics.workers_active() == 0)
            .then_some(())
    });

    let server = NormalTerminalServer::spawn_steering_correlation_loss();
    let trigger = server.steering_failure_trigger();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit(
            &connector,
            crate::syndic::execution_binding().runtime_id(),
            CasProcessGeneration::new(63_798).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &CasProjectionRequest::new(
                fixture.thread,
                fixture.selected_path(fixture.thread),
                crate::syndic::execution_binding(),
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                syndic_storage::SyndicTimestamp::from_unix_millis(65_100),
                TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();

    let before_steering = fixture.store.accepted_input_scheduler_diagnostics();
    let execution_request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let outcome = thread::scope(|scope| {
        let mut release_guard = TerminalHistoryReleaseGuard::new(install_terminal_history_barrier(
            fixture.thread,
            TerminalHistoryBarrierStage::BeforeGateRelease,
        ));
        let capture = scope.spawn(|| {
            let mut lifecycle = NoopLifecycle;
            let mut branch = NoopBranch;
            coordinator
                .execute_ordinary_turn(
                    &fixture.store,
                    fixture.storage,
                    fixture.state.assets(),
                    projection,
                    &fixture.cancellation,
                    &execution_request,
                    OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
                )
                .unwrap()
        });

        let steering_input = admit_and_wait_for_steering(&fixture, submitted.turn);
        trigger.send(accepted_input_correlation(steering_input));
        release_guard.wait();

        let steering_settled =
            wait_until("active steering returns while capture is paused", || {
                let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
                (diagnostics.workers_joined() > before_steering.workers_joined()
                    && diagnostics.workers_active() == 0)
                    .then_some(diagnostics)
            });
        assert!(
            !steering_settled.fatal(),
            "active steering failed while capture owned terminal history: {steering_settled:?}"
        );
        let paused_gate = fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(
            paused_gate.state(),
            &InputGateState::FinalizingHistory(submitted.turn)
        );
        let steering_ids = NextRecordIds {
            thread: fixture.thread,
            accepted_input: steering_input,
            parent: submitted.turn,
        };
        assert_eq!(
            accepted_route_state(&fixture.store, fixture.storage, &steering_ids),
            AcceptedRouteEffectiveState::DeliveryUnknown
        );
        let paused_attempts = attempts.load(Ordering::SeqCst);

        let successor = admit_successor(
            &fixture.store,
            fixture.storage,
            fixture.thread,
            submitted.turn,
        );
        thread::sleep(Duration::from_millis(100));
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            paused_attempts,
            "queued successor reached execution while capture still owned FinalizingHistory"
        );
        assert_eq!(
            accepted_route_state(&fixture.store, fixture.storage, &successor),
            AcceptedRouteEffectiveState::NextTurn(NextTurnReason::TerminalHistory)
        );
        let admitted_gate = fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(
            admitted_gate.revision(),
            paused_gate.revision().checked_next().unwrap()
        );
        assert_eq!(admitted_gate.state(), paused_gate.state());
        let before_release = fixture.store.accepted_input_scheduler_diagnostics();

        release_guard.release();
        let outcome = capture.join().unwrap();
        let released = wait_until("capture releases one successor wake", || {
            let gate = fixture
                .storage
                .input_gate(&fixture.store, fixture.thread, point_limit())
                .ok()
                .flatten()?;
            let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
            (gate.state() == &InputGateState::Idle
                && attempts.load(Ordering::SeqCst) == paused_attempts + 1
                && diagnostics.next_execution_unavailable()
                    == before_release.next_execution_unavailable() + 1)
                .then_some((gate, diagnostics))
        });
        assert_eq!(
            released.0.revision(),
            admitted_gate.revision().checked_next().unwrap(),
            "only the capture flight owner may release FinalizingHistory"
        );
        assert_eq!(
            released.1.next_execution_unavailable(),
            before_release.next_execution_unavailable() + 1
        );
        thread::sleep(Duration::from_millis(100));
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            paused_attempts + 1,
            "one capture-owned release must produce one parked successor attempt"
        );
        outcome
    });
    assert!(matches!(
        outcome,
        OrdinaryTurnExecutionOutcome::Incomplete {
            reason: OrdinaryTurnCaptureLoss::TargetClosed(_)
        }
    ));

    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

fn accepted_input_correlation(input: SyndicAcceptedInputId) -> String {
    let mut encoded = String::with_capacity("beryl.accepted-input.v1:".len() + 32);
    encoded.push_str("beryl.accepted-input.v1:");
    for byte in input.as_bytes() {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}
