include!("../src/response_work.rs");

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Debug, Default)]
struct MutationCounter {
    active: AtomicUsize,
    changes: AtomicUsize,
    unavailable: AtomicBool,
}

#[derive(Debug)]
struct CountedChange<'a>(&'a MutationCounter);

impl ResponseWorkMutation for CountedChange<'_> {}

impl Drop for CountedChange<'_> {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        if std::thread::panicking() {
            self.0.unavailable.store(true, Ordering::SeqCst);
        }
    }
}

impl ResponseWorkMutationObserver for MutationCounter {
    fn begin_change(&self) -> Box<dyn ResponseWorkMutation + '_> {
        self.changes.fetch_add(1, Ordering::SeqCst);
        self.active.fetch_add(1, Ordering::SeqCst);
        Box::new(CountedChange(self))
    }
}

#[derive(Debug)]
struct CompletionCheck {
    observer: ResponseWorkObserver,
    counter: Arc<MutationCounter>,
    wakes: AtomicUsize,
}

impl std::task::Wake for CompletionCheck {
    fn wake(self: Arc<Self>) {
        assert_eq!(self.counter.active.load(Ordering::SeqCst), 0);
        let snapshot = self.observer.try_snapshot().unwrap();
        assert!(snapshot.response_written() || snapshot.retained_capabilities() == 0);
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn registered_changes_bracket_updates_and_release_before_completion_wake() {
    for written in [false, true] {
        let tracker = ResponseWorkTracker::new(1, None);
        let observer = tracker.observe();
        let counter = Arc::new(MutationCounter::default());
        let before = observer.snapshot().unwrap();
        observer
            .register_mutation_observer(counter.clone())
            .unwrap();
        assert_eq!(observer.snapshot().unwrap(), before);
        let replacement = Arc::new(MutationCounter::default());
        assert_eq!(
            observer.register_mutation_observer(replacement.clone()),
            Err(ResponseWorkError::MutationAlreadyRegistered)
        );
        assert_eq!(Arc::strong_count(&replacement), 1);
        tracker
            .bind(12, || {
                assert_eq!(counter.active.load(Ordering::SeqCst), 1);
                Ok::<_, ()>(())
            })
            .unwrap();
        let wake = Arc::new(CompletionCheck {
            observer: observer.clone(),
            counter: counter.clone(),
            wakes: AtomicUsize::new(0),
        });
        observer
            .register_completion_waker(Waker::from(wake.clone()))
            .unwrap();
        tracker.record_response(written, || {
            assert_eq!(counter.active.load(Ordering::SeqCst), 1);
        });
        tracker.release_capability();
        assert_eq!(counter.active.load(Ordering::SeqCst), 0);
        assert_eq!(counter.changes.load(Ordering::SeqCst), 4);
        assert_eq!(wake.wakes.load(Ordering::SeqCst), 1);
        assert_eq!(replacement.changes.load(Ordering::SeqCst), 0);
        assert_eq!(observer.snapshot().unwrap().retained_capabilities(), 0);
        drop(wake);
        drop(observer);
        drop(tracker);
        assert_eq!(Arc::strong_count(&counter), 1);
    }
}

#[test]
fn registration_serializes_with_inflight_update_and_covers_every_later_change() {
    let tracker = Arc::new(ResponseWorkTracker::new(1, None));
    let observer = tracker.observe();
    let counter = Arc::new(MutationCounter::default());
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let writer = tracker.clone();
    let update = std::thread::spawn(move || {
        writer.record_response(true, || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        })
    });
    entered_rx.recv().unwrap();
    let registering = observer.clone();
    let registered_counter = counter.clone();
    let (registered_tx, registered_rx) = std::sync::mpsc::channel();
    let registration = std::thread::spawn(move || {
        registered_tx
            .send(registering.register_mutation_observer(registered_counter))
            .unwrap();
    });
    let early = registered_rx.recv_timeout(std::time::Duration::from_millis(50));
    release_tx.send(()).unwrap();
    update.join().unwrap();
    registration.join().unwrap();
    assert!(matches!(
        early,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    registered_rx.recv().unwrap().unwrap();
    assert!(observer.snapshot().unwrap().response_written());
    assert_eq!(counter.changes.load(Ordering::SeqCst), 0);
    tracker.release_capability();
    assert_eq!(counter.changes.load(Ordering::SeqCst), 1);
}

#[test]
fn unwind_invalidates_registered_observation_without_preventing_cleanup() {
    let tracker = ResponseWorkTracker::new(1, None);
    let observer = tracker.observe();
    let counter = Arc::new(MutationCounter::default());
    observer
        .register_mutation_observer(counter.clone())
        .unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tracker.record_response(true, || panic!("interrupted response update"));
        }))
        .is_err()
    );
    assert!(counter.unavailable.load(Ordering::SeqCst));
    assert_eq!(counter.active.load(Ordering::SeqCst), 0);
    assert_eq!(observer.try_snapshot(), Err(ResponseWorkError::Poisoned));
    tracker.release_capability();
    assert_eq!(counter.changes.load(Ordering::SeqCst), 2);
    assert_eq!(
        observer
            .state
            .lock()
            .unwrap_err()
            .into_inner()
            .retained_capabilities,
        0
    );
}

#[test]
fn failed_bind_and_exhaustion_keep_guards_balanced_and_late_wake_outside_locks() {
    let tracker = ResponseWorkTracker::new(1, None);
    let observer = tracker.observe();
    let counter = Arc::new(MutationCounter::default());
    observer
        .register_mutation_observer(counter.clone())
        .unwrap();
    let before = observer.snapshot().unwrap();
    assert_eq!(tracker.bind(12, || Err::<(), _>(())), Err(()));
    assert_eq!(observer.snapshot().unwrap(), before);
    assert_eq!(counter.active.load(Ordering::SeqCst), 0);
    tracker.record_response(true, || {});
    let wake = Arc::new(CompletionCheck {
        observer: observer.clone(),
        counter: counter.clone(),
        wakes: AtomicUsize::new(0),
    });
    observer
        .register_completion_waker(Waker::from(wake.clone()))
        .unwrap();
    assert_eq!(wake.wakes.load(Ordering::SeqCst), 1);
    assert_eq!(
        observer.register_completion_waker(Waker::from(wake.clone())),
        Err(ResponseWorkError::CompletionAlreadyRegistered)
    );
    assert_eq!(counter.changes.load(Ordering::SeqCst), 3);
    observer.state.lock().unwrap().revision = Some(u64::MAX);
    tracker.release_capability();
    assert_eq!(
        observer.snapshot(),
        Err(ResponseWorkError::RevisionUnavailable)
    );
    assert_eq!(counter.active.load(Ordering::SeqCst), 0);
    assert_eq!(counter.changes.load(Ordering::SeqCst), 4);
    assert_eq!(observer.state.lock().unwrap().retained_capabilities, 0);
}

#[test]
fn unavailable_sources_refuse_registration_without_retaining_the_observer() {
    for poison in [false, true] {
        let tracker = ResponseWorkTracker::new(1, None);
        let observer = tracker.observe();
        if poison {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _held = observer.state.lock().unwrap();
                panic!("poison response state");
            }));
        } else {
            observer.state.lock().unwrap().revision = None;
        }
        let counter = Arc::new(MutationCounter::default());
        assert_eq!(
            observer.register_mutation_observer(counter.clone()),
            Err(if poison {
                ResponseWorkError::Poisoned
            } else {
                ResponseWorkError::RevisionUnavailable
            })
        );
        assert_eq!(Arc::strong_count(&counter), 1);
        tracker.release_capability();
        assert_eq!(counter.changes.load(Ordering::SeqCst), 0);
    }
}
