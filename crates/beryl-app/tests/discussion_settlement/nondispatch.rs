use super::*;
use beryl_state::{BranchHandoffJobRecord, HandoffFailureKind};

fn job(f: &Fixture) -> BranchHandoffJobRecord {
    f.state
        .durable_jobs()
        .job(&f.store, f.job)
        .unwrap()
        .unwrap()
}

fn activated() -> (Fixture, CancelBindingActivation) {
    let f = parent_execution::start();
    let request = activate(&f);
    (f, request)
}

pub(super) fn activate(f: &Fixture) -> CancelBindingActivation {
    let turn = parent_input::identity(&f).turn_id();
    let (_, snapshot) = support::exact_cas::activate_turn(
        &f.store,
        f.syndic.clone(),
        id(30),
        turn,
        support::timestamp(501),
    );
    let binding = f
        .syndic
        .current_binding(&f.store, id(30), limit())
        .unwrap()
        .unwrap();
    let gate = f
        .syndic
        .input_gate(&f.store, id(30), limit())
        .unwrap()
        .unwrap();
    let state = f
        .syndic
        .turn_state(&f.store, turn, limit())
        .unwrap()
        .unwrap();
    let request = CancelBindingActivation::new(
        id(30),
        binding.binding().revision(),
        gate.revision(),
        state.revision(),
        binding.binding().selected_path(),
        snapshot,
        turn,
    );
    request
}

fn prepare(
    f: &Fixture,
    request: CancelBindingActivation,
    kind: HandoffFailureKind,
) -> PreparedDiscussionSettlement<'static> {
    let evidence = match kind {
        HandoffFailureKind::TransientDeliveryFailure => {
            let outcome = beryl_backend::NonIdempotentRequestOutcome::ProvenNotDispatched {
                error: Box::new(beryl_backend::ManagedBackendError::ProcessGenerationExhausted),
            };
            DiscussionParentNondispatch::from_start_outcome_for_test(request, Ok(&outcome)).unwrap()
        }
        HandoffFailureKind::RuntimeUnavailable => {
            DiscussionParentNondispatch::from_start_outcome_for_test(
                request,
                Err(beryl_app::process_admission::ProcessAdmissionError::Fenced),
            )
            .unwrap()
        }
        _ => DiscussionParentNondispatch::for_test(request, kind),
    };
    f.service()
        .prepare_parent_nondispatch(f.job, evidence, CommandCancellation::new())
        .unwrap()
}

fn status(f: &Fixture, request: &CancelBindingActivation) -> BindingPublicationStatus {
    f.syndic
        .cancelled_binding_activation_status(&f.store, request, limit())
        .unwrap()
}

#[test]
fn proven_nondispatch_atomically_pauses_the_same_parent_until_explicit_retry() {
    for kind in [
        HandoffFailureKind::CasRejectedBeforeAcceptance,
        HandoffFailureKind::TransientDeliveryFailure,
        HandoffFailureKind::RuntimeUnavailable,
    ] {
        let (f, request) = activated();
        let old = job(&f);
        let parent = parent_input::identity(&f);
        let input = f
            .syndic
            .accepted_input(&f.store, parent.accepted_input_id(), limit())
            .unwrap();
        let prepared = prepare(&f, request.clone(), kind);
        let audit = prepared.audit();
        assert_eq!(f.audit(&audit), DiscussionSettlementAuditOutcome::Pending);
        let expected = DiscussionSettlementResult::ParentRetryable { parent, kind };
        assert!(
            matches!(prepared.execute(), DiscussionSettlementOutcome::Committed { result, later_failure: None, .. } if result == expected)
        );
        assert_eq!(status(&f, &request), BindingPublicationStatus::Exact);
        assert_eq!(
            f.audit(&audit),
            DiscussionSettlementAuditOutcome::Settled(expected)
        );
        drop(audit);
        let paused = job(&f);
        assert_eq!(
            paused.lifecycle(),
            BranchHandoffJobLifecycle::RetryableFailed
        );
        assert_eq!(paused.state().parent(), Some(parent));
        assert_eq!(paused.resolution(), old.resolution());
        assert_eq!(
            f.syndic
                .accepted_input(&f.store, parent.accepted_input_id(), limit())
                .unwrap(),
            input
        );
        assert_eq!(
            f.syndic
                .discussion_handoff_gate(&f.store, id(36), limit())
                .unwrap(),
            Some(f.gate)
        );
        assert!(
            f.service()
                .prepare_parent_execution(
                    f.job,
                    support::timestamp(600),
                    CommandCancellation::new()
                )
                .is_err()
        );
        assert!(matches!(
            f.service()
                .prepare_retry(f.job, paused.revision(), CommandCancellation::new())
                .unwrap()
                .execute(),
            DiscussionSettlementOutcome::Committed {
                result: DiscussionSettlementResult::RetryResumed,
                ..
            }
        ));
        assert_eq!(job(&f).state(), old.state());
        f.store.close().unwrap();
    }
}

#[test]
fn cancellation_stale_sources_and_unknown_dispatch_cannot_publish_parent_failure() {
    let (f, request) = activated();
    let old = job(&f);
    let unknown = beryl_backend::NonIdempotentRequestOutcome::CompletionUnknown {
        error: Box::new(beryl_backend::ManagedBackendError::ProcessGenerationExhausted),
    };
    assert!(
        DiscussionParentNondispatch::from_start_outcome_for_test(request.clone(), Ok(&unknown))
            .is_err()
    );
    let cancellation = CommandCancellation::new();
    let prepared = f
        .service()
        .prepare_parent_nondispatch(
            f.job,
            DiscussionParentNondispatch::for_test(
                request.clone(),
                HandoffFailureKind::CasRejectedBeforeAcceptance,
            ),
            cancellation.clone(),
        )
        .unwrap();
    cancellation.cancel();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    let prepared = prepare(
        &f,
        request.clone(),
        HandoffFailureKind::TransientDeliveryFailure,
    );
    support::commit(
        &f.store,
        f.syndic.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::DiscussionHandoffGate(f.gate)]),
    );
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert_eq!(job(&f), old);
    assert_eq!(status(&f, &request), BindingPublicationStatus::Prior);
    let binding = f
        .syndic
        .current_binding(&f.store, id(30), limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("active parent");
    };
    let mut command = HomeCommand::new(f.store.home_revision().unwrap());
    command
        .add(f.syndic.publish_active_cas_turn(
            f.syndic.revision(&f.store).unwrap(),
            PublishActiveCasTurn::new(
                id(30),
                request.expected_binding_revision(),
                request.expected_gate_revision(),
                request.snapshot_id(),
                active.usable().cas_thread_id().clone(),
                beryl_model::CasTurnId::new("accepted-parent").unwrap(),
                support::timestamp(502),
            ),
        ))
        .unwrap();
    assert!(matches!(
        f.store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    assert!(
        f.service()
            .prepare_parent_nondispatch(
                f.job,
                DiscussionParentNondispatch::for_test(
                    request,
                    HandoffFailureKind::CasRejectedBeforeAcceptance
                ),
                CommandCancellation::new()
            )
            .is_err()
    );
    assert_eq!(job(&f), old);
    f.store.close().unwrap();
}

#[test]
fn nondispatch_failure_cuts_keep_both_participants_together_through_recovery() {
    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let (f, request) = activated();
        let prepared = prepare(
            &f,
            request.clone(),
            HandoffFailureKind::CasRejectedBeforeAcceptance,
        );
        f.faults.fail_next(fault);
        let outcome = prepared.execute();
        if fault == FaultPoint::BeforeCommit {
            assert!(matches!(
                outcome,
                DiscussionSettlementOutcome::NotCommitted { .. }
            ));
        } else {
            assert!(matches!(
                outcome,
                DiscussionSettlementOutcome::Indeterminate { .. }
            ));
        }
        drop(outcome);
        if f.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(f.store.home_revision().is_err());
        }
        let mut candidate = f.store.recover_same_home().unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let access = candidate.recovery_access().unwrap();
        let actual = syndic
            .cancelled_binding_activation_status_candidate(&access, &request, limit())
            .unwrap();
        let current = state
            .durable_jobs()
            .job_candidate(&access, f.job)
            .unwrap()
            .unwrap();
        if fault == FaultPoint::BeforeCommit {
            assert_eq!(actual, BindingPublicationStatus::Prior);
            assert_eq!(
                current.lifecycle(),
                BranchHandoffJobLifecycle::StartingParent
            );
            assert!(f.operations.retained_audit(f.job).is_none());
        } else {
            assert_eq!(actual, BindingPublicationStatus::Exact);
            assert_eq!(
                current.lifecycle(),
                BranchHandoffJobLifecycle::RetryableFailed
            );
            let audit = f.operations.retained_audit(f.job).unwrap();
            assert!(matches!(
                audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
                DiscussionSettlementAuditOutcome::Settled(
                    DiscussionSettlementResult::ParentRetryable { .. }
                )
            ));
            assert!(f.operations.retained_audit(f.job).is_none());
        }
        assert!(access.pending_reconciliations().is_empty());
        candidate.publish().unwrap().close().unwrap();
    }
}

#[test]
fn mixed_nondispatch_outcome_stays_gated() {
    let (f, request) = activated();
    let old_turn = f
        .syndic
        .turn_state(&f.store, request.turn_id(), limit())
        .unwrap()
        .unwrap();
    let prepared = prepare(&f, request, HandoffFailureKind::CasRejectedBeforeAcceptance);
    f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Indeterminate { .. }
    ));
    support::commit(
        &f.store,
        f.syndic.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::TurnState(
            old_turn,
        )]),
    );
    let audit = f.operations.retained_audit(f.job).unwrap();
    assert_eq!(f.audit(&audit), DiscussionSettlementAuditOutcome::Collision);
    assert!(f.operations.retained_audit(f.job).is_some());
    assert!(matches!(
        f.service()
            .prepare_retry(f.job, job(&f).revision(), CommandCancellation::new()),
        Err(DiscussionSettlementError::DuplicateIdentity)
    ));
    drop(f);
}
