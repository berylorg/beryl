use std::time::Duration;

use beryl_backend::{
    ApprovalRequestKind, ApprovalResponseDisposition, ManagedBackendError, ManagedBackendSession,
    OrderedTurnStreamProgress, ResponseWorkError,
};

use super::support::sink_harness;
use super::transport_support::{
    connect_foreground, expect_close_without_text, read_denial_id, send_approval, spawn_server,
};

#[test]
fn dropping_routed_event_preserves_backend_response_custody_until_successful_denial() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 71, ApprovalRequestKind::CommandExecution);
        read_denial_id(socket)
    });
    let mut session = connect_foreground(endpoint, 2);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(false);
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();
    assert_eq!(
        session
            .poll_ordered_turn_stream_progress(Duration::from_secs(2))
            .unwrap(),
        OrderedTurnStreamProgress::Progress,
    );
    let observer = harness.observations.try_recv().unwrap();
    let settled = observer.snapshot().unwrap();
    assert!(settled.response_written());
    assert_eq!(settled.retained_capabilities(), 0);
    assert_eq!(observer.clone().snapshot().unwrap(), settled);
    observer.validate_revision(settled.revision()).unwrap();
    drop(session);
    assert_eq!(observer.snapshot().unwrap(), settled);
    assert_eq!(server.join().unwrap(), 71);
}

#[test]
fn failed_automatic_denial_releases_custody_without_claiming_response_success() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 72, ApprovalRequestKind::FileChange);
        expect_close_without_text(socket);
    });
    let mut session = connect_foreground(endpoint, 2);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(true);
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    assert!(matches!(
        session.poll_ordered_turn_stream_progress(Duration::from_secs(2)),
        Err(ManagedBackendError::ApprovalDenialWrite { .. }),
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    let request = harness.approval_requests.try_recv().unwrap();
    let observer = harness.observations.try_recv().unwrap();
    assert_eq!(
        request.response_disposition(),
        ApprovalResponseDisposition::ResponseRequired
    );
    let retained = observer.snapshot().unwrap();
    assert!(!retained.response_written());
    assert_eq!(retained.retained_capabilities(), 1);
    drop(request);
    assert_eq!(
        observer.validate_revision(retained.revision()),
        Err(ResponseWorkError::StaleRevision)
    );
    let released = observer.snapshot().unwrap();
    assert!(!released.response_written());
    assert_eq!(released.retained_capabilities(), 0);
    server.join().unwrap();
}

#[test]
fn automatic_response_completion_is_independent_of_retained_caller_capability() {
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 73, ApprovalRequestKind::CommandExecution);
        read_denial_id(socket)
    });
    let mut session = connect_foreground(endpoint, 2);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(true);
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();
    session
        .poll_ordered_turn_stream_progress(Duration::from_secs(2))
        .unwrap();
    let request = harness.approval_requests.try_recv().unwrap();
    let observer = harness.observations.try_recv().unwrap();
    let settled = observer.snapshot().unwrap();
    assert!(settled.response_written());
    assert_eq!(settled.retained_capabilities(), 1);
    assert_eq!(
        request.response_disposition(),
        ApprovalResponseDisposition::AutoDenied
    );
    assert!(matches!(
        session.deny_approval_request(&request),
        Err(ManagedBackendError::ApprovalResponseAlreadySent { .. })
    ));
    assert_eq!(observer.snapshot().unwrap(), settled);
    drop(request);
    assert_eq!(observer.snapshot().unwrap().retained_capabilities(), 0);
    assert_eq!(server.join().unwrap(), 73);
}

#[test]
fn caller_denial_and_foreign_checks_preserve_exact_request_revision() {
    let (endpoint, server) = spawn_server(read_denial_id);
    let mut session = connect_foreground(endpoint, 2);
    let request =
        session.bound_approval_request_for_lifecycle_test(ApprovalRequestKind::FileChange);
    let observer = request.response_work();
    let before = observer.snapshot().unwrap();
    assert_eq!(before.retained_capabilities(), 1);
    assert!(!before.response_written());
    let same_ids =
        session.bound_approval_request_for_lifecycle_test(ApprovalRequestKind::FileChange);
    assert_eq!(
        same_ids
            .response_work()
            .validate_revision(before.revision()),
        Err(ResponseWorkError::ForeignRevision)
    );
    let mut foreign =
        ManagedBackendSession::unsupported_streamed_input_gate_for_lifecycle_test().unwrap();
    assert!(matches!(
        foreign.deny_approval_request(&request),
        Err(ManagedBackendError::ApprovalResponseAuthorityMismatch { .. })
    ));
    assert_eq!(observer.snapshot().unwrap(), before);
    session.deny_approval_request(&request).unwrap();
    assert_eq!(
        request.response_disposition(),
        ApprovalResponseDisposition::Denied
    );
    let written = observer.snapshot().unwrap();
    assert!(written.response_written());
    assert_eq!(written.retained_capabilities(), 1);
    assert_eq!(
        observer.validate_revision(before.revision()),
        Err(ResponseWorkError::StaleRevision)
    );
    assert!(matches!(
        session.deny_approval_request(&request),
        Err(ManagedBackendError::ApprovalResponseAlreadySent { .. })
    ));
    assert_eq!(observer.snapshot().unwrap(), written);
    drop(request);
    assert_eq!(observer.snapshot().unwrap().retained_capabilities(), 0);
    assert_eq!(server.join().unwrap(), 1);
}

#[test]
fn releasing_unbound_request_leaves_only_non_authorizing_observations() {
    let request = beryl_backend::lifecycle_test_support::approval_request(
        ApprovalRequestKind::Permissions,
        ApprovalResponseDisposition::ResponseRequired,
        None,
        None,
        None,
    );
    let observer = request.response_work();
    let before = observer.snapshot().unwrap();
    assert_eq!(before.session_generation(), None);
    assert_eq!(before.retained_capabilities(), 1);
    let clone = observer.clone();
    drop(request);
    let after = clone.snapshot().unwrap();
    assert_eq!(after.retained_capabilities(), 0);
    assert!(!after.response_written());
    assert_eq!(
        clone.validate_revision(before.revision()),
        Err(ResponseWorkError::StaleRevision)
    );
    assert_eq!(observer.snapshot().unwrap(), after);
}
