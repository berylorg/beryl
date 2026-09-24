use super::*;
use beryl_state::{
    BranchHandoffJobRecord, HandoffFailureEvidence, HandoffFailureKind, HandoffJobTransition,
};

fn current(fixture: &Fixture) -> BranchHandoffJobRecord {
    fixture
        .state
        .durable_jobs()
        .job(&fixture.store, fixture.job)
        .unwrap()
        .unwrap()
}
fn pause(fixture: &Fixture) -> BranchHandoffJobRecord {
    let job = current(fixture);
    let paused = fixture
        .state
        .durable_jobs()
        .prepare_handoff_job_transition(
            &fixture.store,
            fixture.job,
            job.revision(),
            HandoffJobTransition::RetryableFailure(
                HandoffFailureEvidence::new(HandoffFailureKind::RuntimeUnavailable, None).unwrap(),
            ),
        )
        .unwrap();
    support::discussion_input::committed(&fixture.store, paused.contribution());
    current(fixture)
}
fn prepare(fixture: &Fixture) -> PreparedDiscussionSettlement<'static> {
    fixture
        .service()
        .prepare_retry(
            fixture.job,
            current(fixture).revision(),
            CommandCancellation::new(),
        )
        .unwrap()
}

#[test]
fn explicit_retry_restores_each_exact_checkpoint_without_changing_syndic() {
    for checkpoint in 0..4 {
        let fixture = if checkpoint >= 2 {
            parent_execution::start()
        } else {
            Fixture::new(false)
        };
        if checkpoint == 1 {
            parent_input::ready(&fixture);
        }
        if checkpoint == 3 {
            parent_execution::accepted(&fixture);
            assert!(matches!(
                fixture
                    .service()
                    .prepare_parent_execution(
                        fixture.job,
                        support::timestamp(600),
                        CommandCancellation::new()
                    )
                    .unwrap()
                    .unwrap()
                    .execute(),
                DiscussionSettlementOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
        }
        let original = current(&fixture);
        assert!(
            fixture
                .service()
                .prepare_retry(fixture.job, original.revision(), CommandCancellation::new())
                .is_err()
        );
        let paused = pause(&fixture);
        assert!(
            fixture
                .service()
                .prepare_retry(fixture.job, original.revision(), CommandCancellation::new())
                .is_err()
        );
        let syndic_before = fixture.syndic.revision(&fixture.store).unwrap();
        let prepared = prepare(&fixture);
        let audit = prepared.audit();
        assert!(matches!(
            prepared.execute(),
            DiscussionSettlementOutcome::Committed {
                result: DiscussionSettlementResult::RetryResumed,
                later_failure: None,
                ..
            }
        ));
        assert_eq!(
            fixture.audit(&audit),
            DiscussionSettlementAuditOutcome::Settled(DiscussionSettlementResult::RetryResumed)
        );
        drop(audit);
        let resumed = current(&fixture);
        assert_eq!(resumed.state(), original.state());
        assert_eq!(resumed.intent_id(), original.intent_id());
        assert_eq!(resumed.request(), original.request());
        assert_eq!(resumed.resolution(), original.resolution());
        assert_eq!(resumed.attempt_ordinal(), original.attempt_ordinal());
        assert_eq!(resumed.revision().get(), paused.revision().get() + 1);
        assert_eq!(
            fixture.syndic.revision(&fixture.store).unwrap(),
            syndic_before
        );
        assert!(
            fixture
                .service()
                .prepare_retry(fixture.job, paused.revision(), CommandCancellation::new())
                .is_err()
        );
        fixture.store.close().unwrap();
    }
}

#[test]
fn retry_rejects_changed_gate_and_stale_writer_without_resuming() {
    let fixture = parent_execution::start();
    let paused = pause(&fixture);
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        fixture
            .service()
            .prepare_retry(fixture.job, paused.revision(), cancelled),
        Err(DiscussionSettlementError::Cancelled)
    ));
    let prepared = prepare(&fixture);
    let release = fixture
        .syndic
        .prepare_discussion_handoff(
            &fixture.store,
            DiscussionHandoffMutation::Release {
                expected: fixture.gate,
            },
        )
        .unwrap();
    support::discussion_input::committed(&fixture.store, release.contribution());
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert_eq!(current(&fixture), paused);
    assert!(
        fixture
            .service()
            .prepare_retry(fixture.job, paused.revision(), CommandCancellation::new())
            .is_err()
    );
    let terminal = fixture
        .state
        .durable_jobs()
        .prepare_handoff_job_transition(
            &fixture.store,
            fixture.job,
            paused.revision(),
            HandoffJobTransition::TerminalFailure(
                HandoffFailureEvidence::new(HandoffFailureKind::InvariantViolation, None).unwrap(),
            ),
        )
        .unwrap();
    support::discussion_input::committed(&fixture.store, terminal.contribution());
    assert!(
        fixture
            .service()
            .prepare_retry(
                fixture.job,
                current(&fixture).revision(),
                CommandCancellation::new()
            )
            .is_err()
    );
    fixture.store.close().unwrap();
}

#[test]
fn retry_rejects_a_parent_cas_identity_that_disagrees_with_syndic() {
    let fixture = parent_execution::start();
    parent_execution::accepted(&fixture);
    let actual = current(&fixture);
    let wrong = fixture
        .state
        .durable_jobs()
        .prepare_handoff_job_transition(
            &fixture.store,
            fixture.job,
            actual.revision(),
            HandoffJobTransition::ParentAccepted(beryl_state::ParentCasIdentity::new(
                beryl_model::CasThreadId::new("wrong-parent").unwrap(),
                beryl_model::CasTurnId::new("wrong-turn").unwrap(),
            )),
        )
        .unwrap();
    support::discussion_input::committed(&fixture.store, wrong.contribution());
    let paused = pause(&fixture);
    assert!(matches!(
        fixture
            .service()
            .prepare_retry(fixture.job, paused.revision(), CommandCancellation::new()),
        Err(DiscussionSettlementError::IdentityMismatch)
    ));
    assert_eq!(current(&fixture), paused);
    fixture.store.close().unwrap();
}

#[test]
fn retry_uncertainty_keeps_the_same_parent_and_reconciles_with_fresh_candidate_handles() {
    let fixture = parent_execution::start();
    let original = current(&fixture);
    pause(&fixture);
    let prepared = prepare(&fixture);
    let audit = prepared.audit();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Indeterminate { .. }
    ));
    drop(audit);
    let audit = fixture.operations.retained_audit(fixture.job).unwrap();
    if fixture.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(fixture.store.home_revision().is_err());
    }
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
        DiscussionSettlementAuditOutcome::Settled(DiscussionSettlementResult::RetryResumed)
    );
    let resumed = state
        .durable_jobs()
        .job_candidate(&access, fixture.job)
        .unwrap()
        .unwrap();
    assert_eq!(resumed.state(), original.state());
    assert_eq!(
        syndic
            .discussion_handoff_gate_candidate(&access, id(36), limit())
            .unwrap(),
        Some(fixture.gate)
    );
    assert!(access.pending_reconciliations().is_empty());
    assert!(fixture.operations.retained_audit(fixture.job).is_none());
    drop(audit);
    candidate.publish().unwrap().close().unwrap();
}
