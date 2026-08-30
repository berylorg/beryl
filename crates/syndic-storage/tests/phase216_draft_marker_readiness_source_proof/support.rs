use super::*;

pub(super) fn manual_root_reference_bytes(root: DraftPieceRootReferenceV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(root.key().draft_id().as_bytes());
    match root.key().build_identity() {
        DraftPieceRootBuildIdentityV1::DirectCanonicalEmpty { operation_id } => {
            bytes.push(0);
            bytes.extend_from_slice(&[0; 16]);
            bytes.extend_from_slice(operation_id.as_bytes());
        }
        DraftPieceRootBuildIdentityV1::EditorCandidate {
            session_id,
            operation_id,
        } => {
            bytes.push(1);
            bytes.extend_from_slice(session_id.as_bytes());
            bytes.extend_from_slice(operation_id.as_bytes());
        }
    }
    match root.root_node() {
        Some(id) => {
            bytes.push(1);
            bytes.extend_from_slice(id.as_bytes());
        }
        None => {
            bytes.push(0);
            bytes.extend_from_slice(&[0; 16]);
        }
    }
    let summary = root.summary();
    bytes.extend_from_slice(&summary.logical_utf8_bytes().to_le_bytes());
    bytes.extend_from_slice(&summary.newline_count().to_le_bytes());
    bytes.extend_from_slice(&summary.logical_line_count().to_le_bytes());
    bytes.extend_from_slice(&summary.piece_count().to_le_bytes());
    bytes.extend_from_slice(&summary.marker_count().to_le_bytes());
    bytes.extend_from_slice(summary.marker_digest().as_bytes());
    bytes.push(summary.height());
    bytes.extend_from_slice(summary.root_digest().as_bytes());
    match root.marker_index_root() {
        Some(id) => {
            bytes.push(1);
            bytes.extend_from_slice(id.as_bytes());
        }
        None => {
            bytes.push(0);
            bytes.extend_from_slice(&[0; 16]);
        }
    }
    let index = root.marker_index_summary();
    bytes.extend_from_slice(&index.record_count().to_le_bytes());
    bytes.push(index.height());
    bytes.extend_from_slice(index.root_digest().as_bytes());
    match root.marker_order_root() {
        Some(id) => {
            bytes.push(1);
            bytes.extend_from_slice(id.as_bytes());
        }
        None => {
            bytes.push(0);
            bytes.extend_from_slice(&[0; 16]);
        }
    }
    bytes.push(root.marker_order_height());
    let commitment = root.marker_commitment();
    bytes.extend_from_slice(&commitment.tree_root_digest());
    bytes.extend_from_slice(&commitment.marker_count().to_le_bytes());
    match commitment.maximum_image_label() {
        Some(label) => bytes.extend_from_slice(&label.get().to_le_bytes()),
        None => bytes.extend_from_slice(&0_u64.to_le_bytes()),
    }
    bytes.extend_from_slice(root.combined_digest().as_bytes());
    assert_eq!(bytes.len(), 327);
    bytes
}

pub(super) fn manual_candidate_entry(
    session: &DraftEditorCandidateSessionV1,
    marker: DraftPieceMarkerV1,
) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.push(0);
    entry.push(0);
    entry.extend_from_slice(session.draft_id().as_bytes());
    entry.extend_from_slice(session.session_id().as_bytes());
    entry.extend_from_slice(&session.newest_candidate_generation().to_le_bytes());
    entry.extend_from_slice(&manual_root_reference_bytes(session.newest_root()));
    entry.extend_from_slice(marker.marker_id().as_bytes());
    entry.extend_from_slice(&marker.label().get().to_le_bytes());
    let asset = marker.asset_id();
    entry.push(asset.version() as u8);
    entry.extend_from_slice(&asset.digest());
    entry.extend_from_slice(&asset.length().get().to_le_bytes());
    entry
}

pub(super) fn manual_cut_entry(
    settlement: DraftPieceSettlementKeyV1,
    successor: &DraftEditorCandidateSessionV1,
    marker: DraftPieceMarkerV1,
) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.push(0);
    entry.push(1);
    entry.extend_from_slice(settlement.draft_id().as_bytes());
    entry.extend_from_slice(settlement.session_id().as_bytes());
    entry.extend_from_slice(settlement.operation_id().as_bytes());
    entry.extend_from_slice(&successor.newest_candidate_generation().to_le_bytes());
    entry.extend_from_slice(&manual_root_reference_bytes(successor.newest_root()));
    entry.extend_from_slice(marker.marker_id().as_bytes());
    entry.extend_from_slice(&marker.label().get().to_le_bytes());
    let asset = marker.asset_id();
    entry.push(asset.version() as u8);
    entry.extend_from_slice(&asset.digest());
    entry.extend_from_slice(&asset.length().get().to_le_bytes());
    entry
}

pub(super) fn manual_correlation(ordinal: NonZeroU64, eof: bool, entry: &[u8]) -> [u8; 32] {
    let mut preimage = Vec::new();
    preimage.extend_from_slice(b"syndic/draft-marker-label-readiness-page/v1");
    preimage.extend_from_slice(&ordinal.get().to_le_bytes());
    preimage.push(u8::from(eof));
    preimage.extend_from_slice(&1_u64.to_le_bytes());
    preimage.extend_from_slice(entry);
    Sha256::digest(preimage).into()
}

pub(super) fn owner(
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> DraftMarkerAdmissionOwnerV1 {
    DraftMarkerAdmissionOwnerV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes([operation; 16]),
    )
}

pub(super) fn complete_marker_edit(
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
        .unwrap()
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

pub(super) fn source(
    session: &DraftEditorCandidateSessionV1,
    marker_id: SyndicDraftMarkerId,
) -> DraftMarkerReadinessSourceSelectorV1 {
    DraftMarkerReadinessSourceSelectorV1::Candidate(DraftMarkerReadinessCandidateSourceV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        marker_id,
    ))
}

pub(super) fn association(
    target: u8,
    session: &DraftEditorCandidateSessionV1,
    marker_id: SyndicDraftMarkerId,
) -> DraftMarkerReadinessSourceAssociationV1 {
    DraftMarkerReadinessSourceAssociationV1::new(
        SyndicDraftMarkerId::from_bytes([target; 16]),
        source(session, marker_id),
    )
}

pub(super) fn marked_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    seed: u8,
) -> (DraftEditorCandidateSessionV1, DraftPieceMarkerV1) {
    let durable = current(storage, store, thread);
    let mut session = open_session(storage, store, &durable, seed, seed.wrapping_add(1));
    session = complete_staged(
        storage,
        store,
        &session,
        seed.wrapping_add(2),
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let marker = marker(seed.wrapping_add(3), 1, 7);
    session = complete_marker_edit(
        storage,
        store,
        &session,
        seed.wrapping_add(4),
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
    );
    (session, marker)
}

pub(super) fn two_marked_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    seed: u8,
) -> (
    DraftEditorCandidateSessionV1,
    DraftPieceMarkerV1,
    DraftPieceMarkerV1,
) {
    let (session, first) = marked_session(storage, store, thread, seed);
    let second = marker(seed.wrapping_add(5), 0, 9);
    let before_all = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let session = complete_marker_edit(
        storage,
        store,
        &session,
        seed.wrapping_add(6),
        DraftPieceReplacementV1::new(before_all, before_all, vec![DraftPieceV1::Marker(second)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    second,
                    DraftPieceMarkerEffectChargesV1::for_marker(second),
                ),
            )),
    );
    (session, first, second)
}
