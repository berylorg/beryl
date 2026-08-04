use std::time::Duration;

use beryl_backend::{
    ApprovalRequestKind, ApprovalResponseDisposition, ForegroundIngressError, ManagedBackendError,
    OrderedTurnStreamProgress, StopAttemptCorrelation, StopAttemptDisposition,
};

use super::support::{
    assert_pre_bind_diagnostics, connect_foreground, read_denial_id, send_approval,
    send_unavailable_compact, sink_harness, spawn_server,
};

#[test]
fn configured_prefix_above_64_admits_exact_capacity_and_binding_acknowledges_fifo_first() {
    const CAPACITY: usize = 70;
    let last_request_id = i64::try_from(CAPACITY).unwrap();
    let (endpoint, server) = spawn_server(move |socket| {
        let mut denials = Vec::new();
        for request_id in 1..=last_request_id {
            send_approval(socket, request_id, ApprovalRequestKind::CommandExecution);
            denials.push(read_denial_id(socket));
        }
        send_unavailable_compact(socket);
        denials
    });
    let mut session = connect_foreground(endpoint, CAPACITY);
    assert_pre_bind_diagnostics(&session, CAPACITY, 0, 0, 0, 0);
    session.prepare_pre_bind_response_wait_for_lifecycle_test(900);

    for admitted in 1..=CAPACITY {
        assert_eq!(
            session
                .poll_pre_bind_response_wait_for_lifecycle_test(Duration::from_secs(2))
                .unwrap(),
            OrderedTurnStreamProgress::Progress,
        );
        assert_pre_bind_diagnostics(
            &session,
            CAPACITY,
            admitted,
            admitted,
            u64::try_from(admitted).unwrap(),
            0,
        );
    }
    assert!(!session.pre_bind_prefix_is_empty_for_lifecycle_test());

    let harness = sink_harness(None);
    let captured = harness.trace.clone();
    session
        .bind_ordered_turn_stream_sink(harness.sink)
        .expect("all configured approvals reconcile through the ordered sink");

    let captured = captured
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    assert_eq!(captured.len(), CAPACITY);
    for (index, approval) in captured.iter().enumerate() {
        assert_eq!(approval.request_id, i64::try_from(index).unwrap() + 1);
        assert_eq!(
            approval.disposition,
            ApprovalResponseDisposition::AutoDenied
        );
    }
    assert_pre_bind_diagnostics(
        &session,
        CAPACITY,
        0,
        CAPACITY,
        u64::try_from(CAPACITY).unwrap(),
        0,
    );
    assert!(session.pre_bind_prefix_is_empty_for_lifecycle_test());

    let error = session
        .poll_ordered_turn_stream_progress(Duration::from_secs(2))
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ForegroundIngress {
            source: ForegroundIngressError::KnownControlUnavailable,
            ..
        }
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert_pre_bind_diagnostics(
        &session,
        CAPACITY,
        0,
        CAPACITY,
        u64::try_from(CAPACITY).unwrap(),
        0,
    );
    assert_eq!(
        server.join().unwrap(),
        (1..=last_request_id).collect::<Vec<_>>(),
    );
}

#[test]
fn bound_approval_remains_synchronous_and_does_not_enter_pre_bind_prefix() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 41, ApprovalRequestKind::FileChange);
        read_denial_id(socket)
    });
    let mut session = connect_foreground(endpoint, 4);
    assert_pre_bind_diagnostics(&session, 4, 0, 0, 0, 0);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(None);
    let captured = harness.trace.clone();
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();

    assert_eq!(
        session
            .poll_ordered_turn_stream_progress(Duration::from_secs(2))
            .unwrap(),
        OrderedTurnStreamProgress::Progress,
    );
    let captured = captured.lock().unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].request_id, 41);
    assert_eq!(
        captured[0].disposition,
        ApprovalResponseDisposition::ResponseRequired,
    );
    assert_pre_bind_diagnostics(&session, 4, 0, 0, 0, 0);
    assert_eq!(server.join().unwrap(), 41);
}

#[test]
fn bound_permission_denial_requires_a_route_matching_durable_stop_owner() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 42, ApprovalRequestKind::Permissions);
        read_denial_id(socket)
    });
    let mut session = connect_foreground(endpoint, 4);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(None);
    let captured = harness.trace.clone();
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();

    assert_eq!(
        session
            .poll_ordered_turn_stream_progress(Duration::from_secs(2))
            .unwrap(),
        OrderedTurnStreamProgress::Progress,
    );
    let captured = captured.lock().unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].request_id, 42);
    assert_eq!(
        captured[0].disposition,
        ApprovalResponseDisposition::ResponseRequired,
    );
    drop(captured);
    session.shutdown().unwrap();
    assert_eq!(server.join().unwrap(), 42);
}

#[test]
fn stop_attempt_disposition_preserves_exact_correlation_and_dispatch_state() {
    let correlation = StopAttemptCorrelation::from_bytes([0x51; 16]);
    let claimed = StopAttemptDisposition::ClaimedNotDispatched(correlation);
    assert_eq!(claimed.correlation(), correlation);
    assert!(!claimed.may_have_dispatched());

    let possible = StopAttemptDisposition::PossiblyDispatched(correlation);
    assert_eq!(possible.correlation(), correlation);
    assert!(possible.may_have_dispatched());
}
