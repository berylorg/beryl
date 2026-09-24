use super::*;
use beryl_model::{SyndicAcceptedInputId, SyndicItemId, SyndicTurnId};
use beryl_state::ParentHandoffIdentity;

fn request() -> DiscussionParentInputRequest {
    DiscussionParentInputRequest {
        turn_id: SyndicTurnId::from_bytes([240; 16]),
        item_id: SyndicItemId::from_bytes([241; 16]),
        admitted_at: support::timestamp(500),
    }
}
fn identity(fixture: &Fixture) -> ParentHandoffIdentity {
    ParentHandoffIdentity::new(
        SyndicAcceptedInputId::from_bytes(*fixture.job.as_bytes()),
        request().turn_id,
    )
}
fn ready(fixture: &Fixture) {
    fixture.finish_child();
    assert!(matches!(
        fixture.prepare().execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::ReadyForParent,
            later_failure: None,
            ..
        }
    ));
    support::converge_and_release_terminal_history(
        &fixture.store,
        fixture.syndic.clone(),
        id(30),
        support::populated::source_turn(),
    );
}
fn prepare(
    fixture: &Fixture,
    cancellation: CommandCancellation,
) -> PreparedDiscussionSettlement<'static> {
    fixture
        .service()
        .prepare_parent_input(fixture.job, request(), cancellation)
        .unwrap()
        .unwrap()
}
fn assert_old(fixture: &Fixture) {
    assert_eq!(
        fixture
            .state
            .durable_jobs()
            .job(&fixture.store, fixture.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::WaitingParent
    );
    assert!(
        fixture
            .syndic
            .accepted_input(
                &fixture.store,
                identity(fixture).accepted_input_id(),
                limit()
            )
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .syndic
            .turn(&fixture.store, request().turn_id, limit())
            .unwrap()
            .is_none()
    );
}

#[test]
fn parent_input_and_starting_job_commit_together_without_changing_draft() {
    let text = "🦀".repeat(65_536);
    let fixture = Fixture::with_resolution(false, &text);
    ready(&fixture);
    let draft = fixture
        .syndic
        .current_draft(&fixture.store, id(30), limit())
        .unwrap()
        .unwrap()
        .draft()
        .clone();
    let prepared = prepare(&fixture, CommandCancellation::new());
    let audit = prepared.audit();
    assert_old(&fixture);
    assert!(matches!(
        fixture
            .service()
            .prepare_parent_input(fixture.job, request(), CommandCancellation::new()),
        Err(DiscussionSettlementError::DuplicateIdentity)
    ));
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::Pending
    );
    let expected = DiscussionSettlementResult::StartingParent(identity(&fixture));
    assert!(
        matches!(prepared.execute(), DiscussionSettlementOutcome::Committed { result, later_failure: None, .. } if result == expected)
    );
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::Settled(expected)
    );
    let job = fixture
        .state
        .durable_jobs()
        .job(&fixture.store, fixture.job)
        .unwrap()
        .unwrap();
    assert_eq!(job.lifecycle(), BranchHandoffJobLifecycle::StartingParent);
    let input = fixture
        .syndic
        .accepted_input(
            &fixture.store,
            identity(&fixture).accepted_input_id(),
            limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(input.content().summary().logical_utf8_bytes(), 262_168);
    let AcceptedInputSource::DiscussionHandoff(receipt) = input.source() else {
        panic!("generated input")
    };
    assert_eq!(receipt.job_id, fixture.job);
    assert_eq!(receipt.canonical_item_id, request().item_id);
    assert_eq!(receipt.parent_turn_id, request().turn_id);
    let lookup = GeneratedDiscussionInputLookup {
        home_id: fixture.store.home_id(),
        parent_thread_id: id(30),
        child_thread_id: id(36),
        intent_id: job.intent_id(),
        job_id: fixture.job,
        context_owner: job.context_owner_id(),
        context_digest: job.context_digest(),
        resolving_turn_id: job.resolving_turn_id(),
        parent_turn_id: request().turn_id,
        canonical_item_id: request().item_id,
        resolution: text,
    };
    assert_eq!(
        fixture
            .syndic
            .discover_generated_discussion_input(&fixture.store, &lookup)
            .unwrap(),
        GeneratedDiscussionInputDiscovery::Exact
    );
    assert_eq!(
        fixture
            .syndic
            .current_draft(&fixture.store, id(30), limit())
            .unwrap()
            .unwrap()
            .draft(),
        &draft
    );
    assert_eq!(
        fixture
            .syndic
            .discussion_handoff_gate(&fixture.store, id(36), limit())
            .unwrap()
            .unwrap(),
        fixture.gate
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
}

#[test]
fn cancellation_and_parent_race_preserve_both_old_participants() {
    let fixture = Fixture::new(false);
    ready(&fixture);
    let cancellation = CommandCancellation::new();
    let prepared = prepare(&fixture, cancellation.clone());
    let audit = prepared.audit();
    cancellation.cancel();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::NotCommitted
    );
    assert_old(&fixture);
    drop(audit);
    let prepared = prepare(&fixture, CommandCancellation::new());
    let audit = prepared.audit();
    let gate = fixture
        .syndic
        .input_gate(&fixture.store, id(30), limit())
        .unwrap()
        .unwrap();
    support::commit(
        &fixture.store,
        fixture.syndic.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::InputGate(gate)]),
    );
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::NotCommitted
    );
    assert_old(&fixture);
    drop(audit);
    let prepared = prepare(&fixture, CommandCancellation::new());
    let audit = prepared.audit();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    let thread = fixture
        .syndic
        .thread(&fixture.store, id(30), limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .syndic
        .input_gate(&fixture.store, id(30), limit())
        .unwrap()
        .unwrap();
    support::commit(
        &fixture.store,
        fixture.syndic.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::InputGate(
            InputGateRecord::new(
                id(30),
                gate.revision().checked_next().unwrap(),
                gate.state().clone(),
                gate.accepted_high_water(),
                gate.route_generation_high_water(),
                gate.selected_route(),
                0,
                0,
                0,
            )
            .unwrap(),
        )]),
    );
    assert_eq!(
        fixture.audit(&audit),
        DiscussionSettlementAuditOutcome::Collision
    );
    assert_eq!(
        fixture
            .syndic
            .thread(&fixture.store, id(30), limit())
            .unwrap()
            .unwrap(),
        thread
    );
    fixture.store.close().unwrap();
}

#[test]
fn parent_input_uncertainty_reconciles_both_domains_in_candidate_without_readmission() {
    for point in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let fixture = Fixture::new(false);
        ready(&fixture);
        let job = fixture
            .state
            .durable_jobs()
            .job(&fixture.store, fixture.job)
            .unwrap()
            .unwrap();
        let lookup = GeneratedDiscussionInputLookup {
            home_id: fixture.store.home_id(),
            parent_thread_id: job.parent_thread_id(),
            child_thread_id: job.discussion_thread_id(),
            intent_id: job.intent_id(),
            job_id: fixture.job,
            context_owner: job.context_owner_id(),
            context_digest: job.context_digest(),
            resolving_turn_id: job.resolving_turn_id(),
            parent_turn_id: request().turn_id,
            canonical_item_id: request().item_id,
            resolution: job.resolution().as_str().to_owned(),
        };
        let prepared = prepare(&fixture, CommandCancellation::new());
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
        let parent_identity = identity(&fixture);
        let mut candidate = fixture.store.recover_same_home().unwrap();
        let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let access = candidate.recovery_access().unwrap();
        let before = access.home_revision().unwrap();
        assert!(
            audit
                .reconcile_candidate(&access, &fixture.syndic, &fixture.state)
                .is_err()
        );
        let expected = if point == FaultPoint::BeforeCommit {
            DiscussionSettlementAuditOutcome::NotCommitted
        } else {
            DiscussionSettlementAuditOutcome::Settled(DiscussionSettlementResult::StartingParent(
                parent_identity,
            ))
        };
        assert_eq!(
            audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
            expected
        );
        assert_eq!(access.home_revision().unwrap(), before);
        assert!(access.pending_reconciliations().is_empty());
        drop(audit);
        if point == FaultPoint::BeforeCommit {
            assert!(
                fixture
                    .operations
                    .prepare_candidate(
                        &access,
                        &state,
                        &syndic,
                        fixture.job,
                        CommandCancellation::new()
                    )
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                syndic
                    .discover_generated_discussion_input_candidate(&access, &lookup)
                    .unwrap(),
                GeneratedDiscussionInputDiscovery::Absent
            );
        }
        let store = candidate.publish().unwrap();
        store.close().unwrap();
    }
}
