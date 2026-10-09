#![cfg(feature = "test-faults")]

#[path = "current_home_commands/domains.rs"]
mod domains;
mod support;

use beryl_home_store::{
    CommandError, CommandOutcome, DomainHandle, HomeCommand, HomeDomainRequirements,
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore, PointReadLimit,
    ReconciliationResolution,
    test_faults::{FaultController, FaultPoint, FaultScope},
};
use domains::{AlphaDomain, BetaDomain};
use support::{BytesRecord, PutBytes, committed, not_committed};
use tempfile::tempdir;

struct Fixture {
    store: HomeStore,
    alpha: DomainHandle<AlphaDomain>,
    beta: DomainHandle<BetaDomain>,
}

impl Fixture {
    fn new(path: &std::path::Path, faults: &FaultController) -> Self {
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
        let beta = candidate.register_domain::<BetaDomain>().unwrap();
        let store = candidate
            .prepare_publication(
                HomeDomainRequirements::new()
                    .with_domain::<AlphaDomain>()
                    .unwrap()
                    .with_domain::<BetaDomain>()
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        Self { store, alpha, beta }
    }

    fn command(&self, key: u64, scope: Option<FaultScope>) -> HomeCommand {
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        if let Some(scope) = scope {
            command = command.with_test_fault_scope(scope);
        }
        command
            .add(self.alpha.contribution(
                self.store.domain_revision(&self.alpha).unwrap(),
                PutBytes::<AlphaDomain>::new(key, b"original".to_vec()),
            ))
            .unwrap();
        command
    }
}

fn alpha_value(store: &HomeStore, alpha: &DomainHandle<AlphaDomain>, key: u64) -> Option<Vec<u8>> {
    store
        .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            alpha,
            &key,
            PointReadLimit::new(1_028).unwrap(),
        )
        .unwrap()
}

#[test]
fn scoped_ordinary_fault_survives_foreign_and_unscoped_commands_until_exact_command() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = Fixture::new(directory.path(), &faults);
    let original_scope = FaultScope::of::<PutBytes<AlphaDomain>>();
    faults.fail_next_in_scope(FaultPoint::BeforeCommit, original_scope);
    let mut foreign = HomeCommand::new(fixture.store.home_revision().unwrap())
        .with_test_fault_scope(FaultScope::of::<PutBytes<BetaDomain>>());
    foreign
        .add(fixture.beta.contribution(
            fixture.store.domain_revision(&fixture.beta).unwrap(),
            PutBytes::<BetaDomain>::new(1, b"foreign".to_vec()),
        ))
        .unwrap();
    committed(fixture.store.execute(foreign));
    committed(fixture.store.execute(fixture.command(2, None)));
    let before = fixture.store.home_revision().unwrap();
    assert!(matches!(
        not_committed(
            fixture
                .store
                .execute(fixture.command(3, Some(original_scope)))
        ),
        CommandError::Commit { .. }
    ));
    let candidate = fixture.store.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let beta = candidate.domain_handle::<BetaDomain>().unwrap();
    let store = candidate.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(alpha_value(&store, &alpha, 2), Some(b"original".to_vec()));
    assert_eq!(alpha_value(&store, &alpha, 3), None);
    assert_eq!(
        store
            .read_point::<BetaDomain, BytesRecord<BetaDomain>>(
                &beta,
                &1,
                PointReadLimit::new(1_028).unwrap(),
            )
            .unwrap(),
        Some(b"foreign".to_vec())
    );
}

#[test]
fn scoped_ordinary_indeterminate_commit_preserves_original_handle_for_fresh_candidate() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = Fixture::new(directory.path(), &faults);
    let before = fixture.store.home_revision().unwrap();
    let scope = FaultScope::of::<PutBytes<AlphaDomain>>();
    faults.fail_next_in_scope(FaultPoint::AfterCommitBeforePersist, scope);
    let handle = match fixture.store.execute(fixture.command(1, Some(scope))) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected scoped original indeterminate outcome: {other:?}"),
    };
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let receipt = match candidate
        .recovery_access()
        .unwrap()
        .reconcile(&handle)
        .unwrap()
    {
        ReconciliationResolution::ExactNew { receipt } => receipt,
        other => panic!("expected original exact new receipt: {other:?}"),
    };
    assert_eq!(receipt.home_revision().get(), before.get() + 1);
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let store = candidate.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), receipt.home_revision());
    assert_eq!(alpha_value(&store, &alpha, 1), Some(b"original".to_vec()));
    assert!(store.pending_reconciliations().is_empty());
}

#[test]
fn scoped_ordinary_after_persist_preserves_known_committed_receipt_and_data() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = Fixture::new(directory.path(), &faults);
    let before = fixture.store.home_revision().unwrap();
    let scope = FaultScope::of::<PutBytes<AlphaDomain>>();
    faults.fail_next_in_scope(FaultPoint::AfterPersist, scope);
    let (receipt, original_finalization) =
        match fixture.store.execute(fixture.command(1, Some(scope))) {
            CommandOutcome::Committed {
                receipt,
                later_failure: Some(CommandError::Persistence { .. }),
                local_finalization: Some(finalization),
            } => (receipt, finalization),
            other => panic!("expected scoped known commit with later failure: {other:?}"),
        };
    assert_eq!(receipt.home_revision().get(), before.get() + 1);
    let candidate = fixture.store.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let store = candidate.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), receipt.home_revision());
    assert_eq!(alpha_value(&store, &alpha, 1), Some(b"original".to_vec()));
    assert!(store.pending_reconciliations().is_empty());
    drop(original_finalization);
}

#[test]
fn explicit_ordinary_scope_preserves_existing_unscoped_fault_fallback() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = Fixture::new(directory.path(), &faults);
    let before = fixture.store.home_revision().unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        not_committed(
            fixture
                .store
                .execute(fixture.command(1, Some(FaultScope::of::<PutBytes<AlphaDomain>>()),))
        ),
        CommandError::Commit { .. }
    ));
    let candidate = fixture.store.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let store = candidate.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(alpha_value(&store, &alpha, 1), None);
}
