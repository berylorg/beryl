#![cfg(feature = "test-faults")]

include!("phase154_durable_builder/support.rs");

use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftHistoricalRootAdoptionOutcomeV1,
    DraftHistoricalRootAdoptionReconciliationV1, DraftHistoricalRootDirectionV1,
    DraftHistoricalRootSelectionIntentV1, DraftHistoricalRootSelectionV1,
    DraftImageLabelProtectionHeadV1, ImageLabelFrontier, PreparedDraftHistoricalRootAdoptionV1,
    test_faults::{
        DraftPieceDescendantCorruption, DraftPieceDescendantTarget, FixtureBatch, FixtureRecord,
        inject_draft_piece_descendant_corruption,
    },
};

#[path = "phase226_draft_marker_historical_adoption/support.rs"]
mod marker_support;

use marker_support::marked_session;

fn operation_id(value: u8) -> syndic_storage::DraftPieceOperationIdV1 {
    syndic_storage::DraftPieceOperationIdV1::from_bytes([value; 16])
}

fn owner(
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> syndic_storage::DraftMarkerAdmissionOwnerV1 {
    syndic_storage::DraftMarkerAdmissionOwnerV1::new(
        session.draft_id(),
        session.session_id(),
        syndic_storage::DraftMarkerAdmissionOperationIdV1::from_bytes([operation; 16]),
    )
}

fn fault_fixture(
    name: &str,
    seed: u8,
) -> (
    TestHome,
    HomeStore,
    SyndicStorage,
    FaultController,
    beryl_model::SyndicThreadId,
) {
    let home = TestHome::new(name);
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = beryl_model::SyndicThreadId::from_bytes([seed; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                beryl_model::SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase226",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    (home, store, storage, faults, thread)
}

fn reopen(home: &TestHome, store: HomeStore) -> (HomeStore, SyndicStorage) {
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    (store, storage)
}

fn protection(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: beryl_model::SyndicThreadId,
) -> DraftImageLabelProtectionHeadV1 {
    storage
        .draft_image_label_protection_head(
            store,
            thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap()
}

fn authority(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: beryl_model::SyndicThreadId,
) -> syndic_storage::ImageLabelAuthorityHeadV1 {
    storage
        .image_label_authority_head(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
}

fn replace_protection(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: beryl_model::SyndicThreadId,
    maximum: ImageLabelFrontier,
) -> DraftImageLabelProtectionHeadV1 {
    let current = protection(storage, store, thread);
    let replacement =
        DraftImageLabelProtectionHeadV1::new(thread, current.revision() + 1, maximum).unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::DraftImageLabelProtectionHead(replacement))
        .unwrap();
    committed(execute(
        store,
        storage.fixture_contribution(storage.revision(store).unwrap(), batch),
    ));
    replacement
}

fn marker_bearing_source(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: beryl_model::SyndicThreadId,
    seed: u8,
) -> (
    DraftEditorCandidateSessionV1,
    syndic_storage::DraftPieceMarkerV1,
    syndic_storage::DraftPieceRootReferenceV1,
) {
    let (session, marker) = marked_session(storage, store, thread, seed);
    replace_protection(
        storage,
        store,
        thread,
        ImageLabelFrontier::from_raw(marker.label().get()),
    );
    let historical_root = session.newest_root();
    let session = complete_staged(
        storage,
        store,
        &session,
        seed.wrapping_add(10),
        DraftPieceReplacementV1::new(
            DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
            DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
            vec![DraftPieceV1::Text("later".to_owned())],
        ),
        DraftLogicalExtentV1::new(6, 1),
    );
    (session, marker, historical_root)
}

fn selected(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    direction: DraftHistoricalRootDirectionV1,
) -> PreparedDraftHistoricalRootAdoptionV1 {
    match storage
        .prepare_draft_historical_root_selection(
            store,
            DraftHistoricalRootSelectionIntentV1::new(
                DraftEditorCandidateActivationBindingV1::from_head(session),
                operation_id(operation),
                direction,
            ),
        )
        .unwrap()
    {
        DraftHistoricalRootSelectionV1::Prepared(prepared) => prepared,
        DraftHistoricalRootSelectionV1::Unavailable => {
            panic!("marker-bearing history was unavailable")
        }
    }
}

fn committed_historical(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: PreparedDraftHistoricalRootAdoptionV1,
) -> syndic_storage::DraftHistoricalRootAdoptionProofV1 {
    let outcome = execute(
        store,
        storage.adopt_draft_historical_root(storage.revision(store).unwrap(), prepared.clone()),
    );
    match storage
        .reconcile_draft_historical_root_adoption(store, &prepared, outcome)
        .unwrap()
    {
        DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
            DraftHistoricalRootAdoptionOutcomeV1::Committed(proof),
        ) => proof,
        value => panic!("marker-bearing historical adoption did not commit: {value:?}"),
    }
}

#[test]
fn opaque_marker_bearing_undo_redo_reuse_exact_roots_at_protection_and_preserve_authority() {
    let (_home, store, storage, thread) = fixture("phase226-equal", 226);
    let (source, marker, undo_target) = marker_bearing_source(&storage, &store, thread, 10);
    let before_protection = protection(&storage, &store, thread);
    let before_authority = authority(&storage, &store, thread);

    assert_eq!(
        before_protection.protected_maximum().get(),
        marker.label().get(),
        "the retained undo target is exactly at the protection boundary"
    );
    let undo = selected(
        &storage,
        &store,
        &source,
        40,
        DraftHistoricalRootDirectionV1::Undo,
    );
    let undo_target_record = storage
        .draft_piece_root(&store, undo_target)
        .unwrap()
        .unwrap();
    assert_eq!(
        undo_target_record
            .reference()
            .marker_commitment()
            .maximum_image_label(),
        Some(marker.label())
    );
    let undo_proof = committed_historical(&storage, &store, undo.clone());
    assert_eq!(
        undo_proof.successor_candidate().unwrap().newest_root(),
        undo_target
    );
    assert_eq!(protection(&storage, &store, thread), before_protection);
    assert_eq!(authority(&storage, &store, thread), before_authority);

    let redo = selected(
        &storage,
        &store,
        undo_proof.successor_candidate().unwrap(),
        41,
        DraftHistoricalRootDirectionV1::Redo,
    );
    let redo_target = storage
        .draft_piece_root(&store, source.newest_root())
        .unwrap()
        .unwrap();
    assert_eq!(
        redo_target
            .reference()
            .marker_commitment()
            .maximum_image_label(),
        Some(marker.label())
    );
    let redo_proof = committed_historical(&storage, &store, redo.clone());
    assert_eq!(
        redo_proof.successor_candidate().unwrap().newest_root(),
        source.newest_root()
    );
    assert_eq!(protection(&storage, &store, thread), before_protection);
    assert_eq!(authority(&storage, &store, thread), before_authority);
}

#[test]
fn marker_bearing_target_above_protection_is_rejected_without_candidate_or_history_settlement() {
    let (_home, store, storage, thread) = fixture("phase226-beyond", 227);
    let (source, marker, _undo_target) = marker_bearing_source(&storage, &store, thread, 20);
    let baseline_authority = authority(&storage, &store, thread);
    let original_protection = protection(&storage, &store, thread);
    assert_eq!(
        original_protection.protected_maximum().get(),
        marker.label().get()
    );
    let prepared = selected(
        &storage,
        &store,
        &source,
        50,
        DraftHistoricalRootDirectionV1::Undo,
    );

    let lowered = replace_protection(&storage, &store, thread, ImageLabelFrontier::EMPTY);
    let outcome = execute(
        &store,
        storage.adopt_draft_historical_root(storage.revision(&store).unwrap(), prepared.clone()),
    );
    assert_eq!(
        storage
            .reconcile_draft_historical_root_adoption(&store, &prepared, outcome)
            .unwrap(),
        DraftHistoricalRootAdoptionReconciliationV1::ExactOld
    );

    assert!(
        storage
            .prepare_draft_historical_root_selection(
                &store,
                DraftHistoricalRootSelectionIntentV1::new(
                    DraftEditorCandidateActivationBindingV1::from_head(&source),
                    operation_id(51),
                    DraftHistoricalRootDirectionV1::Undo,
                ),
            )
            .is_err(),
        "the opaque target must be refused before writer admission when its maximum label exceeds protection"
    );
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, source.draft_id(), source.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(source)
    );
    assert_eq!(protection(&storage, &store, thread), lowered);
    assert_eq!(authority(&storage, &store, thread), baseline_authority);
}

#[test]
fn marker_bearing_prepared_adoption_accepts_a_later_monotonic_protection_advance() {
    let (_home, store, storage, thread) = fixture("phase226-protection-advance", 233);
    let (source, marker, undo_target) = marker_bearing_source(&storage, &store, thread, 40);
    let prepared = selected(
        &storage,
        &store,
        &source,
        70,
        DraftHistoricalRootDirectionV1::Undo,
    );
    let advanced = replace_protection(
        &storage,
        &store,
        thread,
        ImageLabelFrontier::from_raw(marker.label().get() + 1),
    );

    let proof = committed_historical(&storage, &store, prepared);
    assert_eq!(
        proof.successor_candidate().unwrap().newest_root(),
        undo_target
    );
    assert_eq!(protection(&storage, &store, thread), advanced);
}

#[test]
fn marker_bearing_target_with_a_substituted_order_closure_is_rejected_without_settlement() {
    let (_home, store, storage, thread) = fixture("phase226-marker-order-corruption", 232);
    let (source, _marker, undo_target) = marker_bearing_source(&storage, &store, thread, 30);
    committed(execute(
        &store,
        inject_draft_piece_descendant_corruption(
            &store,
            &storage,
            undo_target,
            DraftPieceDescendantTarget::MarkerOrder,
            DraftPieceDescendantCorruption::Digest,
        ),
    ));

    assert!(
        storage
            .prepare_draft_historical_root_selection(
                &store,
                DraftHistoricalRootSelectionIntentV1::new(
                    DraftEditorCandidateActivationBindingV1::from_head(&source),
                    operation_id(61),
                    DraftHistoricalRootDirectionV1::Undo,
                ),
            )
            .is_err(),
        "the opaque target's authenticated marker-order closure must reject substitution"
    );
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(
        storage
            .draft_editor_candidate_session(&store, source.draft_id(), source.session_id())
            .is_err()
    );
    assert!(
        storage
            .draft_image_label_protection_head(
                &store,
                thread,
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .is_err()
    );
    assert!(
        storage
            .image_label_authority_head(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
            .is_err()
    );
}

#[test]
fn marker_bearing_historical_commit_cuts_reconcile_only_exact_old_or_exact_new() {
    for (name, seed, cut, commits) in [
        ("before", 228, FaultPoint::BeforeCommit, false),
        (
            "after-commit",
            229,
            FaultPoint::AfterCommitBeforePersist,
            true,
        ),
        ("after-persist", 230, FaultPoint::AfterPersist, true),
        (
            "before-verification",
            231,
            FaultPoint::BeforeVerification,
            true,
        ),
    ] {
        let (home, store, storage, faults, thread) = fault_fixture(name, seed);
        let (source, marker, undo_target) =
            marker_bearing_source(&storage, &store, thread, seed.wrapping_add(20));
        let prepared = selected(
            &storage,
            &store,
            &source,
            seed.wrapping_add(40),
            DraftHistoricalRootDirectionV1::Undo,
        );
        let target = storage
            .draft_piece_root(&store, undo_target)
            .unwrap()
            .unwrap();
        assert_eq!(
            target.reference().marker_commitment().maximum_image_label(),
            Some(marker.label())
        );
        let expected_protection = protection(&storage, &store, thread);
        let expected_authority = authority(&storage, &store, thread);
        faults.fail_next(cut);
        let outcome = execute(
            &store,
            storage
                .adopt_draft_historical_root(storage.revision(&store).unwrap(), prepared.clone()),
        );
        let (store, storage) = if store.health().state() == HomeHealthState::Failed {
            let recovery = store.recover_same_home().unwrap();
            let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            (recovery.publish(), storage)
        } else {
            (store, storage)
        };
        let reconciled = storage
            .reconcile_draft_historical_root_adoption(&store, &prepared, outcome)
            .unwrap();
        if commits {
            let DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
                DraftHistoricalRootAdoptionOutcomeV1::Committed(proof),
            ) = reconciled
            else {
                panic!("committed marker-bearing cut reconciled outside exact-new: {reconciled:?}")
            };
            assert_eq!(
                proof.successor_candidate().unwrap().newest_root(),
                undo_target
            );
        } else {
            assert_eq!(
                reconciled,
                DraftHistoricalRootAdoptionReconciliationV1::ExactOld
            );
            assert_eq!(
                storage
                    .draft_editor_candidate_session(&store, source.draft_id(), source.session_id())
                    .unwrap(),
                DraftEditorCandidateSessionReadOutcomeV1::Active(source)
            );
        }
        assert_eq!(protection(&storage, &store, thread), expected_protection);
        assert_eq!(authority(&storage, &store, thread), expected_authority);
        let (_store, _storage) = reopen(&home, store);
    }
}
