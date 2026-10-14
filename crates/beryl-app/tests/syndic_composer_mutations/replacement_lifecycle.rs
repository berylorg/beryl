use super::*;
use beryl_home_store::test_faults::FaultPoint;

#[path = "staging_outcome_custody.rs"]
mod staging_outcome_custody;

struct SelectedRun {
    fixture: evidence_fixture::Fixture,
    host: SyndicComposerHost,
    binding: ComposerHostBinding,
    replacement: SourceRange,
    left: InlineObjectNeighbor,
    right: InlineObjectNeighbor,
    removals: [ObjectTarget; 2],
}

fn selected_run(name: &str, seed: u8) -> SelectedRun {
    let fixture = evidence_fixture::fixture(name, seed);
    let store = &fixture.store;
    let (mut host, base) = activated(
        fixture.storage.clone(),
        store,
        fixture.thread,
        seed.wrapping_add(1),
        seed.wrapping_add(2),
    );
    let text = commit_text(&mut host, store, base, 214, 0, 0, "ABCDE", 5, 1);
    let left = InlineObjectNeighbor::new(InlineObjectId::new(0xc101), InlineObjectOrder::new(9));
    let first = InlineObjectNeighbor::new(InlineObjectId::new(0xc102), InlineObjectOrder::new(2));
    let second = InlineObjectNeighbor::new(InlineObjectId::new(0xc103), InlineObjectOrder::new(3));
    let right = InlineObjectNeighbor::new(InlineObjectId::new(0xc104), InlineObjectOrder::new(12));
    let marked = insert_boundary(&mut host, store, text, 2141, 1, left, 1);
    let before = SourcePosition::new(ByteOffset::new(2), InlineObjectGap::before(first));
    let between = SourcePosition::new(
        ByteOffset::new(2),
        InlineObjectGap::between(first, second).unwrap(),
    );
    let after = SourcePosition::new(ByteOffset::new(2), InlineObjectGap::after(second));
    let marked = commit_items(
        &mut host,
        store,
        marked,
        2142,
        range(source_position(2), source_position(2)),
        vec![
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(first.id(), ByteOffset::new(2), first.order(), 17, 5),
            }),
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(
                    second.id(),
                    ByteOffset::new(2),
                    second.order(),
                    17,
                    5,
                ),
            }),
        ],
        MutationPositions::collapsed(after),
        vec![
            labeled_marker_metadata(
                first.id(),
                ImageLabelOrdinal::new(2).unwrap(),
                asset_id_for_object(first.id()),
            ),
            labeled_marker_metadata(
                second.id(),
                ImageLabelOrdinal::new(3).unwrap(),
                asset_id_for_object(second.id()),
            ),
        ],
        5,
        1,
    );
    let binding = insert_boundary(&mut host, store, marked, 2143, 4, right, 4);
    SelectedRun {
        fixture,
        host,
        binding,
        left,
        right,
        replacement: range(
            SourcePosition::new(ByteOffset::new(1), InlineObjectGap::after(left)),
            SourcePosition::new(ByteOffset::new(4), InlineObjectGap::before(right)),
        ),
        removals: [
            ObjectTarget::new(range(before, between), first.id(), first.order()).unwrap(),
            ObjectTarget::new(range(between, after), second.id(), second.order()).unwrap(),
        ],
    }
}

fn admitted(
    run: &mut SelectedRun,
    inserted: &str,
) -> (MutationPass, MutationPage, MutationFinishInput) {
    let (mut editor, evidence, page, finish) = prepare_with_removals(
        &mut run.host,
        &run.fixture.store,
        run.binding,
        &run.fixture.assets,
        run.replacement,
        inserted,
        &run.removals,
    );
    assert!(
        matches!(drive(&mut run.host, &run.fixture.store, run.binding, &run.fixture.assets,
        ComposerHostMutationEvidenceRequest::Finish { pass: evidence, finish }),
        ComposerHostMutationEvidenceOutcome::Began(actual) if actual == finish.key())
    );
    editor.accept_preflight(finish.key(), None).unwrap();
    let staging = editor.mutation_restart(finish.key()).unwrap();
    editor.acknowledge_restart(staging).unwrap();
    (staging, page, finish)
}

fn stage_sources(run: &mut SelectedRun, pass: MutationPass, lose_completion: bool) {
    let source = MutationPage::new(
        MutationPageKey::new(
            pass.key(),
            MutationLane::Source,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        run.removals
            .iter()
            .map(|target| MutationPageItem::Object(ObjectChange::Remove { target: *target }))
            .collect(),
    )
    .unwrap();
    let request = MutationPageRequest::new(source).with_pass(pass);
    if lose_completion {
        run.fixture
            .faults
            .fail_next(FaultPoint::AfterCommitBeforePersist);
    }
    assert!(matches!(
        run.host
            .stage_mutation_page(&run.fixture.store, request.clone(), Box::new([]))
            .unwrap(),
        MutationPageAcceptance::Accepted { .. }
    ));
    assert_eq!(request.page().payload_owner_count(), 1);
    assert_eq!(
        run.host
            .stage_mutation_page(&run.fixture.store, request, Box::new([]))
            .unwrap(),
        MutationPageAcceptance::Replay
    );
}

fn stage_proposal(
    run: &mut SelectedRun,
    pass: MutationPass,
    page: MutationPage,
    lose_completion: bool,
) {
    let request = MutationPageRequest::new(page).with_pass(pass);
    if lose_completion {
        run.fixture
            .faults
            .fail_next(FaultPoint::AfterCommitBeforePersist);
    }
    assert!(matches!(
        run.host
            .stage_mutation_page(&run.fixture.store, request.clone(), Box::new([]))
            .unwrap(),
        MutationPageAcceptance::Accepted { .. }
    ));
    assert_eq!(request.page().payload_owner_count(), 1);
    assert_eq!(
        run.host
            .stage_mutation_page(&run.fixture.store, request, Box::new([]))
            .unwrap(),
        MutationPageAcceptance::Replay
    );
}

fn assert_unchanged(run: &SelectedRun, settled: bool) {
    let binding = run.host.binding().unwrap();
    assert_eq!(binding.root(), run.binding.root());
    assert_eq!(binding.history(), run.binding.history());
    assert_eq!(binding.range_binding(), run.binding.range_binding());
    let session = match run
        .fixture
        .storage
        .draft_editor_candidate_session(
            &run.fixture.store,
            run.binding.candidate().draft_id(),
            run.binding.candidate().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("mapped selected run lost its candidate session: {other:?}"),
    };
    let current = syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&session);
    assert_eq!(current.draft_id(), binding.candidate().draft_id());
    assert_eq!(current.session_id(), binding.candidate().session_id());
    assert_eq!(
        current.candidate_generation(),
        binding.candidate().candidate_generation()
    );
    assert_eq!(current.root(), binding.root());
    assert_eq!(current.history(), binding.history());
    assert_eq!(
        current.logical_extent(),
        binding.candidate().logical_extent()
    );
    assert!(current.session_generation() > run.binding.candidate().session_generation());
    if settled {
        assert_eq!(binding.candidate(), current);
        assert!(session.active_operation().is_none());
    } else {
        assert!(session.active_operation().is_some());
    }
    let page = run
        .fixture
        .storage
        .candidate_draft_piece_text_demand(
            &run.fixture.store,
            current,
            DraftPieceTextDemandV1::Forward(0),
            65_536,
        )
        .unwrap();
    assert_eq!(page.value().bytes(), b"ABCDE");
    assert_eq!(
        page.value().following(),
        syndic_storage::DraftPieceTextEdgeFactV1::DocumentEnd
    );
    assert_eq!(binding.root().summary().marker_count(), 4);
    for (object, anchor, label) in [
        (run.left, 1, 1),
        (run.right, 4, 4),
        (
            InlineObjectNeighbor::new(run.removals[0].id(), run.removals[0].order()),
            2,
            2,
        ),
        (
            InlineObjectNeighbor::new(run.removals[1].id(), run.removals[1].order()),
            2,
            3,
        ),
    ] {
        assert_marker(
            run.fixture.storage.clone(),
            &run.fixture.store,
            binding,
            SyndicDraftMarkerId::from_bytes(object.id().get().to_be_bytes()),
            anchor,
            object.order().get() as u64,
            ImageLabelOrdinal::new(label).unwrap(),
            asset_id_for_object(object.id()),
        );
    }
}

fn cancel(run: &mut SelectedRun, key: MutationKey) {
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    for _ in 0..64 {
        match run.host.execute_mutation(
            &run.fixture.store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation,
        ) {
            Ok(ComposerHostMutationOutcome::Cancelled) => {
                assert_eq!(run.host.mutation_status(), None);
                assert_eq!(run.host.test_mutation_in_flight_custody(), None);
                assert_unchanged(run, true);
                return;
            }
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("mapped selected run did not cancel exactly: {other:?}"),
        }
    }
    panic!("mapped selected run exceeded bounded cancellation drives")
}

#[test]
fn selected_same_anchor_run_preserves_surviving_keys_through_empty_ascii_and_unicode_replacement() {
    for inserted in ["", "Q", "é"] {
        let mut run = selected_run("mapped-selected-run", 81);
        let (pass, page, finish) = admitted(&mut run, inserted);
        stage_sources(&mut run, pass, true);
        stage_proposal(&mut run, pass, page, true);
        run.fixture
            .faults
            .fail_next(FaultPoint::AfterCommitBeforePersist);
        run.host
            .finish_mutation_input(&run.fixture.store, finish)
            .unwrap();
        run.fixture
            .faults
            .fail_times(FaultPoint::AfterCommitBeforePersist, 16);
        let (committed, positions) = commit_mapped(&mut run.host, &run.fixture.store, finish.key());
        assert_eq!(
            candidate_text(run.fixture.storage.clone(), &run.fixture.store, committed),
            format!("A{inserted}E").as_bytes()
        );
        assert_eq!(committed.root().summary().marker_count(), 2);
        let collapse = 1 + inserted.len() as u64;
        let gap = if inserted.is_empty() {
            InlineObjectGap::between(run.left, run.right).unwrap()
        } else {
            InlineObjectGap::before(run.right)
        };
        assert_eq!(
            positions,
            MutationPositions::collapsed(SourcePosition::new(ByteOffset::new(collapse), gap))
        );
        for (object, anchor, label) in [(run.left, 1, 1), (run.right, collapse, 4)] {
            assert_marker(
                run.fixture.storage.clone(),
                &run.fixture.store,
                committed,
                SyndicDraftMarkerId::from_bytes(object.id().get().to_be_bytes()),
                anchor,
                object.order().get() as u64,
                ImageLabelOrdinal::new(label).unwrap(),
                asset_id_for_object(object.id()),
            );
        }
        assert_eq!(run.host.mutation_status(), None);
        assert_eq!(run.host.test_mutation_in_flight_custody(), None);
    }
}

#[test]
fn mapped_selected_run_cancellation_releases_prepared_proposal_page_custody() {
    let mut run = selected_run("mapped-selected-run-prepared-cancel", 91);
    let (pass, page, finish) = admitted(&mut run, "é");
    stage_sources(&mut run, pass, false);
    let request = MutationPageRequest::new(page).with_pass(pass);
    run.host.test_set_mutation_transition_limit(1);
    run.host
        .test_arm_mutation_before_execute_fault(|store, storage| {
            support::bump_home_revision(storage.clone(), store, 119)
        });
    assert!(matches!(
        run.host
            .stage_mutation_page(&run.fixture.store, request.clone(), Box::new([])),
        Err(ComposerHostError::MutationWorkPending)
    ));
    let custody = run.host.test_mutation_in_flight_custody().unwrap();
    assert!(!custody.1);
    assert_eq!(request.page().payload_owner_count(), 2);
    assert_unchanged(&run, false);
    run.host.test_set_mutation_transition_limit(4096);
    cancel(&mut run, finish.key());
    assert_eq!(request.page().payload_owner_count(), 1);
}

#[test]
fn mapped_selected_run_cancellation_while_building_preserves_source_and_history() {
    let mut run = selected_run("mapped-selected-run-building-cancel", 101);
    let (pass, page, finish) = admitted(&mut run, "");
    stage_sources(&mut run, pass, false);
    stage_proposal(&mut run, pass, page, false);
    run.host
        .finish_mutation_input(&run.fixture.store, finish)
        .unwrap();
    run.host.test_set_mutation_transition_limit(1);
    let mut building = false;
    for _ in 0..32 {
        assert!(matches!(
            run.host.execute_mutation(
                &run.fixture.store,
                MutationCommitRequest::new(finish.key(), MutationIdentity::ROOT),
                &CommandCancellation::new()
            ),
            Err(ComposerHostError::MutationWorkPending)
        ));
        if matches!(
            run.fixture
                .storage
                .draft_mutation_staging_status(
                    &run.fixture.store,
                    staging_identity(run.binding, finish.key().operation().get())
                )
                .unwrap(),
            DraftMutationStagingStatusV1::Building { .. }
        ) {
            building = true;
            break;
        }
    }
    assert!(building);
    assert_unchanged(&run, false);
    run.host.test_set_mutation_transition_limit(4096);
    run.fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    cancel(&mut run, finish.key());
}
