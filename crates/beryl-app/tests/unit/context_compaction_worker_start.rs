use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use super::*;

fn verify_partial_failure(fail_at: usize, panic_first: bool) {
    let stop = Arc::new((Mutex::new(false), Condvar::new()));
    let completed = Arc::new(AtomicUsize::new(0));
    let stop_calls = AtomicUsize::new(0);
    let result = start_workers(
        &crate::cas_projection::initial_start::InitialStartGate::ready(),
        |index| {
            if index == fail_at {
                return Err(io::Error::other("injected compaction worker spawn failure"));
            }
            let stop = Arc::clone(&stop);
            let completed = Arc::clone(&completed);
            std::thread::Builder::new().spawn(move || {
                let mut stopped = stop.0.lock().unwrap();
                while !*stopped {
                    stopped = stop.1.wait(stopped).unwrap();
                }
                drop(stopped);
                completed.fetch_add(1, Ordering::SeqCst);
                assert!(!(panic_first && index == 0), "injected worker panic");
            })
        },
        || {
            stop_calls.fetch_add(1, Ordering::SeqCst);
            *stop.0.lock().unwrap() = true;
            stop.1.notify_all();
        },
    );
    assert!(matches!(result, Err(ContextCompactionError::Unavailable)));
    assert_eq!(stop_calls.load(Ordering::SeqCst), 1);
    assert_eq!(completed.load(Ordering::SeqCst), fail_at);
}

#[test]
fn every_partial_spawn_failure_stops_and_joins_all_created_workers() {
    for fail_at in 0..COMPACTION_WORKER_CAPACITY {
        verify_partial_failure(fail_at, false);
    }
}

#[test]
fn a_panicking_worker_does_not_skip_remaining_partial_construction_joins() {
    verify_partial_failure(COMPACTION_WORKER_CAPACITY - 1, true);
}

#[test]
fn successful_construction_transfers_all_join_handles_without_stopping() {
    let completed = Arc::new(AtomicUsize::new(0));
    let workers = start_workers(
        &crate::cas_projection::initial_start::InitialStartGate::ready(),
        |_| {
            let completed = Arc::clone(&completed);
            std::thread::Builder::new().spawn(move || {
                completed.fetch_add(1, Ordering::SeqCst);
            })
        },
        || panic!("successful worker construction must not request shutdown"),
    )
    .unwrap();
    assert_eq!(workers.len(), COMPACTION_WORKER_CAPACITY);
    assert!(!join_all_workers(workers));
    assert_eq!(completed.load(Ordering::SeqCst), COMPACTION_WORKER_CAPACITY);
}

#[test]
fn every_partial_spawn_failure_cancels_and_joins_dormant_workers() {
    use crate::cas_projection::initial_start::InitialStartOwner;
    for fail_at in 0..COMPACTION_WORKER_CAPACITY {
        let owner = InitialStartOwner::new();
        let gate = owner.gate();
        let completed = Arc::new(AtomicUsize::new(0));
        let result = start_workers(
            &gate,
            |index| {
                if index == fail_at {
                    return Err(io::Error::other("injected dormant worker spawn failure"));
                }
                let gate = Arc::clone(&gate);
                let completed = Arc::clone(&completed);
                std::thread::Builder::new().spawn(move || {
                    assert!(!gate.wait());
                    completed.fetch_add(1, Ordering::SeqCst);
                })
            },
            || {},
        );
        assert!(matches!(result, Err(ContextCompactionError::Unavailable)));
        assert_eq!(completed.load(Ordering::SeqCst), fail_at);
        assert!(!owner.release());
    }
}
