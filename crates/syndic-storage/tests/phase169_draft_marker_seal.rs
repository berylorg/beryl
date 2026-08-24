include!("phase154_durable_builder/support.rs");

use syndic_storage::{
    DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS, DraftMarkerSealCustodyReleaseV1, DraftMarkerSealErrorV1,
    DraftMarkerSealFailureReasonV1, DraftMarkerSealOperationIdV1, DraftMarkerSealRequestV1,
    DraftMarkerSealStatusV1,
};
#[cfg(feature = "test-faults")]
use syndic_storage::{
    inject_draft_marker_seal_natural_identity_collision_for_test,
    inject_draft_marker_seal_record_corruption_for_test,
};

#[test]
fn marker_free_seal_is_restartable_replayable_and_opaque_until_eof() {
    let (home, store, storage, thread) = fixture("phase169-empty-seal", 231);
    let source = current(storage, &store, thread).draft().piece_root();
    let request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([232; 16]));
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
    ));
    assert!(matches!(
        storage
            .draft_marker_seal_status(&store, request.key())
            .unwrap(),
        DraftMarkerSealStatusV1::Open {
            completed_marker_count: 0
        }
    ));

    let replay = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    let replay_outcome = execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), replay),
    );
    assert!(matches!(
        replay_outcome,
        CommandOutcome::NotCommitted { .. }
    ));
    let advance = storage
        .prepare_draft_marker_seal_advance(&store, request.key())
        .unwrap()
        .unwrap();
    assert!(advance.page().markers().is_empty());
    assert!(advance.page().exact_eof());
    assert_eq!(advance.page().release().source_frontier(), 0);
    assert_eq!(advance.page().release().target_frontier(), 0);
    committed(execute(
        &store,
        storage.advance_draft_marker_seal(storage.revision(&store).unwrap(), &advance),
    ));

    let DraftMarkerSealStatusV1::Sealed(proof, release) = storage
        .draft_marker_seal_status(&store, request.key())
        .unwrap()
    else {
        panic!("marker-free seal did not close at exact EOF");
    };
    assert_eq!(proof.source(), source);
    assert_eq!(proof.commitment(), source.marker_commitment());
    assert_eq!(proof.sequential().marker_count(), 0);
    assert_eq!(proof.ordered_assets().marker_count(), 0);
    assert_eq!(release.completed_marker_count(), 0);
    assert!(
        storage
            .prepare_draft_marker_seal_advance(&store, request.key())
            .unwrap()
            .is_none()
    );

    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    assert!(matches!(
        storage
            .draft_marker_seal_status(&store, request.key())
            .unwrap(),
        DraftMarkerSealStatusV1::Sealed(_, _)
    ));
}

#[test]
fn cancellation_closes_progress_and_returns_fixed_release() {
    let (_home, store, storage, thread) = fixture("phase169-cancel-seal", 233);
    let source = current(storage, &store, thread).draft().piece_root();
    let request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([234; 16]));
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
    ));
    let cancel = storage
        .prepare_draft_marker_seal_cancel(&store, request.key())
        .unwrap();
    let release = cancel.release();
    assert_eq!(release.key(), request.key());
    assert_eq!(release.completed_marker_count(), 0);
    committed(execute(
        &store,
        storage.cancel_draft_marker_seal(storage.revision(&store).unwrap(), cancel),
    ));
    assert_eq!(
        storage
            .draft_marker_seal_status(&store, request.key())
            .unwrap(),
        DraftMarkerSealStatusV1::Cancelled(release),
    );
    assert!(
        storage
            .prepare_draft_marker_seal_advance(&store, request.key())
            .unwrap()
            .is_none()
    );
}

#[test]
fn operational_failure_closes_with_one_replayable_fixed_release() {
    let (_home, store, storage, thread) = fixture("phase169-failed-seal", 243);
    let source = current(storage, &store, thread).draft().piece_root();
    let request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([244; 16]));
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
    ));

    let failed = storage
        .prepare_draft_marker_seal_fail(
            &store,
            request.key(),
            DraftMarkerSealFailureReasonV1::Operational,
        )
        .unwrap();
    let release = failed.release();
    assert_eq!(release.key(), request.key());
    assert_eq!(release.completed_marker_count(), 0);
    committed(execute(
        &store,
        storage.fail_draft_marker_seal(storage.revision(&store).unwrap(), failed),
    ));
    assert_eq!(
        storage
            .draft_marker_seal_status(&store, request.key())
            .unwrap(),
        DraftMarkerSealStatusV1::Failed {
            reason: DraftMarkerSealFailureReasonV1::Operational,
            release,
        },
    );

    let replay = storage
        .prepare_draft_marker_seal_fail(
            &store,
            request.key(),
            DraftMarkerSealFailureReasonV1::Operational,
        )
        .unwrap();
    assert_eq!(replay.release(), release);
    assert!(matches!(
        execute(
            &store,
            storage.fail_draft_marker_seal(storage.revision(&store).unwrap(), replay),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        storage.prepare_draft_marker_seal_supersede(
            &store,
            request.key(),
            DraftMarkerSealOperationIdV1::from_bytes([245; 16]),
        ),
        Err(DraftMarkerSealErrorV1::IdentityCollision)
    ));
}

#[test]
fn supersession_closes_with_exact_successor_and_replayable_release() {
    let (_home, store, storage, thread) = fixture("phase169-superseded-seal", 246);
    let source = current(storage, &store, thread).draft().piece_root();
    let request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([247; 16]));
    let successor = DraftMarkerSealOperationIdV1::from_bytes([248; 16]);
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
    ));

    let superseded = storage
        .prepare_draft_marker_seal_supersede(&store, request.key(), successor)
        .unwrap();
    let release = superseded.release();
    committed(execute(
        &store,
        storage.supersede_draft_marker_seal(storage.revision(&store).unwrap(), superseded),
    ));
    assert_eq!(
        storage
            .draft_marker_seal_status(&store, request.key())
            .unwrap(),
        DraftMarkerSealStatusV1::Superseded { successor, release },
    );

    let replay = storage
        .prepare_draft_marker_seal_supersede(&store, request.key(), successor)
        .unwrap();
    assert_eq!(replay.release(), release);
    assert!(matches!(
        execute(
            &store,
            storage.supersede_draft_marker_seal(storage.revision(&store).unwrap(), replay),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        storage.prepare_draft_marker_seal_supersede(
            &store,
            request.key(),
            DraftMarkerSealOperationIdV1::from_bytes([249; 16]),
        ),
        Err(DraftMarkerSealErrorV1::IdentityCollision)
    ));
}

#[test]
fn page_and_custody_values_remain_explicitly_bounded() {
    assert_eq!(DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS, 256);
    assert!(std::mem::size_of::<DraftMarkerSealCustodyReleaseV1>() <= 256);

    let (_home, store, storage, thread) = fixture("phase169-seal-bounds", 250);
    let source = current(storage, &store, thread).draft().piece_root();
    let request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([251; 16]));
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
    ));
    assert!(matches!(
        storage.prepare_draft_marker_seal_advance_with_limit(&store, request.key(), 0),
        Err(DraftMarkerSealErrorV1::InvalidPageLimit)
    ));
    assert!(matches!(
        storage.prepare_draft_marker_seal_advance_with_limit(
            &store,
            request.key(),
            DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS + 1,
        ),
        Err(DraftMarkerSealErrorV1::InvalidPageLimit)
    ));
    let page = storage
        .prepare_draft_marker_seal_advance(&store, request.key())
        .unwrap()
        .unwrap();
    assert!(page.page().markers().len() <= DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS);
    assert_eq!(page.page().release().key(), request.key());
}

#[cfg(feature = "test-faults")]
#[test]
fn natural_identity_collision_is_rejected_without_overwrite() {
    let (_home, store, storage, thread) = fixture("phase169-seal-collision", 252);
    let source = current(storage, &store, thread).draft().piece_root();
    let request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([253; 16]));
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
    ));

    let colliding_operation = DraftMarkerSealOperationIdV1::from_bytes([254; 16]);
    let (colliding_key, collision) = inject_draft_marker_seal_natural_identity_collision_for_test(
        &store,
        storage,
        request.key(),
        colliding_operation,
    );
    committed(execute(&store, collision));
    assert_eq!(
        DraftMarkerSealRequestV1::new(source, colliding_operation).key(),
        colliding_key
    );
    assert!(matches!(
        storage.prepare_draft_marker_seal_begin(
            &store,
            DraftMarkerSealRequestV1::new(source, colliding_operation),
        ),
        Err(DraftMarkerSealErrorV1::IdentityCollision)
    ));
    assert!(matches!(
        storage.draft_marker_seal_status(&store, colliding_key),
        Err(DraftMarkerSealErrorV1::IdentityCollision)
    ));
    assert!(matches!(
        storage
            .draft_marker_seal_status(&store, request.key())
            .unwrap(),
        DraftMarkerSealStatusV1::Open { .. }
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn persisted_seal_record_corruption_fails_closed() {
    let (_home, store, storage, thread) = fixture("phase169-seal-corruption", 255);
    let source = current(storage, &store, thread).draft().piece_root();
    let request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([0; 16]));
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
    ));
    inject_draft_marker_seal_record_corruption_for_test(&store, storage, request.key());

    assert!(
        storage
            .prepare_draft_marker_seal_fail(
                &store,
                request.key(),
                DraftMarkerSealFailureReasonV1::Operational,
            )
            .is_err()
    );
}

#[test]
fn ordered_markers_fold_incrementally_into_the_opaque_proof() {
    let (home, store, storage, thread) = fixture("phase169-ordered-seal", 235);
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 236, 237);
    session = complete_staged(
        &storage,
        &store,
        &session,
        238,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".to_owned())],
        ),
        DraftLogicalExtentV1::new(3, 1),
    );
    let first = marker(238, 8, 3);
    session = complete_marker_edit_for_seal(
        &storage,
        &store,
        &session,
        239,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(first)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    first,
                    DraftPieceMarkerEffectChargesV1::for_marker(first),
                ),
            )),
    );
    let second = marker(240, 7, 5);
    let before_all = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    session = complete_marker_edit_for_seal(
        &storage,
        &store,
        &session,
        241,
        DraftPieceReplacementV1::new(before_all, before_all, vec![DraftPieceV1::Marker(second)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    second,
                    DraftPieceMarkerEffectChargesV1::for_marker(second),
                ),
            )),
    );

    let source = session.newest_root();
    let request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([242; 16]));
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
    ));
    let first_page = storage
        .prepare_draft_marker_seal_advance_with_limit(&store, request.key(), 1)
        .unwrap()
        .unwrap();
    assert_eq!(first_page.page().markers().len(), 1);
    assert_eq!(
        first_page.page().markers()[0].marker_id(),
        second.marker_id()
    );
    assert_eq!(first_page.page().markers()[0].asset_id(), second.asset_id());
    assert!(!first_page.page().exact_eof());
    committed(execute(
        &store,
        storage.advance_draft_marker_seal(storage.revision(&store).unwrap(), &first_page),
    ));
    assert_eq!(first_page.page().markers()[0].asset_id(), second.asset_id());
    assert!(matches!(
        storage
            .draft_marker_seal_status(&store, request.key())
            .unwrap(),
        DraftMarkerSealStatusV1::Open {
            completed_marker_count: 1
        }
    ));

    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let second_page = storage
        .prepare_draft_marker_seal_advance_with_limit(&store, request.key(), 1)
        .unwrap()
        .unwrap();
    assert_eq!(second_page.page().markers().len(), 1);
    assert_eq!(
        second_page.page().markers()[0].marker_id(),
        first.marker_id()
    );
    assert_eq!(second_page.page().markers()[0].asset_id(), first.asset_id());
    assert!(second_page.page().exact_eof());
    committed(execute(
        &store,
        storage.advance_draft_marker_seal(storage.revision(&store).unwrap(), &second_page),
    ));

    let DraftMarkerSealStatusV1::Sealed(proof, _) = storage
        .draft_marker_seal_status(&store, request.key())
        .unwrap()
    else {
        panic!("ordered marker seal did not close");
    };
    let expected_digest = beryl_model::advance_sequential_marker_digest(
        beryl_model::advance_sequential_marker_digest(
            beryl_model::sequential_marker_digest_seed(),
            second.marker_id(),
            second.label(),
        ),
        first.marker_id(),
        first.label(),
    );
    assert_eq!(proof.sequential().marker_digest(), expected_digest);
    assert_eq!(proof.sequential().marker_count(), 2);
    assert_eq!(
        proof.sequential().maximum_image_label(),
        Some(second.label())
    );
    let expected_assets = beryl_model::advance_ordered_marker_asset_digest(
        beryl_model::advance_ordered_marker_asset_digest(
            beryl_model::ordered_marker_asset_digest_seed(),
            second.marker_id(),
            second.label(),
            second.asset_id(),
        ),
        first.marker_id(),
        first.label(),
        first.asset_id(),
    );
    assert_eq!(
        proof.ordered_assets().marker_asset_digest(),
        expected_assets
    );
    assert_eq!(proof.ordered_assets().marker_count(), 2);
}

fn complete_marker_edit_for_seal(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacement: DraftPieceReplacementV1,
) -> DraftEditorCandidateSessionV1 {
    let (prepared, identity, _) = stage_replacement(
        storage,
        store,
        session,
        operation,
        replacement,
        session.logical_extent(),
    );
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap_or_else(|error| panic!("marker operation {operation} failed: {error:?}"))
    {
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    active_session(storage, store, session.draft_id(), session.session_id())
}
