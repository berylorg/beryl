use super::*;
use beryl_state::HandoffFailureKind;

fn reserve(f: &Fixture) -> DiscussionParentDispatchReservation {
    f.service()
        .reserve_parent_dispatch(f.job, id(30), parent_input::identity(f).turn_id())
        .unwrap()
}

fn pending(
    f: &Fixture,
    reservation: DiscussionParentDispatchReservation,
) -> ReservedDiscussionNondispatch {
    reservation
        .nondispatched(DiscussionParentNondispatch::for_test(
            nondispatch::activate(f),
            HandoffFailureKind::CasRejectedBeforeAcceptance,
        ))
        .unwrap()
}

#[test]
fn dispatch_reservation_reuses_one_slot_across_a_healthy_writer_conflict() {
    let f = parent_execution::start();
    let turn = parent_input::identity(&f).turn_id();
    assert!(matches!(
        f.service().reserve_parent_dispatch(f.job, id(36), turn),
        Err(DiscussionSettlementError::IdentityMismatch)
    ));
    let reservation = reserve(&f);
    assert!(matches!(
        f.service().reserve_parent_dispatch(f.job, id(30), turn),
        Err(DiscussionSettlementError::DuplicateIdentity)
    ));
    assert!(matches!(
        f.service()
            .reserve_parent_dispatch(JobId::from_bytes([245; 16]), id(30), turn),
        Err(DiscussionSettlementError::Capacity)
    ));
    let mut pending = pending(&f, reservation);
    let prepared = pending.prepare(CommandCancellation::new()).unwrap();
    let audit = prepared.audit();
    support::commit(
        &f.store,
        f.syndic.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::DiscussionHandoffGate(f.gate)]),
    );
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        pending.prepare(CommandCancellation::new()),
        Err(DiscussionSettlementError::DuplicateIdentity)
    ));
    assert_eq!(
        f.audit(&audit),
        DiscussionSettlementAuditOutcome::NotCommitted
    );
    drop(audit);
    assert!(matches!(
        pending
            .prepare(CommandCancellation::new())
            .unwrap()
            .execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::ParentRetryable { .. },
            later_failure: None,
            ..
        }
    ));
    drop(pending);
    assert!(matches!(
        f.service().reserve_parent_dispatch(f.job, id(30), turn),
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
    f.store.close().unwrap();
}

#[test]
fn reserved_uncertainty_outlives_the_dispatch_owner_and_reconciles_in_candidate() {
    let f = parent_execution::start();
    let mut pending = pending(&f, reserve(&f));
    let prepared = pending.prepare(CommandCancellation::new()).unwrap();
    f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Indeterminate { .. }
    ));
    assert!(matches!(
        pending.prepare(CommandCancellation::new()),
        Err(DiscussionSettlementError::DuplicateIdentity)
    ));
    drop(pending);
    let audit = f.operations.retained_audit(f.job).unwrap();
    if f.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
        f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(f.store.home_revision().is_err());
    }
    let mut candidate = f.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(matches!(
        audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
        DiscussionSettlementAuditOutcome::Settled(
            DiscussionSettlementResult::ParentRetryable { .. }
        )
    ));
    assert!(f.operations.retained_audit(f.job).is_none());
    drop(audit);
    assert!(access.pending_reconciliations().is_empty());
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn process_reopening_does_not_revive_an_old_dispatch_reservation() {
    let f = parent_execution::start();
    let mut pending = pending(&f, reserve(&f));
    let prepared = pending.prepare(CommandCancellation::new()).unwrap();
    let fence = f.process.test_fence().unwrap();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    fence.try_reopen(true).unwrap();
    assert!(matches!(
        pending.prepare(CommandCancellation::new()),
        Err(DiscussionSettlementError::Process(_))
    ));
    drop(pending);
    assert_eq!(
        f.state
            .durable_jobs()
            .job(&f.store, f.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::StartingParent
    );
    f.store.close().unwrap();
}

#[test]
fn a_changed_job_revision_cannot_reuse_an_earlier_dispatch_reservation() {
    let f = parent_execution::start();
    let reservation = reserve(&f);
    for transition in [
        beryl_state::HandoffJobTransition::RetryableFailure(
            beryl_state::HandoffFailureEvidence::new(HandoffFailureKind::RuntimeUnavailable, None)
                .unwrap(),
        ),
        beryl_state::HandoffJobTransition::Retry,
    ] {
        let current = f
            .state
            .durable_jobs()
            .job(&f.store, f.job)
            .unwrap()
            .unwrap();
        let prepared = f
            .state
            .durable_jobs()
            .prepare_handoff_job_transition(&f.store, f.job, current.revision(), transition)
            .unwrap();
        let mut command = HomeCommand::new(f.store.home_revision().unwrap());
        command.add(prepared.contribution()).unwrap();
        assert!(matches!(
            f.store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }
    let mut pending = pending(&f, reservation);
    assert!(matches!(
        pending.prepare(CommandCancellation::new()),
        Err(DiscussionSettlementError::IdentityMismatch)
    ));
    drop(pending);
    assert!(matches!(
        f.syndic
            .current_binding(&f.store, id(30), limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Active(_)
    ));
    assert_eq!(f.operations.pending_nondispatch_count(), 1);
    f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(f.store.home_revision().is_err());
    let mut candidate = f.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(matches!(
        f.operations.settle_retained_nondispatch_candidate(
            &access,
            &state,
            &syndic,
            CommandCancellation::new(),
        ),
        Err(HandoffCandidateConvergenceError::Settlement(
            DiscussionSettlementError::IdentityMismatch
        ))
    ));
    assert_eq!(f.operations.pending_nondispatch_count(), 1);
    candidate.publish().unwrap().close().unwrap();
}
