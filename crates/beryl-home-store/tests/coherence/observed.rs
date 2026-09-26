use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, mpsc},
    task::Waker,
    thread,
    time::Duration,
};

use beryl_home_store::{
    HomeDomainRequirements, HomeMutationObservationError as ObservationError,
    HomeObservedCoherenceError as Error, HomeStore,
};
use tempfile::tempdir;

use super::support::{AlphaDomain, PutBytes, committed, open_home};

fn open(path: &std::path::Path) -> HomeStore {
    open_home(path)
        .prepare_publication(HomeDomainRequirements::new())
        .unwrap()
        .publish()
        .unwrap()
}

#[test]
fn observed_election_preserves_reads_and_forwards_only_the_callback_result() {
    let directory = tempdir().unwrap();
    let store = open(directory.path());
    let observer = store.observe_mutations(Waker::noop().clone()).unwrap();
    let observation = observer.observe().unwrap();
    let revision = store.home_revision().unwrap();
    let generation = store.health().generation().unwrap();
    let mut calls = 0;
    assert_eq!(
        store.try_elect_observed_coherent(&observation, generation, || {
            calls += 1;
            42
        }),
        Ok(42)
    );
    assert_eq!(calls, 1);
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(observation.try_elect(|| 7), Ok(7));
    store.close().unwrap();
}

#[test]
fn a_completed_write_invalidates_the_earlier_observed_election() {
    let directory = tempdir().unwrap();
    let mut candidate = open_home(directory.path());
    let domain = candidate.register_domain::<AlphaDomain>().unwrap();
    let store = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let observer = store.observe_mutations(Waker::noop().clone()).unwrap();
    let observation = observer.observe().unwrap();
    let generation = store.health().generation().unwrap();
    committed(
        store.execute_current(domain.current_command(PutBytes::<AlphaDomain>::new(1, vec![1]))),
    );
    assert_eq!(
        store.try_elect_observed_coherent(&observation, generation, || panic!("stale election")),
        Err(Error::Observation(ObservationError::Stale))
    );
    assert_eq!(
        store.try_elect_observed_coherent(&observer.observe().unwrap(), generation, || 7),
        Ok(7)
    );
    store.close().unwrap();
}

#[test]
fn matching_generations_and_reopened_home_identity_cannot_adopt_foreign_observations() {
    let first_directory = tempdir().unwrap();
    let second_directory = tempdir().unwrap();
    let first = open(first_directory.path());
    let second = open(second_directory.path());
    let observer = first.observe_mutations(Waker::noop().clone()).unwrap();
    let observation = observer.observe().unwrap();
    let generation = second.health().generation().unwrap();
    assert_eq!(first.health().generation(), Some(generation));
    assert_eq!(
        second.try_elect_observed_coherent(&observation, generation, || panic!("foreign election")),
        Err(Error::ForeignObservation)
    );
    let home_id = first.home_id();
    first.close().unwrap();
    let reopened = open(first_directory.path());
    assert_eq!(reopened.home_id(), home_id);
    assert_eq!(
        reopened.try_elect_observed_coherent(
            &observation,
            reopened.health().generation().unwrap(),
            || panic!("reopened adoption"),
        ),
        Err(Error::ForeignObservation)
    );
    second.close().unwrap();
    reopened.close().unwrap();
}

#[test]
fn observer_revocation_and_poison_never_invoke_the_election() {
    let directory = tempdir().unwrap();
    let store = open(directory.path());
    let generation = store.health().generation().unwrap();
    let observer = store.observe_mutations(Waker::noop().clone()).unwrap();
    let observation = observer.observe().unwrap();
    let replacement = store.observe_mutations(Waker::noop().clone()).unwrap();
    assert_eq!(
        store.try_elect_observed_coherent(&observation, generation, || panic!("revoked election")),
        Err(Error::Observation(ObservationError::Revoked))
    );
    let dropped = replacement.observe().unwrap();
    drop(replacement);
    assert_eq!(
        store.try_elect_observed_coherent(&dropped, generation, || panic!("dropped observer")),
        Err(Error::Observation(ObservationError::Revoked))
    );
    let replacement = store.observe_mutations(Waker::noop().clone()).unwrap();
    let poisoned = replacement.observe().unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = poisoned.try_elect(|| panic!("synthetic mutation-boundary poison"));
        }))
        .is_err()
    );
    assert_eq!(
        store.try_elect_observed_coherent(&poisoned, generation, || panic!("poisoned election")),
        Err(Error::Observation(ObservationError::Unavailable))
    );
    store.close().unwrap();
}

#[test]
fn contended_observed_election_refuses_before_the_lock_owner_releases() {
    let directory = tempdir().unwrap();
    let store = Arc::new(open(directory.path()));
    let observer = store.observe_mutations(Waker::noop().clone()).unwrap();
    let observation = observer.observe().unwrap();
    let generation = store.health().generation().unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let election_store = store.clone();
    let election_observation = observation.clone();
    let owner = thread::spawn(move || {
        election_store.try_elect_observed_coherent(&election_observation, generation, || {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let (result_tx, result_rx) = mpsc::channel();
    let contender_store = store.clone();
    let contender = thread::spawn(move || {
        let result = contender_store
            .try_elect_observed_coherent(&observation, generation, || panic!("contended election"));
        result_tx.send(result).unwrap();
    });
    let result = result_rx.recv_timeout(Duration::from_secs(2));
    release_tx.send(()).unwrap();
    assert_eq!(owner.join().unwrap(), Ok(()));
    contender.join().unwrap();
    assert_eq!(
        result.unwrap(),
        Err(Error::Observation(ObservationError::Busy))
    );
    Arc::try_unwrap(store).ok().unwrap().close().unwrap();
}
