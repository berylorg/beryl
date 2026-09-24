use super::*;

#[test]
fn candidate_uncertainty_retains_exact_audit_until_reconciled_before_more_progress() {
    let fixture = parent_execution::start();
    let identity = parent_input::identity(&fixture);
    let source = parent_execution::accepted(&fixture);
    parent_execution::finish(&fixture, &source, TurnEndStatus::complete());
    fail_read(&fixture);
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let error = fixture
        .operations
        .converge_candidate(
            &access,
            &state,
            &syndic,
            limits(),
            support::timestamp(600),
            CommandCancellation::new(),
        )
        .unwrap_err();
    let HandoffCandidateConvergenceError::Outcome { job_id, outcome } = error else {
        panic!("retained command outcome")
    };
    assert_eq!(job_id, fixture.job);
    let DiscussionSettlementOutcome::Indeterminate { audit, .. } = *outcome else {
        panic!("uncertain acceptance")
    };
    assert_eq!(access.pending_reconciliations().len(), 1);
    assert!(
        fixture
            .operations
            .converge_candidate(
                &access,
                &state,
                &syndic,
                limits(),
                support::timestamp(600),
                CommandCancellation::new()
            )
            .is_err()
    );
    assert_eq!(
        audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
        DiscussionSettlementAuditOutcome::Settled(DiscussionSettlementResult::ParentActive(
            identity
        ))
    );
    assert!(access.pending_reconciliations().is_empty());
    drop(audit);
    assert_eq!(
        fixture
            .operations
            .converge_candidate(
                &access,
                &state,
                &syndic,
                limits(),
                support::timestamp(600),
                CommandCancellation::new()
            )
            .unwrap(),
        HandoffCandidateConvergenceSummary {
            job_visits: 1,
            transitions: 1
        }
    );
    candidate.publish().unwrap().close().unwrap();
}
