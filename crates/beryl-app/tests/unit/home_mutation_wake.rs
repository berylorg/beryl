use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct WakeCount(AtomicUsize);

impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn scheduler_observation_survives_without_catalog_worker() {
    let scheduler = Arc::new(WakeCount::default());
    let fanout = Arc::new(HomeMutationWake::new(Waker::from(Arc::clone(&scheduler))));
    let wake = Waker::from(fanout);
    wake.wake_by_ref();
    wake.wake();
    assert_eq!(scheduler.0.load(Ordering::SeqCst), 2);
}

#[test]
fn catalog_installation_preserves_scheduler_and_wakes_both_once() {
    let scheduler = Arc::new(WakeCount::default());
    let catalog = Arc::new(WakeCount::default());
    let fanout = Arc::new(HomeMutationWake::new(Waker::from(Arc::clone(&scheduler))));
    let wake = Waker::from(Arc::clone(&fanout));
    wake.wake_by_ref();
    fanout
        .catalog
        .set(Waker::from(Arc::clone(&catalog)))
        .unwrap();
    wake.wake_by_ref();
    wake.wake();
    assert_eq!(scheduler.0.load(Ordering::SeqCst), 3);
    assert_eq!(catalog.0.load(Ordering::SeqCst), 2);
}

#[test]
fn another_catalog_signal_cannot_replace_original_observation() {
    let scheduler = Arc::new(WakeCount::default());
    let original = Arc::new(WakeCount::default());
    let replacement = Arc::new(WakeCount::default());
    let fanout = Arc::new(HomeMutationWake::new(Waker::from(Arc::clone(&scheduler))));
    fanout
        .catalog
        .set(Waker::from(Arc::clone(&original)))
        .unwrap();
    assert!(
        fanout
            .catalog
            .set(Waker::from(Arc::clone(&replacement)))
            .is_err()
    );
    Waker::from(fanout).wake();
    assert_eq!(scheduler.0.load(Ordering::SeqCst), 1);
    assert_eq!(original.0.load(Ordering::SeqCst), 1);
    assert_eq!(replacement.0.load(Ordering::SeqCst), 0);
}
