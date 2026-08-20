use beryl_home_store::RecordVersion;
use beryl_model::{
    DraftRevision, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId, SyndicThreadId,
    ThreadRevision,
};

use crate::codec::parts::{Decoder, Encoder};
use crate::codec::{CodecError, ExactCodec, Family, SMALL_MAX, invalid};

use super::*;

pub(crate) struct DraftPieceRootsFamily;
pub(crate) struct DraftPieceNodesFamily;
pub(crate) struct DraftPieceLeavesFamily;
pub(crate) struct DraftMarkerIdentityIndexFamily;
pub(crate) struct DraftPieceBuildsFamily;
pub(crate) struct DraftPieceBuildFragmentsFamily;
pub(crate) struct DraftPieceBuildProgressFamily;
pub(crate) struct DraftPieceSettlementsFamily;
pub(crate) struct DraftEditorCandidateSessionsFamily;

pub(crate) type DraftPieceRootsCodec = ExactCodec<DraftPieceRootsFamily>;
pub(crate) type DraftPieceNodesCodec = ExactCodec<DraftPieceNodesFamily>;
pub(crate) type DraftPieceLeavesCodec = ExactCodec<DraftPieceLeavesFamily>;
pub(crate) type DraftMarkerIdentityIndexCodec = ExactCodec<DraftMarkerIdentityIndexFamily>;
pub(crate) type DraftPieceBuildsCodec = ExactCodec<DraftPieceBuildsFamily>;
pub(crate) type DraftPieceBuildFragmentsCodec = ExactCodec<DraftPieceBuildFragmentsFamily>;
pub(crate) type DraftPieceBuildProgressCodec = ExactCodec<DraftPieceBuildProgressFamily>;
pub(crate) type DraftPieceSettlementsCodec = ExactCodec<DraftPieceSettlementsFamily>;
pub(crate) type DraftEditorCandidateSessionsCodec = ExactCodec<DraftEditorCandidateSessionsFamily>;

fn enc_root_key(e: &mut Encoder, key: DraftPieceRootKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    match key.build_identity() {
        DraftPieceRootBuildIdentityV1::DirectCanonicalEmpty { operation_id } => {
            e.u8(0);
            e.fixed16(operation_id.as_bytes());
        }
        DraftPieceRootBuildIdentityV1::EditorCandidate {
            session_id,
            operation_id,
        } => {
            e.u8(1);
            e.fixed16(session_id.as_bytes());
            e.fixed16(operation_id.as_bytes());
        }
    }
}

fn dec_root_key(d: &mut Decoder<'_>) -> Result<DraftPieceRootKeyV1, CodecError> {
    let draft_id = SyndicDraftId::from_bytes(d.fixed16()?);
    match d.u8()? {
        0 => Ok(DraftPieceRootKeyV1::direct_canonical_empty(
            draft_id,
            DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
        )),
        1 => Ok(DraftPieceRootKeyV1::editor_candidate(
            draft_id,
            DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
            DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-piece root build identity",
            tag,
        }),
    }
}

fn enc_record_key(e: &mut Encoder, key: DraftPieceRecordKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    e.fixed16(key.id().as_bytes());
}

fn dec_record_key(d: &mut Decoder<'_>) -> Result<DraftPieceRecordKeyV1, CodecError> {
    Ok(DraftPieceRecordKeyV1::new(
        SyndicDraftId::from_bytes(d.fixed16()?),
        DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
    ))
}

fn enc_marker_identity_key(e: &mut Encoder, key: DraftMarkerIdentityRecordKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    e.u8(match key.kind() {
        DraftMarkerIdentityRecordKindV1::Internal => 0,
        DraftMarkerIdentityRecordKindV1::Leaf => 1,
    });
    e.fixed16(key.id().as_bytes());
}

fn dec_marker_identity_key(
    d: &mut Decoder<'_>,
) -> Result<DraftMarkerIdentityRecordKeyV1, CodecError> {
    let draft_id = SyndicDraftId::from_bytes(d.fixed16()?);
    let kind = match d.u8()? {
        0 => DraftMarkerIdentityRecordKindV1::Internal,
        1 => DraftMarkerIdentityRecordKindV1::Leaf,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft marker-index record kind",
                tag,
            });
        }
    };
    Ok(DraftMarkerIdentityRecordKeyV1::new(
        draft_id,
        kind,
        DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
    ))
}

fn enc_settlement_key(e: &mut Encoder, key: DraftPieceSettlementKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    e.fixed16(key.session_id().as_bytes());
    e.fixed16(key.operation_id().as_bytes());
}

fn dec_settlement_key(d: &mut Decoder<'_>) -> Result<DraftPieceSettlementKeyV1, CodecError> {
    Ok(DraftPieceSettlementKeyV1::new(
        SyndicDraftId::from_bytes(d.fixed16()?),
        DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
        DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
    ))
}

fn enc_progress_key(e: &mut Encoder, key: DraftPieceBuildProgressReceiptKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    e.fixed16(key.session_id().as_bytes());
    e.fixed16(key.operation_id().as_bytes());
    e.u64(key.transition_ordinal());
}

fn dec_progress_key(
    d: &mut Decoder<'_>,
) -> Result<DraftPieceBuildProgressReceiptKeyV1, CodecError> {
    let key = DraftPieceBuildProgressReceiptKeyV1::new(
        SyndicDraftId::from_bytes(d.fixed16()?),
        DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
        DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
        d.u64()?,
    );
    if key.transition_ordinal() == 0 {
        return Err(CodecError::InvalidLength(
            "draft-piece progress transition ordinal",
        ));
    }
    Ok(key)
}

fn enc_progress_reference(e: &mut Encoder, reference: DraftPieceBuildProgressReceiptReferenceV1) {
    enc_progress_key(e, reference.key());
    enc_digest(e, reference.digest());
}

fn dec_progress_reference(
    d: &mut Decoder<'_>,
) -> Result<DraftPieceBuildProgressReceiptReferenceV1, CodecError> {
    Ok(DraftPieceBuildProgressReceiptReferenceV1::new(
        dec_progress_key(d)?,
        dec_digest(d)?,
    ))
}

fn enc_fragment_key(e: &mut Encoder, key: DraftPieceBuildFragmentKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    e.fixed16(key.session_id().as_bytes());
    e.fixed16(key.operation_id().as_bytes());
    e.u64(key.ordinal());
}

fn dec_fragment_key(d: &mut Decoder<'_>) -> Result<DraftPieceBuildFragmentKeyV1, CodecError> {
    let key = DraftPieceBuildFragmentKeyV1::new(
        SyndicDraftId::from_bytes(d.fixed16()?),
        DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
        DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
        d.u64()?,
    );
    if !key.is_locally_valid() {
        return Err(CodecError::InvalidLength("draft-piece fragment ordinal"));
    }
    Ok(key)
}

fn enc_digest(e: &mut Encoder, digest: DraftPieceDigestV1) {
    e.fixed32(digest.as_bytes());
}

fn dec_digest(d: &mut Decoder<'_>) -> Result<DraftPieceDigestV1, CodecError> {
    Ok(DraftPieceDigestV1::from_bytes(d.fixed32()?))
}

fn enc_text_summary(e: &mut Encoder, summary: DraftPieceTextSummaryV1) {
    e.u64(summary.logical_utf8_bytes());
    e.u64(summary.newline_count());
    e.u64(summary.logical_line_count());
}

fn dec_text_summary(d: &mut Decoder<'_>) -> Result<DraftPieceTextSummaryV1, CodecError> {
    let summary = DraftPieceTextSummaryV1::new(d.u64()?, d.u64()?, d.u64()?);
    if !summary.is_canonical() {
        return Err(CodecError::InvalidLength(
            "draft-piece logical text summary",
        ));
    }
    Ok(summary)
}

fn enc_summary(e: &mut Encoder, summary: DraftPieceSummaryV1) {
    enc_text_summary(e, summary.text_summary());
    e.u64(summary.piece_count());
    e.u64(summary.marker_count());
    enc_digest(e, summary.marker_digest());
    e.u8(summary.height());
    enc_digest(e, summary.root_digest());
}

fn dec_summary(d: &mut Decoder<'_>) -> Result<DraftPieceSummaryV1, CodecError> {
    let text = dec_text_summary(d)?;
    Ok(DraftPieceSummaryV1::new(
        text.logical_utf8_bytes(),
        text.newline_count(),
        text.logical_line_count(),
        d.u64()?,
        d.u64()?,
        dec_digest(d)?,
        d.u8()?,
        dec_digest(d)?,
    ))
}

fn enc_marker_index_summary(e: &mut Encoder, summary: DraftMarkerIdentityIndexSummaryV1) {
    e.u64(summary.record_count());
    e.u8(summary.height());
    enc_digest(e, summary.root_digest());
}

fn dec_marker_index_summary(
    d: &mut Decoder<'_>,
) -> Result<DraftMarkerIdentityIndexSummaryV1, CodecError> {
    Ok(DraftMarkerIdentityIndexSummaryV1::new(
        d.u64()?,
        d.u8()?,
        dec_digest(d)?,
    ))
}

fn enc_record_id_option(e: &mut Encoder, id: Option<DraftPieceRecordIdV1>) {
    match id {
        Some(id) => {
            e.u8(1);
            e.fixed16(id.as_bytes());
        }
        None => e.u8(0),
    }
}

fn dec_record_id_option(
    d: &mut Decoder<'_>,
    kind: &'static str,
) -> Result<Option<DraftPieceRecordIdV1>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(DraftPieceRecordIdV1::from_bytes(d.fixed16()?))),
        tag => Err(CodecError::InvalidTag { kind, tag }),
    }
}

fn enc_build_roots(e: &mut Encoder, roots: DraftPieceBuildRootsV1) {
    enc_record_id_option(e, roots.sequence_root());
    enc_summary(e, roots.sequence_summary());
    enc_record_id_option(e, roots.marker_index_root());
    enc_marker_index_summary(e, roots.marker_index_summary());
}

fn dec_build_roots(d: &mut Decoder<'_>) -> Result<DraftPieceBuildRootsV1, CodecError> {
    Ok(DraftPieceBuildRootsV1::new(
        dec_record_id_option(d, "draft-piece working sequence root")?,
        dec_summary(d)?,
        dec_record_id_option(d, "draft-piece working identity root")?,
        dec_marker_index_summary(d)?,
    ))
}

fn enc_build_boundary(e: &mut Encoder, boundary: DraftPieceBuildBoundaryV1) {
    e.u64(boundary.rank());
    e.u64(boundary.inner());
}

fn dec_build_boundary(d: &mut Decoder<'_>) -> Result<DraftPieceBuildBoundaryV1, CodecError> {
    Ok(DraftPieceBuildBoundaryV1::new(d.u64()?, d.u64()?))
}

fn enc_build_frontier(e: &mut Encoder, frontier: DraftPieceBuildFrontierV1) {
    match frontier {
        DraftPieceBuildFrontierV1::Receiving {
            next_ordinal,
            chain,
        } => {
            e.u8(0);
            e.u64(next_ordinal);
            enc_digest(e, chain);
        }
        DraftPieceBuildFrontierV1::ReconcilingMoves {
            fragment_ordinal,
            next_move,
        } => {
            e.u8(7);
            e.u64(fragment_ordinal);
            e.u64(next_move);
        }
        DraftPieceBuildFrontierV1::Planning { fragment_ordinal } => {
            e.u8(1);
            e.u64(fragment_ordinal);
        }
        DraftPieceBuildFrontierV1::Removing {
            fragment_ordinal,
            next_rank,
            end_rank,
            base_end,
            successor_start,
            successor_end,
        } => {
            e.u8(2);
            e.u64(fragment_ordinal);
            e.u64(next_rank);
            e.u64(end_rank);
            enc_build_boundary(e, base_end);
            enc_build_boundary(e, successor_start);
            enc_build_boundary(e, successor_end);
        }
        DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal,
            base_end,
            successor_start,
            successor_end,
        } => {
            e.u8(3);
            e.u64(fragment_ordinal);
            enc_build_boundary(e, base_end);
            enc_build_boundary(e, successor_start);
            enc_build_boundary(e, successor_end);
        }
        DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal,
            next_piece,
            next_byte,
            base_end,
            successor_end,
        } => {
            e.u8(4);
            e.u64(fragment_ordinal);
            e.u64(next_piece);
            e.u64(next_byte);
            enc_build_boundary(e, base_end);
            enc_build_boundary(e, successor_end);
        }
        DraftPieceBuildFrontierV1::CrossValidating => e.u8(5),
        DraftPieceBuildFrontierV1::Complete => e.u8(6),
    }
}

fn dec_build_frontier(d: &mut Decoder<'_>) -> Result<DraftPieceBuildFrontierV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftPieceBuildFrontierV1::Receiving {
            next_ordinal: d.u64()?,
            chain: dec_digest(d)?,
        }),
        1 => Ok(DraftPieceBuildFrontierV1::Planning {
            fragment_ordinal: d.u64()?,
        }),
        2 => Ok(DraftPieceBuildFrontierV1::Removing {
            fragment_ordinal: d.u64()?,
            next_rank: d.u64()?,
            end_rank: d.u64()?,
            base_end: dec_build_boundary(d)?,
            successor_start: dec_build_boundary(d)?,
            successor_end: dec_build_boundary(d)?,
        }),
        3 => Ok(DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal: d.u64()?,
            base_end: dec_build_boundary(d)?,
            successor_start: dec_build_boundary(d)?,
            successor_end: dec_build_boundary(d)?,
        }),
        4 => Ok(DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal: d.u64()?,
            next_piece: d.u64()?,
            next_byte: d.u64()?,
            base_end: dec_build_boundary(d)?,
            successor_end: dec_build_boundary(d)?,
        }),
        5 => Ok(DraftPieceBuildFrontierV1::CrossValidating),
        6 => Ok(DraftPieceBuildFrontierV1::Complete),
        7 => Ok(DraftPieceBuildFrontierV1::ReconcilingMoves {
            fragment_ordinal: d.u64()?,
            next_move: d.u64()?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-piece build frontier",
            tag,
        }),
    }
}

pub(crate) fn enc_root_reference(e: &mut Encoder, value: DraftPieceRootReferenceV1) {
    enc_root_key(e, value.key());
    match value.root_node() {
        Some(id) => {
            e.u8(1);
            e.fixed16(id.as_bytes());
        }
        None => e.u8(0),
    }
    enc_summary(e, value.summary());
    match value.marker_index_root() {
        Some(id) => {
            e.u8(1);
            e.fixed16(id.as_bytes());
        }
        None => e.u8(0),
    }
    enc_marker_index_summary(e, value.marker_index_summary());
    enc_digest(e, value.combined_digest());
}

pub(crate) fn canonical_root_reference_bytes(value: DraftPieceRootReferenceV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_root_reference(&mut e, value);
    e.finish()
}

pub(crate) fn dec_root_reference(
    d: &mut Decoder<'_>,
) -> Result<DraftPieceRootReferenceV1, CodecError> {
    let key = dec_root_key(d)?;
    let root_node = match d.u8()? {
        0 => None,
        1 => Some(DraftPieceRecordIdV1::from_bytes(d.fixed16()?)),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece root-node option",
                tag,
            });
        }
    };
    let summary = dec_summary(d)?;
    let marker_index_root = match d.u8()? {
        0 => None,
        1 => Some(DraftPieceRecordIdV1::from_bytes(d.fixed16()?)),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft marker-index root option",
                tag,
            });
        }
    };
    let marker_index_summary = dec_marker_index_summary(d)?;
    let combined_digest = dec_digest(d)?;
    Ok(DraftPieceRootReferenceV1::new(
        key,
        root_node,
        summary,
        marker_index_root,
        marker_index_summary,
        combined_digest,
    ))
}

pub(crate) fn canonical_session_open_request_bytes(
    value: DraftEditorCandidateSessionOpenRequestV1,
) -> Vec<u8> {
    let mut e = Encoder::new();
    e.bytes(b"syndic/draft-editor-candidate-session-open/v1");
    let selector = value.selector();
    e.fixed16(selector.thread_id().as_bytes());
    e.u64(selector.thread_revision().get());
    e.fixed16(selector.draft_id().as_bytes());
    e.u64(selector.selector_revision().get());
    enc_root_reference(&mut e, selector.root());
    enc_history_reference(&mut e, selector.history());
    e.fixed16(value.session_id().as_bytes());
    e.fixed16(value.operation_id().as_bytes());
    e.finish()
}

fn decode_canonical_session_open_request_bytes(
    bytes: &[u8],
) -> Result<DraftEditorCandidateSessionOpenRequestV1, CodecError> {
    let mut d = Decoder::new(bytes);
    if d.bytes("draft editor session open domain")?
        != b"syndic/draft-editor-candidate-session-open/v1"
    {
        return Err(CodecError::InvalidLength(
            "draft editor session open domain",
        ));
    }
    let selector = DraftEditorCurrentSelectorV1::new(
        SyndicThreadId::from_bytes(d.fixed16()?),
        ThreadRevision::new(d.u64()?)
            .map_err(|error| invalid("draft editor session thread revision", error))?,
        SyndicDraftId::from_bytes(d.fixed16()?),
        DraftRevision::new(d.u64()?)
            .map_err(|error| invalid("draft editor session selector revision", error))?,
        dec_root_reference(&mut d)?,
        dec_history_reference(&mut d)?,
    );
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        selector,
        DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
        DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
    );
    d.finish()?;
    if canonical_session_open_request_bytes(request) != bytes {
        return Err(CodecError::InvalidLength(
            "draft editor session open request",
        ));
    }
    Ok(request)
}

fn enc_session_key(e: &mut Encoder, key: DraftEditorCandidateSessionRecordKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    e.fixed16(key.session_id().as_bytes());
    match key {
        DraftEditorCandidateSessionRecordKeyV1::Head { .. } => e.u8(0),
        DraftEditorCandidateSessionRecordKeyV1::OpenReceipt { operation_id, .. } => {
            e.u8(1);
            e.fixed16(operation_id.as_bytes());
        }
    }
}

fn dec_session_key(
    d: &mut Decoder<'_>,
) -> Result<DraftEditorCandidateSessionRecordKeyV1, CodecError> {
    let draft_id = SyndicDraftId::from_bytes(d.fixed16()?);
    let session_id = DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?);
    match d.u8()? {
        0 => Ok(DraftEditorCandidateSessionRecordKeyV1::head(
            draft_id, session_id,
        )),
        1 => Ok(DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            draft_id,
            session_id,
            DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "draft editor session key",
            tag,
        }),
    }
}

fn enc_session_head(e: &mut Encoder, value: &DraftEditorCandidateSessionV1) {
    e.fixed16(value.thread_id().as_bytes());
    e.fixed16(value.draft_id().as_bytes());
    e.fixed16(value.session_id().as_bytes());
    e.fixed16(value.open_operation_id().as_bytes());
    e.u64(value.session_generation());
    e.u64(value.durable_base_selector_revision().get());
    enc_root_reference(e, value.durable_base_root());
    enc_history_reference(e, value.durable_base_history());
    e.u64(value.published_candidate_generation());
    e.u64(value.published_selector_revision().get());
    enc_root_reference(e, value.published_root());
    enc_history_reference(e, value.published_history());
    e.u64(value.newest_candidate_generation());
    enc_root_reference(e, value.newest_root());
    enc_history_reference(e, value.newest_history());
    e.u64(value.dirty_generation());
    e.u64(value.logical_extent().logical_utf8_bytes());
    e.u64(value.logical_extent().logical_line_count());
    e.u8(match value.lifecycle() {
        DraftEditorCandidateSessionLifecycleV1::Active => 0,
        DraftEditorCandidateSessionLifecycleV1::Disposed => 1,
    });
    match value.active_operation() {
        None => e.u8(0),
        Some(operation) => {
            e.u8(1);
            e.fixed16(operation.operation_id().as_bytes());
            enc_digest(e, operation.proposal_digest());
            e.u64(operation.predecessor_candidate_generation());
            enc_root_reference(e, operation.predecessor_root());
            enc_history_reference(e, operation.predecessor_history());
            enc_progress_reference(e, operation.receipt());
        }
    }
}

fn dec_session_head(d: &mut Decoder<'_>) -> Result<DraftEditorCandidateSessionV1, CodecError> {
    let thread_id = SyndicThreadId::from_bytes(d.fixed16()?);
    let draft_id = SyndicDraftId::from_bytes(d.fixed16()?);
    let session_id = DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?);
    let open_operation_id = DraftPieceOperationIdV1::from_bytes(d.fixed16()?);
    let session_generation = d.u64()?;
    let durable_base_selector_revision = DraftRevision::new(d.u64()?)
        .map_err(|error| invalid("draft editor session base selector", error))?;
    let durable_base_root = dec_root_reference(d)?;
    let durable_base_history = dec_history_reference(d)?;
    let published_candidate_generation = d.u64()?;
    let published_selector_revision = DraftRevision::new(d.u64()?)
        .map_err(|error| invalid("draft editor session published selector", error))?;
    let published_root = dec_root_reference(d)?;
    let published_history = dec_history_reference(d)?;
    let newest_candidate_generation = d.u64()?;
    let newest_root = dec_root_reference(d)?;
    let newest_history = dec_history_reference(d)?;
    let dirty_generation = d.u64()?;
    let logical_extent = DraftLogicalExtentV1::new(d.u64()?, d.u64()?);
    let lifecycle = match d.u8()? {
        0 => DraftEditorCandidateSessionLifecycleV1::Active,
        1 => DraftEditorCandidateSessionLifecycleV1::Disposed,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft editor session lifecycle",
                tag,
            });
        }
    };
    let active_operation = match d.u8()? {
        0 => None,
        1 => Some(DraftEditorActiveOperationV1::new(
            DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
            dec_digest(d)?,
            d.u64()?,
            dec_root_reference(d)?,
            dec_history_reference(d)?,
            dec_progress_reference(d)?,
        )),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft editor active operation option",
                tag,
            });
        }
    };
    let value = DraftEditorCandidateSessionV1::from_parts(
        thread_id,
        draft_id,
        session_id,
        open_operation_id,
        session_generation,
        durable_base_selector_revision,
        durable_base_root,
        durable_base_history,
        published_candidate_generation,
        published_selector_revision,
        published_root,
        published_history,
        newest_candidate_generation,
        newest_root,
        newest_history,
        dirty_generation,
        logical_extent,
        lifecycle,
        active_operation,
    );
    if !value.is_coherent() {
        return Err(CodecError::InvalidLength("draft editor session head"));
    }
    Ok(value)
}

fn encode_session_record(
    value: &DraftEditorCandidateSessionRecordV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    match value {
        DraftEditorCandidateSessionRecordV1::Head(head) => {
            e.u8(0);
            enc_session_head(&mut e, head);
        }
        DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) => {
            e.u8(1);
            e.bytes(receipt.request_bytes());
            enc_session_head(&mut e, receipt.head());
        }
    }
    Ok(e.finish())
}

fn decode_session_record(bytes: &[u8]) -> Result<DraftEditorCandidateSessionRecordV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = match d.u8()? {
        0 => DraftEditorCandidateSessionRecordV1::Head(dec_session_head(&mut d)?),
        1 => {
            let request_bytes = d.bytes("draft editor session open request")?.to_vec();
            let head = dec_session_head(&mut d)?;
            let request = decode_canonical_session_open_request_bytes(&request_bytes)?;
            let selector = request.selector();
            if head.thread_id() != selector.thread_id()
                || head.draft_id() != selector.draft_id()
                || head.session_id() != request.session_id()
                || head.open_operation_id() != request.operation_id()
                || head.session_generation() != 1
                || head.durable_base_selector_revision() != selector.selector_revision()
                || head.durable_base_root() != selector.root()
                || head.durable_base_history() != selector.history()
                || head.published_candidate_generation() != 0
                || head.published_selector_revision() != selector.selector_revision()
                || head.published_root() != selector.root()
                || head.published_history() != selector.history()
                || head.newest_candidate_generation() != 0
                || head.newest_root() != selector.root()
                || head.newest_history().root() != selector.root()
                || head.newest_history().key().session_id() != Some(request.session_id())
                || head.logical_extent() != selector.root().summary().logical_extent()
                || head.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
                || head.active_operation().is_some()
            {
                return Err(CodecError::InvalidLength(
                    "draft editor session open receipt",
                ));
            }
            DraftEditorCandidateSessionRecordV1::OpenReceipt(
                DraftEditorCandidateSessionOpenReceiptV1::new(request_bytes, head),
            )
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft editor session record",
                tag,
            });
        }
    };
    d.finish()?;
    Ok(value)
}

fn enc_search_key(e: &mut Encoder, key: DraftCompositeSearchKeyV1) {
    match key {
        DraftCompositeSearchKeyV1::BeforeMarkers(anchor) => {
            e.u8(0);
            e.u64(anchor);
        }
        DraftCompositeSearchKeyV1::Marker {
            anchor,
            order_key,
            marker_id,
        } => {
            e.u8(1);
            e.u64(anchor);
            e.u64(order_key);
            e.fixed16(marker_id.as_bytes());
        }
        DraftCompositeSearchKeyV1::AfterMarkers(anchor) => {
            e.u8(2);
            e.u64(anchor);
        }
    }
}

fn dec_search_key(d: &mut Decoder<'_>) -> Result<DraftCompositeSearchKeyV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftCompositeSearchKeyV1::BeforeMarkers(d.u64()?)),
        1 => Ok(DraftCompositeSearchKeyV1::Marker {
            anchor: d.u64()?,
            order_key: d.u64()?,
            marker_id: SyndicDraftMarkerId::from_bytes(d.fixed16()?),
        }),
        2 => Ok(DraftCompositeSearchKeyV1::AfterMarkers(d.u64()?)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft composite search key",
            tag,
        }),
    }
}

pub(crate) fn enc_position(e: &mut Encoder, position: DraftCompositePositionV1) {
    e.u64(position.utf8_offset());
    match position.gap() {
        DraftCompositeGapWitnessV1::Unambiguous => e.u8(0),
        DraftCompositeGapWitnessV1::BeforeAll => e.u8(1),
        DraftCompositeGapWitnessV1::Between {
            left_order_key,
            left_marker_id,
            right_order_key,
            right_marker_id,
        } => {
            e.u8(2);
            e.u64(left_order_key);
            e.fixed16(left_marker_id.as_bytes());
            e.u64(right_order_key);
            e.fixed16(right_marker_id.as_bytes());
        }
        DraftCompositeGapWitnessV1::AfterAll => e.u8(3),
    }
}

pub(crate) fn canonical_position_bytes(position: DraftCompositePositionV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_position(&mut e, position);
    e.finish()
}

pub(crate) fn dec_position(d: &mut Decoder<'_>) -> Result<DraftCompositePositionV1, CodecError> {
    let offset = d.u64()?;
    let gap = match d.u8()? {
        0 => DraftCompositeGapWitnessV1::Unambiguous,
        1 => DraftCompositeGapWitnessV1::BeforeAll,
        2 => DraftCompositeGapWitnessV1::Between {
            left_order_key: d.u64()?,
            left_marker_id: SyndicDraftMarkerId::from_bytes(d.fixed16()?),
            right_order_key: d.u64()?,
            right_marker_id: SyndicDraftMarkerId::from_bytes(d.fixed16()?),
        },
        3 => DraftCompositeGapWitnessV1::AfterAll,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft composite gap",
                tag,
            });
        }
    };
    Ok(DraftCompositePositionV1::new(offset, gap))
}

fn enc_marker(e: &mut Encoder, marker: DraftPieceMarkerV1) {
    e.fixed16(marker.marker_id().as_bytes());
    e.u64(marker.order_key());
    e.u64(marker.label().get());
}

fn dec_marker(d: &mut Decoder<'_>) -> Result<DraftPieceMarkerV1, CodecError> {
    Ok(DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes(d.fixed16()?),
        d.u64()?,
        ImageLabelOrdinal::new(d.u64()?).map_err(|error| invalid("draft-piece label", error))?,
    ))
}

fn enc_piece(e: &mut Encoder, piece: &DraftPieceV1) {
    match piece {
        DraftPieceV1::Text(text) => {
            e.u8(0);
            e.text(text);
        }
        DraftPieceV1::Marker(marker) => {
            e.u8(1);
            enc_marker(e, *marker);
        }
    }
}

fn dec_piece(d: &mut Decoder<'_>) -> Result<DraftPieceV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftPieceV1::Text(d.text("draft-piece text")?.to_owned())),
        1 => Ok(DraftPieceV1::Marker(dec_marker(d)?)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft piece",
            tag,
        }),
    }
}

fn enc_replacement(e: &mut Encoder, replacement: &DraftPieceReplacementV1) {
    e.u8(u8::from(replacement.is_continuation()));
    enc_position(e, replacement.start());
    enc_position(e, replacement.end());
    e.u64(replacement.inserted().len() as u64);
    for piece in replacement.inserted() {
        enc_piece(e, piece);
    }
    e.u64(replacement.moves().len() as u64);
    for movement in replacement.moves() {
        e.u64(movement.predecessor().anchor());
        enc_marker(e, movement.predecessor().marker());
        enc_marker(e, movement.successor());
        e.u64(movement.removal_fragment_ordinal());
    }
}

fn dec_replacement(d: &mut Decoder<'_>) -> Result<DraftPieceReplacementV1, CodecError> {
    let continuation = match d.u8()? {
        0 => false,
        1 => true,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece fragment continuation",
                tag,
            });
        }
    };
    let start = dec_position(d)?;
    let end = dec_position(d)?;
    let piece_count =
        usize::try_from(d.u64()?).map_err(|error| invalid("draft-piece insertion count", error))?;
    if piece_count > DRAFT_PIECE_STAGE_MAX_RECORDS {
        return Err(CodecError::InvalidLength("draft-piece insertion count"));
    }
    let mut inserted = Vec::with_capacity(piece_count);
    for _ in 0..piece_count {
        inserted.push(dec_piece(d)?);
    }
    let move_count =
        usize::try_from(d.u64()?).map_err(|error| invalid("draft-piece move count", error))?;
    if move_count > DRAFT_PIECE_STAGE_MAX_RECORDS {
        return Err(CodecError::InvalidLength("draft-piece move count"));
    }
    let mut moves = Vec::with_capacity(move_count);
    for _ in 0..move_count {
        moves.push(DraftPieceMarkerMoveV1::new(
            DraftPieceMarkerAtV1::new(d.u64()?, dec_marker(d)?),
            dec_marker(d)?,
            d.u64()?,
        ));
    }
    Ok(if continuation {
        DraftPieceReplacementV1::continuation(start, end, inserted)
    } else {
        DraftPieceReplacementV1::new(start, end, inserted)
    }
    .with_moves(moves))
}

fn encode_root(value: &DraftPieceRootRecordV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_root_reference(&mut e, value.reference());
    Ok(e.finish())
}

fn decode_root(bytes: &[u8]) -> Result<DraftPieceRootRecordV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = DraftPieceRootRecordV1::new(dec_root_reference(&mut d)?);
    d.finish()?;
    Ok(value)
}

fn encode_leaf(value: &DraftPieceLeafRecordV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_record_key(&mut e, value.key());
    match value.value() {
        DraftPieceLeafValueV1::Text(text) => {
            e.u8(0);
            e.text(text);
        }
        DraftPieceLeafValueV1::Marker(marker) => {
            e.u8(1);
            enc_marker(&mut e, *marker);
        }
    }
    enc_text_summary(&mut e, value.text_summary());
    enc_digest(&mut e, value.digest());
    Ok(e.finish())
}

fn decode_leaf(bytes: &[u8]) -> Result<DraftPieceLeafRecordV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_record_key(&mut d)?;
    let value = match d.u8()? {
        0 => DraftPieceLeafValueV1::Text(d.text("draft-piece leaf text")?.to_owned()),
        1 => DraftPieceLeafValueV1::Marker(dec_marker(&mut d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece leaf",
                tag,
            });
        }
    };
    let text_summary = dec_text_summary(&mut d)?;
    let record = DraftPieceLeafRecordV1::new(key, value, text_summary, dec_digest(&mut d)?);
    d.finish()?;
    let expected_summary = match record.value() {
        DraftPieceLeafValueV1::Text(text) => DraftPieceTextSummaryV1::from_utf8(text),
        DraftPieceLeafValueV1::Marker(_) => DraftPieceTextSummaryV1::empty(),
    };
    if record.text_summary() != expected_summary
        || record.digest() != leaf_digest(record.value(), record.text_summary())
    {
        return Err(CodecError::InvalidLength("draft-piece leaf"));
    }
    Ok(record)
}

fn encode_node(value: &DraftPieceNodeRecordV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_record_key(&mut e, value.key());
    e.u8(value.height());
    e.u64(value.children().len() as u64);
    for child in value.children() {
        e.fixed16(child.id().as_bytes());
        enc_digest(&mut e, child.digest());
        enc_text_summary(&mut e, child.text_summary());
        e.u64(child.piece_count());
        e.u64(child.marker_count());
        enc_digest(&mut e, child.marker_digest());
        enc_search_key(&mut e, child.first());
        enc_search_key(&mut e, child.last());
    }
    enc_digest(&mut e, value.digest());
    Ok(e.finish())
}

fn decode_node(bytes: &[u8]) -> Result<DraftPieceNodeRecordV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_record_key(&mut d)?;
    let height = d.u8()?;
    let count =
        usize::try_from(d.u64()?).map_err(|error| invalid("draft-piece child count", error))?;
    if count == 0 || count > DRAFT_PIECE_MAX_CHILDREN {
        return Err(CodecError::InvalidLength("draft-piece child count"));
    }
    let mut children = Vec::with_capacity(count);
    for _ in 0..count {
        let id = DraftPieceRecordIdV1::from_bytes(d.fixed16()?);
        let digest = dec_digest(&mut d)?;
        let text = dec_text_summary(&mut d)?;
        children.push(DraftPieceChildV1::new(
            id,
            digest,
            text.logical_utf8_bytes(),
            text.newline_count(),
            text.logical_line_count(),
            d.u64()?,
            d.u64()?,
            dec_digest(&mut d)?,
            dec_search_key(&mut d)?,
            dec_search_key(&mut d)?,
        ));
    }
    let value = DraftPieceNodeRecordV1::new(key, height, children, dec_digest(&mut d)?);
    d.finish()?;
    if value.height() == 0
        || value.digest() != node_digest(value.height(), value.children())
        || child_for_node(&value).is_err()
    {
        return Err(CodecError::InvalidLength("draft-piece node"));
    }
    Ok(value)
}

fn encode_build(value: &DraftPieceBuildRecordV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    e.fixed16(value.draft_id().as_bytes());
    e.fixed16(value.session_id().as_bytes());
    e.u64(value.predecessor_candidate_generation());
    enc_root_reference(&mut e, value.predecessor_root());
    enc_history_reference(&mut e, value.predecessor_history());
    e.fixed16(value.operation_id().as_bytes());
    enc_position(&mut e, value.predecessor_caret());
    enc_position(&mut e, value.predecessor_selection());
    enc_position(&mut e, value.caret());
    enc_position(&mut e, value.selection());
    e.u64(value.fragment_count());
    enc_digest(&mut e, value.fragment_chain());
    e.bytes(value.canonical_header());
    e.u64(value.staged_fragment_count());
    enc_digest(&mut e, value.staged_fragment_chain());
    enc_digest(&mut e, value.proposal_digest());
    enc_build_roots(&mut e, value.working_roots());
    enc_build_boundary(&mut e, value.base_frontier());
    enc_build_boundary(&mut e, value.successor_frontier());
    e.u64(value.next_record_ordinal());
    enc_build_frontier(&mut e, value.frontier());
    enc_digest(&mut e, value.progress_digest());
    enc_progress_reference(&mut e, value.progress_receipt());
    match value.successor() {
        Some(root) => {
            e.u8(1);
            enc_root_reference(&mut e, root);
        }
        None => e.u8(0),
    }
    match value.build_digest() {
        Some(digest) => {
            e.u8(1);
            enc_digest(&mut e, digest);
        }
        None => e.u8(0),
    }
    e.u8(value.lifecycle() as u8);
    Ok(e.finish())
}

fn decode_build(bytes: &[u8]) -> Result<DraftPieceBuildRecordV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let draft_id = SyndicDraftId::from_bytes(d.fixed16()?);
    let session_id = DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?);
    let predecessor_candidate_generation = d.u64()?;
    let predecessor_root = dec_root_reference(&mut d)?;
    let predecessor_history = dec_history_reference(&mut d)?;
    let operation_id = DraftPieceOperationIdV1::from_bytes(d.fixed16()?);
    let predecessor_caret = dec_position(&mut d)?;
    let predecessor_selection = dec_position(&mut d)?;
    let caret = dec_position(&mut d)?;
    let selection = dec_position(&mut d)?;
    let fragment_count = d.u64()?;
    let fragment_chain = dec_digest(&mut d)?;
    let canonical_header = d.bytes("draft-piece canonical header")?.to_vec();
    let staged_fragment_count = d.u64()?;
    let staged_fragment_chain = dec_digest(&mut d)?;
    let proposal_digest = dec_digest(&mut d)?;
    let working_roots = dec_build_roots(&mut d)?;
    let base_frontier = dec_build_boundary(&mut d)?;
    let successor_frontier = dec_build_boundary(&mut d)?;
    let next_record_ordinal = d.u64()?;
    let frontier = dec_build_frontier(&mut d)?;
    let progress_digest = dec_digest(&mut d)?;
    let progress_receipt = dec_progress_reference(&mut d)?;
    let successor = match d.u8()? {
        0 => None,
        1 => Some(dec_root_reference(&mut d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece successor option",
                tag,
            });
        }
    };
    let build_digest = match d.u8()? {
        0 => None,
        1 => Some(dec_digest(&mut d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece build digest option",
                tag,
            });
        }
    };
    let lifecycle = match d.u8()? {
        0 => DraftPieceBuildLifecycleV1::Open,
        1 => DraftPieceBuildLifecycleV1::Complete,
        2 => DraftPieceBuildLifecycleV1::Committed,
        3 => DraftPieceBuildLifecycleV1::Rejected,
        4 => DraftPieceBuildLifecycleV1::Conflict,
        5 => DraftPieceBuildLifecycleV1::Cancelled,
        6 => DraftPieceBuildLifecycleV1::Error,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece build lifecycle",
                tag,
            });
        }
    };
    let value = DraftPieceBuildRecordV1::new(
        draft_id,
        session_id,
        predecessor_candidate_generation,
        predecessor_root,
        predecessor_history,
        operation_id,
        predecessor_caret,
        predecessor_selection,
        caret,
        selection,
        fragment_count,
        fragment_chain,
        canonical_header,
        staged_fragment_count,
        staged_fragment_chain,
        proposal_digest,
        working_roots,
        base_frontier,
        successor_frontier,
        next_record_ordinal,
        frontier,
        progress_digest,
        progress_receipt,
        successor,
        build_digest,
        lifecycle,
    );
    d.finish()?;
    if !build_record_is_exact(&value) {
        return Err(CodecError::InvalidLength("draft-piece build record"));
    }
    Ok(value)
}

fn encode_progress_family_key(
    key: &DraftPieceBuildProgressReceiptKeyV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    if key.transition_ordinal() == 0 {
        return Err(CodecError::InvalidLength(
            "draft-piece progress transition ordinal",
        ));
    }
    enc_progress_key(&mut e, *key);
    Ok(e.finish())
}

fn decode_progress_family_key(
    bytes: &[u8],
) -> Result<DraftPieceBuildProgressReceiptKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_progress_key(&mut d)?;
    d.finish()?;
    Ok(value)
}

fn encode_progress_receipt(
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> Result<Vec<u8>, CodecError> {
    if !progress_receipt_is_exact(receipt) {
        return Err(CodecError::InvalidLength("draft-piece progress receipt"));
    }
    let mut e = Encoder::new();
    enc_progress_reference(&mut e, receipt.reference());
    match receipt.previous() {
        Some(previous) => {
            e.u8(1);
            enc_progress_reference(&mut e, previous);
        }
        None => e.u8(0),
    }
    match receipt.fragment_endpoint() {
        Some(endpoint) => {
            e.u8(1);
            enc_fragment_key(&mut e, endpoint.key());
            enc_digest(&mut e, endpoint.digest());
            enc_digest(&mut e, endpoint.chain());
        }
        None => e.u8(0),
    }
    enc_digest(&mut e, receipt.state_digest());
    enc_build_roots(&mut e, receipt.working_roots());
    enc_build_boundary(&mut e, receipt.base_frontier());
    enc_build_boundary(&mut e, receipt.successor_frontier());
    e.u64(receipt.next_record_ordinal());
    enc_build_frontier(&mut e, receipt.frontier());
    match receipt.successor() {
        Some(successor) => {
            e.u8(1);
            enc_root_reference(&mut e, successor);
        }
        None => e.u8(0),
    }
    match receipt.build_digest() {
        Some(digest) => {
            e.u8(1);
            enc_digest(&mut e, digest);
        }
        None => e.u8(0),
    }
    e.u8(receipt.lifecycle() as u8);
    Ok(e.finish())
}

fn decode_progress_receipt(bytes: &[u8]) -> Result<DraftPieceBuildProgressReceiptV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let reference = dec_progress_reference(&mut d)?;
    let previous = match d.u8()? {
        0 => None,
        1 => Some(dec_progress_reference(&mut d)?),
        option => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece progress predecessor",
                tag: option,
            });
        }
    };
    let fragment_endpoint = match d.u8()? {
        0 => None,
        1 => Some(DraftPieceCanonicalFragmentEndpointV1::new(
            dec_fragment_key(&mut d)?,
            dec_digest(&mut d)?,
            dec_digest(&mut d)?,
        )),
        option => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece progress fragment endpoint",
                tag: option,
            });
        }
    };
    let state_digest = dec_digest(&mut d)?;
    let working_roots = dec_build_roots(&mut d)?;
    let base_frontier = dec_build_boundary(&mut d)?;
    let successor_frontier = dec_build_boundary(&mut d)?;
    let next_record_ordinal = d.u64()?;
    let frontier = dec_build_frontier(&mut d)?;
    let successor = match d.u8()? {
        0 => None,
        1 => Some(dec_root_reference(&mut d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece progress successor",
                tag,
            });
        }
    };
    let build_digest = match d.u8()? {
        0 => None,
        1 => Some(dec_digest(&mut d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece progress build digest",
                tag,
            });
        }
    };
    let lifecycle = match d.u8()? {
        0 => DraftPieceBuildLifecycleV1::Open,
        1 => DraftPieceBuildLifecycleV1::Complete,
        2 => DraftPieceBuildLifecycleV1::Committed,
        3 => DraftPieceBuildLifecycleV1::Rejected,
        4 => DraftPieceBuildLifecycleV1::Conflict,
        5 => DraftPieceBuildLifecycleV1::Cancelled,
        6 => DraftPieceBuildLifecycleV1::Error,
        lifecycle => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece progress lifecycle",
                tag: lifecycle,
            });
        }
    };
    d.finish()?;
    let receipt = DraftPieceBuildProgressReceiptV1::new(
        reference,
        previous,
        fragment_endpoint,
        state_digest,
        working_roots,
        base_frontier,
        successor_frontier,
        next_record_ordinal,
        frontier,
        successor,
        build_digest,
        lifecycle,
    );
    if !progress_receipt_is_exact(&receipt) {
        return Err(CodecError::InvalidLength("draft-piece progress receipt"));
    }
    Ok(receipt)
}

fn enc_rejected(e: &mut Encoder, reason: DraftPieceRejectedReasonV1) {
    e.u8(reason as u8);
}

fn dec_rejected(d: &mut Decoder<'_>) -> Result<DraftPieceRejectedReasonV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftPieceRejectedReasonV1::EmptyTransaction),
        1 => Ok(DraftPieceRejectedReasonV1::TooManyReplacements),
        2 => Ok(DraftPieceRejectedReasonV1::InsertedPayloadTooLarge),
        3 => Ok(DraftPieceRejectedReasonV1::EmptyTextLeaf),
        4 => Ok(DraftPieceRejectedReasonV1::InvalidUtf8Boundary),
        5 => Ok(DraftPieceRejectedReasonV1::InvalidGapWitness),
        6 => Ok(DraftPieceRejectedReasonV1::OutOfOrder),
        7 => Ok(DraftPieceRejectedReasonV1::Overlap),
        8 => Ok(DraftPieceRejectedReasonV1::DuplicateEmptyRange),
        9 => Ok(DraftPieceRejectedReasonV1::DuplicateMarkerIdentity),
        10 => Ok(DraftPieceRejectedReasonV1::DuplicateMarkerOrder),
        11 => Ok(DraftPieceRejectedReasonV1::AggregateOverflow),
        12 => Ok(DraftPieceRejectedReasonV1::TreeLimit),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-piece rejection",
            tag,
        }),
    }
}

fn enc_error(e: &mut Encoder, reason: DraftPieceErrorReasonV1) {
    e.u8(reason as u8);
}

fn dec_error(d: &mut Decoder<'_>) -> Result<DraftPieceErrorReasonV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftPieceErrorReasonV1::OccupiedIdentity),
        1 => Ok(DraftPieceErrorReasonV1::UnsettledOperation),
        2 => Ok(DraftPieceErrorReasonV1::MissingRecord),
        3 => Ok(DraftPieceErrorReasonV1::CorruptRecord),
        4 => Ok(DraftPieceErrorReasonV1::ResourceLimit),
        5 => Ok(DraftPieceErrorReasonV1::OccupiedIdentityNoncommit),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-piece error reason",
            tag,
        }),
    }
}

fn enc_optional_byte(e: &mut Encoder, value: Option<u8>) {
    match value {
        Some(value) => {
            e.u8(1);
            e.u8(value);
        }
        None => e.u8(0),
    }
}

fn dec_optional_byte(d: &mut Decoder<'_>) -> Result<Option<u8>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(d.u8()?)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-piece optional collision byte",
            tag,
        }),
    }
}

fn enc_optional_fragment(
    e: &mut Encoder,
    value: Option<&DraftPieceBuildFragmentV1>,
) -> Result<(), CodecError> {
    match value {
        Some(value) => {
            e.u8(1);
            e.bytes(&encode_fragment(value)?);
        }
        None => e.u8(0),
    }
    Ok(())
}

fn dec_optional_fragment(
    d: &mut Decoder<'_>,
) -> Result<Option<DraftPieceBuildFragmentV1>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(decode_fragment(
            d.bytes("draft-piece collision fragment")?,
        )?)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-piece optional collision fragment",
            tag,
        }),
    }
}

fn enc_occupied_identity_proof(
    e: &mut Encoder,
    proof: &OccupiedIdentityNoncommitProofV1,
) -> Result<(), CodecError> {
    enc_digest(e, proof.requested_proposal_digest());
    enc_digest(e, proof.occupied_proposal_digest());
    enc_settlement_key(e, proof.key());
    match proof.difference() {
        OccupiedIdentityDifferenceV1::Header {
            offset,
            requested,
            occupied,
        } => {
            e.u8(0);
            e.u64(*offset);
            enc_optional_byte(e, *requested);
            enc_optional_byte(e, *occupied);
        }
        OccupiedIdentityDifferenceV1::Fragment {
            key,
            requested,
            occupied,
        } => {
            e.u8(1);
            enc_fragment_key(e, *key);
            enc_optional_fragment(e, requested.as_ref())?;
            enc_optional_fragment(e, occupied.as_ref())?;
        }
        OccupiedIdentityDifferenceV1::Root {
            key,
            requested,
            occupied,
        } => {
            e.u8(2);
            enc_root_key(e, *key);
            enc_root_reference(e, requested.reference());
            enc_root_reference(e, occupied.reference());
        }
    }
    Ok(())
}

fn dec_occupied_identity_proof(
    d: &mut Decoder<'_>,
) -> Result<OccupiedIdentityNoncommitProofV1, CodecError> {
    let requested_proposal_digest = dec_digest(d)?;
    let occupied_proposal_digest = dec_digest(d)?;
    let key = dec_settlement_key(d)?;
    let difference = match d.u8()? {
        0 => OccupiedIdentityDifferenceV1::Header {
            offset: d.u64()?,
            requested: dec_optional_byte(d)?,
            occupied: dec_optional_byte(d)?,
        },
        1 => OccupiedIdentityDifferenceV1::Fragment {
            key: dec_fragment_key(d)?,
            requested: dec_optional_fragment(d)?,
            occupied: dec_optional_fragment(d)?,
        },
        2 => {
            let root_key = dec_root_key(d)?;
            OccupiedIdentityDifferenceV1::Root {
                key: root_key,
                requested: DraftPieceRootRecordV1::new(dec_root_reference(d)?),
                occupied: DraftPieceRootRecordV1::new(dec_root_reference(d)?),
            }
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece occupied identity difference",
                tag,
            });
        }
    };
    Ok(OccupiedIdentityNoncommitProofV1::new(
        requested_proposal_digest,
        occupied_proposal_digest,
        key,
        difference,
    ))
}

fn encode_settlement(value: &DraftPieceSettlementV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_settlement_key(&mut e, value.key());
    enc_digest(&mut e, value.proposal_digest());
    e.u64(value.predecessor_candidate_generation());
    enc_root_reference(&mut e, value.predecessor_root());
    enc_history_reference(&mut e, value.predecessor_history());
    e.u64(value.fragment_count());
    enc_digest(&mut e, value.fragment_chain());
    enc_position(&mut e, value.predecessor_caret());
    enc_position(&mut e, value.predecessor_selection());
    enc_position(&mut e, value.caret());
    enc_position(&mut e, value.selection());
    match value.build_digest() {
        Some(digest) => {
            e.u8(1);
            enc_digest(&mut e, digest);
        }
        None => e.u8(0),
    }
    e.bytes(value.canonical_header());
    match value.terminal_source() {
        Some(source) => {
            e.u8(1);
            e.bytes(&encode_build(source)?);
        }
        None => e.u8(0),
    }
    enc_progress_reference(&mut e, value.terminal_receipt());
    match value.outcome() {
        DraftPieceSettlementOutcomeV1::Committed {
            candidate_generation,
            successor,
            history,
            caret,
            selection,
        } => {
            e.u8(0);
            e.u64(*candidate_generation);
            enc_root_reference(&mut e, *successor);
            enc_history_reference(&mut e, *history);
            enc_position(&mut e, *caret);
            enc_position(&mut e, *selection);
        }
        DraftPieceSettlementOutcomeV1::Rejected(reason) => {
            e.u8(1);
            enc_rejected(&mut e, *reason);
        }
        DraftPieceSettlementOutcomeV1::Conflict {
            current_candidate_generation,
            current_root,
            current_history,
        } => {
            e.u8(2);
            e.u64(*current_candidate_generation);
            enc_root_reference(&mut e, *current_root);
            enc_history_reference(&mut e, *current_history);
        }
        DraftPieceSettlementOutcomeV1::Cancelled => e.u8(3),
        DraftPieceSettlementOutcomeV1::Error(reason) => {
            e.u8(4);
            enc_error(&mut e, *reason);
        }
    }
    match value.closure() {
        DraftPieceSettlementClosureV1::Committed(adoption) => {
            e.u8(0);
            enc_session_head(&mut e, adoption.predecessor_session());
            enc_session_head(&mut e, adoption.adopted_session());
            enc_root_reference(&mut e, adoption.adopted_root().reference());
            enc_history_frontier(&mut e, adoption.predecessor_history());
            enc_history_transition(&mut e, adoption.transition());
            enc_history_frontier(&mut e, adoption.adopted_history());
        }
        DraftPieceSettlementClosureV1::Noncommit(noncommit) => {
            e.u8(1);
            enc_session_head(&mut e, noncommit.observed_session());
            enc_history_frontier(&mut e, noncommit.observed_history());
            match noncommit.proposed_successor() {
                Some(successor) => {
                    e.u8(1);
                    enc_root_reference(&mut e, successor);
                }
                None => e.u8(0),
            }
            match noncommit.occupied_identity() {
                Some(proof) => {
                    e.u8(1);
                    enc_occupied_identity_proof(&mut e, proof)?;
                }
                None => e.u8(0),
            }
        }
    }
    Ok(e.finish())
}

fn decode_settlement(bytes: &[u8]) -> Result<DraftPieceSettlementV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_settlement_key(&mut d)?;
    let proposal = dec_digest(&mut d)?;
    let predecessor_candidate_generation = d.u64()?;
    let predecessor_root = dec_root_reference(&mut d)?;
    let predecessor_history = dec_history_reference(&mut d)?;
    let fragment_count = d.u64()?;
    let fragment_chain = dec_digest(&mut d)?;
    let predecessor_caret = dec_position(&mut d)?;
    let predecessor_selection = dec_position(&mut d)?;
    let caret = dec_position(&mut d)?;
    let selection = dec_position(&mut d)?;
    let build_digest = match d.u8()? {
        0 => None,
        1 => Some(dec_digest(&mut d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece settlement build option",
                tag,
            });
        }
    };
    let canonical_header = d.bytes("draft-piece settlement canonical header")?.to_vec();
    let terminal_source = match d.u8()? {
        0 => None,
        1 => Some(decode_build(
            d.bytes("draft-piece settlement terminal source")?,
        )?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece settlement terminal source option",
                tag,
            });
        }
    };
    let terminal_receipt = dec_progress_reference(&mut d)?;
    let outcome = match d.u8()? {
        0 => DraftPieceSettlementOutcomeV1::Committed {
            candidate_generation: d.u64()?,
            successor: dec_root_reference(&mut d)?,
            history: dec_history_reference(&mut d)?,
            caret: dec_position(&mut d)?,
            selection: dec_position(&mut d)?,
        },
        1 => DraftPieceSettlementOutcomeV1::Rejected(dec_rejected(&mut d)?),
        2 => DraftPieceSettlementOutcomeV1::Conflict {
            current_candidate_generation: d.u64()?,
            current_root: dec_root_reference(&mut d)?,
            current_history: dec_history_reference(&mut d)?,
        },
        3 => DraftPieceSettlementOutcomeV1::Cancelled,
        4 => DraftPieceSettlementOutcomeV1::Error(dec_error(&mut d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece settlement outcome",
                tag,
            });
        }
    };
    let closure = match d.u8()? {
        0 => DraftPieceSettlementClosureV1::Committed(DraftPieceCommittedAdoptionV1::new(
            dec_session_head(&mut d)?,
            dec_session_head(&mut d)?,
            DraftPieceRootRecordV1::new(dec_root_reference(&mut d)?),
            dec_history_frontier(&mut d)?,
            dec_history_transition(&mut d)?,
            dec_history_frontier(&mut d)?,
        )),
        1 => {
            let observed_session = dec_session_head(&mut d)?;
            let observed_history = dec_history_frontier(&mut d)?;
            let proposed_successor = match d.u8()? {
                0 => None,
                1 => Some(dec_root_reference(&mut d)?),
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "draft-piece proposed successor option",
                        tag,
                    });
                }
            };
            let occupied_identity = match d.u8()? {
                0 => None,
                1 => Some(dec_occupied_identity_proof(&mut d)?),
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "draft-piece occupied identity proof option",
                        tag,
                    });
                }
            };
            let noncommit = match (proposed_successor, occupied_identity) {
                (Some(successor), Some(proof)) => {
                    DraftPieceNoncommitClosureV1::with_occupied_identity(
                        observed_session,
                        observed_history,
                        successor,
                        proof,
                    )
                }
                (successor, None) => {
                    DraftPieceNoncommitClosureV1::new(observed_session, observed_history, successor)
                }
                (None, Some(_)) => {
                    return Err(CodecError::InvalidLength(
                        "draft-piece occupied identity successor",
                    ));
                }
            };
            DraftPieceSettlementClosureV1::Noncommit(noncommit)
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece settlement closure",
                tag,
            });
        }
    };
    let value = DraftPieceSettlementV1::new(
        key,
        proposal,
        predecessor_candidate_generation,
        predecessor_root,
        predecessor_history,
        fragment_count,
        fragment_chain,
        predecessor_caret,
        predecessor_selection,
        caret,
        selection,
        build_digest,
        canonical_header,
        terminal_source,
        terminal_receipt,
        outcome,
        closure,
    );
    d.finish()?;
    if !settlement_closure_is_exact(&value) {
        return Err(CodecError::InvalidLength("draft-piece settlement closure"));
    }
    Ok(value)
}

fn encode_marker_identity_record(
    value: &DraftMarkerIdentityRecordV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    match value {
        DraftMarkerIdentityRecordV1::Internal {
            key,
            height,
            children,
            digest,
        } => {
            e.u8(0);
            enc_marker_identity_key(&mut e, *key);
            e.u8(*height);
            e.u64(children.len() as u64);
            for child in children {
                e.fixed16(child.id().as_bytes());
                enc_digest(&mut e, child.digest());
                e.u64(child.record_count());
                e.fixed16(child.first().as_bytes());
                e.fixed16(child.last().as_bytes());
            }
            enc_digest(&mut e, *digest);
        }
        DraftMarkerIdentityRecordV1::Leaf {
            key,
            occurrence,
            digest,
        } => {
            e.u8(1);
            enc_marker_identity_key(&mut e, *key);
            e.fixed16(occurrence.marker_id().as_bytes());
            e.u64(occurrence.label().get());
            e.u64(occurrence.order_key());
            e.fixed16(occurrence.sequence_leaf_id().as_bytes());
            enc_digest(&mut e, occurrence.sequence_leaf_digest());
            enc_digest(&mut e, *digest);
        }
    }
    Ok(e.finish())
}

fn decode_marker_identity_record(bytes: &[u8]) -> Result<DraftMarkerIdentityRecordV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = match d.u8()? {
        0 => {
            let key = dec_marker_identity_key(&mut d)?;
            let height = d.u8()?;
            let count = usize::try_from(d.u64()?)
                .map_err(|_| CodecError::InvalidLength("draft marker-index child count"))?;
            if !(1..=DRAFT_PIECE_MAX_CHILDREN).contains(&count) {
                return Err(CodecError::InvalidLength("draft marker-index child count"));
            }
            let mut children = Vec::with_capacity(count);
            for _ in 0..count {
                children.push(DraftMarkerIdentityChildV1::new(
                    DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
                    dec_digest(&mut d)?,
                    d.u64()?,
                    SyndicDraftMarkerId::from_bytes(d.fixed16()?),
                    SyndicDraftMarkerId::from_bytes(d.fixed16()?),
                ));
            }
            DraftMarkerIdentityRecordV1::Internal {
                key,
                height,
                children,
                digest: dec_digest(&mut d)?,
            }
        }
        1 => {
            let key = dec_marker_identity_key(&mut d)?;
            let occurrence = DraftMarkerIdentityOccurrenceV1::new(
                SyndicDraftMarkerId::from_bytes(d.fixed16()?),
                ImageLabelOrdinal::new(d.u64()?)
                    .map_err(|error| invalid("draft marker-index label", error))?,
                d.u64()?,
                DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
                dec_digest(&mut d)?,
            );
            DraftMarkerIdentityRecordV1::Leaf {
                key,
                occurrence,
                digest: dec_digest(&mut d)?,
            }
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft marker-index record",
                tag,
            });
        }
    };
    d.finish()?;
    Ok(value)
}

macro_rules! family {
    ($family:ty, $key:ty, $value:ty, $name:literal, $key_size:expr, $value_size:expr, $enc_key:expr, $dec_key:expr, $enc_value:expr, $dec_value:expr) => {
        impl Family for $family {
            type Key = $key;
            type Value = $value;
            const NAME: &'static str = $name;
            const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = $key_size;
            const MAX_VALUE_BYTES: usize = $value_size;
            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
                $enc_key(key)
            }
            fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
                $dec_key(bytes)
            }
            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
                $enc_value(value)
            }
            fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
                $dec_value(bytes)
            }
        }
    };
}

fn encode_root_key(key: &DraftPieceRootKeyV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_root_key(&mut e, *key);
    Ok(e.finish())
}
fn decode_root_key(bytes: &[u8]) -> Result<DraftPieceRootKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_root_key(&mut d)?;
    d.finish()?;
    Ok(value)
}
fn encode_record_key(key: &DraftPieceRecordKeyV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_record_key(&mut e, *key);
    Ok(e.finish())
}
fn decode_record_key(bytes: &[u8]) -> Result<DraftPieceRecordKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_record_key(&mut d)?;
    d.finish()?;
    Ok(value)
}
fn encode_marker_identity_family_key(
    key: &DraftMarkerIdentityRecordKeyV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_marker_identity_key(&mut e, *key);
    Ok(e.finish())
}
fn decode_marker_identity_family_key(
    bytes: &[u8],
) -> Result<DraftMarkerIdentityRecordKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_marker_identity_key(&mut d)?;
    d.finish()?;
    Ok(value)
}
fn encode_settlement_family_key(key: &DraftPieceSettlementKeyV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_settlement_key(&mut e, *key);
    Ok(e.finish())
}
fn decode_settlement_family_key(bytes: &[u8]) -> Result<DraftPieceSettlementKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_settlement_key(&mut d)?;
    d.finish()?;
    Ok(value)
}

fn encode_fragment_family_key(key: &DraftPieceBuildFragmentKeyV1) -> Result<Vec<u8>, CodecError> {
    if !key.is_locally_valid() {
        return Err(CodecError::InvalidLength("draft-piece fragment ordinal"));
    }
    let mut e = Encoder::new();
    enc_fragment_key(&mut e, *key);
    Ok(e.finish())
}

fn decode_fragment_family_key(bytes: &[u8]) -> Result<DraftPieceBuildFragmentKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_fragment_key(&mut d)?;
    d.finish()?;
    Ok(key)
}
fn encode_session_family_key(
    key: &DraftEditorCandidateSessionRecordKeyV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_session_key(&mut e, *key);
    Ok(e.finish())
}
fn decode_session_family_key(
    bytes: &[u8],
) -> Result<DraftEditorCandidateSessionRecordKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_session_key(&mut d)?;
    d.finish()?;
    Ok(value)
}
fn encode_fragment(value: &DraftPieceBuildFragmentV1) -> Result<Vec<u8>, CodecError> {
    if !value.key().is_locally_valid()
        || value.chain_digest()
            != draft_piece_fragment_chain_link_v1(
                value.preceding_chain(),
                value.key().ordinal(),
                value.replacement(),
            )
        || (value.key().ordinal() == 1
            && value.preceding_chain() != canonical_empty_draft_piece_fragment_chain_v1())
    {
        return Err(CodecError::InvalidLength("draft-piece fragment shape"));
    }
    let mut e = Encoder::new();
    enc_fragment_key(&mut e, value.key());
    enc_replacement(&mut e, value.replacement());
    enc_digest(&mut e, value.preceding_chain());
    enc_digest(&mut e, value.chain_digest());
    Ok(e.finish())
}

#[cfg(feature = "test-faults")]
pub(crate) fn encode_fragment_unchecked_for_test_fault(
    value: &DraftPieceBuildFragmentV1,
) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_fragment_key(&mut e, value.key());
    enc_replacement(&mut e, value.replacement());
    enc_digest(&mut e, value.preceding_chain());
    enc_digest(&mut e, value.chain_digest());
    e.finish()
}

fn decode_fragment(bytes: &[u8]) -> Result<DraftPieceBuildFragmentV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_fragment_key(&mut d)?;
    let replacement = dec_replacement(&mut d)?;
    let preceding_chain = dec_digest(&mut d)?;
    let chain_digest = dec_digest(&mut d)?;
    d.finish()?;
    validate_fragment(&replacement)
        .map_err(|_| CodecError::InvalidLength("draft-piece fragment replacement"))?;
    if !key.is_locally_valid()
        || (key.ordinal() == 1
            && preceding_chain != canonical_empty_draft_piece_fragment_chain_v1())
        || chain_digest
            != draft_piece_fragment_chain_link_v1(preceding_chain, key.ordinal(), &replacement)
    {
        return Err(CodecError::InvalidLength("draft-piece fragment chain"));
    }
    let value = DraftPieceBuildFragmentV1::new(key, replacement, preceding_chain, chain_digest);
    Ok(value)
}

family!(
    DraftPieceRootsFamily,
    DraftPieceRootKeyV1,
    DraftPieceRootRecordV1,
    "draft-piece-roots",
    49,
    SMALL_MAX,
    encode_root_key,
    decode_root_key,
    encode_root,
    decode_root
);
family!(
    DraftPieceNodesFamily,
    DraftPieceRecordKeyV1,
    DraftPieceNodeRecordV1,
    "draft-piece-nodes",
    32,
    32_768,
    encode_record_key,
    decode_record_key,
    encode_node,
    decode_node
);
family!(
    DraftPieceLeavesFamily,
    DraftPieceRecordKeyV1,
    DraftPieceLeafRecordV1,
    "draft-piece-leaves",
    32,
    33_000,
    encode_record_key,
    decode_record_key,
    encode_leaf,
    decode_leaf
);
family!(
    DraftMarkerIdentityIndexFamily,
    DraftMarkerIdentityRecordKeyV1,
    DraftMarkerIdentityRecordV1,
    "draft-marker-identity-index",
    33,
    32_768,
    encode_marker_identity_family_key,
    decode_marker_identity_family_key,
    encode_marker_identity_record,
    decode_marker_identity_record
);
family!(
    DraftPieceBuildsFamily,
    DraftPieceSettlementKeyV1,
    DraftPieceBuildRecordV1,
    "draft-piece-builds",
    48,
    8_192,
    encode_settlement_family_key,
    decode_settlement_family_key,
    encode_build,
    decode_build
);
family!(
    DraftPieceBuildFragmentsFamily,
    DraftPieceBuildFragmentKeyV1,
    DraftPieceBuildFragmentV1,
    "draft-piece-build-fragments",
    56,
    75_000,
    encode_fragment_family_key,
    decode_fragment_family_key,
    encode_fragment,
    decode_fragment
);
family!(
    DraftPieceBuildProgressFamily,
    DraftPieceBuildProgressReceiptKeyV1,
    DraftPieceBuildProgressReceiptV1,
    "draft-piece-build-progress",
    56,
    1_024,
    encode_progress_family_key,
    decode_progress_family_key,
    encode_progress_receipt,
    decode_progress_receipt
);
family!(
    DraftPieceSettlementsFamily,
    DraftPieceSettlementKeyV1,
    DraftPieceSettlementV1,
    "draft-piece-settlements",
    48,
    65_536,
    encode_settlement_family_key,
    decode_settlement_family_key,
    encode_settlement,
    decode_settlement
);
family!(
    DraftEditorCandidateSessionsFamily,
    DraftEditorCandidateSessionRecordKeyV1,
    DraftEditorCandidateSessionRecordV1,
    "draft-editor-candidate-sessions",
    49,
    4_096,
    encode_session_family_key,
    decode_session_family_key,
    encode_session_record,
    decode_session_record
);
