use super::*;

#[test]
fn restart_preserves_candidate_and_history_but_fresh_activation_uses_current_selector() {
    let (home, store, storage, thread) = fixture("phase153-restart", 91);
    let current_before = current(storage, &store, thread);
    let (mut host, base) = activated(storage, &store, thread, 92, 93);
    let candidate = commit_text(&mut host, &store, base, 94, 0, 0, "candidate", 9, 1);
    assert_ne!(candidate.root(), base.root());
    assert_ne!(candidate.history(), base.history());
    assert_eq!(candidate_text(storage, &store, candidate), b"candidate");
    drop(host);
    drop(store);

    let (store, storage) = reopen(&home);
    assert_eq!(current(storage, &store, thread), current_before);
    assert_eq!(candidate_text(storage, &store, candidate), b"candidate");
    let (fresh, rebound) = activated(storage, &store, thread, 95, 96);
    assert_eq!(rebound.root(), current_before.draft().piece_root());
    assert_ne!(rebound.root(), candidate.root());
    assert_eq!(fresh.binding(), Some(rebound));
}

#[test]
fn detached_late_commit_is_stale_conflict_and_cannot_block_or_adopt_into_fresh_binding() {
    let (_home, store, storage, thread) = fixture("phase155-detached-commit", 97);
    let (mut host, old_binding) = activated(storage, &store, thread, 98, 99);
    let (old_key, old_finish) = stage_text(&mut host, &store, old_binding, 7, 0, 0, "old", 3, 1);
    host.finish_mutation_input(&store, old_finish).unwrap();
    assert!(host.release().unwrap());

    let fresh = reactivate(&mut host, &store, thread, 100, 101);
    let fresh = commit_text(&mut host, &store, fresh, 7, 0, 0, "fresh", 5, 1);
    assert_eq!(host.binding(), Some(fresh));
    assert_eq!(candidate_text(storage, &store, fresh), b"fresh");

    assert_eq!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(old_key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostMutationOutcome::Conflict
    );
    assert_eq!(host.binding(), Some(fresh));
    assert_eq!(candidate_text(storage, &store, fresh), b"fresh");
    assert!(matches!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(old_key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ),
        Err(ComposerHostError::MutationNotPending)
    ));

    let fresh = commit_text(&mut host, &store, fresh, 1, 5, 5, "!", 6, 1);
    assert_eq!(candidate_text(storage, &store, fresh), b"fresh!");
}

#[test]
fn detached_late_terminal_conflict_cannot_make_fresh_binding_unavailable() {
    let (_home, store, storage, thread) = fixture("phase155-detached-conflict", 102);
    let (mut host, old_binding) = activated(storage, &store, thread, 103, 104);
    let session = match storage
        .draft_editor_candidate_session(
            &store,
            old_binding.candidate().draft_id(),
            old_binding.candidate().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("candidate session was not active: {other:?}"),
    };
    let advance = transaction_for_session(
        storage,
        &store,
        session,
        105,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("advanced".into())],
        )],
        point(0),
    );
    run_transaction(storage, &store, &advance, 2);
    let old_key = mutation_key(old_binding, 8);
    let zero = source_position(0);
    host.begin_mutation(
        &store,
        old_binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                old_key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    assert!(host.release().unwrap());

    let fresh = reactivate(&mut host, &store, thread, 106, 107);
    let fresh = commit_text(&mut host, &store, fresh, 8, 0, 0, "fresh", 5, 1);
    assert_eq!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(old_key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostMutationOutcome::Conflict
    );
    assert_eq!(host.binding(), Some(fresh));
    let fresh = commit_text(&mut host, &store, fresh, 1, 5, 5, "!", 6, 1);
    assert_eq!(candidate_text(storage, &store, fresh), b"fresh!");
}

#[test]
fn detached_late_cancellation_is_stale_conflict_and_leaves_fresh_slot_usable() {
    let (_home, store, storage, thread) = fixture("phase155-detached-cancel", 108);
    let (mut host, old_binding) = activated(storage, &store, thread, 109, 110);
    let old_key = mutation_key(old_binding, 9);
    let zero = source_position(0);
    host.begin_mutation(
        &store,
        old_binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                old_key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    assert!(host.release().unwrap());
    let fresh = reactivate(&mut host, &store, thread, 111, 112);
    let fresh = commit_text(&mut host, &store, fresh, 9, 0, 0, "fresh", 5, 1);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(old_key, MutationIdentity::ROOT),
            &cancellation,
        )
        .unwrap(),
        ComposerHostMutationOutcome::Conflict
    );
    assert_eq!(host.binding(), Some(fresh));
    let fresh = commit_text(&mut host, &store, fresh, 1, 5, 5, "!", 6, 1);
    assert_eq!(candidate_text(storage, &store, fresh), b"fresh!");
}

#[cfg(feature = "test-faults")]
#[test]
fn indeterminate_batch_finish_and_build_commands_reconcile_exact_target() {
    use beryl_home_store::test_faults::FaultPoint;
    use support::fault_fixture;

    let (_home, store, storage, thread, faults) = fault_fixture("phase153-faults", 101);
    let (mut host, base) = activated(storage, &store, thread, 102, 103);
    let key = mutation_key(base, 104);
    let zero = source_position(0);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    host.begin_mutation(
        &store,
        base,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "fault-cut".into(),
        }],
    )
    .unwrap();
    let finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    host.stage_mutation_page(&store, MutationPageRequest::new(page), Box::new([]))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    host.finish_mutation_input(&store, finish_input(key, empty_finish(), finish, 9, 1))
        .unwrap();
    faults.fail_times(FaultPoint::AfterCommitBeforePersist, 16);
    let binding = commit(&mut host, &store, key);
    assert_eq!(candidate_text(storage, &store, binding), b"fault-cut");
    assert_eq!(host.binding(), Some(binding));
}

#[cfg(feature = "test-faults")]
#[test]
fn indeterminate_pre_build_cancellation_reconciles_exact_cancelled_terminal() {
    use beryl_home_store::test_faults::FaultPoint;
    use support::fault_fixture;

    let (_home, store, storage, thread, faults) = fault_fixture("phase153-cancel-fault", 111);
    let current_before = current(storage, &store, thread);
    let (mut host, base) = activated(storage, &store, thread, 112, 113);
    let (key, finish) = stage_text(&mut host, &store, base, 114, 0, 0, "x", 1, 1);
    host.finish_mutation_input(&store, finish).unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert_eq!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation,
        )
        .unwrap(),
        ComposerHostMutationOutcome::Cancelled
    );
    assert_eq!(current(storage, &store, thread), current_before);
}

#[cfg(feature = "test-faults")]
#[test]
fn source_selected_page_retains_exact_payload_and_pre_admission_cancel_releases_it() {
    let (_home, store, storage, thread) = fixture("phase155-source-selected-page", 115);
    let (mut host, base) = activated(storage, &store, thread, 116, 117);
    let key = mutation_key(base, 118);
    let zero = source_position(0);
    host.begin_mutation(
        &store,
        base,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "retained".into(),
        }],
    )
    .unwrap();
    let payload = page.clone();
    host.test_set_mutation_transition_limit(1);
    host.test_arm_mutation_before_execute_fault(move |store, storage| {
        let _ = thread;
        support::bump_home_revision(storage, store, 119);
    });
    let source_result =
        host.stage_mutation_page(&store, MutationPageRequest::new(page), Box::new([]));
    assert!(
        matches!(source_result, Err(ComposerHostError::MutationWorkPending)),
        "source-selected page result was {source_result:?}"
    );
    host.test_set_mutation_transition_limit(4096);
    assert_eq!(payload.payload_owner_count(), 2);
    assert_eq!(
        storage
            .draft_mutation_staging_head(&store, staging_identity(base, 118))
            .unwrap()
            .unwrap()
            .proposal()
            .next_cursor(),
        0
    );
    let differing = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "different".into(),
        }],
    )
    .unwrap();
    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(differing), Box::new([]),),
        Err(ComposerHostError::MutationPending)
    ));
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation,
        )
        .unwrap(),
        ComposerHostMutationOutcome::Cancelled
    );
    assert_eq!(payload.payload_owner_count(), 1);
}

#[test]
fn valid_empty_edit_settles_rejected_without_candidate_adoption() {
    let (_home, store, storage, thread) = fixture("phase153-rejected", 121);
    let current_before = current(storage, &store, thread);
    let (mut host, base) = activated(storage, &store, thread, 122, 123);
    let key = mutation_key(base, 124);
    let zero = source_position(0);
    host.begin_mutation(
        &store,
        base,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![
            MutationPageItem::Utf8 {
                inserted_offset: 0,
                text: "".into(),
            },
            MutationPageItem::Utf8 {
                inserted_offset: 0,
                text: "".into(),
            },
        ],
    )
    .unwrap();
    let finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    host.stage_mutation_page(&store, MutationPageRequest::new(page), Box::new([]))
        .unwrap();
    host.finish_mutation_input(&store, finish_input(key, empty_finish(), finish, 0, 0))
        .unwrap();
    let outcome = loop {
        match host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Err(ComposerHostError::MutationWorkPending) => continue,
            result => break result.unwrap(),
        }
    };
    assert_eq!(outcome, ComposerHostMutationOutcome::Rejected);
    assert_eq!(host.binding(), Some(base));
    assert_eq!(current(storage, &store, thread), current_before);
}

#[test]
fn valid_edit_with_insufficient_history_capacity_settles_error_without_adoption() {
    let (_home, store, storage, thread) =
        support::fixture_with_history_budget("phase153-error", 131, 1_390);
    let current_before = current(storage, &store, thread);
    let (mut host, base) = activated(storage, &store, thread, 132, 133);
    let (key, finish) = stage_text(&mut host, &store, base, 134, 0, 0, "x", 1, 1);
    host.finish_mutation_input(&store, finish).unwrap();
    let outcome = loop {
        match host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Err(ComposerHostError::MutationWorkPending) => continue,
            result => break result.unwrap(),
        }
    };
    assert_eq!(outcome, ComposerHostMutationOutcome::Error);
    assert_eq!(host.binding(), Some(base));
    assert_eq!(current(storage, &store, thread), current_before);
}

#[test]
fn released_finished_cancellation_settles_as_stale_conflict_without_adoption() {
    let (_home, store, storage, thread) = fixture("phase149-cancel", 61);
    let (mut host, base) = activated(storage, &store, thread, 62, 63);
    let (key, finish) = stage_text(&mut host, &store, base, 64, 0, 0, "cancel", 6, 1);
    host.finish_mutation_input(&store, finish).unwrap();
    assert!(host.release().unwrap());
    assert_eq!(host.binding(), None);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation
        )
        .unwrap(),
        ComposerHostMutationOutcome::Conflict
    );
    assert_eq!(host.mutation_status(), None);
    let fresh = activated(storage, &store, thread, 65, 66).1;
    assert_eq!(fresh.root(), base.root());
}
