use super::*;

#[test]
fn nested_parent_replacement_and_queued_input_wait_without_consuming_draft() {
    let home = TestHome::new("parent-draft-and-queue");
    let (store, storage, _) = seeded(&home, FaultController::new());
    support::discussion_handoff::complete_resolving_turn(&store, &storage);
    create_child(&store, &storage, id(36), 230, 231);
    let child = support::discussion_handoff::active_request_for(
        &store,
        &storage,
        id(230),
        draft_id(232),
        SyndicItemId::from_bytes([233; 16]),
        ResolutionIntentId::from_bytes([234; 16]),
        JobId::from_bytes([235; 16]),
    );
    let request = admit_request(&store, &storage, child);
    assert_eq!(
        proven(&store, &storage, request).disposition(),
        DiscussionParentDisposition::Ready
    );
    let current = storage
        .current_draft(&store, id(36), limit())
        .unwrap()
        .unwrap();
    let draft = current.draft().clone();
    let head = storage
        .transcript_view_head(&store, id(36), limit())
        .unwrap()
        .unwrap();
    let entries = storage
        .transcript_entries(
            &store,
            id(36),
            head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == SyndicItemId::from_bytes([202; 16]))
        .unwrap();
    let replacement = DraftRecord::new(
        draft.id(),
        draft.thread_id(),
        draft.revision(),
        DraftSubmissionIntent::Replacement(ReplacementEditIntent::new(
            current.thread().committed_tail().unwrap(),
            current.thread().selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
        )),
        draft.root_history(),
        draft.created_at(),
        draft.updated_at(),
    );
    support::commit(
        &store,
        storage.clone(),
        support::batch([FixtureRecord::Draft(replacement.clone())]),
    );
    assert!(matches!(
        storage.prepare_discussion_parent(&store, request).unwrap(),
        DiscussionParentEligibility::Waiting
    ));
    assert_eq!(
        storage
            .current_draft(&store, id(36), limit())
            .unwrap()
            .unwrap()
            .draft(),
        &replacement
    );
    support::commit(
        &store,
        storage.clone(),
        support::batch([FixtureRecord::Draft(draft)]),
    );
    let turn = exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(36),
        draft_id(240),
        SyndicItemId::from_bytes([241; 16]),
        "ongoing parent work",
        timestamp(100),
    );
    support::discussion_input::accept_next(&store, &storage);
    let source = exact_cas::establish_turn(&store, storage.clone(), id(36), turn, timestamp(301));
    exact_cas::admit_event(
        &store,
        storage.clone(),
        id(36),
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(302),
    );
    exact_cas::correlate_user_item(
        &store,
        storage.clone(),
        id(36),
        turn,
        SyndicItemId::from_bytes([241; 16]),
        &source,
        timestamp(303),
    );
    exact_cas::admit_event(
        &store,
        storage.clone(),
        id(36),
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(304),
    );
    exact_cas::converge_and_release_terminal_history(&store, storage.clone(), id(36), turn);
    let gate = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    assert!(gate.live_next_turn_count() > 0);
    let draft = storage
        .current_draft(&store, id(36), limit())
        .unwrap()
        .unwrap()
        .draft()
        .clone();
    assert!(matches!(
        storage.prepare_discussion_parent(&store, request).unwrap(),
        DiscussionParentEligibility::Waiting
    ));
    assert_eq!(
        storage.input_gate(&store, id(36), limit()).unwrap(),
        Some(gate)
    );
    assert_eq!(
        storage
            .current_draft(&store, id(36), limit())
            .unwrap()
            .unwrap()
            .draft(),
        &draft
    );
    store.close().unwrap();
}
