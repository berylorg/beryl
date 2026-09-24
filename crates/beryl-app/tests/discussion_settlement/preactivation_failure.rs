use super::*;
use beryl_state::HandoffFailureKind;

fn reserve(f: &Fixture) -> DiscussionParentDispatchReservation {
    f.service()
        .reserve_parent_dispatch(f.job, id(30), parent_input::identity(f).turn_id())
        .unwrap()
}

fn failed(f: &Fixture) -> ReservedDiscussionNondispatch {
    reserve(f)
        .preparation_failed_for_test(HandoffFailureKind::RuntimeUnavailable)
        .unwrap()
}

fn evidence(f: &Fixture) -> PendingDispatchEvidence {
    f.syndic
        .pending_dispatch_evidence(&f.store, id(30), limit())
        .unwrap()
        .unwrap()
}

#[test]
fn confirmed_preparation_failure_changes_only_state_and_requires_exact_retry() {
    for kind in [
        HandoffFailureKind::RuntimeUnavailable,
        HandoffFailureKind::RootUnavailable,
        HandoffFailureKind::CasUnavailable,
    ] {
        let f = parent_execution::start();
        let before = evidence(&f);
        let syndic_revision = f.syndic.revision(&f.store).unwrap();
        let mut pending = reserve(&f).preparation_failed_for_test(kind).unwrap();
        assert_eq!(f.operations.pending_nondispatch_count(), 1);
        assert!(
            matches!(pending.prepare(CommandCancellation::new()).unwrap().execute(),
            DiscussionSettlementOutcome::Committed {
                result: DiscussionSettlementResult::ParentRetryable { kind: actual, .. },
                later_failure: None, local_finalization: None, ..
            } if actual == kind)
        );
        drop(pending);
        assert_eq!(f.operations.pending_nondispatch_count(), 0);
        assert_eq!(f.syndic.revision(&f.store).unwrap(), syndic_revision);
        assert_eq!(evidence(&f), before);
        assert!(matches!(
            f.service()
                .reserve_parent_dispatch(f.job, id(30), before.turn_id()),
            Err(DiscussionSettlementError::IdentityMismatch)
        ));
        let job = f
            .state
            .durable_jobs()
            .job(&f.store, f.job)
            .unwrap()
            .unwrap();
        assert!(matches!(
            f.service()
                .prepare_retry(f.job, job.revision(), CommandCancellation::new())
                .unwrap()
                .execute(),
            DiscussionSettlementOutcome::Committed { .. }
        ));
        drop(reserve(&f));
        assert_eq!(evidence(&f), before);
        f.store.close().unwrap();
    }
}

#[test]
fn captured_failure_rejects_later_activation_and_binding_drift() {
    for activation in [true, false] {
        let f = parent_execution::start();
        let mut pending = failed(&f);
        if activation {
            nondispatch::activate(&f);
        } else {
            let execution = f
                .syndic
                .thread_execution(&f.store, id(30), limit())
                .unwrap()
                .unwrap();
            let binding = beryl_model::ExecutionBinding::new(
                execution.execution().runtime_id(),
                beryl_model::RootId::from_bytes([244; 16]),
                execution.execution().root_path().clone(),
            );
            support::commit(
                &f.store,
                f.syndic.clone(),
                support::batch(
                    [syndic_storage::test_faults::FixtureRecord::ThreadExecution(
                        ThreadExecutionRecord::new(id(30), binding),
                    )],
                ),
            );
        }
        assert!(pending.prepare(CommandCancellation::new()).is_err());
        assert_eq!(
            f.state
                .durable_jobs()
                .job(&f.store, f.job)
                .unwrap()
                .unwrap()
                .lifecycle(),
            BranchHandoffJobLifecycle::StartingParent
        );
        drop(pending);
        assert_eq!(f.operations.pending_nondispatch_count(), 1);
        f.store.close().unwrap();
    }
}

#[test]
fn captured_failure_survives_conflict_cancel_and_disposal_into_candidate() {
    let f = parent_execution::start();
    let before = evidence(&f);
    let mut pending = failed(&f);
    let prepared = pending.prepare(CommandCancellation::new()).unwrap();
    support::commit(
        &f.store,
        f.syndic.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::DiscussionHandoffGate(f.gate)]),
    );
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        pending.prepare(cancelled),
        Err(DiscussionSettlementError::Cancelled)
    ));
    drop(pending);
    recover(f, before, None);
}

#[test]
fn captured_failure_reconciles_both_uncertain_writer_outcomes() {
    for committed in [false, true] {
        let f = parent_execution::start();
        let before = evidence(&f);
        let mut pending = failed(&f);
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
        recover(f, before, Some(committed));
    }
}

fn recover(f: Fixture, before: PendingDispatchEvidence, committed: Option<bool>) {
    assert_eq!(f.operations.pending_nondispatch_count(), 1);
    if f.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
        f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(f.store.home_revision().is_err());
    }
    let mut candidate = f.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    let syndic_revision = syndic.revision_candidate(&access).unwrap();
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
        access.home_revision().unwrap().get(),
        revision.get() + u64::from(committed != Some(true))
    );
    assert_eq!(syndic.revision_candidate(&access).unwrap(), syndic_revision);
    assert_eq!(
        state
            .durable_jobs()
            .job_candidate(&access, f.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::RetryableFailed
    );
    let after = syndic
        .pending_dispatch_evidence_candidate(&access, id(30), limit())
        .unwrap()
        .unwrap();
    assert_eq!(after.turn_id(), before.turn_id());
    assert_eq!(after.state_revision(), before.state_revision());
    assert_eq!(after.binding_revision(), before.binding_revision());
    assert_eq!(after.dispatch_provenance(), before.dispatch_provenance());
    assert_eq!(after.input(), before.input());
    assert!(access.pending_reconciliations().is_empty());
    assert_eq!(f.operations.pending_nondispatch_count(), 0);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn preparation_failure_after_explicit_retry_preserves_cancelled_dispatch_provenance() {
    let f = parent_execution::start();
    let reservation = reserve(&f);
    let mut rejected = reservation
        .nondispatched(DiscussionParentNondispatch::for_test(
            nondispatch::activate(&f),
            HandoffFailureKind::CasRejectedBeforeAcceptance,
        ))
        .unwrap();
    assert!(matches!(
        rejected
            .prepare(CommandCancellation::new())
            .unwrap()
            .execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    drop(rejected);
    let job = f
        .state
        .durable_jobs()
        .job(&f.store, f.job)
        .unwrap()
        .unwrap();
    assert!(matches!(
        f.service()
            .prepare_retry(f.job, job.revision(), CommandCancellation::new())
            .unwrap()
            .execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    let before = evidence(&f);
    assert!(matches!(
        before.dispatch_provenance(),
        TurnDispatchProvenance::Cancelled(_)
    ));
    let mut pending = failed(&f);
    assert!(matches!(
        pending
            .prepare(CommandCancellation::new())
            .unwrap()
            .execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    drop(pending);
    assert_eq!(evidence(&f), before);
    f.store.close().unwrap();
}
