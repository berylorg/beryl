use beryl_backend::{
    ManagedBackendError, StreamedInputSourceError, StreamedInputSourceRevision, TurnStartOptions,
};
use beryl_home_store::HomeHealthState;

use crate::{
    content::{LogicalInput, seed_submitted_input},
    fixture::{CompletedExecution, PreparedExecution, close_execution},
    server::{RawCasServer, TIMEOUT},
    syndic::Fixture,
    wire::{RequestAbortReason, RequestOutcome},
};

use super::common;

pub fn revision_drift() {
    run_source_failure(
        141,
        31,
        StreamedInputSourceError::RevisionDrift {
            expected: StreamedInputSourceRevision::new(1),
            actual: StreamedInputSourceRevision::new(2),
        },
    );
}

pub fn read_failure() {
    run_source_failure(142, 32, StreamedInputSourceError::ReadFailed);
}

fn run_source_failure(seed: u8, run_id: u64, expected_source: StreamedInputSourceError) {
    let mut fixture = Fixture::new(seed);
    let thread = fixture.thread;
    let seeded = seed_submitted_input(&mut fixture, thread, LogicalInput::marker_free(4_096), None);
    let server = RawCasServer::spawn(run_id, seeded.wire);
    let prepared = PreparedExecution::new(&fixture, thread, &server);
    let request = beryl_app::cas_projection::OrdinaryTurnExecutionRequest::new(
        TurnStartOptions::default(),
        TIMEOUT,
    );
    let diagnostics = request.input_replay_diagnostics();
    let page = diagnostics.install_source_page_handoff_barrier(1);
    diagnostics.install_source_page_failure(2, expected_source.clone());
    let mut request_outcome = None;

    let CompletedExecution { result, session } = prepared.execute(&fixture, &request, |_| {
        assert!(page.wait_until_paused(TIMEOUT));
        let paused = diagnostics.snapshot();
        assert!(paused.source_request_count() >= 1);
        assert_eq!(paused.text_page_requests(), 1);
        page.release();
        request_outcome = Some(server.wait_for_request());
    });

    let error = common::start_completion_unknown(result);
    let ManagedBackendError::StreamedInputSource {
        method,
        source,
        transport_bytes_written,
    } = error.as_ref()
    else {
        panic!("source failure lost its typed streamed-input cause: {error:?}")
    };
    assert_eq!(method, "turn/start");
    assert!(*transport_bytes_written);
    assert_eq!(source, &expected_source);

    let RequestOutcome::Aborted(abort) = request_outcome.unwrap() else {
        panic!("failed streamed source unexpectedly completed the request")
    };
    assert!(abort.compared_bytes() > 0);
    assert!(matches!(
        abort.reason(),
        RequestAbortReason::PeerClose | RequestAbortReason::TransportEof
    ));
    assert_eq!(fixture.store.health().state(), HomeHealthState::Healthy);
    common::assert_durable_stream_loss(&fixture, thread, seeded.submitted.turn, 1);
    common::assert_released(&session);
    close_execution(session, server);
    common::finish_fixture(fixture);
}
