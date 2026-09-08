use super::*;

use beryl_home_store::HomeHealthState;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use syndic_storage::{
    DraftPieceBuildFrontierV1, DraftPieceOperationStatusV1, DraftPieceReconciledCommandV1,
    DraftPieceSettlementKeyV1, DraftPieceSettlementOutcomeV1, DraftPieceTransactionOutcomeV1,
};
use syndic_storage::test_faults::{
    DraftPieceFragmentCorruption, DraftPieceProgressReceiptCorruption,
    DraftPieceProgressRootCorruption, inject_draft_piece_fragment_corruption,
    inject_miskeyed_draft_piece_build_for_test,
    inject_draft_piece_progress_receipt_corruption, inject_draft_piece_progress_root_corruption,
};

#[test]
fn markers_outside_an_ordinary_range_keep_their_exact_authorities() {
    let (_home, store, storage, thread) = fixture("ordinary-range-marker-gap", 200);
    let initial = transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        201,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abcdef".to_owned())],
        )],
        point(6),
    );
    run_transaction(&storage, &store, &initial, 202);

    let leading = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([203; 16]),
        21,
        ImageLabelOrdinal::new(31).unwrap(),
        AssetId::sha256_v1([203; 32], std::num::NonZeroU64::new(1).unwrap()),
    );
    let leading_position = point(1);
    let leading_caret = point(0);
    let leading_edit = marker_transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        204,
        leading_position,
        leading_caret,
        leading,
    );
    run_marker_transaction(&storage, &store, &leading_edit);

    let trailing = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([205; 16]),
        22,
        ImageLabelOrdinal::new(32).unwrap(),
        AssetId::sha256_v1([205; 32], std::num::NonZeroU64::new(1).unwrap()),
    );
    let trailing_position = point(6);
    let trailing_caret = point(0);
    let trailing_edit = marker_transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        206,
        trailing_position,
        trailing_caret,
        trailing,
    );
    run_marker_transaction(&storage, &store, &trailing_edit);

    let base = current(&storage, &store, thread);
    let root_before = base.draft().piece_root();
    let leading_before = storage
        .draft_marker_identity(&store, root_before, leading.marker_id())
        .unwrap()
        .unwrap();
    let trailing_before = storage
        .draft_marker_identity(&store, root_before, trailing.marker_id())
        .unwrap()
        .unwrap();
    let edit = transaction(
        &storage,
        &store,
        &base,
        207,
        vec![DraftPieceReplacementV1::new(
            point(2),
            point(5),
            vec![DraftPieceV1::Text("X".to_owned())],
        )],
        point(3),
    );
    run_transaction(&storage, &store, &edit, 208);

    let root_after = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(root_after.marker_index_root(), root_before.marker_index_root());
    assert_eq!(
        root_after.marker_index_summary(),
        root_before.marker_index_summary()
    );
    assert_eq!(root_after.marker_order_root(), root_before.marker_order_root());
    assert_eq!(
        root_after.marker_order_height(),
        root_before.marker_order_height()
    );
    assert_eq!(
        root_after.marker_commitment(),
        root_before.marker_commitment()
    );
    assert_exact_marker(
        storage
            .draft_marker_identity(&store, root_after, leading.marker_id())
            .unwrap()
            .unwrap(),
        leading_before,
    );
    assert_exact_marker(
        storage
            .draft_marker_identity(&store, root_after, trailing.marker_id())
            .unwrap()
            .unwrap(),
        trailing_before,
    );
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                root_after,
                DraftPieceMarkerAtV1::new(1, leading),
            )
            .unwrap()
    );
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                root_after,
                DraftPieceMarkerAtV1::new(4, trailing),
            )
            .unwrap()
    );
}

#[test]
fn cancellation_after_partial_applying_retains_nonadoption_and_releases_custody() {
    let (_home, store, storage, thread) = fixture("ordinary-range-cancel", 210);
    let (base, edit) = partial_applying_transaction(&storage, &store, thread, 211, 212);

    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    match exact_status(&storage, &store, &edit) {
        DraftPieceOperationStatusV1::Settled(settlement) => {
            assert!(matches!(
                settlement.outcome(),
                DraftPieceSettlementOutcomeV1::Cancelled
            ));
        }
        other => panic!("cancelled partial range did not settle: {other:?}"),
    }
    let current = current(&storage, &store, thread);
    assert_eq!(current.draft().piece_root(), base.draft().piece_root());
    assert!(current.session.active_operation().is_none());
    assert!(matches!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                base.draft().id(),
                edit.session,
                edit.operation,
            ),
        Err(DraftPiecePrepareErrorV1::InvalidRoot)
    ));
}

#[test]
fn selected_or_predecessor_progress_corruption_refuses_another_applying_command() {
    for (name, corruption, seed) in [
        (
            "selected-progress",
            DraftPieceProgressReceiptCorruption::StateMismatch,
            220,
        ),
        (
            "predecessor-progress",
            DraftPieceProgressReceiptCorruption::PreviousStateMismatch,
            230,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(name, seed);
        let (base, edit) = partial_applying_transaction(
            &storage,
            &store,
            thread,
            seed.wrapping_add(1),
            seed.wrapping_add(2),
        );
        let key = DraftPieceSettlementKeyV1::new(base.draft().id(), edit.session, edit.operation);
        committed(execute(
            &store,
            inject_draft_piece_progress_receipt_corruption(&store, &storage, key, corruption),
        ));
        let revision = storage.revision(&store).unwrap();
        assert!(storage
            .prepare_draft_piece_build_advance(
                &store,
                base.draft().id(),
                edit.session,
                edit.operation,
            )
            .is_err());
        assert!(storage
            .draft_piece_operation_status_page(&store, &edit.prepared, 1, &edit.fragments)
            .is_err());
        assert_eq!(storage.revision(&store).unwrap(), revision);
    }
}

#[test]
fn selected_root_and_fragment_corruption_refuse_another_applying_command_without_writing() {
    let (_home, store, storage, thread) = fixture("ordinary-range-corruption-root", 240);
    let (base, edit) = partial_applying_transaction(&storage, &store, thread, 241, 242);
    let key = DraftPieceSettlementKeyV1::new(base.draft().id(), edit.session, edit.operation);
    committed(execute(
        &store,
        inject_draft_piece_progress_root_corruption(
            &store,
            &storage,
            key,
            DraftPieceProgressRootCorruption::PublishedSequence,
        ),
    ));
    assert!(storage
        .prepare_draft_piece_build_advance(&store, base.draft().id(), edit.session, edit.operation)
        .is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    let (_home, store, storage, thread) = fixture("ordinary-range-corruption-fragment", 245);
    let (base, edit) = partial_applying_transaction(&storage, &store, thread, 246, 247);
    inject_draft_piece_fragment_corruption(
        &store,
        &storage,
        edit.fragments[0].key(),
        DraftPieceFragmentCorruption::ChainDigest,
    )
    .unwrap();
    assert!(storage
        .prepare_draft_piece_build_advance(&store, base.draft().id(), edit.session, edit.operation)
        .is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}

#[test]
fn indeterminate_applying_command_reconciles_its_exact_durable_target_without_resubmission() {
    let home = TestHome::new("ordinary-range-reconciliation");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([250; 16]);
    let draft = SyndicDraftId::from_bytes([251; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                execution(),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let (base, edit) = partial_applying_transaction(&storage, &store, thread, 252, 253);
    let advance = storage
        .prepare_draft_piece_build_advance(&store, base.draft().id(), edit.session, edit.operation)
        .unwrap()
        .unwrap();
    assert!(matches!(
        open_build(&storage, &store, &edit).frontier(),
        DraftPieceBuildFrontierV1::Applying { .. }
    ));
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = execute(&store, storage.advance_draft_piece_edit(advance));
    assert!(matches!(outcome, CommandOutcome::Indeterminate { .. }));
    let fragments = edit.fragments.clone();
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(&store, &edit.prepared, outcome, |start| {
                fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .take(256)
                    .cloned()
                    .collect()
            })
            .unwrap(),
        DraftPieceReconciledCommandV1::Pending(DraftPieceOperationStatusV1::Open(_))
    ));
    assert!(matches!(
        open_build(&storage, &store, &edit).frontier(),
        DraftPieceBuildFrontierV1::Applying { .. }
    ));
}

#[test]
fn applying_preparation_rejects_a_valid_build_stored_under_another_build_key() {
    let (_home, store, storage, first_thread) = fixture("ordinary-range-miskeyed-build", 110);
    let (first_base, first_edit) =
        partial_applying_transaction(&storage, &store, first_thread, 111, 112);
    let second_thread = SyndicThreadId::from_bytes([113; 16]);
    let second_draft = SyndicDraftId::from_bytes([114; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                second_thread,
                second_draft,
                execution(),
                SyndicTimestamp::from_unix_millis(2),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let (second_base, second_edit) =
        partial_applying_transaction(&storage, &store, second_thread, 115, 116);
    let second_build = open_build(&storage, &store, &second_edit);
    assert!(matches!(
        second_build.frontier(),
        DraftPieceBuildFrontierV1::Applying { .. }
    ));
    let second_session = match storage
        .draft_editor_candidate_session(&store, second_base.draft().id(), second_edit.session)
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("second Applying build lost its active session: {other:?}"),
    };
    let first_key =
        DraftPieceSettlementKeyV1::new(first_base.draft().id(), first_edit.session, first_edit.operation);
    let second_key = DraftPieceSettlementKeyV1::new(
        second_base.draft().id(),
        second_edit.session,
        second_edit.operation,
    );
    committed(execute(
        &store,
        inject_miskeyed_draft_piece_build_for_test(&store, &storage, first_key, second_key),
    ));
    let revision = storage.revision(&store).unwrap();
    assert!(matches!(
        storage.prepare_draft_piece_build_advance(
            &store,
            first_base.draft().id(),
            first_edit.session,
            first_edit.operation,
        ),
        Err(DraftPiecePrepareErrorV1::InvalidRoot)
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert_eq!(open_build(&storage, &store, &second_edit), second_build);
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, second_base.draft().id(), second_edit.session)
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(second_session)
    );
}

fn marker_transaction(
    storage: &SyndicStorage,
    store: &HomeStore,
    current: &CandidateCurrent,
    operation: u8,
    position: DraftCompositePositionV1,
    caret: DraftCompositePositionV1,
    marker: DraftPieceMarkerV1,
) -> Transaction {
    let session = current.session.session_id();
    let operation = DraftPieceOperationIdV1::from_bytes([operation; 16]);
    let replacement = DraftPieceReplacementV1::new(position, position, vec![DraftPieceV1::Marker(marker)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(DraftPieceMarkerInsertionV1::new(
            position.utf8_offset(),
            marker,
            DraftPieceMarkerEffectChargesV1::for_marker(marker),
        )));
    let replacements = vec![replacement];
    let header = DraftPieceEditHeaderV1::new(
        current.draft().id(),
        session,
        current.session.newest_candidate_generation(),
        current.draft().piece_root(),
        current.session.newest_history(),
        operation,
        current.positions.caret,
        current.positions.selection,
        caret,
        caret,
        1,
        canonical_draft_piece_fragment_chain_v1(&replacements),
    );
    let prepared = storage
        .prepare_draft_piece_edit(store, header, &current.session)
        .unwrap();
    let fragment = storage
        .unadmitted_marker_builder_for_test()
        .prepare_fragment(
            &prepared,
            1,
            canonical_empty_draft_piece_fragment_chain_v1(),
            replacements.into_iter().next().unwrap(),
        )
        .unwrap();
    Transaction {
        session,
        operation,
        prepared,
        fragments: vec![fragment],
        successor_positions: FixturePositions {
            caret,
            selection: caret,
        },
    }
}

fn run_marker_transaction(storage: &SyndicStorage, store: &HomeStore, transaction: &Transaction) {
    committed(execute(
        store,
        storage.begin_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
    let builder = storage.unadmitted_marker_builder_for_test();
    for fragment in &transaction.fragments {
        committed(execute(
            store,
            builder.stage_fragment(
                storage.revision(store).unwrap(),
                transaction.prepared.clone(),
                fragment.clone(),
            ),
        ));
    }
    advance_until_complete(storage, store, transaction);
    committed(execute(
        store,
        storage.settle_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
    remember_settled_transaction(storage, store, transaction);
}

fn partial_applying_transaction(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    seed_operation: u8,
    edit_operation: u8,
) -> (CandidateCurrent, Transaction) {
    let seed = transaction(
        storage,
        store,
        &current(storage, store, thread),
        seed_operation,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("a".to_owned()); 4],
        )],
        point(4),
    );
    run_transaction(storage, store, &seed, 1);
    let base = current(storage, store, thread);
    let edit = transaction(
        storage,
        store,
        &base,
        edit_operation,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(4),
            vec![DraftPieceV1::Text("b".to_owned())],
        )],
        point(1),
    );
    begin_and_stage(storage, store, &edit);
    for step in 0..32 {
        let before = open_build(storage, store, &edit);
        let advance = storage
            .prepare_draft_piece_build_advance(store, base.draft().id(), edit.session, edit.operation)
            .unwrap_or_else(|error| panic!("preparation at step {step} failed: {error:?}"))
            .unwrap();
        let applying = matches!(before.frontier(), DraftPieceBuildFrontierV1::Applying { .. });
        committed(execute(store, storage.advance_draft_piece_edit(advance)));
        if applying {
            return (base, edit);
        }
    }
    panic!("ordinary range did not publish partial Applying progress")
}

fn assert_exact_marker(
    actual: syndic_storage::DraftMarkerIdentityOccurrenceV1,
    expected: syndic_storage::DraftMarkerIdentityOccurrenceV1,
) {
    assert_eq!(actual.marker_id(), expected.marker_id());
    assert_eq!(actual.order_key(), expected.order_key());
    assert_eq!(actual.label(), expected.label());
    assert_eq!(actual.asset_id(), expected.asset_id());
}
