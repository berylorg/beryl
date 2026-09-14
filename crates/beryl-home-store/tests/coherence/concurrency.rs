use std::{
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use beryl_home_store::{
    HomeCoherenceError, HomeHealthState,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

use super::{
    custody::open,
    support::{AlphaDomain, PutBytes, committed},
};

const TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn mutation_that_starts_first_refuses_election_until_its_outcome_releases_custody() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), &faults);
    let domain = store.register_domain::<AlphaDomain>().unwrap();
    let generation = store.health().generation().unwrap();
    let store = Arc::new(store);
    let pause = faults.block_next(FaultPoint::BeforeCommit);
    let worker_store = Arc::clone(&store);
    let worker = thread::spawn(move || {
        worker_store
            .execute_current(domain.current_command(PutBytes::<AlphaDomain>::new(1, vec![1])))
    });
    let reached = pause.wait_until_reached(TIMEOUT);
    let result = store.try_elect_coherent(generation, || panic!("active writer was missed"));
    pause.release();
    committed(worker.join().unwrap());
    assert!(reached);
    assert_eq!(result, Err(HomeCoherenceError::Busy));
    assert_eq!(store.try_elect_coherent(generation, || ()), Ok(()));
}

#[test]
fn election_that_starts_first_excludes_reservation_and_competing_election() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), &faults);
    let domain = store.register_domain::<AlphaDomain>().unwrap();
    let generation = store.health().generation().unwrap();
    let store = Arc::new(store);
    let pause = faults.block_next(FaultPoint::BeforeCommit);
    let (start_tx, start_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let worker_store = Arc::clone(&store);
    let worker = thread::spawn(move || {
        start_rx.recv_timeout(TIMEOUT).unwrap();
        attempt_tx.send(()).unwrap();
        worker_store
            .execute_current(domain.current_command(PutBytes::<AlphaDomain>::new(1, vec![1])))
    });
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let election_store = Arc::clone(&store);
    let election = thread::spawn(move || {
        election_store.try_elect_coherent(generation, || {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(TIMEOUT).unwrap();
        })
    });
    entered_rx.recv_timeout(TIMEOUT).unwrap();
    start_tx.send(()).unwrap();
    attempt_rx.recv_timeout(TIMEOUT).unwrap();
    let excluded = !pause.wait_until_reached(Duration::from_millis(100));
    let competing = store.try_elect_coherent(generation, || panic!("competing election ran"));
    release_tx.send(()).unwrap();
    let election_result = election.join().unwrap();
    let reached = pause.wait_until_reached(TIMEOUT);
    pause.release();
    committed(worker.join().unwrap());
    assert!(excluded);
    assert_eq!(competing, Err(HomeCoherenceError::Busy));
    assert_eq!(election_result, Ok(()));
    assert!(reached);
}

#[test]
fn structural_failure_waits_for_election_then_prevents_further_elections() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = Arc::new(open(directory.path(), &faults));
    let generation = store.health().generation().unwrap();
    let read_pause = faults.block_next(FaultPoint::BeforeReadConfirmation);
    let (done_tx, done_rx) = mpsc::channel();
    let worker_store = Arc::clone(&store);
    let worker = thread::spawn(move || {
        let failed = worker_store.home_revision().is_err();
        done_tx.send(failed).unwrap();
    });
    assert!(read_pause.wait_until_reached(TIMEOUT));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let election_store = Arc::clone(&store);
    let election = thread::spawn(move || {
        election_store.try_elect_coherent(generation, || {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(TIMEOUT).unwrap();
        })
    });
    entered_rx.recv_timeout(TIMEOUT).unwrap();
    read_pause.release_with_error(std::io::ErrorKind::Other);
    let excluded = done_rx.recv_timeout(Duration::from_millis(100)).is_err();
    release_tx.send(()).unwrap();
    let election_result = election.join().unwrap();
    worker.join().unwrap();
    assert!(excluded);
    assert_eq!(election_result, Ok(()));
    assert!(done_rx.recv_timeout(TIMEOUT).unwrap());
    assert_eq!(
        store.try_elect_coherent(generation, || panic!("failed health was missed")),
        Err(HomeCoherenceError::Unhealthy(HomeHealthState::Failed))
    );
}
