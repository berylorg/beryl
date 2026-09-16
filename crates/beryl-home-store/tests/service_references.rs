#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCandidateError, HomeDomainRequirements,
    HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, PointReadLimit,
    test_faults::{FaultController, FaultPoint},
};
use support::{AlphaDomain, BytesRecord, PutBytes, committed};
use tempfile::tempdir;

fn requirements() -> HomeDomainRequirements {
    HomeDomainRequirements::new()
        .with_domain::<AlphaDomain>()
        .unwrap()
}

#[test]
fn opening_service_reference_stays_gated_for_reads_and_commands() {
    let directory = tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
    let publication = candidate.prepare_publication(requirements()).unwrap();
    let reference = publication.service_reference();

    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(reference.home_revision().is_err());
    assert!(
        reference
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(64).unwrap(),
            )
            .is_err()
    );
    assert!(matches!(
        reference
            .execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(1, b"closed"))),
        CommandOutcome::NotCommitted { .. }
    ));
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        reference.execute_current(
            alpha
                .current_command(PutBytes::<AlphaDomain>::new(2, b"cancelled"))
                .with_cancellation(cancellation),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    publication.close().unwrap();
}

#[test]
fn published_service_references_share_generation_without_owning_the_home() {
    let directory = tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
    let publication = candidate.prepare_publication(requirements()).unwrap();
    let opening_reference = publication.service_reference();
    let store = publication.publish().unwrap();
    let reference = store.service_reference();
    let cloned = reference.clone();

    assert_eq!(
        opening_reference.health().generation(),
        reference.health().generation()
    );
    committed(
        reference
            .execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(1, b"service"))),
    );
    assert_eq!(
        cloned
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(64).unwrap(),
            )
            .unwrap(),
        Some(b"service".to_vec())
    );

    store.close().unwrap();
    assert!(reference.home_revision().is_err());
    assert!(cloned.home_revision().is_err());
    let reopened = support::open_home(directory.path());
    reopened.close().unwrap();
}

#[test]
fn recovered_reference_is_gated_until_publication_and_old_reference_cannot_cross_generation() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut opening = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let old_domain = opening.register_domain::<AlphaDomain>().unwrap();
    let store = opening
        .prepare_publication(requirements())
        .unwrap()
        .publish()
        .unwrap();
    let old_reference = store.service_reference();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());

    let mut recovered = store.recover_same_home().unwrap();
    let fresh_domain = recovered.domain_handle::<AlphaDomain>().unwrap();
    let fresh_reference = recovered.service_reference();
    assert!(fresh_reference.home_revision().is_err());
    assert!(old_reference.home_revision().is_err());
    assert!(fresh_reference.domain_revision(&old_domain).is_err());

    let fresh_store = recovered.publish().unwrap();
    assert!(fresh_reference.home_revision().is_ok());
    assert!(old_reference.home_revision().is_err());
    committed(
        fresh_reference.execute_current(
            fresh_domain.current_command(PutBytes::<AlphaDomain>::new(2, b"fresh")),
        ),
    );
    assert_eq!(
        fresh_store
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &fresh_domain,
                &2,
                PointReadLimit::new(64).unwrap(),
            )
            .unwrap(),
        Some(b"fresh".to_vec())
    );
    fresh_store.close().unwrap();
}

#[test]
fn failed_and_unsettled_candidate_references_remain_gated_and_keep_the_lock() {
    let failed_directory = tempdir().unwrap();
    let failed_faults = FaultController::new();
    let mut failed_candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(failed_directory.path(), HomeSchemaVersion::CURRENT),
        failed_faults.clone(),
    )
    .unwrap();
    let failed_alpha = failed_candidate.register_domain::<AlphaDomain>().unwrap();
    let mut failed_publication = failed_candidate
        .prepare_publication(requirements())
        .unwrap();
    let failed_reference = failed_publication.service_reference();
    failed_faults.fail_next(FaultPoint::BeforeCommit);
    let failed_access = failed_publication.recovery_access().unwrap();
    assert!(matches!(
        failed_access.execute_current(
            failed_alpha.current_command(PutBytes::<AlphaDomain>::new(1, b"failed")),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    drop(failed_access);
    assert_eq!(failed_reference.health().state(), HomeHealthState::Failed);
    assert!(failed_reference.home_revision().is_err());
    failed_publication
        .publish()
        .unwrap_err()
        .into_parts()
        .1
        .close()
        .unwrap();

    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
    let mut publication = candidate.prepare_publication(requirements()).unwrap();
    let reference = publication.service_reference();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let access = publication.recovery_access().unwrap();
    let custody = match access
        .execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(1, b"uncertain")))
    {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation,
        outcome => panic!("expected indeterminate candidate command, got {outcome:?}"),
    };
    drop(access);
    let failure = publication.publish().unwrap_err();
    assert!(matches!(
        failure.error(),
        HomeCandidateError::PendingReconciliation { count: 1 }
    ));
    publication = failure.into_parts().1;
    drop(reference);
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .is_err()
    );
    let replacement = publication.service_reference();
    assert!(replacement.home_revision().is_err());
    let handle = custody.install_and_handle();
    let access = publication.recovery_access().unwrap();
    assert!(access.reconcile(&handle).is_ok());
    assert_eq!(access.pending_reconciliations().len(), 1);
    drop(access);
    publication.close().unwrap();
    support::open_home(directory.path()).close().unwrap();
}
