use super::*;

#[test]
fn different_cas_identity_in_job_cannot_settle_matching_generated_input() {
    let fixture = start();
    let source = accepted(&fixture);
    finish(&fixture, &source, TurnEndStatus::complete());
    let job = fixture
        .state
        .durable_jobs()
        .job(&fixture.store, fixture.job)
        .unwrap()
        .unwrap();
    let transition = fixture
        .state
        .durable_jobs()
        .prepare_handoff_job_transition(
            &fixture.store,
            fixture.job,
            job.revision(),
            beryl_state::HandoffJobTransition::ParentAccepted(beryl_state::ParentCasIdentity::new(
                source.thread_id().clone(),
                beryl_model::CasTurnId::new("different-parent-turn").unwrap(),
            )),
        )
        .unwrap();
    support::discussion_input::committed(&fixture.store, transition.contribution());
    assert!(matches!(
        fixture.service().prepare_parent_execution(
            fixture.job,
            support::timestamp(600),
            CommandCancellation::new()
        ),
        Err(DiscussionSettlementError::IdentityMismatch)
    ));
    assert_eq!(lifecycle(&fixture), BranchHandoffJobLifecycle::ParentActive);
    assert_eq!(
        fixture
            .syndic
            .discussion_handoff_gate(&fixture.store, id(36), limit())
            .unwrap(),
        Some(fixture.gate)
    );
    fixture.store.close().unwrap();
}

#[test]
fn mixed_terminal_job_and_child_gate_cannot_report_success() {
    let fixture = start();
    let source = accepted(&fixture);
    finish(&fixture, &source, TurnEndStatus::complete());
    advance(&fixture);
    let prepared = observe(&fixture).unwrap();
    let audit = prepared.audit();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::ParentSucceeded(_),
            later_failure: None,
            ..
        }
    ));
    support::commit(
        &fixture.store,
        fixture.syndic.clone(),
        support::batch([
            syndic_storage::test_faults::FixtureRecord::DiscussionHandoffGate(fixture.gate),
        ]),
    );
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::Collision
    );
    fixture.store.close().unwrap();
}
