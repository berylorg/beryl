use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};

use super::*;

const TIMEOUT: Duration = Duration::from_secs(10);

fn open(faults: FaultController, path: &std::path::Path) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn active_writer_refuses_election_and_settles_to_the_replacement_observer() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(faults.clone(), directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let store = Arc::new(store);
    let (first, first_wake) = observe(&store);
    let before = first.observe().unwrap();
    let pause = faults.block_next(FaultPoint::BeforeCommit);
    let worker_store = Arc::clone(&store);
    let worker = thread::spawn(move || {
        worker_store
            .execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(1, vec![1])))
    });
    assert!(pause.wait_until_reached(TIMEOUT));
    assert_eq!(first.observe().unwrap_err(), ObservationError::Busy);
    assert_eq!(
        before.try_elect(|| panic!("busy callback ran")),
        Err(ObservationError::Busy)
    );
    let (second, second_wake) = observe(&store);
    assert_eq!(first.observe().unwrap_err(), ObservationError::Revoked);
    assert_eq!(second.observe().unwrap_err(), ObservationError::Busy);
    assert!(first_wake.outcomes().is_empty());
    assert!(second_wake.outcomes().is_empty());
    pause.release();
    committed(worker.join().unwrap());
    assert!(first_wake.outcomes().is_empty());
    assert_eq!(second_wake.outcomes(), vec![Ok(())]);
    assert_eq!(second.observe().unwrap().try_elect(|| ()), Ok(()));
}

#[test]
fn election_that_wins_first_excludes_writer_entry_until_its_callback_finishes() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(faults.clone(), directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let store = Arc::new(store);
    let (observer, _) = observe(&store);
    let token = observer.observe().unwrap();
    let pause = faults.block_next(FaultPoint::BeforeCommit);
    let (start_tx, start_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let worker_store = Arc::clone(&store);
    let worker = thread::spawn(move || {
        start_rx.recv_timeout(TIMEOUT).unwrap();
        attempt_tx.send(()).unwrap();
        worker_store
            .execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(1, vec![1])))
    });
    let excluded = token
        .try_elect(|| {
            start_tx.send(()).unwrap();
            attempt_rx.recv_timeout(TIMEOUT).unwrap();
            !pause.wait_until_reached(Duration::from_millis(100))
        })
        .unwrap();
    let entered = pause.wait_until_reached(TIMEOUT);
    pause.release();
    committed(worker.join().unwrap());
    assert!(excluded);
    assert!(entered);
    assert_eq!(token.try_elect(|| ()), Err(ObservationError::Stale));
}

#[test]
fn same_home_recovery_revokes_old_tokens_before_reopening_the_generation() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(faults.clone(), directory.path());
    let (observer, _) = observe(&store);
    let token = observer.observe().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let pause = faults.block_next(FaultPoint::BeforeReopen);
    let worker = thread::spawn(move || store.recover_same_home());
    assert!(pause.wait_until_reached(TIMEOUT));
    assert_eq!(observer.observe().unwrap_err(), ObservationError::Closed);
    assert_eq!(token.try_elect(|| ()), Err(ObservationError::Closed));
    pause.release();
    let recovered = worker.join().unwrap().unwrap().publish();
    let (fresh, _) = observe(&recovered);
    assert_eq!(fresh.observe().unwrap().try_elect(|| ()), Ok(()));
    assert_eq!(token.try_elect(|| ()), Err(ObservationError::Closed));
    recovered.close().unwrap();
}

#[test]
fn last_observer_release_linearizes_with_an_inflight_election() {
    let directory = tempdir().unwrap();
    let store = open_home(directory.path());
    let (observer, _) = observe(&store);
    let token = observer.observe().unwrap();
    let election_token = token.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();
    let election = thread::spawn(move || {
        election_token.try_elect(|| {
            entered_tx.send(()).unwrap();
            finish_rx.recv_timeout(TIMEOUT).unwrap();
        })
    });
    entered_rx.recv_timeout(TIMEOUT).unwrap();
    let (dropping_tx, dropping_rx) = mpsc::channel();
    let (dropped_tx, dropped_rx) = mpsc::channel();
    let dropping = thread::spawn(move || {
        dropping_tx.send(()).unwrap();
        drop(observer);
        dropped_tx.send(()).unwrap();
    });
    dropping_rx.recv_timeout(TIMEOUT).unwrap();
    let drop_waited = dropped_rx.recv_timeout(Duration::from_millis(100)).is_err();
    finish_tx.send(()).unwrap();
    assert_eq!(election.join().unwrap(), Ok(()));
    dropping.join().unwrap();
    assert!(drop_waited);
    assert_eq!(token.try_elect(|| ()), Err(ObservationError::Revoked));
}

#[test]
fn persisted_corruption_injection_invalidates_the_mutation_interval() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let (observer, wake) = observe(&store);
    let token = observer.observe().unwrap();
    store
        .inject_persisted_corrupt_record::<AlphaDomain, support::BytesRecord<AlphaDomain>>(
            &alpha,
            &1_u64.to_be_bytes(),
            &[0, 0, 1],
        )
        .unwrap();
    assert_eq!(token.try_elect(|| ()), Err(ObservationError::Stale));
    assert_eq!(wake.outcomes(), vec![Ok(())]);
}

struct DropObservedMutation {
    observer: Arc<HomeMutationObserver>,
    dropped_busy: Arc<AtomicBool>,
    destroyed: Arc<AtomicBool>,
}

impl beryl_home_store::DomainMutation<AlphaDomain> for DropObservedMutation {
    type Error = support::FixtureMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        _reader: &beryl_home_store::DomainReader<'_, AlphaDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        Err(support::FixtureMutationError::Rejected(
            "health must refuse before preparation",
        ))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<support::BytesRecord<AlphaDomain>>(1)?;
        Ok(())
    }

    fn contribute(
        _prepared: Self::Prepared,
        _mutations: &mut beryl_home_store::MutationBuilder<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        Err(support::FixtureMutationError::Rejected(
            "health must refuse before contribution",
        ))
    }
}

impl Drop for DropObservedMutation {
    fn drop(&mut self) {
        self.dropped_busy.store(
            matches!(self.observer.observe(), Err(ObservationError::Busy)),
            Ordering::SeqCst,
        );
        self.destroyed.store(true, Ordering::SeqCst);
    }
}

#[test]
fn early_health_refusal_drops_command_custody_before_publishing_quiescence() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(faults.clone(), directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let (observer, wake) = observe(&store);
    let destroyed = Arc::new(AtomicBool::new(false));
    let dropped_busy = Arc::new(AtomicBool::new(false));
    let command = alpha.current_command(DropObservedMutation {
        observer: Arc::clone(&observer),
        destroyed: Arc::clone(&destroyed),
        dropped_busy: Arc::clone(&dropped_busy),
    });
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    assert!(matches!(
        not_committed(store.execute_current(command)),
        beryl_home_store::CommandError::HealthGate(_)
    ));
    assert!(destroyed.load(Ordering::SeqCst));
    assert!(dropped_busy.load(Ordering::SeqCst));
    assert_eq!(wake.outcomes(), vec![Ok(())]);
    store.close().unwrap();
}
