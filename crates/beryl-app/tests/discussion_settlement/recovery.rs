use super::*;

#[test]
fn uncertain_settlement_keeps_custody_until_both_domains_reconcile() {
    uncertain_settlement(true);
}

#[test]
fn uncertain_ready_transition_reconciles_validation_only_syndic_participation() {
    uncertain_settlement(false);
}

fn uncertain_settlement(queued: bool) {
    let fixture = Fixture::new(queued);
    fixture.finish_child();
    uncertain_outcome(
        fixture,
        if queued {
            DiscussionSettlementResult::ChildInputPending
        } else {
            DiscussionSettlementResult::ReadyForParent
        },
    );
}

pub(super) fn uncertain_outcome(fixture: Fixture, expected: DiscussionSettlementResult) {
    let prepared = fixture.prepare();
    let retained = prepared.audit();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Indeterminate { .. }
    ));
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    fixture
        .faults
        .fail_next(FaultPoint::BeforeReconciliationSnapshot);
    assert!(
        retained
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .is_err()
    );
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    assert_eq!(
        fixture.audit(&retained),
        DiscussionSettlementAuditOutcome::Settled(expected)
    );
    assert!(fixture.store.pending_reconciliations().is_empty());
    fixture.store.close().unwrap();
}

#[test]
fn candidate_settlement_uses_fresh_handles_without_ordinary_process_admission() {
    let fixture = Fixture::new(true);
    fixture.finish_child();
    candidate_outcome(fixture, DiscussionSettlementResult::ChildInputPending);
}

pub(super) fn candidate_outcome(fixture: Fixture, expected: DiscussionSettlementResult) {
    let stale = fixture.prepare();
    let fence = fixture.process.test_fence().unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    assert!(matches!(
        stale.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    let access = candidate.recovery_access().unwrap();
    assert!(
        fixture
            .operations
            .prepare_candidate(
                &access,
                &fixture.state,
                &fixture.syndic,
                fixture.job,
                CommandCancellation::new()
            )
            .is_err()
    );
    let prepared = fixture
        .operations
        .prepare_candidate(
            &access,
            &state,
            &syndic,
            fixture.job,
            CommandCancellation::new(),
        )
        .unwrap()
        .unwrap();
    let audit = prepared.audit();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Committed {
            result,
            later_failure: None,
            ..
        } if result == expected
    ));
    assert_eq!(
        audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
        DiscussionSettlementAuditOutcome::Settled(expected)
    );
    let store = candidate.publish().unwrap();
    fence.try_reopen(true).unwrap();
    store.close().unwrap();
}
