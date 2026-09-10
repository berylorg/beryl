use std::sync::{Arc, Barrier};

use beryl_backend::{ApprovalRequestKind, ApprovalResponseDisposition, ResponseWorkError};

use super::support::CompletionProbe;
use super::transport_support::{connect_foreground, read_denial_id, spawn_server};

fn request() -> beryl_backend::ApprovalRequest {
    beryl_backend::lifecycle_test_support::approval_request(
        ApprovalRequestKind::Permissions,
        ApprovalResponseDisposition::ResponseRequired,
        None,
        None,
        None,
    )
}

#[test]
fn registration_preserves_revision_and_rejects_replacement_before_and_after_completion() {
    let request = request();
    let observer = request.response_work();
    let before = observer.snapshot().unwrap();
    let first = CompletionProbe::new(&observer);
    first.register().unwrap();
    assert_eq!(observer.snapshot().unwrap(), before);
    let second = CompletionProbe::new(&observer.clone());
    assert_eq!(
        second.register(),
        Err(ResponseWorkError::CompletionAlreadyRegistered)
    );
    assert_eq!(first.count(), 0);
    assert_eq!(second.count(), 0);
    drop(request);
    assert_eq!(first.count(), 1);
    assert_eq!(first.snapshot().retained_capabilities(), 0);
    assert_eq!(second.count(), 0);
    assert_eq!(
        second.register(),
        Err(ResponseWorkError::CompletionAlreadyRegistered)
    );
    let released_waker = Arc::downgrade(&first);
    drop(first);
    assert!(released_waker.upgrade().is_none());
    assert_eq!(observer.snapshot().unwrap().retained_capabilities(), 0);
}

#[test]
fn registration_after_unwritten_final_release_wakes_without_retaining_the_slot() {
    let request = request();
    let observer = request.response_work();
    drop(request);
    let before = observer.snapshot().unwrap();
    let completion = CompletionProbe::new(&observer);
    completion.register().unwrap();
    assert_eq!(completion.count(), 1);
    assert_eq!(completion.snapshot(), before);
    assert_eq!(observer.snapshot().unwrap(), before);
    let released_waker = Arc::downgrade(&completion);
    drop(completion);
    assert!(released_waker.upgrade().is_none());
    assert_eq!(observer.snapshot().unwrap(), before);
}

#[test]
fn registration_after_successful_write_wakes_while_response_capability_is_retained() {
    let (endpoint, server) = spawn_server(read_denial_id);
    let mut session = connect_foreground(endpoint, 2);
    let request =
        session.bound_approval_request_for_lifecycle_test(ApprovalRequestKind::FileChange);
    let observer = request.response_work();
    session.deny_approval_request(&request).unwrap();
    let before = observer.snapshot().unwrap();
    let completion = CompletionProbe::new(&observer);
    completion.register().unwrap();
    assert_eq!(completion.count(), 1);
    assert_eq!(completion.snapshot(), before);
    assert_eq!(before.retained_capabilities(), 1);
    assert!(before.response_written());
    assert_eq!(observer.snapshot().unwrap(), before);
    drop(request);
    assert_eq!(completion.count(), 1);
    assert_eq!(server.join().unwrap(), 1);
}

#[test]
fn competing_registrations_racing_final_release_deliver_exactly_one_wake() {
    for _ in 0..32 {
        let request = request();
        let observer = request.response_work();
        let first = CompletionProbe::new(&observer);
        let second = CompletionProbe::new(&observer);
        let start = Barrier::new(3);
        let (first_result, second_result) = std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                start.wait();
                first.register()
            });
            let second = scope.spawn(|| {
                start.wait();
                second.register()
            });
            start.wait();
            drop(request);
            (first.join().unwrap(), second.join().unwrap())
        });
        assert!(matches!(
            (first_result, second_result),
            (Ok(()), Err(ResponseWorkError::CompletionAlreadyRegistered))
                | (Err(ResponseWorkError::CompletionAlreadyRegistered), Ok(()))
        ));
        assert_eq!(first.count() + second.count(), 1);
        assert_eq!(observer.snapshot().unwrap().retained_capabilities(), 0);
    }
}

#[test]
fn registration_racing_successful_write_never_loses_or_repeats_completion() {
    for _ in 0..8 {
        let (endpoint, server) = spawn_server(read_denial_id);
        let mut session = connect_foreground(endpoint, 2);
        let request =
            session.bound_approval_request_for_lifecycle_test(ApprovalRequestKind::FileChange);
        let observer = request.response_work();
        let completion = CompletionProbe::new(&observer);
        let start = Barrier::new(2);
        std::thread::scope(|scope| {
            let registration = scope.spawn(|| {
                start.wait();
                completion.register().unwrap();
            });
            start.wait();
            session.deny_approval_request(&request).unwrap();
            registration.join().unwrap();
        });
        assert_eq!(completion.count(), 1);
        assert!(completion.snapshot().response_written());
        assert_eq!(completion.snapshot().retained_capabilities(), 1);
        drop(request);
        assert_eq!(completion.count(), 1);
        assert_eq!(server.join().unwrap(), 1);
    }
}
