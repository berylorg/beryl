use super::*;
use syndic_storage::test_faults::sealed_import_eof_corruption_rejected_for_test;
use syndic_storage::test_faults::{
    sealed_import_checkpoint_for_test, sealed_import_coherent_marker_substitution_for_test,
};

fn key(f: &Fixture, p: &PreparedDraftReplacementImportV1) -> DraftPieceSettlementKeyV1 {
    DraftPieceSettlementKeyV1::new(
        f.session.draft_id(),
        f.session.session_id(),
        p.operation_id(),
    )
}
fn cross(f: &Fixture, p: &PreparedDraftReplacementImportV1) {
    for _ in 0..100 {
        let check = sealed_import_checkpoint_for_test(&f.store, &f.storage, key(f, p));
        if matches!(check.build.frontier(),DraftPieceBuildFrontierV1::SealedImport(fr) if fr.phase()==DraftSealedImportPhaseV1::CrossValidating)
        {
            return;
        }
        assert!(advance_one(f, p).is_none());
    }
    panic!("bounded source never reached EOF")
}

#[test]
fn eof_resets_source_folds_and_partial_cross_validation_survives_reopen() {
    let markers = vec![
        (
            SyndicDraftMarkerId::from_bytes([11; 16]),
            ImageLabelOrdinal::new(3).unwrap(),
        ),
        (
            SyndicDraftMarkerId::from_bytes([12; 16]),
            ImageLabelOrdinal::new(7).unwrap(),
        ),
    ];
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text("before\r\n").unwrap(),
        ComposerAtom::image_marker(markers[0].0, markers[0].1),
        ComposerAtom::text("🙂").unwrap(),
        ComposerAtom::image_marker(markers[1].0, markers[1].1),
        ComposerAtom::text("after").unwrap(),
    ])
    .unwrap();
    let f = Fixture::new("EOF-fold-reset", payload, markers.clone(), 65_536);
    let p = f.prepare();
    begin(&f, &p);
    cross(&f, &p);
    let reset = sealed_import_checkpoint_for_test(&f.store, &f.storage, key(&f, &p));
    assert_eq!(reset.decoder, DraftSealedImportDecoderV1::End);
    assert_eq!(reset.next_leaf_rank, 0);
    assert_eq!(reset.checked_markers, 0);
    assert_eq!(
        reset.sequential_digest,
        beryl_model::sequential_marker_digest_seed()
    );
    assert_eq!(
        reset.ordered_digest,
        beryl_model::ordered_marker_asset_digest_seed()
    );
    assert_eq!(reset.maximum_label, None);
    assert!(advance_one(&f, &p).is_none());
    let text_checked = sealed_import_checkpoint_for_test(&f.store, &f.storage, key(&f, &p));
    assert_eq!(text_checked.next_leaf_rank, 1);
    assert_eq!(text_checked.checked_markers, 0);
    assert_eq!(text_checked.sequential_digest, reset.sequential_digest);
    assert!(advance_one(&f, &p).is_none());
    let marker_checked = sealed_import_checkpoint_for_test(&f.store, &f.storage, key(&f, &p));
    assert_eq!(marker_checked.next_leaf_rank, 2);
    assert_eq!(marker_checked.checked_markers, 1);
    assert_eq!(
        marker_checked.sequential_digest,
        beryl_model::advance_sequential_marker_digest(
            reset.sequential_digest,
            markers[0].0,
            markers[0].1
        )
    );
    assert_eq!(
        marker_checked.ordered_digest,
        beryl_model::advance_ordered_marker_asset_digest(
            reset.ordered_digest,
            markers[0].0,
            markers[0].1,
            f.asset.unwrap()
        )
    );
    let before = f.head();
    let f = f.reopen();
    assert_eq!(f.head(), before);
    assert_eq!(
        sealed_import_checkpoint_for_test(&f.store, &f.storage, key(&f, &p)),
        marker_checked
    );
    assert!(matches!(
        f.storage.prepare_replacement_import_step(&f.store, &p),
        Err(DraftReplacementErrorV1::Proof)
    ));
    let cleanup = f
        .storage
        .prepare_replacement_import_termination(&f.store, key(&f, &p))
        .unwrap();
    let outcome = execute(
        &f.store,
        f.storage.terminate_replacement_draft_import(
            f.storage.revision(&f.store).unwrap(),
            cleanup.clone(),
        ),
    );
    assert_eq!(
        f.storage
            .reconcile_replacement_import_termination(&f.store, &cleanup, outcome)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::ImportTerminated
    );
}

#[test]
fn three_coherent_substituted_trees_cannot_reuse_source_marker_folds() {
    for substitution in 0..4 {
        let markers = vec![
            (
                SyndicDraftMarkerId::from_bytes([21; 16]),
                ImageLabelOrdinal::new(3).unwrap(),
            ),
            (
                SyndicDraftMarkerId::from_bytes([22; 16]),
                ImageLabelOrdinal::new(5).unwrap(),
            ),
            (
                SyndicDraftMarkerId::from_bytes([23; 16]),
                ImageLabelOrdinal::new(7).unwrap(),
            ),
        ];
        let payload = ComposerPayload::new(
            markers
                .iter()
                .map(|&(id, label)| ComposerAtom::image_marker(id, label))
                .collect(),
        )
        .unwrap();
        let f = Fixture::new(
            "coherent-tree-substitution",
            payload,
            markers.clone(),
            65_536,
        );
        let p = f.prepare();
        begin(&f, &p);
        cross(&f, &p);
        let prior = ordinary::current(&f.storage, &f.store, f.thread);
        let prefix = sealed_import_checkpoint_for_test(&f.store, &f.storage, key(&f, &p));
        let asset = f.asset.unwrap();
        let mut replaced = markers
            .iter()
            .enumerate()
            .map(|(n, &(id, label))| DraftPieceMarkerV1::new(id, n as u64 + 1, label, asset))
            .collect::<Vec<_>>();
        match substitution {
            0 => {
                replaced[0] = DraftPieceMarkerV1::new(
                    SyndicDraftMarkerId::from_bytes([24; 16]),
                    1,
                    markers[0].1,
                    asset,
                )
            }
            1 => {
                replaced[0] = DraftPieceMarkerV1::new(
                    markers[0].0,
                    1,
                    ImageLabelOrdinal::new(4).unwrap(),
                    asset,
                )
            }
            2 => {
                replaced[0] = DraftPieceMarkerV1::new(
                    markers[0].0,
                    1,
                    markers[0].1,
                    beryl_model::AssetId::sha256_v1(
                        [25; 32],
                        std::num::NonZeroU64::new(17).unwrap(),
                    ),
                )
            }
            _ => {
                replaced[0] = DraftPieceMarkerV1::new(markers[1].0, 1, markers[1].1, asset);
                replaced[1] = DraftPieceMarkerV1::new(markers[0].0, 2, markers[0].1, asset);
            }
        }
        let rejected = sealed_import_coherent_marker_substitution_for_test(
            &f.store,
            &f.storage,
            key(&f, &p),
            &replaced,
        );
        assert_eq!(rejected.checked_leaf_count, 3);
        assert_eq!(rejected.checked_marker_count, 3);
        assert!(rejected.rejected_completion);
        assert_eq!(
            rejected
                .coherent_roots
                .marker_commitment()
                .maximum_image_label(),
            prefix
                .build
                .working_roots()
                .marker_commitment()
                .maximum_image_label()
        );
        assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), prior);
        assert_eq!(
            sealed_import_checkpoint_for_test(&f.store, &f.storage, key(&f, &p)),
            prefix
        );
        let genuine = advance_all(&f, &p);
        start(&f, &p);
        assert_eq!(f.head().newest_root(), genuine);
    }
}

#[test]
fn completed_source_summaries_must_authenticate_before_eof_fold_reset() {
    let id = SyndicDraftMarkerId::from_bytes([27; 16]);
    let label = ImageLabelOrdinal::new(9).unwrap();
    let f = Fixture::new(
        "source-EOF-authentication",
        ComposerPayload::new(vec![ComposerAtom::image_marker(id, label)]).unwrap(),
        vec![(id, label)],
        65_536,
    );
    let p = f.prepare();
    begin(&f, &p);
    for _ in 0..10 {
        let check = sealed_import_checkpoint_for_test(&f.store, &f.storage, key(&f, &p));
        if matches!(check.build.frontier(),DraftPieceBuildFrontierV1::SealedImport(fr) if fr.marker_count()==1)
        {
            assert_eq!(check.decoder, DraftSealedImportDecoderV1::BetweenAtoms);
            assert!(sealed_import_eof_corruption_rejected_for_test(
                &f.store,
                &f.storage,
                key(&f, &p),
                false
            ));
            assert!(sealed_import_eof_corruption_rejected_for_test(
                &f.store,
                &f.storage,
                key(&f, &p),
                true
            ));
            assert_eq!(
                sealed_import_checkpoint_for_test(&f.store, &f.storage, key(&f, &p)),
                check
            );
            let root = advance_all(&f, &p);
            start(&f, &p);
            assert_eq!(f.head().newest_root(), root);
            return;
        }
        assert!(advance_one(&f, &p).is_none());
    }
    panic!("source did not finish within bounded fixture")
}
