use super::*;
use beryl_state::HandoffFailureKind;

fn pending(f: &Fixture) -> ReservedDiscussionNondispatch {
    let reservation = f
        .service()
        .reserve_parent_dispatch(f.job, id(30), parent_input::identity(f).turn_id())
        .unwrap();
    reservation
        .nondispatched(DiscussionParentNondispatch::for_test(
            nondispatch::activate(f),
            HandoffFailureKind::CasRejectedBeforeAcceptance,
        ))
        .unwrap()
}

fn fail_home(f: &Fixture) {
    if f.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
        f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(f.store.home_revision().is_err());
    }
}

#[test]
fn nondispatch_survives_conflicts_cancellation_fencing_and_owner_disposal() {
    let f = parent_execution::start();
    let mut pending = pending(&f);
    for _ in 0..2 {
        let prepared = pending.prepare(CommandCancellation::new()).unwrap();
        support::commit(
            &f.store,
            f.syndic.clone(),
            support::batch([
                syndic_storage::test_faults::FixtureRecord::DiscussionHandoffGate(f.gate),
            ]),
        );
        assert!(matches!(
            prepared.execute(),
            DiscussionSettlementOutcome::NotCommitted {
                evidence: DiscussionSettlementError::Command(
                    beryl_home_store::CommandError::Conflict { .. }
                ),
            }
        ));
    }
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        pending.prepare(cancelled),
        Err(DiscussionSettlementError::Cancelled)
    ));
    let _fence = f.process.test_fence().unwrap();
    assert!(matches!(
        pending.prepare(CommandCancellation::new()),
        Err(DiscussionSettlementError::Process(_))
    ));
    drop(pending);
    assert_eq!(f.operations.pending_nondispatch_count(), 1);
    assert!(f.operations.retained_audit(f.job).is_none());
    fail_home(&f);
    let mut candidate = f.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(access.pending_reconciliations().is_empty());
    assert_eq!(
        f.operations
            .settle_retained_nondispatch_candidate(
                &access,
                &state,
                &syndic,
                CommandCancellation::new(),
            )
            .unwrap(),
        1
    );
    assert_eq!(f.operations.pending_nondispatch_count(), 0);
    assert_eq!(
        state
            .durable_jobs()
            .job_candidate(&access, f.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::RetryableFailed
    );
    assert!(access.pending_reconciliations().is_empty());
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn candidate_rejects_live_owner_and_prepared_audit_then_settles_without_new_slot() {
    let f = parent_execution::start();
    let mut pending = pending(&f);
    let prepared = pending.prepare(CommandCancellation::new()).unwrap();
    let audit = prepared.audit();
    drop(prepared);
    fail_home(&f);
    let mut candidate = f.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(
        f.operations
            .settle_retained_nondispatch_candidate(
                &access,
                &state,
                &syndic,
                CommandCancellation::new()
            )
            .is_err()
    );
    drop(pending);
    assert!(
        f.operations
            .settle_retained_nondispatch_candidate(
                &access,
                &state,
                &syndic,
                CommandCancellation::new()
            )
            .is_err()
    );
    drop(audit);
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        f.operations
            .settle_retained_nondispatch_candidate(&access, &state, &syndic, cancelled),
        Err(HandoffCandidateConvergenceError::Cancelled)
    ));
    assert_eq!(f.operations.pending_nondispatch_count(), 1);
    assert_eq!(
        f.operations
            .settle_retained_nondispatch_candidate(
                &access,
                &state,
                &syndic,
                CommandCancellation::new()
            )
            .unwrap(),
        1
    );
    assert_eq!(
        f.operations
            .settle_retained_nondispatch_candidate(
                &access,
                &state,
                &syndic,
                CommandCancellation::new()
            )
            .unwrap(),
        0
    );
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn retained_nondispatch_reconciles_uncertain_commit_before_candidate_mutation() {
    uncertain_commit(true);
}

#[test]
fn retained_nondispatch_reconciles_exact_old_then_commits_the_original_rejection() {
    uncertain_commit(false);
}

fn uncertain_commit(committed: bool) {
    let f = parent_execution::start();
    let mut pending = pending(&f);
    let prepared = pending.prepare(CommandCancellation::new()).unwrap();
    let journal_fault = if committed {
        f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        None
    } else {
        Some(beryl_home_store::test_faults::fail_next_journal_write())
    };
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Indeterminate { .. }
    ));
    drop(journal_fault);
    drop(pending);
    assert_eq!(f.operations.pending_nondispatch_count(), 1);
    fail_home(&f);
    let mut candidate = f.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    let before = access.home_revision().unwrap();
    assert_eq!(
        f.operations
            .settle_retained_nondispatch_candidate(
                &access,
                &state,
                &syndic,
                CommandCancellation::new()
            )
            .unwrap(),
        1
    );
    if committed {
        assert_eq!(access.home_revision().unwrap(), before);
    } else {
        assert_eq!(access.home_revision().unwrap().get(), before.get() + 1);
    }
    assert_eq!(
        state
            .durable_jobs()
            .job_candidate(&access, f.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::RetryableFailed
    );
    assert_eq!(f.operations.pending_nondispatch_count(), 0);
    assert!(f.operations.retained_audit(f.job).is_none());
    assert!(access.pending_reconciliations().is_empty());
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn foreign_candidate_cannot_consume_retained_rejection() {
    let f = parent_execution::start();
    drop(pending(&f));
    let other = Fixture::new(false);
    fail_home(&other);
    let mut candidate = other.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(matches!(
        f.operations.settle_retained_nondispatch_candidate(
            &access,
            &state,
            &syndic,
            CommandCancellation::new()
        ),
        Err(HandoffCandidateConvergenceError::Settlement(
            DiscussionSettlementError::ForeignHome
        ))
    ));
    assert_eq!(f.operations.pending_nondispatch_count(), 1);
    candidate.publish().unwrap().close().unwrap();
    f.store.close().unwrap();
}
