use super::*;

#[test]
fn exact_import_head_budget_refuses_first_append_and_preserves_baseline_and_cow_owners() {
    let marker = SyndicDraftMarkerId::from_bytes([244; 16]);
    let label = ImageLabelOrdinal::new(1).unwrap();
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text("base").unwrap(),
        ComposerAtom::image_marker(marker, label),
    ])
    .unwrap();
    let measure = Fixture::new(
        "import-append-charge",
        payload.clone(),
        vec![(marker, label)],
        65_536,
    );
    let p = measure.prepare();
    begin(&measure, &p);
    advance_all(&measure, &p);
    let prepared = measure
        .storage
        .prepare_replacement_draft_start(&measure.store, &p, SyndicTimestamp::from_unix_millis(3))
        .unwrap();
    let DraftPieceSettlementClosureV1::SealedImport(closure) = prepared.settlement().closure()
    else {
        panic!()
    };
    let budget = closure
        .baseline()
        .retained_encoded_bytes()
        .max(closure.session_fork().retained_encoded_bytes());
    let f = Fixture::new(
        "import-append-exhaustion",
        payload,
        vec![(marker, label)],
        budget,
    );
    let p = f.prepare();
    begin(&f, &p);
    let imported = advance_all(&f, &p);
    start(&f, &p);
    let baseline = f.head();
    let durable = ordinary::current(&f.storage, &f.store, f.thread);
    let historical = f
        .state
        .assets()
        .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
        .unwrap();
    let cow = f
        .state
        .assets()
        .owner_head(&f.store, AssetOwner::CurrentDraft(baseline.draft_id()))
        .unwrap();
    let asset_revision = f.state.assets().revision(&f.store).unwrap();
    assert!(historical.is_some() && cow.is_some());
    let tx = ordinary::transaction(&f.storage, &f.store, &baseline, 245, "x", point(1));
    ordinary::build(&f.storage, &f.store, &tx);
    committed(execute(
        &f.store,
        f.storage
            .settle_draft_piece_edit(f.storage.revision(&f.store).unwrap(), tx.prepared.clone()),
    ));
    let settlement = ordinary::settled(&f.storage, &f.store, &tx);
    assert!(matches!(
        settlement.outcome(),
        DraftPieceSettlementOutcomeV1::Error(DraftPieceErrorReasonV1::HistoryCapacityUnavailable)
    ));
    let DraftPieceSettlementClosureV1::Noncommit(noncommit) = settlement.closure() else {
        panic!("capacity refusal selected a successor")
    };
    assert_eq!(
        noncommit.observed_history().reference(),
        baseline.newest_history()
    );
    let after = f.head();
    assert!(after.active_operation().is_none());
    assert_eq!(after.newest_root(), imported);
    assert_eq!(after.newest_history(), baseline.newest_history());
    assert_eq!(after.published_root(), baseline.published_root());
    assert_eq!(after.published_history(), baseline.published_history());
    assert_eq!(
        after.newest_candidate_generation(),
        baseline.newest_candidate_generation()
    );
    assert_eq!(after.dirty_generation(), baseline.dirty_generation());
    assert!(!after.newest_history().availability().undo_available());
    assert!(!after.newest_history().availability().redo_available());
    assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), durable);
    assert_eq!(text(&f, imported), b"base");
    assert_eq!(imported.summary().marker_count(), 1);
    assert_eq!(f.state.assets().revision(&f.store).unwrap(), asset_revision);
    assert_eq!(
        f.state
            .assets()
            .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
            .unwrap(),
        historical
    );
    assert_eq!(
        f.state
            .assets()
            .owner_head(&f.store, AssetOwner::CurrentDraft(baseline.draft_id()))
            .unwrap(),
        cow
    );
    let f = f.reopen();
    assert_eq!(f.head(), after);
    assert_eq!(ordinary::settled(&f.storage, &f.store, &tx), settlement);
    assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), durable);
    assert_eq!(text(&f, f.head().newest_root()), b"base");
    assert!(
        f.storage
            .draft_editor_candidate_is_saved(
                &f.store,
                DraftEditorCandidateActivationBindingV1::from_head(&f.head()),
                ordinary::selector(&durable)
            )
            .unwrap()
    );
    assert_eq!(
        f.state
            .assets()
            .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
            .unwrap(),
        historical
    );
    assert_eq!(
        f.state
            .assets()
            .owner_head(&f.store, AssetOwner::CurrentDraft(baseline.draft_id()))
            .unwrap(),
        cow
    );
}

#[test]
fn fresh_reopen_authenticates_clean_and_dirty_import_checkpoints_and_directed_positions() {
    for dirty in [false, true] {
        let f = Fixture::new(
            "import-checkpoint-reopen",
            ComposerPayload::new(vec![ComposerAtom::text("source").unwrap()]).unwrap(),
            vec![],
            65_536,
        );
        let request = DraftReplacementImportRequestV1::new(
            ordinary::selector(&ordinary::current(&f.storage, &f.store, f.thread)),
            DraftEditorCandidateActivationBindingV1::from_head(&f.session),
            operation(207),
            f.intent,
            f.item,
            beryl_model::ProjectionRevision::new(1).unwrap(),
            f.content,
            f.storage
                .input_gate(
                    &f.store,
                    f.thread,
                    SyndicPointReadLimit::new(65_536).unwrap(),
                )
                .unwrap()
                .unwrap()
                .revision(),
            f.proof,
            point(0),
            point(0),
            point(3),
            point(0),
        );
        let p = f
            .storage
            .prepare_replacement_draft_import(&f.store, request)
            .unwrap();
        begin(&f, &p);
        let imported = advance_all(&f, &p);
        start(&f, &p);
        if dirty {
            let tx = ordinary::transaction_with_positions(
                &f.storage,
                &f.store,
                &f.head(),
                246,
                "xx",
                point(3),
                point(0),
                point(2),
                point(0),
            );
            ordinary::build(&f.storage, &f.store, &tx);
            committed(execute(
                &f.store,
                f.storage.settle_draft_piece_edit(
                    f.storage.revision(&f.store).unwrap(),
                    tx.prepared.clone(),
                ),
            ));
            let settlement = ordinary::settled(&f.storage, &f.store, &tx);
            let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
                panic!()
            };
            assert_eq!(adoption.transition().before_caret(), point(3));
            assert_eq!(adoption.transition().before_selection(), point(0));
            assert_eq!(adoption.transition().after_caret(), point(2));
            assert_eq!(adoption.transition().after_selection(), point(0));
        }
        cancel(&f, &f.head(), 247);
        let before = f.head();
        let durable = ordinary::current(&f.storage, &f.store, f.thread);
        let f = f.reopen();
        let revision = f.store.home_revision().unwrap();
        assert_eq!(f.head(), before);
        assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), durable);
        assert_eq!(
            f.storage
                .draft_editor_candidate_is_saved(
                    &f.store,
                    DraftEditorCandidateActivationBindingV1::from_head(&before),
                    ordinary::selector(&durable)
                )
                .unwrap(),
            !dirty
        );
        assert_eq!(
            text(&f, before.newest_root()),
            if dirty {
                b"xxsource".as_slice()
            } else {
                b"source".as_slice()
            }
        );
        assert_eq!(f.store.home_revision().unwrap(), revision);
        if dirty {
            let intent = DraftHistoricalRootSelectionIntentV1::new(
                DraftEditorCandidateActivationBindingV1::from_head(&before),
                operation(248),
                DraftHistoricalRootDirectionV1::Undo,
            );
            let DraftHistoricalRootSelectionV1::Prepared(undo) = f
                .storage
                .prepare_draft_historical_root_selection(&f.store, intent)
                .unwrap()
            else {
                panic!("reopened imported edit lost Undo")
            };
            let outcome = execute(
                &f.store,
                f.storage.adopt_draft_historical_root(
                    f.storage.revision(&f.store).unwrap(),
                    undo.clone(),
                ),
            );
            let DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
                DraftHistoricalRootAdoptionOutcomeV1::Committed(proof),
            ) = f
                .storage
                .reconcile_draft_historical_root_adoption(&f.store, &undo, outcome)
                .unwrap()
            else {
                panic!("reopened Undo did not commit")
            };
            assert_eq!(proof.caret(), point(3));
            assert_eq!(proof.selection(), point(0));
            assert_eq!(f.head().newest_root(), imported);
            assert_eq!(text(&f, imported), b"source");
        } else {
            let next = ordinary::transaction_with_positions(
                &f.storage,
                &f.store,
                &before,
                249,
                "x",
                point(3),
                point(0),
                point(1),
                point(0),
            );
            ordinary::build(&f.storage, &f.store, &next);
            committed(execute(
                &f.store,
                f.storage.settle_draft_piece_edit(
                    f.storage.revision(&f.store).unwrap(),
                    next.prepared.clone(),
                ),
            ));
            let settlement = ordinary::settled(&f.storage, &f.store, &next);
            let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
                panic!("reopened clean checkpoint rejected its directed positions")
            };
            assert_eq!(adoption.transition().before_caret(), point(3));
            assert_eq!(adoption.transition().before_selection(), point(0));
        }
    }
}
