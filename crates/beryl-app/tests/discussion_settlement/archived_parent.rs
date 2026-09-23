use super::*;
use beryl_model::SyndicItemId;
use support::{draft_id, exact_cas, timestamp};

fn fixture(waiting_parent: bool) -> Fixture {
    let mut fixture = Fixture::new(false);
    let parent_gate = fixture.gate;
    support::discussion_creation::create_child(&fixture.store, &fixture.syndic, id(36), 230, 231);
    let request = support::discussion_handoff::active_request_for(
        &fixture.store,
        &fixture.syndic,
        id(230),
        draft_id(232),
        SyndicItemId::from_bytes([233; 16]),
        ResolutionIntentId::from_bytes([234; 16]),
        JobId::from_bytes([235; 16]),
    );
    let turn = request.resolving_target.pending().active_turn_id();
    let source = CasTurnSource::new(
        request.resolving_target.pending().cas_thread_id().clone(),
        request.resolving_target.cas_turn_id().clone(),
    );
    (fixture.job, fixture.gate) =
        admit_request(&fixture.store, &fixture.state, &fixture.syndic, request);
    if waiting_parent {
        exact_cas::admit_event(
            &fixture.store,
            fixture.syndic.clone(),
            id(230),
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(30),
        );
        exact_cas::correlate_user_item(
            &fixture.store,
            fixture.syndic.clone(),
            id(230),
            turn,
            SyndicItemId::from_bytes([233; 16]),
            &source,
            timestamp(31),
        );
        exact_cas::admit_event(
            &fixture.store,
            fixture.syndic.clone(),
            id(230),
            turn,
            &source,
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
            ),
            timestamp(32),
        );
        exact_cas::converge_and_release_terminal_history(
            &fixture.store,
            fixture.syndic.clone(),
            id(230),
            turn,
        );
        assert!(matches!(
            fixture.prepare().execute(),
            DiscussionSettlementOutcome::Committed {
                result: DiscussionSettlementResult::ReadyForParent,
                later_failure: None,
                ..
            }
        ));
        assert!(
            fixture
                .service()
                .prepare(fixture.job, CommandCancellation::new())
                .unwrap()
                .is_none()
        );
    }
    let attributes = fixture
        .syndic
        .thread_attributes(&fixture.store, id(36), limit())
        .unwrap()
        .unwrap();
    let archive = fixture
        .syndic
        .prepare_discussion_handoff(
            &fixture.store,
            DiscussionHandoffMutation::ReleaseAndArchive {
                expected: parent_gate,
                attributes_revision: attributes.revision(),
                archived_at: timestamp(40),
            },
        )
        .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command.add(archive.contribution()).unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    fixture
}

#[test]
fn archived_parent_cancels_or_fails_atomically_at_both_preappend_checkpoints() {
    for waiting_parent in [false, true] {
        let fixture = fixture(waiting_parent);
        let before = fixture
            .state
            .durable_jobs()
            .job(&fixture.store, fixture.job)
            .unwrap()
            .unwrap();
        let parent = fixture
            .syndic
            .thread(&fixture.store, id(36), limit())
            .unwrap();
        let parent_input = fixture
            .syndic
            .input_gate(&fixture.store, id(36), limit())
            .unwrap();
        let child_input = fixture
            .syndic
            .input_gate(&fixture.store, id(230), limit())
            .unwrap();
        let child_draft = fixture
            .syndic
            .current_draft(&fixture.store, id(230), limit())
            .unwrap()
            .unwrap()
            .draft()
            .clone();
        let cancellation = CommandCancellation::new();
        let cancelled = fixture
            .service()
            .prepare(fixture.job, cancellation.clone())
            .unwrap()
            .unwrap();
        cancellation.cancel();
        assert!(matches!(
            cancelled.execute(),
            DiscussionSettlementOutcome::NotCommitted { .. }
        ));
        assert_eq!(
            fixture
                .state
                .durable_jobs()
                .job(&fixture.store, fixture.job)
                .unwrap(),
            Some(before.clone())
        );
        assert_eq!(
            fixture
                .syndic
                .discussion_handoff_gate(&fixture.store, id(230), limit())
                .unwrap(),
            Some(fixture.gate)
        );
        let prepared = fixture.prepare();
        let audit = prepared.audit();
        assert!(matches!(
            prepared.execute(),
            DiscussionSettlementOutcome::Committed {
                result: DiscussionSettlementResult::ParentArchived,
                later_failure: None,
                ..
            }
        ));
        assert_eq!(
            fixture.audit(&audit),
            DiscussionSettlementAuditOutcome::Settled(DiscussionSettlementResult::ParentArchived)
        );
        let after = fixture
            .state
            .durable_jobs()
            .job(&fixture.store, fixture.job)
            .unwrap()
            .unwrap();
        assert!(
            matches!(after.state(), beryl_state::BranchHandoffJobState::TerminalFailed { evidence, .. }
            if evidence.kind() == beryl_state::HandoffFailureKind::ParentArchived)
        );
        assert_eq!(after.state().parent(), None);
        assert_eq!(
            fixture
                .syndic
                .discussion_handoff_gate(&fixture.store, id(230), limit())
                .unwrap()
                .unwrap()
                .state(),
            DiscussionHandoffGateState::Open
        );
        assert_eq!(
            fixture
                .syndic
                .thread_attributes(&fixture.store, id(230), limit())
                .unwrap()
                .unwrap()
                .archive(),
            ThreadArchiveState::BranchDiscussionOpen
        );
        assert_eq!(
            fixture
                .syndic
                .thread(&fixture.store, id(36), limit())
                .unwrap(),
            parent
        );
        assert_eq!(
            fixture
                .syndic
                .input_gate(&fixture.store, id(36), limit())
                .unwrap(),
            parent_input
        );
        assert_eq!(
            fixture
                .syndic
                .input_gate(&fixture.store, id(230), limit())
                .unwrap(),
            child_input
        );
        assert_eq!(
            fixture
                .syndic
                .current_draft(&fixture.store, id(230), limit())
                .unwrap()
                .unwrap()
                .draft(),
            &child_draft
        );
        fixture.store.close().unwrap();
    }
}

#[test]
fn archived_parent_uncertainty_retains_registry_and_both_domain_proofs() {
    super::recovery::uncertain_outcome(fixture(true), DiscussionSettlementResult::ParentArchived);
}

#[test]
fn archived_parent_candidate_convergence_rejects_stale_handles_without_parent_input() {
    super::recovery::candidate_outcome(fixture(false), DiscussionSettlementResult::ParentArchived);
}
