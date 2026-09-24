use super::*;

#[test]
fn uncertain_acceptance_reconciles_validation_only_source_without_dispatch() {
    let fixture = start();
    accepted(&fixture);
    let expected = DiscussionSettlementResult::ParentActive(identity(&fixture));
    let prepared = observe(&fixture).unwrap();
    let audit = prepared.audit();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Indeterminate { .. }
    ));
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
        DiscussionSettlementAuditOutcome::Settled(expected)
    );
    assert!(access.pending_reconciliations().is_empty());
    drop(audit);
    let before = access.home_revision().unwrap();
    assert!(
        fixture
            .operations
            .prepare_parent_execution_candidate(
                &access,
                &state,
                &syndic,
                fixture.job,
                support::timestamp(600),
                CommandCancellation::new()
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(access.home_revision().unwrap(), before);
    let store = candidate.publish().unwrap();
    assert_eq!(
        state
            .durable_jobs()
            .job(&store, fixture.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::ParentActive
    );
    assert_eq!(
        syndic
            .discussion_handoff_gate(&store, id(36), limit())
            .unwrap(),
        Some(fixture.gate)
    );
    store.close().unwrap();
}

#[test]
fn terminal_uncertainty_reconciles_both_domains_and_candidate_finishes_exact_old() {
    for point in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let fixture = start();
        let source = accepted(&fixture);
        finish(&fixture, &source, TurnEndStatus::complete());
        advance(&fixture);
        let expected = DiscussionSettlementResult::ParentSucceeded(identity(&fixture));
        let prepared = observe(&fixture).unwrap();
        let audit = prepared.audit();
        fixture.faults.fail_next(point);
        match prepared.execute() {
            DiscussionSettlementOutcome::NotCommitted { .. }
                if point == FaultPoint::BeforeCommit => {}
            DiscussionSettlementOutcome::Indeterminate { .. }
                if point == FaultPoint::AfterCommitBeforePersist => {}
            _ => panic!("unexpected injected outcome"),
        }
        if fixture.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(fixture.store.home_revision().is_err());
        }
        let mut candidate = fixture.store.recover_same_home().unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let access = candidate.recovery_access().unwrap();
        assert!(
            audit
                .reconcile_candidate(&access, &fixture.syndic, &fixture.state)
                .is_err()
        );
        assert_eq!(
            audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
            if point == FaultPoint::BeforeCommit {
                DiscussionSettlementAuditOutcome::NotCommitted
            } else {
                DiscussionSettlementAuditOutcome::Settled(expected)
            }
        );
        assert!(access.pending_reconciliations().is_empty());
        drop(audit);
        if point == FaultPoint::BeforeCommit {
            let prepared = fixture
                .operations
                .prepare_parent_execution_candidate(
                    &access,
                    &state,
                    &syndic,
                    fixture.job,
                    support::timestamp(600),
                    CommandCancellation::new(),
                )
                .unwrap()
                .unwrap();
            assert!(
                matches!(prepared.execute(), DiscussionSettlementOutcome::Committed { result, later_failure: None, .. } if result == expected)
            );
        }
        let store = candidate.publish().unwrap();
        assert_eq!(
            state
                .durable_jobs()
                .job(&store, fixture.job)
                .unwrap()
                .unwrap()
                .lifecycle(),
            BranchHandoffJobLifecycle::Succeeded
        );
        assert_ne!(
            syndic
                .thread_attributes(&store, id(36), limit())
                .unwrap()
                .unwrap()
                .archive(),
            ThreadArchiveState::BranchDiscussionOpen
        );
        store.close().unwrap();
    }
}
