include!("../src/response_work.rs");

#[test]
fn nonblocking_snapshot_preserves_exact_identity_and_all_response_facts() {
    let tracker = ResponseWorkTracker::new(2, Some(17));
    let observer = tracker.observe();
    let before = observer.snapshot().unwrap();
    assert_eq!(observer.try_snapshot().unwrap(), before);
    let held = observer.try_read().unwrap();
    assert_eq!(held.snapshot(), &before);
    assert!(matches!(observer.try_read(), Err(ResponseWorkError::Busy)));
    drop(held);
    assert_eq!(observer.try_snapshot().unwrap(), before);
    let foreign = ResponseWorkTracker::new(2, Some(17)).observe();
    assert_ne!(foreign.try_snapshot().unwrap(), before);
    tracker.bind(18, || Ok::<_, ()>(())).unwrap();
    tracker.record_response(true, || {});
    tracker.release_capability();
    let after = observer.try_snapshot().unwrap();
    assert_eq!(after, observer.snapshot().unwrap());
    assert_eq!(after.session_generation(), Some(18));
    assert!(after.response_written());
    assert_eq!(after.retained_capabilities(), 1);
    assert_eq!(
        observer.validate_revision(before.revision()),
        Err(ResponseWorkError::StaleRevision)
    );
    observer.validate_revision(after.revision()).unwrap();
}

#[test]
fn contended_snapshot_refuses_before_response_owner_releases() {
    let tracker = ResponseWorkTracker::new(2, Some(17));
    let observer = tracker.observe();
    let before = observer.snapshot().unwrap();
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let owner = std::thread::spawn(move || {
        tracker.bind(18, || {
            held_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Err::<(), _>(())
        })
    });
    held_rx.recv().unwrap();
    let contender = observer.clone();
    let (result_tx, result_rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        result_tx.send(contender.try_snapshot()).unwrap();
    });
    let result = result_rx.recv_timeout(std::time::Duration::from_secs(2));
    release_tx.send(()).unwrap();
    assert_eq!(owner.join().unwrap(), Err(()));
    reader.join().unwrap();
    assert_eq!(result.unwrap(), Err(ResponseWorkError::Busy));
    assert_eq!(observer.try_snapshot().unwrap(), before);
}

#[test]
fn poisoned_snapshot_refuses_without_recovering_response_state() {
    let tracker = ResponseWorkTracker::new(2, Some(17));
    let observer = tracker.observe();
    let state = Arc::clone(&observer.state);
    assert!(
        std::thread::spawn(move || {
            let _guard = state.lock().unwrap();
            panic!("poison response observation");
        })
        .join()
        .is_err()
    );
    assert_eq!(observer.try_snapshot(), Err(ResponseWorkError::Poisoned));
    assert!(matches!(
        observer.try_read(),
        Err(ResponseWorkError::Poisoned)
    ));
    let state = observer.state.lock().unwrap_err().into_inner();
    assert_eq!(state.revision, Some(0));
    assert_eq!(state.retained_capabilities, 2);
    assert!(!state.response_written);
    assert!(!state.completion_registered);
}

#[test]
fn exhausted_revision_refuses_without_consuming_completion_registration() {
    struct WakeCount(std::sync::atomic::AtomicUsize);
    impl std::task::Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let tracker = ResponseWorkTracker::new(1, Some(17));
    let observer = tracker.observe();
    let wake = Arc::new(WakeCount(std::sync::atomic::AtomicUsize::new(0)));
    observer
        .register_completion_waker(Waker::from(Arc::clone(&wake)))
        .unwrap();
    observer.state.lock().unwrap().revision = Some(u64::MAX);
    tracker.bind(18, || Ok::<_, ()>(())).unwrap();
    assert_eq!(
        observer.try_snapshot(),
        Err(ResponseWorkError::RevisionUnavailable)
    );
    assert!(matches!(
        observer.try_read(),
        Err(ResponseWorkError::RevisionUnavailable)
    ));
    assert_eq!(
        observer.snapshot(),
        Err(ResponseWorkError::RevisionUnavailable)
    );
    assert_eq!(wake.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    tracker.release_capability();
    assert_eq!(wake.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        observer.register_completion_waker(Waker::from(wake)),
        Err(ResponseWorkError::CompletionAlreadyRegistered)
    );
}

#[test]
fn retained_read_defers_response_transition_and_wake_until_release() {
    struct WakeCount(std::sync::atomic::AtomicUsize);
    impl std::task::Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let tracker = ResponseWorkTracker::new(1, Some(17));
    let observer = tracker.observe();
    let before = observer.snapshot().unwrap();
    let wake = Arc::new(WakeCount(std::sync::atomic::AtomicUsize::new(0)));
    observer
        .register_completion_waker(Waker::from(Arc::clone(&wake)))
        .unwrap();
    let guard = observer.try_read().unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let writer = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        tracker.record_response(true, || {});
        tracker.release_capability();
        finished_tx.send(()).unwrap();
    });
    started_rx.recv().unwrap();
    let early = finished_rx.recv_timeout(std::time::Duration::from_millis(50));
    assert_eq!(guard.snapshot(), &before);
    assert_eq!(wake.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    drop(guard);
    writer.join().unwrap();
    assert!(matches!(
        early,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    finished_rx.recv().unwrap();
    let after = observer.try_read().unwrap();
    assert!(after.snapshot().response_written());
    assert_eq!(after.snapshot().retained_capabilities(), 0);
    assert_ne!(after.snapshot().revision(), before.revision());
    assert_eq!(wake.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn read_guard_release_alone_preserves_custody_and_completion_registration() {
    let tracker = ResponseWorkTracker::new(2, Some(17));
    let observer = tracker.observe();
    let before = observer.snapshot().unwrap();
    {
        let held = observer.try_read().unwrap();
        assert_eq!(held.snapshot(), &before);
    }
    assert_eq!(observer.snapshot().unwrap(), before);
    let state = observer.state.try_lock().unwrap();
    assert!(!state.completion_registered);
    assert!(state.completion_waker.is_none());
}
