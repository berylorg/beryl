use std::{sync::Arc, task::Waker, thread, time::Duration};

use beryl_home_store::{
    CommandOutcome, HomeCoherenceError, HomeDomainRequirements, HomeHealthState,
    HomeMutationObservationError as ObservationError, HomeObservedCoherenceError as Error,
    ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

use super::{
    custody::{ReconciledDomain, open},
    support::{AlphaDomain, PutBytes, committed},
};

#[test]
fn active_writer_refuses_observed_election_and_completion_invalidates_the_observation() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = open(directory.path(), &faults);
    let domain = candidate.register_domain::<AlphaDomain>().unwrap();
    let store = Arc::new(
        candidate
            .prepare_publication(
                HomeDomainRequirements::new()
                    .with_domain::<AlphaDomain>()
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap(),
    );
    let observer = store.observe_mutations(Waker::noop().clone()).unwrap();
    let observation = observer.observe().unwrap();
    let generation = store.health().generation().unwrap();
    let pause = faults.block_next(FaultPoint::BeforeCommit);
    let worker_store = store.clone();
    let worker = thread::spawn(move || {
        worker_store
            .execute_current(domain.current_command(PutBytes::<AlphaDomain>::new(1, vec![1])))
    });
    let reached = pause.wait_until_reached(Duration::from_secs(10));
    let result =
        store.try_elect_observed_coherent(&observation, generation, || panic!("active writer"));
    pause.release();
    committed(worker.join().unwrap());
    assert!(reached);
    assert_eq!(result, Err(Error::Observation(ObservationError::Busy)));
    assert_eq!(
        store.try_elect_observed_coherent(&observation, generation, || panic!(
            "changed writer interval"
        )),
        Err(Error::Observation(ObservationError::Stale))
    );
    Arc::try_unwrap(store).ok().unwrap().close().unwrap();
}

#[test]
fn observed_election_performs_no_storage_read_and_rejects_failed_health_and_old_generation() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), &faults)
        .prepare_publication(HomeDomainRequirements::new())
        .unwrap()
        .publish()
        .unwrap();
    let observer = store.observe_mutations(Waker::noop().clone()).unwrap();
    let observation = observer.observe().unwrap();
    let generation = store.health().generation().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert_eq!(
        store.try_elect_observed_coherent(&observation, generation, || 42),
        Ok(42)
    );
    assert!(store.home_revision().is_err());
    assert_eq!(
        store.try_elect_observed_coherent(&observation, generation, || panic!("failed health")),
        Err(Error::Coherence(HomeCoherenceError::Unhealthy(
            HomeHealthState::Failed
        )))
    );
    let recovered = store.recover_same_home().unwrap().publish().unwrap();
    let fresh_observer = recovered.observe_mutations(Waker::noop().clone()).unwrap();
    let fresh = fresh_observer.observe().unwrap();
    assert_eq!(
        recovered.try_elect_observed_coherent(&fresh, generation, || panic!("old generation")),
        Err(Error::Coherence(HomeCoherenceError::StaleGeneration))
    );
    assert_eq!(
        recovered.try_elect_observed_coherent(
            &fresh,
            recovered.health().generation().unwrap(),
            || 7
        ),
        Ok(7)
    );
    recovered.close().unwrap();
}

#[test]
fn observed_election_preserves_returned_and_installed_reconciliation_custody() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = open(directory.path(), &faults);
    let domain = candidate.register_domain::<ReconciledDomain>().unwrap();
    let store = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<ReconciledDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let generation = store.health().generation().unwrap();
    let observer = store.observe_mutations(Waker::noop().clone()).unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } = store
        .execute_current(domain.current_command(PutBytes::<ReconciledDomain>::new(1, vec![1])))
    else {
        panic!("expected retained custody");
    };
    let observation = observer.observe().unwrap();
    assert_eq!(
        store.try_elect_observed_coherent(&observation, generation, || panic!("returned custody")),
        Err(Error::Coherence(HomeCoherenceError::ReconciliationPending))
    );
    let handle = reconciliation.install_and_handle();
    assert_eq!(
        store.try_elect_observed_coherent(&observation, generation, || panic!("installed custody")),
        Err(Error::Coherence(HomeCoherenceError::ReconciliationPending))
    );
    assert_eq!(store.pending_reconciliations().len(), 1);
    assert!(matches!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(
        store.try_elect_observed_coherent(&observer.observe().unwrap(), generation, || 7),
        Ok(7)
    );
    store.close().unwrap();
}
