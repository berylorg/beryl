use std::time::Duration;

use beryl_backend::{
    ApprovalRequestKind, ForegroundIngressError, ManagedBackendError,
    OrderedTurnStreamBindingError, OrderedTurnStreamProgress, OrderedTurnStreamRejection,
    OrderedTurnStreamSubmitCause,
};

use super::support::{
    SinkFailure, assert_pre_bind_diagnostics, connect_foreground, expect_close_without_text,
    read_denial_id, send_approval, send_initialize_response, send_unavailable_compact,
    sink_harness, spawn_server,
};

#[test]
fn next_approval_after_exact_configured_capacity_gets_no_denial_and_closes_with_release() {
    const CAPACITY: usize = 70;
    let last_admitted_id = i64::try_from(CAPACITY).unwrap();
    let rejected_id = last_admitted_id + 1;
    let (endpoint, server) = spawn_server(move |socket| {
        let mut denials = Vec::new();
        for request_id in 1..=last_admitted_id {
            send_approval(socket, request_id, ApprovalRequestKind::CommandExecution);
            denials.push(read_denial_id(socket));
        }
        send_approval(socket, rejected_id, ApprovalRequestKind::CommandExecution);
        expect_close_without_text(socket);
        denials
    });
    let mut session = connect_foreground(endpoint, CAPACITY);
    session.prepare_pre_bind_response_wait_for_lifecycle_test(500);

    for _ in 0..CAPACITY {
        assert_eq!(
            session
                .poll_pre_bind_response_wait_for_lifecycle_test(Duration::from_secs(2))
                .unwrap(),
            OrderedTurnStreamProgress::Progress,
        );
    }
    assert_pre_bind_diagnostics(
        &session,
        CAPACITY,
        CAPACITY,
        CAPACITY,
        u64::try_from(CAPACITY).unwrap(),
        0,
    );
    let error = session
        .poll_pre_bind_response_wait_for_lifecycle_test(Duration::from_secs(2))
        .unwrap_err();
    match error {
        ManagedBackendError::PreBindControlCapacityExceeded { capacity, .. } => {
            assert_eq!(capacity, CAPACITY);
        }
        other => panic!("unexpected full-prefix result: {other:?}"),
    }
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(session.pre_bind_prefix_is_empty_for_lifecycle_test());
    assert_pre_bind_diagnostics(
        &session,
        CAPACITY,
        0,
        CAPACITY,
        u64::try_from(CAPACITY).unwrap(),
        1,
    );
    assert_eq!(
        server.join().unwrap(),
        (1..=last_admitted_id).collect::<Vec<_>>(),
    );
}

#[test]
fn denial_write_failure_retires_the_connection_and_releases_the_admitted_prefix() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 7, ApprovalRequestKind::CommandExecution);
        expect_close_without_text(socket);
    });
    let mut session = connect_foreground(endpoint, 2);
    session.prepare_pre_bind_response_wait_for_lifecycle_test(501);
    session.fail_next_write_before_dispatch_for_lifecycle_test();

    let error = session
        .poll_pre_bind_response_wait_for_lifecycle_test(Duration::from_secs(2))
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ApprovalDenialWrite { .. }
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(session.pre_bind_prefix_is_empty_for_lifecycle_test());
    assert_pre_bind_diagnostics(&session, 2, 0, 1, 1, 0);
    server.join().unwrap();
}

#[test]
fn unbound_permission_approval_gets_no_denial_and_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 8, ApprovalRequestKind::Permissions);
        expect_close_without_text(socket);
    });
    let mut session = connect_foreground(endpoint, 2);
    session.prepare_pre_bind_response_wait_for_lifecycle_test(508);

    let error = session
        .poll_pre_bind_response_wait_for_lifecycle_test(Duration::from_secs(2))
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::PermissionApprovalStopOwnerUnbound
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(session.pre_bind_prefix_is_empty_for_lifecycle_test());
    assert_pre_bind_diagnostics(&session, 2, 0, 0, 0, 0);
    server.join().unwrap();
}

#[test]
fn fatal_bind_failure_releases_current_and_remaining_fifo_before_close() {
    let (denials_ready_tx, denials_ready_rx) = std::sync::mpsc::sync_channel(0);
    let (endpoint, server) = spawn_server(move |socket| {
        send_approval(socket, 11, ApprovalRequestKind::CommandExecution);
        let first_denial = read_denial_id(socket);
        send_approval(socket, 12, ApprovalRequestKind::FileChange);
        let second_denial = read_denial_id(socket);
        send_initialize_response(socket, 502);
        denials_ready_tx.send(()).unwrap();
        expect_close_without_text(socket);
        [first_denial, second_denial]
    });
    let mut session = connect_foreground(endpoint, 4);
    session.prepare_pre_bind_response_wait_for_lifecycle_test(502);
    for _ in 0..2 {
        session
            .poll_pre_bind_response_wait_for_lifecycle_test(Duration::from_secs(2))
            .unwrap();
    }
    assert_pre_bind_diagnostics(&session, 4, 2, 2, 2, 0);
    denials_ready_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap();

    let harness = sink_harness(Some((
        1,
        SinkFailure::Submit(OrderedTurnStreamSubmitCause::ReceiverLost),
    )));
    let captured = harness.trace.clone();
    let error = session
        .bind_ordered_turn_stream_sink(harness.sink)
        .unwrap_err();
    assert_eq!(
        error,
        OrderedTurnStreamBindingError::BufferedSubmission(
            OrderedTurnStreamSubmitCause::ReceiverLost,
        ),
    );
    let captured = captured.lock().unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].request_id, 11);
    assert_eq!(
        captured[0].disposition,
        beryl_backend::ApprovalResponseDisposition::AutoDenied,
    );
    assert_pre_bind_diagnostics(&session, 4, 0, 2, 2, 0);
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(session.pre_bind_prefix_is_empty_for_lifecycle_test());
    assert_eq!(server.join().unwrap(), [11, 12]);
}

#[test]
fn non_interrupting_target_failure_is_classified_during_binding_and_retires_the_candidate() {
    let cause = OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::StagingConflict);
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 21, ApprovalRequestKind::CommandExecution);
        let denial = read_denial_id(socket);
        expect_close_without_text(socket);
        denial
    });
    let mut session = connect_foreground(endpoint, 2);
    session.prepare_pre_bind_response_wait_for_lifecycle_test(503);
    session
        .poll_pre_bind_response_wait_for_lifecycle_test(Duration::from_secs(2))
        .unwrap();
    assert_pre_bind_diagnostics(&session, 2, 1, 1, 1, 0);

    let harness = sink_harness(Some((1, SinkFailure::Target(cause))));
    let captured = harness.trace.clone();
    let error = session
        .bind_ordered_turn_stream_sink(harness.sink)
        .unwrap_err();
    assert_eq!(
        error,
        OrderedTurnStreamBindingError::BufferedSubmission(cause),
    );
    let captured = captured.lock().unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].request_id, 21);
    assert_eq!(
        captured[0].disposition,
        beryl_backend::ApprovalResponseDisposition::AutoDenied,
    );
    assert_pre_bind_diagnostics(&session, 2, 0, 1, 1, 0);
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(session.pre_bind_prefix_is_empty_for_lifecycle_test());
    assert_eq!(server.join().unwrap(), 21);
}

#[test]
fn bound_permission_target_failure_gets_no_denial_and_retires_the_connection() {
    let cause = OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::StagingConflict);
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 22, ApprovalRequestKind::Permissions);
        expect_close_without_text(socket);
    });
    let mut session = connect_foreground(endpoint, 2);
    session.prepare_pre_bind_response_wait_for_lifecycle_test(505);
    let harness = sink_harness(Some((1, SinkFailure::Target(cause))));
    let captured = harness.trace.clone();
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();

    let error = session
        .poll_ordered_turn_stream_progress(Duration::from_secs(2))
        .unwrap_err();
    match error {
        ManagedBackendError::ApprovalTargetFailed {
            request,
            cause: actual,
        } => {
            assert_eq!(actual, cause);
            assert_eq!(request.request_id().as_i64(), Some(22));
            assert_eq!(
                request.response_disposition(),
                beryl_backend::ApprovalResponseDisposition::ResponseRequired,
            );
        }
        other => panic!("unexpected target-local result: {other:?}"),
    }
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert_pre_bind_diagnostics(&session, 2, 0, 0, 0, 0);
    let captured = captured.lock().unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].request_id, 22);
    assert_eq!(
        captured[0].disposition,
        beryl_backend::ApprovalResponseDisposition::ResponseRequired,
    );
    drop(captured);

    server.join().unwrap();
}

#[test]
fn bound_permission_wrong_durable_target_gets_no_denial_and_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 23, ApprovalRequestKind::Permissions);
        expect_close_without_text(socket);
    });
    let mut session = connect_foreground(endpoint, 2);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(Some((1, SinkFailure::WrongPermissionTarget)));
    let captured = harness.trace.clone();
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();

    let error = session
        .poll_ordered_turn_stream_progress(Duration::from_secs(2))
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ApprovalInterruptionMismatch {
            kind: ApprovalRequestKind::Permissions,
            ..
        }
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    let captured = captured.lock().unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].disposition,
        beryl_backend::ApprovalResponseDisposition::ResponseRequired,
    );
    server.join().unwrap();
}

#[test]
fn bound_permission_without_durable_owner_gets_no_denial_and_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 24, ApprovalRequestKind::Permissions);
        expect_close_without_text(socket);
    });
    let mut session = connect_foreground(endpoint, 2);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(Some((1, SinkFailure::MissingPermissionStopOwner)));
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();

    let error = session
        .poll_ordered_turn_stream_progress(Duration::from_secs(2))
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ApprovalInterruptionMismatch {
            kind: ApprovalRequestKind::Permissions,
            actual: beryl_backend::ApprovalInterruption::NotRequired,
        }
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn non_interrupting_approval_rejects_durable_owner_after_sending_its_safe_denial() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 25, ApprovalRequestKind::CommandExecution);
        let denial = read_denial_id(socket);
        expect_close_without_text(socket);
        denial
    });
    let mut session = connect_foreground(endpoint, 2);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(Some((1, SinkFailure::UnexpectedNonInterruptingStopOwner)));
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();

    let error = session
        .poll_ordered_turn_stream_progress(Duration::from_secs(2))
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ApprovalInterruptionMismatch {
            kind: ApprovalRequestKind::CommandExecution,
            actual: beryl_backend::ApprovalInterruption::DurableStopOwned { .. },
        }
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert_eq!(server.join().unwrap(), 25);
}

#[test]
fn unavailable_compact_control_abandons_prefix_and_blocks_later_response_publication() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 31, ApprovalRequestKind::CommandExecution);
        let denial = read_denial_id(socket);
        send_unavailable_compact(socket);
        send_initialize_response(socket, 504);
        expect_close_without_text(socket);
        denial
    });
    let mut session = connect_foreground(endpoint, 2);
    session.prepare_pre_bind_response_wait_for_lifecycle_test(504);
    session
        .poll_pre_bind_response_wait_for_lifecycle_test(Duration::from_secs(2))
        .unwrap();
    assert_pre_bind_diagnostics(&session, 2, 1, 1, 1, 0);

    let error = session
        .poll_pre_bind_response_wait_for_lifecycle_test(Duration::from_secs(2))
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ForegroundIngress {
            source: ForegroundIngressError::KnownControlUnavailable,
            ..
        }
    ));
    assert_pre_bind_diagnostics(&session, 2, 0, 1, 1, 0);
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(session.pre_bind_prefix_is_empty_for_lifecycle_test());

    let harness = sink_harness(None);
    assert_eq!(
        session.bind_ordered_turn_stream_sink(harness.sink),
        Err(OrderedTurnStreamBindingError::TransportClosed),
    );
    assert_eq!(server.join().unwrap(), 31);
}
