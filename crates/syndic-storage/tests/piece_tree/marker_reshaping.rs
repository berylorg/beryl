use super::*;
use syndic_storage::{DraftPieceBuildRecordV1, DraftPieceBuildRootsV1};

fn seeded_marker_tree(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    pieces: usize,
    anchors: &[u64],
) -> Vec<(u64, DraftPieceMarkerV1)> {
    let seed = transaction(
        storage,
        store,
        &current(storage, store, thread),
        31,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("ab".into()); pieces],
        )],
        point(0),
    );
    run_transaction(storage, store, &seed, 32);
    anchors
        .iter()
        .enumerate()
        .map(|(index, &anchor)| {
            let ordinal = u8::try_from(index).unwrap();
            let marker = DraftPieceMarkerV1::new(
                SyndicDraftMarkerId::from_bytes([33 + ordinal; 16]),
                u64::from(ordinal) + 1,
                ImageLabelOrdinal::new(u64::from(ordinal) + 1).unwrap(),
                AssetId::sha256_v1([33 + ordinal; 32], std::num::NonZeroU64::new(1).unwrap()),
            );
            let edit = marker_transaction(
                storage,
                store,
                &current(storage, store, thread),
                34 + ordinal,
                point(anchor),
                point(0),
                marker,
            );
            run_marker_transaction(storage, store, &edit);
            (anchor, marker)
        })
        .collect()
}

fn assert_marker_authority(before: DraftPieceBuildRootsV1, after: DraftPieceBuildRootsV1) {
    assert_eq!(before.marker_index_root(), after.marker_index_root());
    assert_eq!(before.marker_index_summary(), after.marker_index_summary());
    assert_eq!(before.marker_order_root(), after.marker_order_root());
    assert_eq!(before.marker_order_height(), after.marker_order_height());
    assert_eq!(before.marker_commitment(), after.marker_commitment());
    assert_eq!(
        before.sequence_summary().marker_count(),
        after.sequence_summary().marker_count()
    );
}

fn advance_until_reshaped(
    storage: &SyndicStorage,
    store: &HomeStore,
    edit: &Transaction,
) -> DraftPieceBuildRecordV1 {
    for step in 0..2048 {
        let before = open_build(storage, store, edit);
        let advance = storage
            .prepare_draft_piece_build_advance(
                store,
                edit.prepared.header().draft_id(),
                edit.session,
                edit.operation,
            )
            .unwrap_or_else(|error| panic!("reshape step {step}: {error:?}"))
            .expect("the edit must reshape its marker-bearing sequence");
        assert!(advance.staged_record_count() <= 256);
        committed(execute(store, storage.advance_draft_piece_edit(advance)));
        let after = open_build(storage, store, edit);
        assert_marker_authority(before.working_roots(), after.working_roots());
        if before.working_roots().sequence_summary().marker_digest()
            != after.working_roots().sequence_summary().marker_digest()
        {
            return after;
        }
    }
    panic!("sequence reshaping exceeded the bounded test command count")
}

fn finish_and_check(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    edit: &Transaction,
    markers: &[(u64, DraftPieceMarkerV1)],
    expected: &str,
) {
    let roots = open_build(storage, store, edit).working_roots();
    advance_until_complete(storage, store, edit);
    assert_marker_authority(roots, open_build(storage, store, edit).working_roots());
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), edit.prepared.clone()),
    ));
    remember_settled_transaction(storage, store, edit);
    let root = current(storage, store, thread).draft().piece_root();
    assert_eq!(
        storage
            .draft_piece_text_demand(store, root, DraftPieceTextDemandV1::Forward(0), 1024,)
            .unwrap()
            .bytes(),
        expected.as_bytes()
    );
    for &(anchor, marker) in markers {
        assert!(
            storage
                .validate_draft_marker_location(
                    store,
                    root,
                    DraftPieceMarkerAtV1::new(anchor, marker),
                )
                .unwrap()
        );
        assert_eq!(
            storage
                .draft_marker_identity(store, root, marker.marker_id())
                .unwrap(),
            storage
                .draft_marker_identity(
                    store,
                    edit.prepared.header().predecessor_root(),
                    marker.marker_id(),
                )
                .unwrap(),
        );
    }
}

fn reopen(home: &TestHome) -> (HomeStore, SyndicStorage) {
    let mut candidate = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    (store, storage)
}

#[test]
fn text_insertion_preserves_markers_across_root_and_descendant_splits() {
    for (pieces, inner) in [(127, false), (190, false), (190, true)] {
        let (home, store, storage, thread) = fixture("marker-insertion-reshape", 30);
        let length = (pieces * 2) as u64;
        let anchors = if pieces == 127 {
            vec![length - 2]
        } else {
            vec![130, length - 2]
        };
        let markers = seeded_marker_tree(&store, &storage, thread, pieces, &anchors);
        let anchor = length - u64::from(inner);
        let edit = transaction(
            &storage,
            &store,
            &current(&storage, &store, thread),
            40,
            vec![DraftPieceReplacementV1::new(
                point(anchor),
                point(anchor),
                vec![DraftPieceV1::Text("X\né".into())],
            )],
            point(0),
        );
        begin_and_stage(&storage, &store, &edit);
        let persisted = advance_until_reshaped(&storage, &store, &edit);
        drop(store);
        let (store, storage) = reopen(&home);
        assert_eq!(open_build(&storage, &store, &edit), persisted);
        let mut expected = "ab".repeat(pieces);
        expected.insert_str(anchor as usize, "X\né");
        finish_and_check(&storage, &store, thread, &edit, &markers, &expected);
    }
}

#[test]
fn text_removal_reshaping_resumes_after_physical_reopen() {
    let (home, store, storage, thread) = fixture("marker-removal-reshape-reopen", 30);
    let markers = seeded_marker_tree(&store, &storage, thread, 129, &[2]);
    let edit = transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        35,
        vec![DraftPieceReplacementV1::new(point(4), point(258), vec![])],
        point(0),
    );
    begin_and_stage(&storage, &store, &edit);
    let persisted = advance_until_reshaped(&storage, &store, &edit);
    drop(store);
    let (store, storage) = reopen(&home);
    assert_eq!(open_build(&storage, &store, &edit), persisted);
    finish_and_check(&storage, &store, thread, &edit, &markers, "abab");
}

#[test]
fn reshaped_text_progress_rejects_substituted_sequence_and_marker_roots() {
    for corruption in [
        DraftPieceProgressRootCorruption::PublishedSequence,
        DraftPieceProgressRootCorruption::PublishedMarkerIndex,
        DraftPieceProgressRootCorruption::PublishedMarkerOrder,
    ] {
        let (_home, store, storage, thread) = fixture("marker-reshape-corruption", 30);
        seeded_marker_tree(&store, &storage, thread, 127, &[252]);
        let base = current(&storage, &store, thread);
        let edit = transaction(
            &storage,
            &store,
            &base,
            35,
            vec![DraftPieceReplacementV1::new(
                point(254),
                point(254),
                vec![DraftPieceV1::Text("X".into())],
            )],
            point(0),
        );
        begin_and_stage(&storage, &store, &edit);
        advance_until_reshaped(&storage, &store, &edit);
        committed(execute(
            &store,
            inject_draft_piece_progress_root_corruption(
                &store,
                &storage,
                DraftPieceSettlementKeyV1::new(base.draft().id(), edit.session, edit.operation),
                corruption,
            ),
        ));
        let revision = store.home_revision().unwrap();
        let error = storage
            .prepare_draft_piece_build_advance(
                &store,
                base.draft().id(),
                edit.session,
                edit.operation,
            )
            .err()
            .expect("corrupted root must refuse advancement");
        assert!(
            matches!(
                error,
                DraftPiecePrepareErrorV1::InvalidRoot | DraftPiecePrepareErrorV1::Read(_)
            ),
            "{corruption:?}: {error:?}"
        );
        if store.health().state() == HomeHealthState::Healthy {
            assert_eq!(store.home_revision().unwrap(), revision);
        } else {
            assert_eq!(store.health().state(), HomeHealthState::Failed);
        }
    }
}
