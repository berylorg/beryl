use super::*;
use crate::process_admission::ProcessAdmissionGate;

#[test]
fn connection_observation_exact_identity_and_weak_lifetime() {
    let boundary = ConnectionWorkBoundary::new();
    let observation = boundary.try_observe().unwrap();
    let foreign = ConnectionWorkBoundary::new();
    assert_eq!(
        foreign.try_elect(&observation, || panic!("foreign publication")),
        Err(RuntimeWorkError::Foreign)
    );
    assert_eq!(boundary.try_elect(&observation, || 7), Ok(7));
    assert_eq!(observation.owner.strong_count(), 1);
    drop(boundary);
    assert!(observation.owner.upgrade().is_none());
}

#[test]
fn connection_observation_nested_changes_and_completed_aba_are_detected() {
    let boundary = ConnectionWorkBoundary::new();
    let before = boundary.try_observe().unwrap();
    let first = boundary.begin_change();
    let second = boundary.begin_change();
    assert!(matches!(
        boundary.try_observe(),
        Err(RuntimeWorkError::Busy)
    ));
    drop(first);
    assert_eq!(
        boundary.try_elect(&before, || panic!("active publication")),
        Err(RuntimeWorkError::Busy)
    );
    drop(second);
    assert_eq!(
        boundary.try_elect(&before, || panic!("stale publication")),
        Err(RuntimeWorkError::Stale)
    );
    let after = boundary.try_observe().unwrap();
    assert_eq!(boundary.try_elect(&after, || ()), Ok(()));
    assert_eq!(boundary.inner.state.lock().unwrap().active, 0);
}

#[test]
fn connection_observation_busy_refusal_does_not_wait_for_owner_or_change_execution() {
    let boundary = ConnectionWorkBoundary::new();
    let observation = boundary.try_observe().unwrap();
    let gate = ProcessAdmissionGate::new();
    let permit = gate.execution_permit();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let held = boundary.inner.state.lock().unwrap();
        let reader = scope.spawn(|| {
            send.send(
                gate.admit(|| boundary.try_elect(&observation, || panic!("busy publication"))),
            )
            .unwrap();
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(2));
        drop(held);
        reader.join().unwrap();
        assert_eq!(result.unwrap().unwrap(), Err(RuntimeWorkError::Busy));
    });
    permit.commit(|| ()).unwrap();
    assert_eq!(boundary.try_elect(&observation, || ()), Ok(()));
}

#[test]
fn connection_observation_election_excludes_new_changes_through_publication() {
    let boundary = ConnectionWorkBoundary::new();
    let observation = boundary.try_observe().unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (changed_tx, changed_rx) = std::sync::mpsc::channel();
    let changed = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let worker = boundary
            .try_elect(&observation, || {
                assert!(matches!(
                    boundary.try_observe(),
                    Err(RuntimeWorkError::Busy)
                ));
                let worker = scope.spawn(|| {
                    started_tx.send(()).unwrap();
                    let _change = boundary.begin_change();
                    changed.store(true, std::sync::atomic::Ordering::SeqCst);
                    changed_tx.send(()).unwrap();
                });
                started_rx.recv().unwrap();
                assert!(matches!(
                    changed_rx.recv_timeout(std::time::Duration::from_millis(50)),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ));
                assert!(!changed.load(std::sync::atomic::Ordering::SeqCst));
                worker
            })
            .unwrap();
        worker.join().unwrap();
    });
    assert!(changed.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        boundary.try_elect(&observation, || panic!("stale publication")),
        Err(RuntimeWorkError::Stale)
    );
}

#[test]
fn connection_observation_close_is_terminal_and_allows_cleanup_to_finish() {
    let boundary = ConnectionWorkBoundary::new();
    let observation = boundary.try_observe().unwrap();
    let change = boundary.begin_change();
    boundary.close();
    drop(change);
    drop(boundary.begin_change());
    assert_eq!(
        boundary.try_elect(&observation, || panic!("closed publication")),
        Err(RuntimeWorkError::Closed)
    );
    assert!(matches!(
        boundary.try_observe(),
        Err(RuntimeWorkError::Closed)
    ));
    assert_eq!(boundary.inner.state.lock().unwrap().active, 0);
}

#[test]
fn connection_observation_exhaustion_is_sticky_without_blocking_cleanup() {
    for exhaust_revision in [true, false] {
        let boundary = ConnectionWorkBoundary::new();
        let observation = boundary.try_observe().unwrap();
        {
            let mut state = boundary.inner.state.lock().unwrap();
            if exhaust_revision {
                state.revision = Some(u64::MAX);
            } else {
                state.active = usize::MAX;
            }
        }
        drop(boundary.begin_change());
        drop(boundary.begin_change());
        assert_eq!(
            boundary.try_elect(&observation, || panic!("exhausted publication")),
            Err(RuntimeWorkError::Unavailable)
        );
        assert_eq!(boundary.inner.state.lock().unwrap().revision, None);
        boundary.close();
    }
}

#[test]
fn connection_observation_poison_and_unwind_cannot_certify_partial_changes() {
    for poison_mutex in [true, false] {
        let boundary = ConnectionWorkBoundary::new();
        let observation = boundary.try_observe().unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if poison_mutex {
                    boundary
                        .try_elect(&observation, || panic!("publication unwind"))
                        .unwrap();
                } else {
                    let _change = boundary.begin_change();
                    panic!("mutation unwind");
                }
            }))
            .is_err()
        );
        drop(boundary.begin_change());
        assert_eq!(
            boundary.try_elect(&observation, || panic!("unavailable publication")),
            Err(RuntimeWorkError::Unavailable)
        );
        boundary.close();
    }
}

#[test]
fn connection_observation_repeated_history_keeps_only_constant_boundary_state() {
    let boundary = ConnectionWorkBoundary::new();
    for _ in 0..10_000 {
        let before = boundary.try_observe().unwrap();
        drop(boundary.begin_change());
        assert_eq!(
            boundary.try_elect(&before, || panic!("old publication")),
            Err(RuntimeWorkError::Stale)
        );
    }
    let state = boundary.inner.state.lock().unwrap();
    assert_eq!(state.active, 0);
    assert_eq!(state.revision, Some(10_000));
    assert_eq!(Arc::strong_count(&boundary.inner), 1);
    assert_eq!(Arc::weak_count(&boundary.inner), 0);
}
