use super::*;

#[test]
fn own_parent_archive_rescans_an_earlier_waiting_child_job() {
    let fixture = parent_execution::start();
    support::discussion_creation::create_child(&fixture.store, &fixture.syndic, id(36), 230, 231);
    let request = support::discussion_handoff::active_request_for(
        &fixture.store,
        &fixture.syndic,
        id(230),
        support::draft_id(232),
        beryl_model::SyndicItemId::from_bytes([233; 16]),
        ResolutionIntentId::from_bytes([100; 16]),
        JobId::from_bytes([101; 16]),
    );
    let (child_job, _) = admit_request(&fixture.store, &fixture.state, &fixture.syndic, request);
    assert!(child_job < fixture.job);
    let source = parent_execution::accepted(&fixture);
    parent_execution::finish(&fixture, &source, TurnEndStatus::complete());
    fail_read(&fixture);
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    let summary = fixture
        .operations
        .converge_candidate(
            &access,
            &state,
            &syndic,
            limits(),
            support::timestamp(600),
            CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(
        summary,
        HandoffCandidateConvergenceSummary {
            job_visits: 3,
            transitions: 3
        }
    );
    let child = state
        .durable_jobs()
        .job_candidate(&access, child_job)
        .unwrap()
        .unwrap();
    assert!(
        matches!(child.state(), beryl_state::BranchHandoffJobState::TerminalFailed { evidence, .. } if evidence.kind() == HandoffFailureKind::ParentArchived)
    );
    let store = candidate.publish().unwrap();
    assert_ne!(
        syndic
            .thread_attributes(&store, id(36), limit())
            .unwrap()
            .unwrap()
            .archive(),
        ThreadArchiveState::BranchDiscussionOpen
    );
    assert_eq!(
        syndic
            .thread_attributes(&store, id(230), limit())
            .unwrap()
            .unwrap()
            .archive(),
        ThreadArchiveState::BranchDiscussionOpen
    );
    assert_eq!(
        syndic
            .discussion_handoff_gate(&store, id(230), limit())
            .unwrap()
            .unwrap()
            .state(),
        DiscussionHandoffGateState::Open
    );
    store.close().unwrap();
}
