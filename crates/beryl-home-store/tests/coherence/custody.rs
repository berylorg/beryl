use std::{convert::Infallible, path::Path};

use beryl_home_store::{
    CommandOutcome, DomainReader, DomainReconciliation, DomainSchemaVersion, HomeCoherenceError,
    HomeDomainRequirements, HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    KeyspaceSchemaVersion, ReconciliationReader, ReconciliationResolution, RecordFamily,
    StorageDomain,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

use super::support::{AlphaDomain, BytesRecord, PutBytes, ValidatedDomainError};

pub(super) struct ReconciledDomain;

impl StorageDomain for ReconciledDomain {
    const NAME: &'static str = "coherence";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<BytesRecord<Self>>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = ValidatedDomainError;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment(
        _: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }

    fn reconcile(
        reader: &ReconciliationReader<'_, Self>,
    ) -> Result<DomainReconciliation, Self::ValidationError> {
        let mut side = None;
        for record in reader
            .records::<BytesRecord<Self>>()
            .map_err(ValidatedDomainError::Read)?
        {
            let current = if record.current() == record.new() {
                DomainReconciliation::ExactNew
            } else if record.current() == record.old() {
                DomainReconciliation::ExactOld
            } else {
                DomainReconciliation::Collision
            };
            if side.is_some_and(|side| side != current) {
                return Ok(DomainReconciliation::Collision);
            }
            side = Some(current);
        }
        Ok(side.unwrap_or(DomainReconciliation::Collision))
    }
}

pub(super) fn open(path: &Path, faults: &FaultController) -> HomeOpenCandidate {
    HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap()
}

#[test]
fn returned_custody_blocks_before_installation_and_exact_resolution_restores_election() {
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
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } = store.execute_current(
        domain.current_command(PutBytes::<ReconciledDomain>::new(1, b"retained".to_vec())),
    ) else {
        panic!("expected indeterminate custody");
    };
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert!(store.pending_reconciliations().is_empty());
    assert_eq!(
        store.try_elect_coherent(generation, || panic!("returned custody was missed")),
        Err(HomeCoherenceError::ReconciliationPending)
    );
    let handle = reconciliation.install_and_handle();
    assert_eq!(store.pending_reconciliations().len(), 1);
    assert_eq!(
        store.try_elect_coherent(generation, || panic!("installed custody was missed")),
        Err(HomeCoherenceError::ReconciliationPending)
    );
    assert!(matches!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(store.try_elect_coherent(generation, || 7), Ok(7));
    store.close().unwrap();
}

#[test]
fn drop_installed_and_collision_closed_custody_both_block_election() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = open(directory.path(), &faults);
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
    let generation = store.health().generation().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } =
        store.execute_current(domain.current_command(PutBytes::<AlphaDomain>::new(1, vec![1])))
    else {
        panic!("expected indeterminate custody");
    };
    drop(reconciliation);
    let handle = store.pending_reconciliations().pop().unwrap();
    assert_eq!(
        store.try_elect_coherent(generation, || panic!("drop-installed custody was missed")),
        Err(HomeCoherenceError::ReconciliationPending)
    );
    assert!(matches!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision { .. }
    ));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert_eq!(
        store.try_elect_coherent(generation, || panic!("closed custody was missed")),
        Err(HomeCoherenceError::ReconciliationPending)
    );
    store.close().unwrap();
}

#[test]
fn failed_health_and_stale_generation_never_invoke_election() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), &faults)
        .prepare_publication(HomeDomainRequirements::new())
        .unwrap()
        .publish()
        .unwrap();
    let original = store.health().generation().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    assert_eq!(
        store.try_elect_coherent(original, || panic!("failed health was missed")),
        Err(HomeCoherenceError::Unhealthy(HomeHealthState::Failed))
    );
    let recovered = store.recover_same_home().unwrap().publish().unwrap();
    assert_eq!(
        recovered.try_elect_coherent(original, || panic!("old generation was admitted")),
        Err(HomeCoherenceError::StaleGeneration)
    );
    assert_eq!(
        recovered.try_elect_coherent(recovered.health().generation().unwrap(), || 3),
        Ok(3)
    );
    recovered.close().unwrap();
}
