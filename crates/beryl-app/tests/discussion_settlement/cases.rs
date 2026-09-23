use super::*;

#[test]
fn settlement_waits_then_advances_or_releases_atomically_without_parent_input() {
    for queued in [false, true] {
        let fixture = Fixture::new(queued);
        assert!(
            fixture
                .service()
                .prepare(fixture.job, CommandCancellation::new())
                .unwrap()
                .is_none()
        );
        fixture.finish_child();
        let input = fixture
            .syndic
            .input_gate(&fixture.store, id(36), limit())
            .unwrap();
        let parent = fixture
            .syndic
            .input_gate(&fixture.store, id(30), limit())
            .unwrap();
        let prepared = fixture.prepare();
        let audit = prepared.audit();
        assert_eq!(
            fixture.audit(&audit),
            DiscussionSettlementAuditOutcome::Pending
        );
        let result = if queued {
            DiscussionSettlementResult::ChildInputPending
        } else {
            DiscussionSettlementResult::ReadyForParent
        };
        assert!(
            matches!(prepared.execute(), DiscussionSettlementOutcome::Committed { result: actual, later_failure: None, .. } if actual == result)
        );
        assert_eq!(
            fixture.audit(&audit),
            DiscussionSettlementAuditOutcome::Settled(result)
        );
        let job = fixture
            .state
            .durable_jobs()
            .job(&fixture.store, fixture.job)
            .unwrap()
            .unwrap();
        assert_eq!(
            job.lifecycle(),
            if queued {
                BranchHandoffJobLifecycle::TerminalFailed
            } else {
                BranchHandoffJobLifecycle::WaitingParent
            }
        );
        let gate = fixture
            .syndic
            .discussion_handoff_gate(&fixture.store, id(36), limit())
            .unwrap()
            .unwrap();
        if queued {
            assert_eq!(gate.state(), DiscussionHandoffGateState::Open);
        } else {
            assert_eq!(gate, fixture.gate);
        }
        assert_eq!(
            fixture
                .syndic
                .input_gate(&fixture.store, id(36), limit())
                .unwrap(),
            input
        );
        assert_eq!(
            fixture
                .syndic
                .input_gate(&fixture.store, id(30), limit())
                .unwrap(),
            parent
        );
        assert_eq!(
            fixture
                .syndic
                .thread_attributes(&fixture.store, id(36), limit())
                .unwrap()
                .unwrap()
                .archive(),
            ThreadArchiveState::BranchDiscussionOpen
        );
        fixture.store.close().unwrap();
    }
}

#[test]
fn cancellation_drop_and_retained_audit_preserve_bounded_capacity() {
    let fixture = Fixture::new(true);
    fixture.finish_child();
    let cancellation = CommandCancellation::new();
    let prepared = fixture
        .service()
        .prepare(fixture.job, cancellation.clone())
        .unwrap()
        .unwrap();
    let audit = prepared.audit();
    assert!(matches!(
        fixture
            .service()
            .prepare(fixture.job, CommandCancellation::new()),
        Err(DiscussionSettlementError::DuplicateIdentity)
    ));
    assert!(matches!(
        fixture
            .service()
            .prepare(JobId::from_bytes([250; 16]), CommandCancellation::new()),
        Err(DiscussionSettlementError::Capacity)
    ));
    cancellation.cancel();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::NotCommitted
    );
    assert!(
        fixture
            .service()
            .prepare(fixture.job, CommandCancellation::new())
            .is_err()
    );
    drop(audit);
    let prepared = fixture.prepare();
    let audit = prepared.audit();
    drop(prepared);
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::NotCommitted
    );
    drop(audit);
    drop(fixture.prepare());
    fixture.store.close().unwrap();
}

#[test]
fn process_reopen_cannot_revive_prepared_settlement() {
    let fixture = Fixture::new(false);
    fixture.finish_child();
    let prepared = fixture.prepare();
    let fence = fixture.process.test_fence().unwrap();
    fence.try_reopen(true).unwrap();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted {
            evidence: DiscussionSettlementError::Process(_)
        }
    ));
    assert_eq!(
        fixture
            .state
            .durable_jobs()
            .job(&fixture.store, fixture.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::WaitingResolvingTurn
    );
    assert!(matches!(
        fixture.prepare().execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    fixture.store.close().unwrap();
}
