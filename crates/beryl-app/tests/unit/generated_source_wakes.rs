use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Wake, Waker};

#[derive(Default)]
struct WakeCount(AtomicUsize);

impl Wake for WakeCount {
    fn wake(self: Arc<Self>) { self.0.fetch_add(1, Ordering::Relaxed); }
    fn wake_by_ref(self: &Arc<Self>) { self.0.fetch_add(1, Ordering::Relaxed); }
}

#[test]
fn quiet_driver_observations_preserve_work_revision_without_restarting_source_discovery() {
    let boundary = ConnectionWorkBoundary::new();
    let wakes = Arc::new(WakeCount::default());
    boundary.install_generated_waker(Waker::from(Arc::clone(&wakes)));
    let before = boundary.try_observe().unwrap();
    for _ in 0..64 { drop(boundary.begin_quiet_change()); }
    assert_eq!(boundary.try_elect(&before, || ()), Err(RuntimeWorkError::Stale));
    assert_eq!(wakes.0.load(Ordering::Relaxed), 0);
    drop(boundary.begin_change());
    assert_eq!(wakes.0.load(Ordering::Relaxed), 1);
}

#[test]
fn source_capacity_notification_requires_an_actual_waiter_and_coalesces_repeated_refusals() {
    let boundary = ConnectionWorkBoundary::new();
    let wakes = Arc::new(WakeCount::default());
    boundary.install_generated_waker(Waker::from(Arc::clone(&wakes)));
    for _ in 0..64 { boundary.wake_generated_capacity(); }
    assert_eq!(wakes.0.load(Ordering::Relaxed), 0);
    boundary.await_generated_capacity();
    boundary.await_generated_capacity();
    boundary.wake_generated_capacity();
    boundary.wake_generated_capacity();
    assert_eq!(wakes.0.load(Ordering::Relaxed), 1);
    boundary.await_generated_capacity();
    drop(boundary.begin_quiet_change());
    assert_eq!(wakes.0.load(Ordering::Relaxed), 2);
    drop(boundary.begin_quiet_change());
    assert_eq!(wakes.0.load(Ordering::Relaxed), 2);
}

#[test]
fn unavailable_command_observation_does_not_claim_proven_source_loss() {
    use crate::cas_projection::{GeneratedSourceEligibility, LiveCommandAdmissionError, ProjectionCoordinatorError};
    let unavailable = GeneratedSourceEligibility::from(LiveCommandAdmissionError::Unavailable);
    assert!(matches!(unavailable,
        GeneratedSourceEligibility::Internal(ProjectionCoordinatorError::AcquisitionServiceUnavailable(
            LiveCommandAdmissionError::Unavailable,
        ))));
    assert!(matches!(GeneratedSourceEligibility::from(LiveCommandAdmissionError::Closed),
        GeneratedSourceEligibility::LostSourceAuthority));
}
