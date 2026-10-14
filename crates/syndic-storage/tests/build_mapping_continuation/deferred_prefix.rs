use super::*;
use syndic_storage::test_faults::draft_build_mapping_snapshot;
use syndic_storage::{
    DraftPieceActiveMarkerPhaseV1, DraftPieceBuildFragmentV1, DraftPieceBuildRecordV1,
};

#[path = "deferred_prefix/failures.rs"]
mod failures;

fn after(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::AfterAll)
}

fn indexed_marker(index: u64, order: u64) -> DraftPieceMarkerV1 {
    DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes(u128::from(index).to_be_bytes()),
        order,
        ImageLabelOrdinal::new(index).unwrap(),
        beryl_model::AssetId::sha256_v1([91; 32], std::num::NonZeroU64::new(index).unwrap()),
    )
}

fn build_status(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
    fragments: &[DraftPieceBuildFragmentV1],
) -> DraftPieceBuildRecordV1 {
    let start = (fragments.len() - 1) / 256 * 256;
    match storage
        .draft_piece_operation_status_page(store, prepared, start as u64 + 1, &fragments[start..])
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(
            DraftPieceOperationStatusV1::Open(build) | DraftPieceOperationStatusV1::Complete(build),
        ) => build,
        other => panic!("unexpected deferred prefix status: {other:?}"),
    }
}

fn verify_staged_fragments(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
    fragments: &[DraftPieceBuildFragmentV1],
) {
    for (page, values) in fragments.chunks(256).enumerate() {
        match storage
            .draft_piece_operation_status_page(store, prepared, page as u64 * 256 + 1, values)
            .unwrap()
        {
            DraftPieceOperationVerificationV1::More { next_ordinal } => {
                assert_eq!(next_ordinal, (page as u64 + 1) * 256 + 1)
            }
            DraftPieceOperationVerificationV1::Status(
                DraftPieceOperationStatusV1::Open(_) | DraftPieceOperationStatusV1::Complete(_),
            ) => return,
            other => panic!("unexpected deferred prefix status: {other:?}"),
        }
    }
    panic!("fragment verification did not reach the selected build");
}

fn finish_build(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
    fragments: &[DraftPieceBuildFragmentV1],
) -> DraftPieceBuildRecordV1 {
    for step in 0..30_000 {
        let before = build_status(storage, store, prepared, fragments);
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                store,
                prepared.header().draft_id(),
                prepared.header().session_id(),
                prepared.header().operation_id(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "deferred command {step}, {:?}, {:?}: {error:?}",
                    before.frontier(),
                    draft_build_mapping_snapshot(&before)
                )
            })
        else {
            return before;
        };
        let measured = advance.clone();
        committed(execute(store, storage.advance_draft_piece_edit(advance)));
        let work = measured.bounded_work().unwrap();
        assert!(work.stored_structure_records() <= 256 && work.point_attempts() <= 512);
        assert!(work.encoded_bytes() <= 4_194_304 && work.peak_encoded_bytes() <= 4_194_304);
    }
    panic!("deferred continuation did not finish");
}

fn seed(
    count: u64,
) -> (
    TestHome,
    HomeStore,
    SyndicStorage,
    DraftEditorCandidateSessionV1,
    Vec<DraftPieceMarkerV1>,
) {
    let (home, store, storage, thread) = fixture("deferred-prefix-selection", 181);
    let mut session = seeded_text(&storage, &store, thread, "ABCDE", 184);
    let left = indexed_marker(1, count + 10);
    let right = indexed_marker(count + 2, count + 20);
    session = complete_checked(
        &storage,
        &store,
        &session,
        185,
        insert_at(point(1), 1, left),
        DraftLogicalExtentV1::new(5, 1),
    );
    let selected: Vec<_> = (0..count)
        .map(|index| indexed_marker(index + 2, index + 1))
        .collect();
    let replacements: Vec<_> = selected
        .iter()
        .map(|item| insert_at(point(2), 2, *item))
        .collect();
    let (prepared, fragments) = stage(&storage, &store, &session, 186, replacements, point(5));
    verify_staged_fragments(&storage, &store, &prepared, &fragments);
    finish_build(&storage, &store, &prepared, &fragments);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
    session = complete_checked(
        &storage,
        &store,
        &session,
        187,
        insert_at(point(4), 4, right),
        DraftLogicalExtentV1::new(5, 1),
    );
    (home, store, storage, session, selected)
}

fn deletion(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    selected: &[DraftPieceMarkerV1],
) -> (PreparedDraftPieceEditV1, Vec<DraftPieceBuildFragmentV1>) {
    let mut replacements = vec![DraftPieceReplacementV1::new(after(1), before(2), vec![])];
    for (index, item) in selected.iter().enumerate() {
        let position = if index == 0 {
            before(2)
        } else {
            let previous = selected[index - 1];
            DraftCompositePositionV1::new(
                2,
                DraftCompositeGapWitnessV1::Between {
                    left_order_key: previous.order_key(),
                    left_marker_id: previous.marker_id(),
                    right_order_key: item.order_key(),
                    right_marker_id: item.marker_id(),
                },
            )
        };
        let occurrence = storage
            .draft_marker_identity(store, session.newest_root(), item.marker_id())
            .unwrap()
            .unwrap();
        replacements.push(
            DraftPieceReplacementV1::new(position, position, vec![]).with_marker_effect(
                DraftPieceMarkerEffectV1::Remove {
                    removal: DraftPieceMarkerRemovalProofV1::new(position, occurrence),
                    charges: DraftPieceMarkerEffectChargesV1::for_marker(*item),
                },
            ),
        );
    }
    replacements.push(DraftPieceReplacementV1::new(after(2), before(4), vec![]));
    let (prepared, fragments) = stage(storage, store, session, 188, replacements, after(1));
    verify_staged_fragments(storage, store, &prepared, &fragments);
    (prepared, fragments)
}

#[test]
fn selected_same_anchor_run_removes_before_prefix_coalescence_with_unlimited_fragments() {
    for count in [1, 3, 257] {
        let (_home, store, storage, session, selected) = seed(count);
        let (prepared, fragments) = deletion(&storage, &store, &session, &selected);
        let build = finish_build(&storage, &store, &prepared, &fragments);
        assert_eq!(
            build
                .marker_effect_continuation()
                .scan()
                .completed_effect_count(),
            count
        );
        assert_eq!(build.working_roots().sequence_summary().marker_count(), 2);
        committed(execute(
            &store,
            storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
        ));
        let successor = active_session(&storage, &store, session.draft_id(), session.session_id())
            .newest_root();
        assert_eq!(read_text(&storage, &store, successor, 8), "AE");
        for item in selected {
            assert!(
                storage
                    .draft_marker_identity(&store, successor, item.marker_id())
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn deferred_prefix_reopens_each_closed_boundary_without_publishing_candidate() {
    let (home, mut store, mut storage, session, selected) = seed(3);
    let (prepared, fragments) = deletion(&storage, &store, &session, &selected);
    let mut saw_private_retirement = false;
    let mut saw_certified_prefix = false;
    for step in 0..512 {
        let before = build_status(&storage, &store, &prepared, &fragments);
        let scan = before.marker_effect_continuation().scan();
        if scan.next_fragment_ordinal() > 2
            && matches!(
                before.frontier(),
                DraftPieceBuildFrontierV1::Applying {
                    fragment_ordinal: 1,
                    ..
                }
            )
        {
            saw_private_retirement = true;
        }
        if before
            .marker_effect_continuation()
            .active()
            .is_some_and(|active| {
                active.phase() == DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap
            })
        {
            saw_certified_prefix = true;
        }
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                prepared.header().operation_id(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "reopen step {step}, {:?}: {error:?}",
                    draft_build_mapping_snapshot(&before)
                )
            })
        else {
            break;
        };
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        let after = build_status(&storage, &store, &prepared, &fragments);
        assert_eq!(
            active_session(&storage, &store, session.draft_id(), session.session_id())
                .newest_root(),
            session.newest_root()
        );
        drop(store);
        let mut candidate = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
            &home.0,
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        storage = SyndicStorage::register(&mut candidate).unwrap();
        store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(build_status(&storage, &store, &prepared, &fragments), after);
        assert_eq!(build_status(&storage, &store, &prepared, &fragments), after);
        assert!(step < 511);
    }
    assert!(saw_private_retirement && saw_certified_prefix);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    assert_eq!(
        read_text(
            &storage,
            &store,
            active_session(&storage, &store, session.draft_id(), session.session_id())
                .newest_root(),
            8
        ),
        "AE"
    );
}
