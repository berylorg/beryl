use beryl_app::cas_projection::{
    LiveEventTargetCloseReason, OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionOutcome,
    test_faults::install_checked_user_publication_barrier,
};
use beryl_backend::{TurnStartOptions, UserMessageEchoLifecycle};
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use syndic_storage::TurnEndStatus;

use crate::{
    content::{LogicalInput, seed_submitted_input},
    fixture::{CompletedExecution, PreparedExecution, close_execution},
    server::{RawCasServer, ServerScenario, TIMEOUT},
    syndic::Fixture,
    verification::assert_durable_success,
    wire::RequestOutcome,
};

use super::common;

#[derive(Clone, Copy)]
enum ExpectedTerminalResult {
    DefinitiveFailure,
    Reconciled,
}

pub fn definitive_terminal_publication_failure() {
    run_terminal_publication_fault(
        145,
        35,
        FaultPoint::BeforeCommit,
        ExpectedTerminalResult::DefinitiveFailure,
    );
}

pub fn ambiguous_terminal_publication() {
    run_terminal_publication_fault(
        146,
        36,
        FaultPoint::AfterPersist,
        ExpectedTerminalResult::Reconciled,
    );
}

fn run_terminal_publication_fault(
    seed: u8,
    run_id: u64,
    point: FaultPoint,
    expected: ExpectedTerminalResult,
) {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(seed, faults.clone());
    let thread = fixture.thread;
    let seeded = seed_submitted_input(&mut fixture, thread, LogicalInput::marker_free(4_096), None);
    let server = RawCasServer::spawn_scenario(
        run_id,
        seeded.wire,
        ServerScenario::ObserveTailAfterTerminal,
    );
    let prepared = PreparedExecution::new(&fixture, thread, &server);
    let request = beryl_app::cas_projection::OrdinaryTurnExecutionRequest::new(
        TurnStartOptions::default(),
        TIMEOUT,
    );
    let CompletedExecution { result, session } = prepared.execute(&fixture, &request, |session| {
        let RequestOutcome::Complete(_) = server.wait_for_request() else {
            panic!("terminal-publication request aborted before lifecycle publication")
        };
        let completed =
            install_checked_user_publication_barrier(session, UserMessageEchoLifecycle::Completed);
        server.release_lifecycle();
        assert!(completed.wait_until_paused(TIMEOUT));
        server.wait_for_tail();

        let scope = syndic_storage::test_faults::live_source_event_fault_scope();
        // Hold the Completed mutation only after it is durable. Arming the terminal fault
        // before releasing this cut makes the next scoped mutation unambiguously terminal,
        // independent of which terminal fault point this case exercises.
        let completed_cut = faults.block_next_in_scope(FaultPoint::AfterPersist, scope);
        completed.release();
        assert!(completed_cut.wait_until_reached(TIMEOUT));
        faults.fail_next_in_scope(point, scope);
        completed_cut.release();
    });

    match expected {
        ExpectedTerminalResult::DefinitiveFailure => {
            match result.unwrap() {
                OrdinaryTurnExecutionOutcome::Incomplete {
                    reason:
                        OrdinaryTurnCaptureLoss::TargetClosed(
                            LiveEventTargetCloseReason::SourcePublicationFailed,
                        ),
                } => {}
                other => panic!(
                    "definitive terminal-publication failure lost its target taxonomy: {other:?}"
                ),
            }
            common::assert_durable_stream_loss(&fixture, thread, seeded.submitted.turn, 4);
        }
        ExpectedTerminalResult::Reconciled => {
            let (projection, status) = match result.unwrap() {
                OrdinaryTurnExecutionOutcome::Terminal { projection, status } => {
                    (projection, status)
                }
                other => panic!(
                    "post-persist terminal publication did not reconcile to terminal: {other:?}"
                ),
            };
            assert_eq!(status, TurnEndStatus::complete());
            drop(projection);
            assert_durable_success(&fixture, thread, seeded.submitted.turn, status);
        }
    }
    common::assert_released(&session);
    close_execution(session, server);
    common::finish_fixture(fixture);
}
