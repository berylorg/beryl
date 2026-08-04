use std::thread;

use super::*;

#[test]
fn dropped_service_returns_typed_unavailability_and_releases_the_page() {
    let prepared = prepare().unwrap();
    let diagnostics = prepared.diagnostics();
    let (mut source, service) = prepared.into_parts();
    drop(service);

    assert_eq!(
        source.next_page(64).unwrap_err(),
        ThreadInjectionSourceError::Unavailable
    );
    let live = diagnostics.snapshot().live_capacity().unwrap();
    assert_eq!(live.pages().leased, 0);
    assert_eq!(live.pages().available, 1);
    assert_eq!(live.pages().total_leases, 1);
    assert!(!live.requests().receiver_open);
    assert!(!live.replies().sender_open);

    drop(source);
    assert!(diagnostics.snapshot().released());
}

#[test]
fn typed_cancellation_crosses_the_reply_ring_and_releases_every_owner() {
    let prepared = prepare().unwrap();
    let diagnostics = prepared.diagnostics();
    let (mut source, service) = prepared.into_parts();
    let worker = thread::spawn(move || source.next_page(64));

    let service_result = service_until_finished(service, &diagnostics, |_limit, page| {
        assert_eq!(page.capacity(), THREAD_INJECTION_MAX_PAGE_BYTES);
        Err(ThreadInjectionSourceError::Cancelled)
    });
    assert!(matches!(
        service_result,
        Err(ProjectionCoordinatorError::ProjectionWorkerStopped)
    ));
    assert_eq!(
        worker.join().unwrap().unwrap_err(),
        ThreadInjectionSourceError::Cancelled
    );
    let snapshot = diagnostics.snapshot();
    assert!(snapshot.released());
    assert_eq!(snapshot.logical_pages(), 0);
    assert_eq!(snapshot.logical_items(), 0);
}
