use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct WakeCount(AtomicUsize);
impl std::task::Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn capacity_release_coalesces_and_preserves_a_wake_before_registration() {
    let operations = DiscussionSettlementOperations::new(
        ProcessAdmissionGate::new(),
        NonZeroUsize::new(1).unwrap(),
    );
    let first = JobId::from_bytes([1; 16]);
    let second = JobId::from_bytes([2; 16]);
    let flight = operations.acquire(first).unwrap();
    assert!(matches!(
        operations.acquire(second),
        Err(DiscussionSettlementError::Capacity)
    ));
    drop(flight);
    let wake = Arc::new(WakeCount(AtomicUsize::new(0)));
    operations.set_dispatch_capacity_waker(Waker::from(Arc::clone(&wake)));
    assert_eq!(wake.0.load(Ordering::SeqCst), 1);
    let flight = operations.acquire(first).unwrap();
    for _ in 0..3 {
        assert!(matches!(
            operations.acquire(first),
            Err(DiscussionSettlementError::DuplicateIdentity)
        ));
        assert!(matches!(
            operations.acquire(second),
            Err(DiscussionSettlementError::Capacity)
        ));
    }
    assert_eq!(wake.0.load(Ordering::SeqCst), 1);
    drop(flight);
    assert_eq!(wake.0.load(Ordering::SeqCst), 2);
    drop(operations.acquire(second).unwrap());
    assert_eq!(wake.0.load(Ordering::SeqCst), 2);
}

#[test]
fn capacity_release_fans_out_to_both_fixed_recipients() {
    let operations = DiscussionSettlementOperations::new(
        ProcessAdmissionGate::new(),
        NonZeroUsize::new(1).unwrap(),
    );
    let dispatch = Arc::new(WakeCount(AtomicUsize::new(0)));
    let coordinator = Arc::new(WakeCount(AtomicUsize::new(0)));
    operations.set_dispatch_capacity_waker(Waker::from(Arc::clone(&dispatch)));
    assert!(operations.set_coordinator_waker(Some(Waker::from(Arc::clone(&coordinator)))));
    let flight = operations.acquire(JobId::from_bytes([1; 16])).unwrap();
    assert!(!operations.capacity_ready());
    drop(flight);
    assert_eq!(dispatch.0.load(Ordering::SeqCst), 1);
    assert_eq!(coordinator.0.load(Ordering::SeqCst), 1);
    assert!(operations.capacity_ready());
}
