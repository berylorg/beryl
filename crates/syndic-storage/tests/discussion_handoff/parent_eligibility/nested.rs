use super::*;

#[test]
fn nested_parent_initial_context_waits_then_archive_releases_only_child_gate() {
    let home = TestHome::new("nested-archived-parent");
    let faults = FaultController::new();
    let (store, storage, _) = seeded(&home, faults.clone());
    create_child(&store, &storage, id(30), 210, 211);
    create_child(&store, &storage, id(210), 212, 213);
    let child = support::discussion_handoff::active_request_for(
        &store,
        &storage,
        id(212),
        draft_id(214),
        SyndicItemId::from_bytes([215; 16]),
        ResolutionIntentId::from_bytes([216; 16]),
        JobId::from_bytes([217; 16]),
    );
    let request = admit_request(&store, &storage, child.clone());
    assert!(matches!(
        storage.prepare_discussion_parent(&store, request).unwrap(),
        DiscussionParentEligibility::Waiting
    ));
    let parent = support::discussion_handoff::active_request_for(
        &store,
        &storage,
        id(210),
        draft_id(218),
        SyndicItemId::from_bytes([219; 16]),
        ResolutionIntentId::from_bytes([220; 16]),
        JobId::from_bytes([221; 16]),
    );
    let parent_request = admit_request(&store, &storage, parent);
    assert!(matches!(
        storage.prepare_discussion_parent(&store, request).unwrap(),
        DiscussionParentEligibility::Waiting
    ));
    let attributes = storage
        .thread_attributes(&store, id(210), limit())
        .unwrap()
        .unwrap();
    let archive = storage
        .prepare_discussion_handoff(
            &store,
            DiscussionHandoffMutation::ReleaseAndArchive {
                expected: parent_request.child_gate,
                attributes_revision: attributes.revision(),
                archived_at: timestamp(30),
            },
        )
        .unwrap();
    execute(&store, archive);
    let parent_before = storage.thread(&store, id(210), limit()).unwrap();
    let child_input_before = storage.input_gate(&store, id(212), limit()).unwrap();
    let child_draft_before = storage
        .current_draft(&store, id(212), limit())
        .unwrap()
        .unwrap()
        .draft()
        .clone();
    let proof = proven(&store, &storage, request);
    assert_eq!(proof.disposition(), DiscussionParentDisposition::Archived);
    assert!(proof.clone().into_ready_validation().is_err());
    let stale = proof.into_archived_release().unwrap();
    let intent = stale.intent().clone();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    assert!(
        storage
            .prepare_discussion_parent_candidate(&access, request)
            .is_err()
    );
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command.add(stale.contribution()).unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    let DiscussionParentEligibility::Proven(proof) = fresh
        .prepare_discussion_parent_candidate(&access, request)
        .unwrap()
    else {
        panic!("archived proof");
    };
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(proof.into_archived_release().unwrap().contribution())
        .unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let store = recovery.publish().unwrap();
    assert_eq!(
        fresh.discussion_handoff_status(&store, &intent).unwrap(),
        DiscussionHandoffStatus::ExactNew
    );
    assert_eq!(
        fresh.thread(&store, id(210), limit()).unwrap(),
        parent_before
    );
    assert_eq!(
        fresh.input_gate(&store, id(212), limit()).unwrap(),
        child_input_before
    );
    assert_eq!(
        fresh
            .current_draft(&store, id(212), limit())
            .unwrap()
            .unwrap()
            .draft(),
        &child_draft_before
    );
    assert_eq!(
        fresh
            .thread_attributes(&store, id(212), limit())
            .unwrap()
            .unwrap()
            .archive(),
        ThreadArchiveState::BranchDiscussionOpen
    );
    let mut retry = child;
    retry.handoff_gate_revision = intent.new_gate().revision();
    retry.parent.thread_revision = fresh
        .thread(&store, id(210), limit())
        .unwrap()
        .unwrap()
        .revision();
    let parent_input = fresh.input_gate(&store, id(210), limit()).unwrap().unwrap();
    retry.parent.input_gate_revision = parent_input.revision();
    retry.parent.accepted_high_water = parent_input.accepted_high_water();
    let rejected = fresh
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(retry))
        .unwrap();
    assert!(matches!(
        store.execute(super::super::command(&store, rejected)),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        fresh
            .discussion_handoff_gate(&store, id(212), limit())
            .unwrap(),
        Some(intent.new_gate())
    );
    store.close().unwrap();
}
