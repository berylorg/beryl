use super::*;
use beryl_home_store::{HomeHealthState, HomeRecoveryCandidate};

fn required() -> beryl_home_store::HomeDomainRequirements {
    BerylState::required_domains()
        .unwrap()
        .merge(SyndicStorage::required_domains().unwrap())
        .unwrap()
}

#[test]
fn committed_retry_failure_survives_ui_disposal_and_only_genuine_recovery_releases_it() {
    let fixture = Fixture::new();
    let reader = reader(&fixture);
    let job = admitted(
        fixture
            .admit("retained-retry", 210, "Retain exact original retry")
            .unwrap(),
    );
    pause(&fixture, job);
    let failed = reader.observe(id(36)).unwrap();
    let prepared = reader
        .prepare_retry(failed.retry().unwrap(), CommandCancellation::new())
        .unwrap();
    let audit = prepared.audit();
    fixture
        .faults
        .fail_next_with_kind(FaultPoint::AfterPersist, std::io::ErrorKind::StorageFull);
    let outcome = prepared.execute();
    assert!(outcome.has_original_failure());
    drop(outcome);
    assert!(audit.retains_retry_obligations_for_test());
    assert!(fixture.operations.retained_audit(job).is_some());
    assert!(!reader.current());
    let Fixture {
        directory,
        service,
        settlement,
        operations,
        state,
        syndic,
        faults,
        source,
    } = fixture;
    drop((settlement, state, syndic, faults, source));
    service.close().unwrap();
    assert!(audit.retains_retry_obligations_for_test());
    assert!(operations.retained_audit(job).is_some());

    let faults = FaultController::new();
    let mut opened = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut opened).unwrap();
    let syndic = SyndicStorage::register(&mut opened).unwrap();
    let mut opened = opened.prepare_publication(required()).unwrap();
    let access = opened.recovery_access().unwrap();
    assert_eq!(
        audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
        DiscussionSettlementAuditOutcome::Settled(DiscussionSettlementResult::RetryResumed)
    );
    assert!(audit.retains_retry_obligations_for_test());
    assert!(operations.retained_audit(job).is_some());
    drop(access);
    let store = opened.publish().unwrap();
    assert_eq!(
        audit.reconcile(&store, &syndic, &state).unwrap(),
        DiscussionSettlementAuditOutcome::Settled(DiscussionSettlementResult::RetryResumed)
    );
    assert!(audit.retains_retry_obligations_for_test());
    assert!(operations.retained_audit(job).is_some());
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    assert_ne!(store.health().state(), HomeHealthState::Healthy);
    let mut recovered = store.recover_same_home().unwrap();
    let fresh_state = BerylState::reacquire_candidate(&recovered).unwrap();
    let fresh_syndic = SyndicStorage::reacquire_candidate(&recovered).unwrap();

    let foreign_directory = tempfile::tempdir().unwrap();
    let foreign_faults = FaultController::new();
    let foreign = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(foreign_directory.path(), HomeSchemaVersion::CURRENT),
        foreign_faults.clone(),
    )
    .unwrap()
    .prepare_publication(beryl_home_store::HomeDomainRequirements::new())
    .unwrap()
    .publish()
    .unwrap();
    foreign_faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(foreign.home_revision().is_err());
    let mut foreign = foreign.recover_same_home().unwrap();
    assert_eq!(foreign.generation(), recovered.generation());
    assert!(
        operations
            .qualify_recovered_originals(&mut foreign, &fresh_state, &fresh_syndic)
            .is_err()
    );
    assert!(audit.retains_retry_obligations_for_test());
    assert!(operations.retained_audit(job).is_some());
    close_candidate(foreign);

    operations
        .qualify_recovered_originals(&mut recovered, &fresh_state, &fresh_syndic)
        .unwrap();
    assert!(!audit.retains_retry_obligations_for_test());
    assert!(operations.retained_audit(job).is_none());
    let access = recovered.recovery_access().unwrap();
    let resumed = fresh_state
        .durable_jobs()
        .job_candidate(&access, job)
        .unwrap()
        .unwrap();
    assert_eq!(resumed.job_id(), job);
    assert_eq!(resumed.resolution().as_str(), "Retain exact original retry");
    drop(access);
    drop(audit);
    close_candidate(recovered);
}

fn close_candidate(candidate: HomeRecoveryCandidate) {
    candidate.publish().unwrap().close().unwrap();
}
