mod support;

use beryl_home_store::{
    CommandCancellation, CommandError, CursorDirection, CursorRange, CursorReadLimits, HomeCommand,
    HomeDomainRequirements, HomeHealthState, PointReadLimit, ReadError,
};
use support::{AlphaDomain, BytesRecord, PutBytes, committed, not_committed};
use tempfile::tempdir;

#[test]
fn candidate_writes_are_bounded_revision_checked_and_durable_before_publication() {
    let directory = tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
    let mut publication = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap();
    let home_id = publication.home_id();
    let generation = publication.generation();
    let access = publication.recovery_access().unwrap();
    assert_eq!(access.home_id(), home_id);
    assert_eq!(access.generation(), generation);
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(alpha.contribution(
            access.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"one"),
        ))
        .unwrap();
    let receipt = committed(access.execute(command));
    assert_eq!(
        access.receipt_domain_revision(&receipt, &alpha).unwrap(),
        Some(access.domain_revision(&alpha).unwrap())
    );
    for key in 2..=3 {
        committed(
            access.execute_current(
                alpha.current_command(PutBytes::<AlphaDomain>::new(key, b"value")),
            ),
        );
    }
    assert_eq!(
        access
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(64).unwrap()
            )
            .unwrap(),
        Some(b"one".to_vec())
    );
    assert!(
        access
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(1).unwrap()
            )
            .is_err()
    );
    let page = access
        .read_cursor::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &CursorRange::closed(1, 3),
            CursorDirection::Forward,
            CursorReadLimits::new(2, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 2);
    assert!(page.has_more());
    assert!(page.stored_bytes() <= 128 && page.decoded_bytes() <= 128);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        not_committed(
            access.execute_current(
                alpha
                    .current_command(PutBytes::<AlphaDomain>::new(4, b"cancelled"))
                    .with_cancellation(cancellation)
            )
        ),
        CommandError::CancelledBeforeAdmission
    ));
    let mut stale = HomeCommand::new(receipt.home_revision());
    stale
        .add(alpha.contribution(
            access.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(5, b"stale"),
        ))
        .unwrap();
    assert!(matches!(
        access.execute(stale),
        beryl_home_store::CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(publication.health().state(), HomeHealthState::Opening);
    #[cfg(feature = "test-faults")]
    beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
        assert!(matches!(
            store.home_revision(),
            Err(ReadError::HealthGate(_))
        ));
        assert!(store.receipt_domain_revision(&receipt, &alpha).is_err());
    });
    publication.close().unwrap();
    let mut candidate = support::open_home(directory.path());
    assert_eq!(candidate.home_id(), home_id);
    let fresh = candidate.register_domain::<AlphaDomain>().unwrap();
    let mut publication = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap();
    let access = publication.recovery_access().unwrap();
    assert!(access.domain_revision(&alpha).is_err());
    assert!(access.receipt_domain_revision(&receipt, &fresh).is_err());
    assert_eq!(
        access
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &fresh,
                &3,
                PointReadLimit::new(64).unwrap()
            )
            .unwrap(),
        Some(b"value".to_vec())
    );
    assert!(
        access
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &fresh,
                &4,
                PointReadLimit::new(64).unwrap()
            )
            .unwrap()
            .is_none()
    );
    let store = publication.publish().unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    store.close().unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn recovered_candidate_uses_fresh_identity_and_can_commit_before_publication() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = beryl_home_store::HomeOpenCandidate::open_with_faults(
        beryl_home_store::HomeOpenOptions::new(
            directory.path(),
            beryl_home_store::HomeSchemaVersion::CURRENT,
        ),
        faults.clone(),
    )
    .unwrap();
    let old = candidate.register_domain::<AlphaDomain>().unwrap();
    let store = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let receipt = committed(
        store.execute_current(old.current_command(PutBytes::<AlphaDomain>::new(1, b"old"))),
    );
    let old_generation = store.health().generation().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut candidate = store.recover_same_home().unwrap();
    let fresh = candidate.domain_handle::<AlphaDomain>().unwrap();
    let access = candidate.recovery_access().unwrap();
    assert_ne!(access.generation(), old_generation);
    assert!(access.domain_revision(&old).is_err());
    assert!(access.receipt_domain_revision(&receipt, &fresh).is_err());
    let new = committed(
        access.execute_current(fresh.current_command(PutBytes::<AlphaDomain>::new(2, b"new"))),
    );
    assert!(
        access
            .receipt_domain_revision(&new, &fresh)
            .unwrap()
            .is_some()
    );
    let store = candidate.publish().unwrap();
    assert_eq!(
        store
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &fresh,
                &2,
                PointReadLimit::new(64).unwrap()
            )
            .unwrap(),
        Some(b"new".to_vec())
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut candidate = store.recover_same_home().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .is_err()
    );
    assert!(candidate.recovery_access().is_err());
    let candidate = candidate.publish().unwrap_err().into_parts().1;
    assert!(matches!(
        beryl_home_store::HomeOpenCandidate::open(beryl_home_store::HomeOpenOptions::new(
            directory.path(),
            beryl_home_store::HomeSchemaVersion::CURRENT
        )),
        Err(beryl_home_store::HomeOpenError::Busy { .. })
    ));
    candidate.abort().close().unwrap();
    support::open_home(directory.path()).close().unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn unsettled_initial_command_retains_candidate_until_targeted_settlement() {
    use beryl_home_store::{
        CommandOutcome, HomeCandidateError, ReconciliationResolution,
        test_faults::{FaultController, FaultPoint},
    };
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = beryl_home_store::HomeOpenCandidate::open_with_faults(
        beryl_home_store::HomeOpenOptions::new(
            directory.path(),
            beryl_home_store::HomeSchemaVersion::CURRENT,
        ),
        faults.clone(),
    )
    .unwrap();
    let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
    let mut publication = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let custody = match publication
        .recovery_access()
        .unwrap()
        .execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(1, b"uncertain")))
    {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation,
        other => panic!("expected indeterminate candidate command, got {other:?}"),
    };
    let failure = publication.publish().unwrap_err();
    assert!(matches!(
        failure.error(),
        HomeCandidateError::PendingReconciliation { count: 1 }
    ));
    let mut publication = failure.into_parts().1;
    assert_eq!(publication.health().state(), HomeHealthState::Opening);
    assert!(
        beryl_home_store::HomeOpenCandidate::open(beryl_home_store::HomeOpenOptions::new(
            directory.path(),
            beryl_home_store::HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );
    let handle = custody.install_and_handle();
    let access = publication.recovery_access().unwrap();
    assert_eq!(
        access.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(access.pending_reconciliations().len(), 1);
    beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
        assert!(store.reconcile(&handle).is_err());
        assert!(store.retry_reconciliation(&handle).is_err());
    });
    publication.publish().unwrap().close().unwrap();
    support::open_home(directory.path()).close().unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn failed_candidate_publication_retains_cleanup_and_committed_outcome_truth() {
    use beryl_home_store::{
        CommandOutcome,
        test_faults::{FaultController, FaultPoint},
    };
    for point in [FaultPoint::BeforeCommit, FaultPoint::AfterPersist] {
        let directory = tempdir().unwrap();
        let faults = FaultController::new();
        let mut candidate = beryl_home_store::HomeOpenCandidate::open_with_faults(
            beryl_home_store::HomeOpenOptions::new(
                directory.path(),
                beryl_home_store::HomeSchemaVersion::CURRENT,
            ),
            faults.clone(),
        )
        .unwrap();
        let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
        let mut publication = candidate
            .prepare_publication(
                HomeDomainRequirements::new()
                    .with_domain::<AlphaDomain>()
                    .unwrap(),
            )
            .unwrap();
        faults.fail_next(point);
        let outcome = publication
            .recovery_access()
            .unwrap()
            .execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(1, b"durable")));
        let was_committed = match (point, outcome) {
            (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { .. }) => false,
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    later_failure: Some(_),
                    local_finalization: Some(_),
                    ..
                },
            ) => true,
            (_, other) => panic!("unexpected candidate outcome: {other:?}"),
        };
        assert_eq!(publication.health().state(), HomeHealthState::Failed);
        assert!(publication.recovery_access().is_err());
        publication
            .publish()
            .unwrap_err()
            .into_parts()
            .1
            .close()
            .unwrap();
        let mut candidate = support::open_home(directory.path());
        let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
        let mut publication = candidate
            .prepare_publication(
                HomeDomainRequirements::new()
                    .with_domain::<AlphaDomain>()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            publication
                .recovery_access()
                .unwrap()
                .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                    &alpha,
                    &1,
                    PointReadLimit::new(64).unwrap()
                )
                .unwrap()
                .is_some(),
            was_committed
        );
        publication.close().unwrap();
    }
}
