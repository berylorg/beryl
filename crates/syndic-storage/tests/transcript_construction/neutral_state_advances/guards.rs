use super::*;

#[test]
fn refused_live_state_guard_preserves_active_build_and_all_state() {
    let f = fixture(true);
    let source = establish_turn(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        timestamp(9),
    );
    let before = stage(&f, BuildStage::Publishing);
    let head = f
        .storage
        .transcript_view_head(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    let state = f
        .storage
        .turn_state(&f.store, f.pending.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = f
        .storage
        .input_gate(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    let request = LiveSourceEvent::new(
        f.thread,
        f.pending.turn,
        state.revision().checked_next().unwrap(),
        gate.revision(),
        SourceEventSequence::new(1).unwrap(),
        Some(source),
        SourceEventPayload::TurnActivated,
        timestamp(10),
    )
    .unwrap();
    let activity = support::enroll_fixture_activity(&f.store, &f.storage, f.thread, f.pending.turn);
    let home_revision = f.store.home_revision().unwrap();
    let outcome = f
        .store
        .execute_current(f.storage.current_admit_live_source_event(request, activity));
    assert!(matches!(&outcome, CommandOutcome::NotCommitted { evidence }
        if matches!(typed_error(evidence), SyndicMutationError::TurnStateRevisionConflict { .. })));
    assert_eq!(f.store.home_revision().unwrap(), home_revision);
    assert_eq!(
        f.storage
            .transcript_view_head(&f.store, f.thread, point_limit())
            .unwrap()
            .unwrap(),
        head
    );
    assert_eq!(
        f.storage
            .transcript_build(&f.store, f.thread, before.generation(), point_limit())
            .unwrap()
            .unwrap(),
        before
    );
    assert_eq!(
        f.storage
            .turn_state(&f.store, f.pending.turn, point_limit())
            .unwrap()
            .unwrap(),
        state
    );
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    f.store.close().unwrap();
}

#[test]
fn neutral_state_advance_leaves_an_unrelated_selected_build_unchanged() {
    let f = fixture(false);
    let source = establish_turn(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        timestamp(9),
    );
    let other = id(40);
    execute(
        &f.store,
        f.storage.create_thread(
            f.storage.revision(&f.store).unwrap(),
            CreateThread::ordinary(
                other,
                draft_id(41),
                support::exact_cas::execution_binding(),
                timestamp(20),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    let other_turn = submit_text(
        &f.store,
        f.storage.clone(),
        other,
        "unrelated selected history",
        draft_id(42),
        SyndicItemId::from_bytes([43; 16]),
        timestamp(21),
    );
    project_item(&f.store, f.storage.clone(), other_turn.item);
    let (generation, _) = start_transcript_build(&f.store, f.storage.clone(), other);
    let other_build = advance_transcript_build(&f.store, f.storage.clone(), other, generation);
    let other_head = f
        .storage
        .transcript_view_head(&f.store, other, point_limit())
        .unwrap()
        .unwrap();
    let other_paths = path_turns(&f.store, f.storage.clone(), other, generation);
    admit(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(22),
    );
    assert_eq!(
        f.storage
            .transcript_view_head(&f.store, other, point_limit())
            .unwrap()
            .unwrap(),
        other_head
    );
    assert_eq!(
        f.storage
            .transcript_build(&f.store, other, generation, point_limit())
            .unwrap()
            .unwrap(),
        other_build
    );
    assert_eq!(
        path_turns(&f.store, f.storage.clone(), other, generation),
        other_paths
    );
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    f.store.close().unwrap();
}

#[test]
fn exhausted_transcript_revision_refuses_neutral_state_advance_atomically() {
    use syndic_storage::test_faults::FixtureRecord;
    let f = fixture(true);
    let source = establish_turn(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        timestamp(9),
    );
    let before = stage(&f, BuildStage::Publishing);
    let prior_head = f
        .storage
        .transcript_view_head(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    let max_revision = beryl_model::ProjectionRevision::new(u64::MAX).unwrap();
    let build = TranscriptBuildRecord::new(
        before.thread_id(),
        before.generation(),
        max_revision,
        before.source_thread_revision(),
        before.committed_tail(),
        before.selected_path_digest(),
        before.path_turn_count(),
        before.entry_count(),
        before.entry_digest(),
        before.history_complete(),
        before.phase(),
    );
    let head = syndic_storage::TranscriptViewHeadRecord::new(
        prior_head.thread_id(),
        prior_head.generation(),
        max_revision,
        prior_head.entry_count(),
        prior_head.committed_tail(),
        prior_head.selected_path_digest(),
        prior_head.lifecycle(),
    );
    support::commit(
        &f.store,
        f.storage.clone(),
        support::batch([
            FixtureRecord::TranscriptBuild(build),
            FixtureRecord::TranscriptViewHead(head.clone()),
        ]),
    );
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let state = f
        .storage
        .turn_state(&f.store, f.pending.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = f
        .storage
        .input_gate(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    let binding = f
        .storage
        .current_binding(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    let paths = path_turns(&f.store, f.storage.clone(), f.thread, before.generation());
    let entries = transcript_entries(&f.store, f.storage.clone(), f.thread, before.generation());
    let activity = support::enroll_fixture_activity(&f.store, &f.storage, f.thread, f.pending.turn);
    let revision = f.store.home_revision().unwrap();
    let event = LiveSourceEvent::new(
        f.thread,
        f.pending.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(1).unwrap(),
        Some(source),
        SourceEventPayload::TurnActivated,
        timestamp(10),
    )
    .unwrap();
    let outcome = f
        .store
        .execute_current(f.storage.current_admit_live_source_event(event, activity));
    assert!(matches!(&outcome, CommandOutcome::NotCommitted { evidence }
        if matches!(typed_error(evidence), SyndicMutationError::Revision(_))));
    assert_eq!(f.store.home_revision().unwrap(), revision);
    assert_eq!(
        f.storage
            .turn_state(&f.store, f.pending.turn, point_limit())
            .unwrap()
            .unwrap(),
        state
    );
    assert_eq!(
        f.storage
            .input_gate(&f.store, f.thread, point_limit())
            .unwrap()
            .unwrap(),
        gate
    );
    assert_eq!(
        f.storage
            .current_binding(&f.store, f.thread, point_limit())
            .unwrap()
            .unwrap(),
        binding
    );
    assert_eq!(
        f.storage
            .transcript_build(&f.store, f.thread, before.generation(), point_limit())
            .unwrap()
            .unwrap(),
        build
    );
    assert_eq!(
        f.storage
            .transcript_view_head(&f.store, f.thread, point_limit())
            .unwrap()
            .unwrap(),
        head
    );
    assert_eq!(
        path_turns(&f.store, f.storage.clone(), f.thread, before.generation()),
        paths
    );
    assert_eq!(
        transcript_entries(&f.store, f.storage.clone(), f.thread, before.generation()),
        entries
    );
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    f.store.close().unwrap();
}
