include!("phase154_durable_builder/support.rs");

use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, OrderedMarkerAssetSummaryV1,
    SealedAssetReferenceSetProof,
};
use syndic_storage::{
    CapturedDraftEditorCandidatePublicationSourceV1, DraftEditorCandidateActivationBindingV1,
    DraftEditorCandidatePublicationEvidenceV1, DraftEditorCandidatePublicationOutcomeV1,
    DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1, DraftMarkerSealOperationIdV1,
    DraftMarkerSealProofV1, DraftMarkerSealRequestV1, DraftMarkerSealStatusV1,
    DraftRootHistoryPairV1,
};

#[test]
fn publication_evidence_branches_are_exact_and_rejections_are_nonpublishing() {
    let (_home, store, storage, thread) = fixture("phase169-publication-evidence", 180);
    let initial = current(&storage, &store, thread);
    let initial_empty_seal = seal_root(&storage, &store, initial.draft().piece_root(), 181);
    let empty_asset = asset_proof(initial_empty_seal, 182);

    let mut changed_nonempty = open_session(&storage, &store, &initial, 183, 184);
    changed_nonempty = complete_staged(
        &storage,
        &store,
        &changed_nonempty,
        185,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("abc".into())]),
        DraftLogicalExtentV1::new(3, 1),
    );
    let value = marker(186, 7, 9);
    changed_nonempty = complete_staged(
        &storage,
        &store,
        &changed_nonempty,
        187,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(value)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    value,
                    DraftPieceMarkerEffectChargesV1::for_marker(value),
                ),
            )),
        DraftLogicalExtentV1::new(1, 1),
    );
    let same_commitment_wrong_root_seal =
        seal_root(&storage, &store, changed_nonempty.newest_root(), 188);
    changed_nonempty = complete_staged(
        &storage,
        &store,
        &changed_nonempty,
        189,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("z".into())]),
        DraftLogicalExtentV1::new(2, 1),
    );
    let changed_nonempty_captured = changed_nonempty.clone();
    let changed_nonempty_seal = seal_root(
        &storage,
        &store,
        changed_nonempty_captured.newest_root(),
        190,
    );
    let nonempty_asset = asset_proof(changed_nonempty_seal, 191);
    let wrong_ordered_asset = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([197; 16]),
        changed_nonempty_seal.sequential(),
        OrderedMarkerAssetSummaryV1::new(
            [0xF1; 32],
            changed_nonempty_seal.ordered_assets().marker_count(),
        ),
        changed_nonempty_seal.sequential().marker_count(),
        AssetReferenceSetDigest::from_bytes([198; 32]),
    )
    .unwrap();

    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &initial,
            &changed_nonempty_captured,
            192,
            DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
                seal_proof: same_commitment_wrong_root_seal,
                asset_proof: nonempty_asset,
            },
        ),
    );
    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &initial,
            &changed_nonempty_captured,
            199,
            DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
                seal_proof: changed_nonempty_seal,
                asset_proof: wrong_ordered_asset,
            },
        ),
    );
    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &initial,
            &changed_nonempty_captured,
            193,
            DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
                seal_proof: changed_nonempty_seal,
                asset_proof: empty_asset,
            },
        ),
    );
    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &initial,
            &changed_nonempty_captured,
            194,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        ),
    );
    publish(
        &storage,
        &store,
        publication_request(
            &initial,
            &changed_nonempty_captured,
            195,
            DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
                seal_proof: changed_nonempty_seal,
                asset_proof: nonempty_asset,
            },
        ),
    );
}

#[test]
fn unchanged_nonempty_requires_the_exact_branch_and_asset_summary() {
    let (_home, store, storage, thread) = fixture("phase169-unchanged-nonempty", 20);
    let initial = current(&storage, &store, thread);
    let empty_seal = seal_root(&storage, &store, initial.draft().piece_root(), 21);
    let empty_asset = asset_proof(empty_seal, 22);
    let mut session = open_session(&storage, &store, &initial, 23, 24);
    session = complete_staged(
        &storage,
        &store,
        &session,
        25,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".into())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let value = marker(26, 5, 7);
    session = complete_staged(
        &storage,
        &store,
        &session,
        27,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(value)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    value,
                    DraftPieceMarkerEffectChargesV1::for_marker(value),
                ),
            )),
        DraftLogicalExtentV1::new(1, 1),
    );
    let changed = session.clone();
    let changed_seal = seal_root(&storage, &store, changed.newest_root(), 28);
    let nonempty_asset = asset_proof(changed_seal, 29);
    let changed_request = publication_request(
        &initial,
        &changed,
        30,
        DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
            seal_proof: changed_seal,
            asset_proof: nonempty_asset,
        },
    );
    let changed_source = capture_publication_source(&storage, &store, changed_request);
    let changed_prepared = storage
        .prepare_draft_editor_candidate_publication(
            &store,
            changed_source,
            changed_request.evidence(),
        )
        .unwrap();
    session = complete_staged(
        &storage,
        &store,
        &session,
        31,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("z".into())]),
        DraftLogicalExtentV1::new(2, 1),
    );
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(
            storage.revision(&store).unwrap(),
            changed_prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &changed_prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));

    let nonempty = current(&storage, &store, thread);
    let unchanged = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert_ne!(unchanged.newest_root(), nonempty.draft().piece_root());
    assert_eq!(
        unchanged.newest_root().marker_commitment(),
        nonempty.draft().piece_root().marker_commitment()
    );
    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &nonempty,
            &unchanged,
            32,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedNonempty {
                asset_proof: empty_asset,
            },
        ),
    );
    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &nonempty,
            &unchanged,
            33,
            DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
                seal_proof: changed_seal,
                asset_proof: nonempty_asset,
            },
        ),
    );
    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &nonempty,
            &unchanged,
            34,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        ),
    );
    publish(
        &storage,
        &store,
        publication_request(
            &nonempty,
            &unchanged,
            35,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedNonempty {
                asset_proof: nonempty_asset,
            },
        ),
    );
}

#[test]
fn changed_empty_requires_the_exact_empty_seal_and_rejects_nonempty_evidence() {
    let (_home, store, storage, thread) = fixture("phase169-changed-empty", 60);
    let initial = current(&storage, &store, thread);
    let initial_empty_seal = seal_root(&storage, &store, initial.draft().piece_root(), 61);
    let mut session = open_session(&storage, &store, &initial, 62, 63);
    session = complete_staged(
        &storage,
        &store,
        &session,
        64,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".into())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let value = marker(65, 3, 5);
    let marker_position = point(1);
    session = complete_staged(
        &storage,
        &store,
        &session,
        66,
        DraftPieceReplacementV1::new(
            marker_position,
            marker_position,
            vec![DraftPieceV1::Marker(value)],
        )
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                1,
                value,
                DraftPieceMarkerEffectChargesV1::for_marker(value),
            ),
        )),
        DraftLogicalExtentV1::new(3, 1),
    );
    let changed_nonempty = session.clone();
    let nonempty_seal = seal_root(&storage, &store, changed_nonempty.newest_root(), 67);
    let nonempty_asset = asset_proof(nonempty_seal, 68);
    let changed_request = publication_request(
        &initial,
        &changed_nonempty,
        69,
        DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
            seal_proof: nonempty_seal,
            asset_proof: nonempty_asset,
        },
    );
    let changed_source = capture_publication_source(&storage, &store, changed_request);
    let changed_prepared = storage
        .prepare_draft_editor_candidate_publication(
            &store,
            changed_source,
            changed_request.evidence(),
        )
        .unwrap();
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), value.marker_id())
        .unwrap()
        .unwrap();
    let marker_position = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    session = complete_staged(
        &storage,
        &store,
        &session,
        70,
        DraftPieceReplacementV1::new(marker_position, marker_position, Vec::new())
            .with_marker_effect(DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(marker_position, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(value),
            }),
        DraftLogicalExtentV1::new(3, 1),
    );
    let changed_empty_seal = seal_root(&storage, &store, session.newest_root(), 71);
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(
            storage.revision(&store).unwrap(),
            changed_prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &changed_prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let nonempty = current(&storage, &store, thread);
    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &nonempty,
            &session,
            72,
            DraftEditorCandidatePublicationEvidenceV1::ChangedEmpty {
                seal_proof: initial_empty_seal,
            },
        ),
    );
    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &nonempty,
            &session,
            73,
            DraftEditorCandidatePublicationEvidenceV1::ChangedEmpty {
                seal_proof: nonempty_seal,
            },
        ),
    );
    assert_rejected_without_publication(
        &storage,
        &store,
        thread,
        publication_request(
            &nonempty,
            &session,
            74,
            DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
                seal_proof: changed_empty_seal,
                asset_proof: nonempty_asset,
            },
        ),
    );
    publish(
        &storage,
        &store,
        publication_request(
            &nonempty,
            &session,
            75,
            DraftEditorCandidatePublicationEvidenceV1::ChangedEmpty {
                seal_proof: changed_empty_seal,
            },
        ),
    );
}

fn publication_request(
    current: &syndic_storage::SyndicCurrentDraft,
    head: &DraftEditorCandidateSessionV1,
    operation: u8,
    evidence: DraftEditorCandidatePublicationEvidenceV1,
) -> DraftEditorCandidatePublicationRequestV1 {
    DraftEditorCandidatePublicationRequestV1::new(
        selector(current),
        head.session_id(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        head.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
        evidence,
        SyndicTimestamp::from_unix_millis(operation.into()),
    )
}

fn assert_rejected_without_publication(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    request: DraftEditorCandidatePublicationRequestV1,
) {
    let before = current(&storage, store, thread);
    let revision = storage.revision(store).unwrap();
    let source = capture_publication_source(storage, store, request);
    assert!(
        storage
            .prepare_draft_editor_candidate_publication(store, source, request.evidence())
            .is_err()
    );
    assert_eq!(storage.revision(store).unwrap(), revision);
    assert_eq!(current(storage, store, thread), before);
}

fn publish(
    storage: &SyndicStorage,
    store: &HomeStore,
    request: DraftEditorCandidatePublicationRequestV1,
) {
    let source = capture_publication_source(storage, store, request);
    let prepared = storage
        .prepare_draft_editor_candidate_publication(store, source, request.evidence())
        .unwrap();
    let outcome = execute(
        store,
        storage.publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
}

fn capture_publication_source(
    storage: &SyndicStorage,
    store: &HomeStore,
    request: DraftEditorCandidatePublicationRequestV1,
) -> CapturedDraftEditorCandidatePublicationSourceV1 {
    let head = active_session(
        storage,
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

fn asset_proof(seal: DraftMarkerSealProofV1, seed: u8) -> SealedAssetReferenceSetProof {
    SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([seed; 16]),
        seal.sequential(),
        seal.ordered_assets(),
        seal.sequential().marker_count(),
        AssetReferenceSetDigest::from_bytes([seed.wrapping_add(1); 32]),
    )
    .unwrap()
}
