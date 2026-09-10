mod support;

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, Weak},
    task::{Wake, Waker},
};

use beryl_home_store::{
    HomeCommand, HomeMutationObservationError as ObservationError, HomeMutationObserver,
};
use tempfile::tempdir;

use support::{AlphaDomain, PutBytes, committed, not_committed, open_home};

#[derive(Default)]
struct WakeProbe {
    observer: Mutex<Weak<HomeMutationObserver>>,
    outcomes: Mutex<Vec<Result<(), ObservationError>>>,
}

impl WakeProbe {
    fn attach(&self, observer: &Arc<HomeMutationObserver>) {
        *self.observer.lock().unwrap() = Arc::downgrade(observer);
    }

    fn outcomes(&self) -> Vec<Result<(), ObservationError>> {
        self.outcomes.lock().unwrap().clone()
    }
}

impl Wake for WakeProbe {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let observer = self.observer.lock().unwrap().upgrade();
        let result = observer
            .ok_or(ObservationError::Revoked)
            .and_then(|observer| observer.observe()?.try_elect(|| ()));
        self.outcomes.lock().unwrap().push(result);
    }
}

fn observe(store: &beryl_home_store::HomeStore) -> (Arc<HomeMutationObserver>, Arc<WakeProbe>) {
    let wake = Arc::new(WakeProbe::default());
    let observer = Arc::new(
        store
            .observe_mutations(Waker::from(Arc::clone(&wake)))
            .unwrap(),
    );
    wake.attach(&observer);
    (observer, wake)
}

#[test]
fn commands_invalidate_old_reads_and_notify_after_election_becomes_available() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let (observer, wake) = observe(&store);
    let original = observer.observe().unwrap();
    let revision = store.home_revision().unwrap();
    assert_eq!(original.try_elect(|| 17), Ok(17));
    assert_eq!(store.home_revision().unwrap(), revision);
    assert!(wake.outcomes().is_empty());

    committed(store.execute_current(
        alpha.current_command(PutBytes::<AlphaDomain>::new(1, b"current".to_vec())),
    ));
    assert_eq!(original.try_elect(|| ()), Err(ObservationError::Stale));
    assert_eq!(wake.outcomes(), vec![Ok(())]);

    let before_second = observer.observe().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(2, b"fenced".to_vec()),
        ))
        .unwrap();
    committed(store.execute(command));
    assert_eq!(before_second.try_elect(|| ()), Err(ObservationError::Stale));
    assert_eq!(wake.outcomes(), vec![Ok(()), Ok(())]);
    assert_eq!(store.domain_revision(&alpha).unwrap().get(), 3);
}

#[test]
fn rejected_writer_attempt_invalidates_without_claiming_a_commit() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let stale_revision = store.home_revision().unwrap();
    committed(
        store.execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(1, vec![1]))),
    );
    let (observer, wake) = observe(&store);
    let before = observer.observe().unwrap();
    let committed_revision = store.home_revision().unwrap();
    let mut command = HomeCommand::new(stale_revision);
    command
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(2, vec![2]),
        ))
        .unwrap();
    let _ = not_committed(store.execute(command));
    assert_eq!(store.home_revision().unwrap(), committed_revision);
    assert_eq!(before.try_elect(|| ()), Err(ObservationError::Stale));
    assert_eq!(wake.outcomes(), vec![Ok(())]);
}

#[test]
fn domain_registration_participates_in_mutation_exclusion() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let (observer, wake) = observe(&store);
    let before = observer.observe().unwrap();
    store.register_domain::<AlphaDomain>().unwrap();
    assert_eq!(before.try_elect(|| ()), Err(ObservationError::Stale));
    assert_eq!(wake.outcomes(), vec![Ok(())]);
}

#[test]
fn replacement_and_last_owner_drop_revoke_tokens_without_retaining_the_home() {
    let directory = tempdir().unwrap();
    let store = open_home(directory.path());
    let (first, _) = observe(&store);
    let first_clone = first.as_ref().clone();
    let first_token = first.observe().unwrap();
    drop(first);
    assert_eq!(first_token.try_elect(|| ()), Ok(()));
    let (second, _) = observe(&store);
    assert_eq!(
        first_clone.observe().unwrap_err(),
        ObservationError::Revoked
    );
    assert_eq!(first_token.try_elect(|| ()), Err(ObservationError::Revoked));
    drop(first_clone);
    let second_token = second.observe().unwrap();
    drop(second);
    assert_eq!(
        second_token.try_elect(|| ()),
        Err(ObservationError::Revoked)
    );

    let (third, _) = observe(&store);
    let third_token = third.observe().unwrap();
    let home_id = store.home_id();
    store.close().unwrap();
    assert_eq!(third.observe().unwrap_err(), ObservationError::Closed);
    assert_eq!(third_token.try_elect(|| ()), Err(ObservationError::Closed));
    let reopened = open_home(directory.path());
    assert_eq!(reopened.home_id(), home_id);
    let (fresh, _) = observe(&reopened);
    assert_eq!(fresh.observe().unwrap().try_elect(|| ()), Ok(()));
    assert_eq!(third_token.try_elect(|| ()), Err(ObservationError::Closed));
}

#[test]
fn poisoned_observation_refuses_election_without_changing_writer_outcomes() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let (observer, wake) = observe(&store);
    let token = observer.observe().unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = token.try_elect(|| panic!("synthetic election panic"));
        }))
        .is_err()
    );
    assert_eq!(
        observer.observe().unwrap_err(),
        ObservationError::Unavailable
    );
    assert_eq!(token.try_elect(|| ()), Err(ObservationError::Unavailable));
    committed(
        store.execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(1, vec![1]))),
    );
    assert_eq!(store.domain_revision(&alpha).unwrap().get(), 2);
    assert_eq!(wake.outcomes(), vec![Err(ObservationError::Unavailable)]);
    store.close().unwrap();
}

#[cfg(feature = "test-faults")]
#[path = "mutation_observation/concurrency.rs"]
mod concurrency;
