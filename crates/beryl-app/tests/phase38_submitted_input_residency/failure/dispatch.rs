use beryl_app::cas_projection::{
    OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure, ProjectionExecutionError,
    test_faults::install_checked_user_submit_receiver_loss,
};
use beryl_backend::{ManagedBackendError, OrderedTurnStreamSubmitCause, TurnStartOptions};

use crate::{
    content::{LogicalInput, seed_submitted_input},
    fixture::{CompletedExecution, PreparedExecution, close_execution},
    server::{RawCasServer, ServerScenario, TIMEOUT},
    syndic::Fixture,
    verification::assert_connection_released,
    wire::{RequestAbortReason, RequestOutcome},
};

use super::common;

pub fn cancellation_before_dispatch() {
    let mut fixture = Fixture::new(140);
    let thread = fixture.thread;
    let seeded = seed_submitted_input(&mut fixture, thread, LogicalInput::marker_free(4_096), None);
    let server = RawCasServer::spawn(30, seeded.wire);
    let prepared = PreparedExecution::new(&fixture, thread, &server);
    let request = beryl_app::cas_projection::OrdinaryTurnExecutionRequest::new(
        TurnStartOptions::default(),
        TIMEOUT,
    );
    fixture.cancellation.cancel();

    let CompletedExecution { result, session } = prepared.execute(&fixture, &request, |_| {});
    let projection = match result {
        Err(OrdinaryTurnExecutionFailure::PreActivation {
            projection,
            source:
                OrdinaryTurnExecutionError::ProjectionExecution(ProjectionExecutionError::Cancelled),
        }) => projection,
        other => panic!("pre-dispatch cancellation returned the wrong taxonomy: {other:?}"),
    };
    assert_eq!(projection.syndic_thread_id(), thread);
    drop(projection);
    common::assert_durable_pending(&fixture, thread, seeded.submitted.turn);
    assert_connection_released(&session);

    session.invalidate_connection();
    let RequestOutcome::Aborted(abort) = server.wait_for_request() else {
        panic!("cancelled execution dispatched a turn/start request")
    };
    assert_eq!(abort.compared_bytes(), 0);
    assert_eq!(abort.frame_count(), 0);
    assert!(matches!(
        abort.reason(),
        RequestAbortReason::PeerClose | RequestAbortReason::TransportEof
    ));
    close_execution(session, server);
    common::finish_fixture(fixture);
}

pub fn raw_websocket_byte_cutoff() {
    let mut fixture = Fixture::new(143);
    let thread = fixture.thread;
    let seeded = seed_submitted_input(
        &mut fixture,
        thread,
        LogicalInput::marker_free(16_384),
        None,
    );
    let server = RawCasServer::spawn_scenario(
        33,
        seeded.wire,
        ServerScenario::close_request_after_bytes(1_024),
    );
    let prepared = PreparedExecution::new(&fixture, thread, &server);
    let request = beryl_app::cas_projection::OrdinaryTurnExecutionRequest::new(
        TurnStartOptions::default(),
        TIMEOUT,
    );
    let mut observed = None;

    let CompletedExecution { result, session } = prepared.execute(&fixture, &request, |_| {
        observed = Some(server.wait_for_request());
    });
    let error = common::start_completion_unknown(result);
    let ManagedBackendError::WebSocketTransport { method, .. } = error.as_ref() else {
        panic!("raw WebSocket cutoff lost its typed transport failure: {error:?}")
    };
    assert_eq!(method, "turn/start");

    let RequestOutcome::Aborted(abort) = observed.unwrap() else {
        panic!("raw WebSocket byte cutoff unexpectedly accepted the complete request")
    };
    assert_eq!(abort.reason(), RequestAbortReason::ServerByteCutoff);
    assert_eq!(abort.compared_bytes(), 1_024);
    assert_eq!(abort.frame_count(), 1);
    common::assert_durable_stream_loss(&fixture, thread, seeded.submitted.turn, 1);
    common::assert_released(&session);
    close_execution(session, server);
    common::finish_fixture(fixture);
}

pub fn checked_user_receiver_loss() {
    let mut fixture = Fixture::new(144);
    let thread = fixture.thread;
    let seeded = seed_submitted_input(&mut fixture, thread, LogicalInput::marker_free(4_096), None);
    let server = RawCasServer::spawn(34, seeded.wire);
    let prepared = PreparedExecution::new(&fixture, thread, &server);
    let request = beryl_app::cas_projection::OrdinaryTurnExecutionRequest::new(
        TurnStartOptions::default(),
        TIMEOUT,
    );
    let mut receiver_loss = None;

    let CompletedExecution { result, session } = prepared.execute(&fixture, &request, |session| {
        let RequestOutcome::Complete(_) = server.wait_for_request() else {
            panic!("receiver-loss request aborted before the checked-user operation")
        };
        receiver_loss = Some(install_checked_user_submit_receiver_loss(session));
        server.release_lifecycle();
    });
    drop(receiver_loss);
    let error = common::start_completion_unknown(result);
    let ManagedBackendError::OrderedTurnStream { method, source } = error.as_ref() else {
        panic!("checked-user receiver loss lost its ordered failure: {error:?}")
    };
    assert_eq!(method, "turn/start");
    assert_eq!(source.cause(), OrderedTurnStreamSubmitCause::ReceiverLost);

    common::assert_durable_stream_loss(&fixture, thread, seeded.submitted.turn, 1);
    common::assert_released(&session);
    close_execution(session, server);
    common::finish_fixture(fixture);
}
