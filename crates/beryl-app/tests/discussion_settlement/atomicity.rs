use super::*;

fn put_gate(fixture: &Fixture, gate: DiscussionHandoffGateRecord) {
    support::commit(
        &fixture.store,
        fixture.syndic.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::DiscussionHandoffGate(gate)]),
    );
}

#[test]
fn mismatched_attempt_identity_rejects_before_any_job_transition() {
    let fixture = Fixture::new(false);
    fixture.finish_child();
    let DiscussionHandoffGateState::Pending {
        intent_id,
        job_id,
        resolving_turn_id,
    } = fixture.gate.state()
    else {
        panic!("pending gate")
    };
    for state in [
        DiscussionHandoffGateState::Pending {
            intent_id: ResolutionIntentId::from_bytes([240; 16]),
            job_id,
            resolving_turn_id,
        },
        DiscussionHandoffGateState::Pending {
            intent_id,
            job_id: JobId::from_bytes([240; 16]),
            resolving_turn_id,
        },
        DiscussionHandoffGateState::Pending {
            intent_id,
            job_id,
            resolving_turn_id: beryl_model::SyndicTurnId::from_bytes([240; 16]),
        },
    ] {
        put_gate(
            &fixture,
            DiscussionHandoffGateRecord::new(id(36), fixture.gate.revision(), state),
        );
        assert!(matches!(
            fixture
                .service()
                .prepare(fixture.job, CommandCancellation::new()),
            Err(DiscussionSettlementError::IdentityMismatch)
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
    }
    put_gate(&fixture, fixture.gate);
    assert!(matches!(
        fixture.prepare().execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    fixture.store.close().unwrap();
}

#[test]
fn stale_writer_source_and_mixed_outcome_never_report_settlement() {
    let fixture = Fixture::new(true);
    fixture.finish_child();
    let prepared = fixture.prepare();
    let audit = prepared.audit();
    put_gate(&fixture, fixture.gate);
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::NotCommitted
    );
    drop(audit);
    let prepared = fixture.prepare();
    let audit = prepared.audit();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    put_gate(&fixture, fixture.gate);
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::Collision
    );
    fixture.store.close().unwrap();
}

#[test]
fn writer_failure_preserves_both_old_records_and_releases_operation_capacity() {
    let fixture = Fixture::new(true);
    fixture.finish_child();
    let prepared = fixture.prepare();
    let audit = prepared.audit();
    fixture.faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
        DiscussionSettlementAuditOutcome::NotCommitted
    );
    drop(audit);
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
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    candidate.publish().unwrap().close().unwrap();
}
