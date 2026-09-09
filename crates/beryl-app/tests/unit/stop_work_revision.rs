use super::*;

#[test]
fn stop_work_revision_reads_are_stable_and_exhaustion_is_sticky() {
    let state = StopState::new(StopCoordinatorState::default());
    assert_eq!(state.lock().unwrap().revision(), Some(0));
    assert!(state.lock().unwrap().stops.is_empty());
    assert_eq!(state.lock().unwrap().revision(), Some(0));
    state.inner.lock().unwrap().revision = Some(u64::MAX);
    state.lock().unwrap().stops.clear();
    assert_eq!(state.lock().unwrap().revision(), None);
    state.lock().unwrap().stops.clear();
    assert_eq!(state.lock().unwrap().revision(), None);
}

#[test]
fn stop_work_revision_poison_cannot_produce_a_healthy_snapshot() {
    let state = StopState::new(StopCoordinatorState::default());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = state.lock().unwrap();
            panic!("injected stop state poison");
        }))
        .is_err()
    );
    assert!(state.lock().is_err());
}
