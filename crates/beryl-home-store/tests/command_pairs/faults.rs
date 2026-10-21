use super::*;
use beryl_home_store::{
    CommandOutcome, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, PointReadLimit,
    ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};

fn fault_fixture(path: &std::path::Path, faults: &FaultController) -> Fixture {
    Fixture::publish(
        HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap(),
    )
}

#[test]
fn failure_before_commit_keeps_both_domains_unchanged_after_recovery() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = fault_fixture(directory.path(), &faults);
    let before = fixture.store.home_revision().unwrap();
    let command = fixture.command(
        [Mutation, Mutation],
        requirements([Mutation, Mutation]),
        false,
    );
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        not_committed(fixture.store.execute(command)),
        CommandError::Commit { .. }
    ));
    let candidate = fixture.store.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let beta = candidate.domain_handle::<BetaDomain>().unwrap();
    let store = candidate.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), before);
    for value in [
        store
            .read_point::<AlphaDomain, support::BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(1_028).unwrap(),
            )
            .unwrap(),
        store
            .read_point::<BetaDomain, support::BytesRecord<BetaDomain>>(
                &beta,
                &1,
                PointReadLimit::new(1_028).unwrap(),
            )
            .unwrap(),
    ] {
        assert!(value.is_none());
    }
    assert!(store.pending_reconciliations().is_empty());
}

#[test]
fn indeterminate_pairs_keep_original_custody_and_exact_receipt_across_recovery() {
    for kinds in [
        [Mutation, Mutation],
        [Mutation, Validation],
        [Validation, Mutation],
    ] {
        let directory = tempdir().unwrap();
        let faults = FaultController::new();
        let fixture = fault_fixture(directory.path(), &faults);
        let before = fixture.store.home_revision().unwrap();
        let original_generation = fixture.store.health().generation().unwrap();
        let command = fixture.command(kinds, requirements(kinds), false);
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let handle = match fixture.store.execute(command) {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                reconciliation.install_and_handle()
            }
            other => panic!("expected original paired uncertainty: {other:?}"),
        };
        assert_eq!(fixture.store.pending_reconciliations().len(), 1);
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(fixture.store.home_revision().is_err());
        let candidate = fixture.store.recover_same_home().unwrap();
        let failure = candidate.publish().unwrap_err();
        assert!(matches!(
            failure.error(),
            beryl_home_store::HomeCandidateError::PendingReconciliation { count: 1 }
        ));
        let mut candidate = failure.into_parts().1;
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
        assert_eq!(receipt.generation(), original_generation);
        let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
        let beta = candidate.domain_handle::<BetaDomain>().unwrap();
        let store = candidate.publish().unwrap();
        assert_eq!(
            store.domain_revision(&alpha).unwrap().get(),
            if kinds[0] == Mutation { 2 } else { 1 }
        );
        assert_eq!(
            store.domain_revision(&beta).unwrap().get(),
            if kinds[1] == Mutation { 2 } else { 1 }
        );
        assert_eq!(
            store
                .read_point::<AlphaDomain, support::BytesRecord<AlphaDomain>>(
                    &alpha,
                    &1,
                    PointReadLimit::new(1_028).unwrap()
                )
                .unwrap()
                .is_some(),
            kinds[0] == Mutation
        );
        assert_eq!(
            store
                .read_point::<BetaDomain, support::BytesRecord<BetaDomain>>(
                    &beta,
                    &1,
                    PointReadLimit::new(1_028).unwrap()
                )
                .unwrap()
                .is_some(),
            kinds[1] == Mutation
        );
        assert!(matches!(
            store.receipt_domain_revision(&receipt, &alpha),
            Err(beryl_home_store::CommitReceiptError::StaleOrForeign { .. })
        ));
        assert!(store.pending_reconciliations().is_empty());
    }
}

#[test]
fn journal_failure_reconciles_the_original_pair_to_exact_old() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = fault_fixture(directory.path(), &faults);
    let before = fixture.store.home_revision().unwrap();
    let command = fixture.command(
        [Mutation, Mutation],
        requirements([Mutation, Mutation]),
        false,
    );
    let journal_fault = beryl_home_store::test_faults::fail_next_journal_write();
    let handle = match fixture.store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected paired journal uncertainty: {other:?}"),
    };
    drop(journal_fault);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .reconcile(&handle)
            .unwrap(),
        ReconciliationResolution::ExactOld
    );
    let store = candidate.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), before);
    assert!(store.pending_reconciliations().is_empty());
}

#[test]
fn failure_after_persist_retains_the_committed_pair_receipt() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = fault_fixture(directory.path(), &faults);
    let before = fixture.store.home_revision().unwrap();
    let original_generation = fixture.store.health().generation().unwrap();
    let command = fixture.command(
        [Mutation, Mutation],
        requirements([Mutation, Mutation]),
        false,
    );
    faults.fail_next(FaultPoint::AfterPersist);
    let receipt = match fixture.store.execute(command) {
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(CommandError::Persistence { .. }),
            local_finalization: None,
        } => receipt,
        other => panic!("expected known paired commit with acknowledgement failure: {other:?}"),
    };
    let candidate = fixture.store.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let beta = candidate.domain_handle::<BetaDomain>().unwrap();
    let store = candidate.publish().unwrap();
    assert_eq!(receipt.home_revision().get(), before.get() + 1);
    assert_eq!(receipt.generation(), original_generation);
    assert_eq!(store.home_revision().unwrap(), receipt.home_revision());
    assert_eq!(store.domain_revision(&alpha).unwrap().get(), 2);
    assert_eq!(store.domain_revision(&beta).unwrap().get(), 2);
    assert!(
        store
            .read_point::<AlphaDomain, support::BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(1_028).unwrap()
            )
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .read_point::<BetaDomain, support::BytesRecord<BetaDomain>>(
                &beta,
                &1,
                PointReadLimit::new(1_028).unwrap()
            )
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        store.receipt_domain_revision(&receipt, &alpha),
        Err(beryl_home_store::CommitReceiptError::StaleOrForeign { .. })
    ));
    assert!(store.pending_reconciliations().is_empty());
}

#[test]
fn candidate_access_refuses_single_bound_participant_and_accepts_its_matching_peer() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = fault_fixture(directory.path(), &faults);
    let missing_peer = fixture.mutation::<AlphaDomain>(pair(BetaDomain::NAME, Mutation));
    let alpha_mutation = fixture.mutation::<AlphaDomain>(pair(BetaDomain::NAME, Mutation));
    let beta_mutation = fixture.mutation::<BetaDomain>(pair(AlphaDomain::NAME, Mutation));
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let beta = candidate.domain_handle::<BetaDomain>().unwrap();
    let access = candidate.recovery_access().unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(alpha.contribution(access.domain_revision(&alpha).unwrap(), missing_peer))
        .unwrap();
    assert!(matches!(
        not_committed(access.execute(command)),
        CommandError::InvalidCommandPair
    ));
    assert_eq!(fixture.calls.reserved.load(Ordering::SeqCst), 0);
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(alpha.contribution(access.domain_revision(&alpha).unwrap(), alpha_mutation))
        .unwrap();
    command
        .add(beta.contribution(access.domain_revision(&beta).unwrap(), beta_mutation))
        .unwrap();
    committed(access.execute(command));
    let store = candidate.publish().unwrap();
    assert!(
        store
            .read_point::<AlphaDomain, support::BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(1_028).unwrap()
            )
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .read_point::<BetaDomain, support::BytesRecord<BetaDomain>>(
                &beta,
                &1,
                PointReadLimit::new(1_028).unwrap()
            )
            .unwrap()
            .is_some()
    );
}
