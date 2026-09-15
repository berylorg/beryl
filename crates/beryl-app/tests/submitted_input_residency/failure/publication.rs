use beryl_app::cas_projection::{
    OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure, ProjectionCoordinatorError,
    test_faults::install_checked_user_publication_barrier,
};
use beryl_backend::{TurnStartOptions, UserMessageEchoLifecycle};
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use beryl_state::BerylState;
use syndic_storage::{
    SourceEventPayload, SourceEventSequence, SyndicStorage, TurnEndStatus, TurnLifecycle,
};

use crate::{
    content::{LogicalInput, seed_submitted_input},
    fixture::{CompletedExecution, PreparedExecution, close_execution},
    server::{RawCasServer, ServerScenario, TIMEOUT},
    syndic::{Fixture, point_limit},
    wire::RequestOutcome,
};

#[derive(Clone, Copy)]
enum ExpectedTerminalCommit {
    NotCommitted,
    Committed,
}

pub fn definitive_terminal_publication_failure() {
    run_terminal_publication_fault(
        145,
        35,
        FaultPoint::BeforeCommit,
        ExpectedTerminalCommit::NotCommitted,
    );
}

pub fn ambiguous_terminal_publication() {
    run_terminal_publication_fault(
        146,
        36,
        FaultPoint::AfterPersist,
        ExpectedTerminalCommit::Committed,
    );
}

fn run_terminal_publication_fault(
    seed: u8,
    run_id: u64,
    point: FaultPoint,
    expected: ExpectedTerminalCommit,
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
    let release_fault = |session: &beryl_app::cas_projection::AdmittedProjectionSession| {
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
    };
    let (CompletedExecution { result, session }, directory) =
        prepared.execute_with_failed_service_disposal(fixture, &request, release_fault);
    assert!(
        matches!(
            result,
            Err(OrdinaryTurnExecutionFailure::AfterActivation {
                source: OrdinaryTurnExecutionError::Coordinator(
                    ProjectionCoordinatorError::ProjectionWorkerStopped
                ),
            })
        ),
        "failed-service execution returned an unexpected result: {result:?}"
    );
    close_execution(session, server);
    assert_reopened_terminal(directory.path(), thread, seeded.submitted.turn, expected);
    drop(directory);
}

fn assert_reopened_terminal(
    path: &std::path::Path,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    expected: ExpectedTerminalCommit,
) {
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(
            SyndicStorage::required_domains()
                .unwrap()
                .merge(BerylState::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let turn_state = storage
        .turn_state(&home, turn, point_limit())
        .unwrap()
        .unwrap();
    let header = storage.turn(&home, turn, point_limit()).unwrap().unwrap();
    assert_eq!(header.origin_thread_id(), thread);
    assert_eq!(turn_state.turn_id(), turn);
    assert_eq!(turn_state.item_count(), 1);
    assert_eq!(turn_state.finalized_item_count(), 0);
    assert_eq!(turn_state.open_item_count(), 0);
    assert_eq!(turn_state.incomplete_reason(), None);
    let terminal = storage
        .source_event(
            &home,
            turn,
            SourceEventSequence::new(4).unwrap(),
            point_limit(),
        )
        .unwrap();
    match expected {
        ExpectedTerminalCommit::NotCommitted => {
            assert_eq!(turn_state.lifecycle(), TurnLifecycle::Active);
            assert_eq!(turn_state.source_event_count(), 3);
            assert_eq!(turn_state.end_status(), None);
            assert!(terminal.is_none());
        }
        ExpectedTerminalCommit::Committed => {
            assert_eq!(turn_state.lifecycle(), TurnLifecycle::Complete);
            assert_eq!(turn_state.source_event_count(), 4);
            assert_eq!(turn_state.end_status(), Some(TurnEndStatus::complete()));
            let terminal = terminal.unwrap();
            assert!(terminal.source().is_some());
            assert!(
                matches!(terminal.payload(), SourceEventPayload::TurnEnded(status) if *status == TurnEndStatus::complete())
            );
        }
    }
    assert!(
        storage
            .source_event(
                &home,
                turn,
                SourceEventSequence::new(5).unwrap(),
                point_limit()
            )
            .unwrap()
            .is_none()
    );
    drop(state);
    drop(storage);
    home.close().unwrap();
}
