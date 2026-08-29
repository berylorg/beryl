include!("phase154_durable_builder/support.rs");

use beryl_model::{AssetReferenceSetDigest, AssetReferenceSetId, SealedAssetReferenceSetProof};
use syndic_storage::{
    CapturedDraftEditorCandidatePublicationSourceV1, DraftEditorCandidateActivationBindingV1,
    DraftEditorCandidatePublicationEvidenceV1, DraftEditorCandidatePublicationOutcomeV1,
    DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1, DraftMarkerSealOperationIdV1,
    DraftMarkerSealProofV1, DraftMarkerSealRequestV1, DraftMarkerSealStatusV1,
    DraftRootHistoryPairV1, canonical_empty_draft_marker_commitment_v1,
    test_faults::{
        DraftPieceCandidateRootCollision, inject_draft_piece_candidate_root_collision,
        reset_syndic_point_read_count, syndic_point_read_count,
    },
};

#[test]
fn asset_identity_changes_structural_and_ordered_but_not_sequential_commitments() {
    let (_home, store, storage, thread) = fixture("phase169-asset-bound-marker", 211);
    let durable = current(&storage, &store, thread);
    let first = marker(212, 9, 4);
    let second = DraftPieceMarkerV1::new(
        first.marker_id(),
        first.order_key(),
        first.label(),
        beryl_model::AssetId::sha256_v1([0xED; 32], std::num::NonZeroU64::new(999).unwrap()),
    );

    let build = |session_seed, operation_seed, marker| {
        let session = open_session(&storage, &store, &durable, session_seed, operation_seed);
        let session = complete_staged(
            &storage,
            &store,
            &session,
            operation_seed.wrapping_add(1),
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("x".to_owned())],
            ),
            DraftLogicalExtentV1::new(1, 1),
        );
        complete_staged(
            &storage,
            &store,
            &session,
            operation_seed.wrapping_add(2),
            DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
                .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        1,
                        marker,
                        DraftPieceMarkerEffectChargesV1::for_marker(marker),
                    ),
                )),
            DraftLogicalExtentV1::new(1, 1),
        )
    };
    let first_session = build(213, 214, first);
    let second_session = build(216, 217, second);

    assert_ne!(
        first_session.newest_root().summary().root_digest(),
        second_session.newest_root().summary().root_digest()
    );
    assert_ne!(
        first_session.newest_root().marker_commitment(),
        second_session.newest_root().marker_commitment()
    );
    let first_seal = seal_root(&storage, &store, first_session.newest_root(), 219);
    let second_seal = seal_root(&storage, &store, second_session.newest_root(), 220);
    assert_eq!(first_seal.sequential(), second_seal.sequential());
    assert_ne!(first_seal.ordered_assets(), second_seal.ordered_assets());
}

#[test]
fn marker_order_commitment_is_structural_reused_published_and_restart_visible() {
    let (home, store, storage, thread) = fixture_with_marker_limit("phase169-commitment", 180, 8);
    let durable = current(&storage, &store, thread);
    let empty_root = durable.draft().piece_root();
    let empty = empty_root.marker_commitment();
    assert_eq!(empty, canonical_empty_draft_marker_commitment_v1());
    assert_eq!(empty_root.marker_order_root(), None);

    let mut session = open_session(&storage, &store, &durable, 181, 182);
    session = complete_staged(
        &storage,
        &store,
        &session,
        183,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".to_owned())],
        ),
        DraftLogicalExtentV1::new(3, 1),
    );
    assert_eq!(session.newest_root().marker_commitment(), empty);
    assert_eq!(session.newest_root().marker_order_root(), None);

    let first = marker(184, 7, 9);
    session = complete_marker_edit(
        &storage,
        &store,
        &session,
        185,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(first)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    first,
                    DraftPieceMarkerEffectChargesV1::for_marker(first),
                ),
            )),
    );
    let first_root = session.newest_root();
    let first_commitment = first_root.marker_commitment();
    assert_eq!(first_commitment.marker_count(), 1);
    assert_eq!(first_commitment.maximum_image_label(), Some(first.label()));
    assert!(first_root.marker_order_height() > 0);
    assert!(first_root.marker_order_root().is_some());

    let second = marker(186, 6, 9);
    let before_all = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    session = complete_marker_edit(
        &storage,
        &store,
        &session,
        187,
        DraftPieceReplacementV1::new(before_all, before_all, vec![DraftPieceV1::Marker(second)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    second,
                    DraftPieceMarkerEffectChargesV1::for_marker(second),
                ),
            )),
    );
    let ordered_root = session.newest_root();
    let ordered = ordered_root.marker_commitment();
    assert_eq!(ordered.marker_count(), 2);
    assert_eq!(ordered.maximum_image_label(), Some(first.label()));
    assert_ne!(ordered, first_commitment);
    assert_ne!(
        ordered_root.marker_order_root(),
        first_root.marker_order_root()
    );

    let before_text_root = session.newest_root();
    session = complete_staged(
        &storage,
        &store,
        &session,
        188,
        DraftPieceReplacementV1::new(point(3), point(3), vec![DraftPieceV1::Text("z".to_owned())]),
        DraftLogicalExtentV1::new(4, 1),
    );
    assert_ne!(session.newest_root(), before_text_root);
    assert_eq!(session.newest_root().marker_commitment(), ordered);
    assert_eq!(
        session.newest_root().marker_order_root(),
        ordered_root.marker_order_root()
    );

    let seal_proof = seal_root(&storage, &store, session.newest_root(), 190);
    let asset_proof = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([191; 16]),
        seal_proof.sequential(),
        seal_proof.ordered_assets(),
        seal_proof.sequential().marker_count(),
        AssetReferenceSetDigest::from_bytes([192; 32]),
    )
    .unwrap();
    let evidence = DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
        seal_proof,
        asset_proof,
    };
    let request = publication_request(&durable, &session, 189, 2, evidence);
    let source = capture_publication_source(&storage, &store, request);
    let replay_source = capture_publication_source(&storage, &store, request);
    reset_syndic_point_read_count();
    let prepared = storage
        .prepare_draft_editor_candidate_publication(&store, source, request.evidence())
        .unwrap();
    let publication_point_reads = syndic_point_read_count();
    assert!(
        publication_point_reads <= 64,
        "publication used {publication_point_reads} point reads"
    );
    assert_eq!(prepared.marker_commitment(), ordered);
    let command_outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), prepared.clone()),
    );
    let published = storage
        .reconcile_draft_editor_candidate_publication(&store, &prepared, command_outcome)
        .unwrap();
    assert!(matches!(
        published,
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    assert_eq!(published.marker_commitment(), Some(ordered));

    let replay = storage
        .prepare_draft_editor_candidate_publication(&store, replay_source, request.evidence())
        .unwrap();
    let command_outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), replay.clone()),
    );
    let replayed = storage
        .reconcile_draft_editor_candidate_publication(&store, &replay, command_outcome)
        .unwrap();
    let DraftEditorCandidatePublicationOutcomeV1::ExactReplay(receipt) = replayed else {
        panic!("publication did not replay exactly");
    };
    assert_eq!(receipt.marker_commitment(), ordered);

    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let reopened = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(reopened.marker_commitment(), ordered);
    assert_eq!(
        reopened.marker_order_root(),
        ordered_root.marker_order_root()
    );
    assert_eq!(
        storage
            .draft_piece_root(&store, reopened)
            .unwrap()
            .unwrap()
            .reference(),
        reopened
    );
}

#[test]
fn persisted_marker_commitment_corruption_fails_publication_closed() {
    let (_home, store, storage, thread) = fixture("phase169-corruption", 200);
    let durable = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &durable, 201, 202);
    session = complete_staged(
        &storage,
        &store,
        &session,
        203,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let value = marker(204, 5, 11);
    session = complete_marker_edit(
        &storage,
        &store,
        &session,
        205,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(value)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    value,
                    DraftPieceMarkerEffectChargesV1::for_marker(value),
                ),
            )),
    );
    let seal_proof = seal_root(&storage, &store, session.newest_root(), 207);
    let asset_proof = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([208; 16]),
        seal_proof.sequential(),
        seal_proof.ordered_assets(),
        seal_proof.sequential().marker_count(),
        AssetReferenceSetDigest::from_bytes([209; 32]),
    )
    .unwrap();
    let request = publication_request(
        &durable,
        &session,
        206,
        2,
        DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
            seal_proof,
            asset_proof,
        },
    );
    let source = capture_publication_source(&storage, &store, request);
    committed(execute(
        &store,
        inject_draft_piece_candidate_root_collision(
            &store,
            &storage,
            session.newest_root(),
            DraftPieceCandidateRootCollision::MarkerCommitmentDigest,
        ),
    ));
    assert!(
        storage
            .prepare_draft_editor_candidate_publication(&store, source, request.evidence(),)
            .is_err()
    );
    assert!(
        storage
            .draft_piece_root(&store, session.newest_root())
            .is_err()
    );
}

fn complete_marker_edit(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacement: DraftPieceReplacementV1,
) -> DraftEditorCandidateSessionV1 {
    complete_staged(
        storage,
        store,
        session,
        operation,
        replacement,
        session.logical_extent(),
    )
}

fn publication_request(
    current: &syndic_storage::SyndicCurrentDraft,
    head: &DraftEditorCandidateSessionV1,
    operation: u8,
    at: u64,
    evidence: DraftEditorCandidatePublicationEvidenceV1,
) -> DraftEditorCandidatePublicationRequestV1 {
    DraftEditorCandidatePublicationRequestV1::new(
        selector(current),
        head.session_id(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        head.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
        evidence,
        SyndicTimestamp::from_unix_millis(at),
    )
}

fn capture_publication_source(
    storage: &SyndicStorage,
    store: &HomeStore,
    request: DraftEditorCandidatePublicationRequestV1,
) -> CapturedDraftEditorCandidatePublicationSourceV1 {
    let head = active_session(
        &storage,
        store,
        request.selector().draft_id(),
        request.session_id(),
    );
    let candidate = DraftEditorCandidateActivationBindingV1::new(
        request.selector().draft_id(),
        request.session_id(),
        head.session_generation(),
        request.candidate_generation(),
        request.candidate().root(),
        request.candidate().history(),
        request.candidate().root().summary().logical_extent(),
    );
    storage
        .capture_draft_editor_candidate_publication_source(
            store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                request.selector(),
                candidate,
                request.operation_id(),
                request.published_at(),
            ),
        )
        .unwrap()
}

fn seal_root(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    operation: u8,
) -> DraftMarkerSealProofV1 {
    let request = DraftMarkerSealRequestV1::new(
        root,
        DraftMarkerSealOperationIdV1::from_bytes([operation; 16]),
    );
    let begin = storage
        .prepare_draft_marker_seal_begin(store, request)
        .unwrap();
    committed(execute(
        store,
        storage.begin_draft_marker_seal(storage.revision(store).unwrap(), begin),
    ));
    while let Some(advance) = storage
        .prepare_draft_marker_seal_advance(store, request.key())
        .unwrap()
    {
        committed(execute(
            store,
            storage.advance_draft_marker_seal(storage.revision(store).unwrap(), &advance),
        ));
    }
    let DraftMarkerSealStatusV1::Sealed(proof, _) = storage
        .draft_marker_seal_status(store, request.key())
        .unwrap()
    else {
        panic!("marker seal did not close");
    };
    proof
}

fn fixture_with_marker_limit(
    name: &str,
    seed: u8,
    maximum_markers: u64,
) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase169",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, maximum_markers).unwrap(),
            ),
        ),
    ));
    (home, store, storage, thread)
}
