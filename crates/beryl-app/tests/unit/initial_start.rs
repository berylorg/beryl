use super::*;
use std::{
    sync::{Barrier, mpsc},
    thread,
    time::Duration,
};

fn verify_waiters(release: bool) {
    let owner = InitialStartOwner::new();
    let gate = owner.gate();
    let (entered, entries) = mpsc::channel();
    let (completed, completions) = mpsc::channel();
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let gate = Arc::clone(&gate);
            let entered = entered.clone();
            let completed = completed.clone();
            thread::spawn(move || {
                entered.send(()).unwrap();
                completed.send(gate.wait()).unwrap();
            })
        })
        .collect();
    for _ in 0..8 {
        entries.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    assert!(matches!(
        completions.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    if release {
        assert!(owner.release());
    } else {
        drop(owner);
    }
    for _ in 0..8 {
        assert_eq!(
            completions.recv_timeout(Duration::from_secs(5)).unwrap(),
            release
        );
    }
    for worker in workers {
        worker.join().unwrap();
    }
    gate.cancel();
    assert_eq!(gate.wait(), release);
}

#[test]
fn dormant_workers_wait_for_release_and_all_wake() {
    verify_waiters(true);
}

#[test]
fn abandoned_owner_wakes_all_workers_without_releasing_work() {
    verify_waiters(false);
}

#[test]
fn cancellation_before_wait_prevents_later_release() {
    let owner = InitialStartOwner::new();
    let gate = owner.gate();
    gate.cancel();
    gate.cancel();
    assert!(!owner.release());
    assert!(!gate.wait());
}

#[test]
fn ordinary_ready_gate_stays_released() {
    let gate = InitialStartGate::ready();
    assert!(gate.wait());
    gate.cancel();
    assert!(gate.wait());
}

#[test]
fn competing_release_and_cancel_have_one_stable_outcome() {
    for _ in 0..128 {
        let owner = InitialStartOwner::new();
        let gate = owner.gate();
        let barrier = Arc::new(Barrier::new(2));
        let release_barrier = Arc::clone(&barrier);
        let worker = thread::spawn(move || {
            release_barrier.wait();
            owner.release()
        });
        barrier.wait();
        gate.cancel();
        let released = worker.join().unwrap();
        assert_eq!(gate.wait(), released);
        gate.cancel();
        assert_eq!(gate.wait(), released);
    }
}
