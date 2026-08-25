use beryl_model::{DraftRevision, SyndicDraftId};
use sha2::{Digest, Sha256};

use crate::{SyndicPointReadLimit, SyndicReadError};

use super::*;

const EMPTY_MARKERS: &[u8] = b"syndic/draft-ordered-marker-fold/v1/empty";
const EMPTY_ROOT: &[u8] = b"syndic/draft-sequence-root/v1/empty";
const EMPTY_MARKER_INDEX: &[u8] = b"syndic/draft-marker-identity-index-root/v1/empty";
const COMBINED_ROOT: &[u8] = b"syndic/draft-combined-root/v1";
const EMPTY_ROOT_OPERATION: &[u8] = b"syndic/canonical-empty-draft-root-build-operation/v1";
const TEXT_LEAF: &[u8] = b"beryl.syndic.draft-piece.text-leaf.v1";
const MARKER_LEAF: &[u8] = b"beryl.syndic.draft-piece.marker-leaf.v2";
const MARKER_FOLD: &[u8] = b"beryl.syndic.draft-piece.marker-fold.v1";
const NODE: &[u8] = b"beryl.syndic.draft-piece.node.v1";
const ROOT: &[u8] = b"beryl.syndic.draft-piece.root.v1";
const RECORD_ID: &[u8] = b"beryl.syndic.draft-piece.record-id.v1";
const PROPOSAL: &[u8] = b"beryl.syndic.draft-piece.proposal.v1";
const FRAGMENT_CHAIN_EMPTY: &[u8] = b"syndic/draft-piece-build-fragment-chain/v1/empty";
const FRAGMENT_CHAIN_LINK: &[u8] = b"syndic/draft-piece-build-fragment-chain/v1/link";

#[derive(Debug, thiserror::Error)]
pub enum DraftPiecePrepareErrorV1 {
    #[error("draft-piece read failed: {0}")]
    Read(#[from] SyndicReadError),
    #[error("draft-piece edit was rejected: {0:?}")]
    Rejected(DraftPieceRejectedReasonV1),
    #[error("draft-piece root is missing or inconsistent")]
    InvalidRoot,
    #[error("draft-piece immutable record is absent")]
    Absent,
    #[error("current draft changed during the exact read")]
    ConcurrentChange,
}

pub fn canonical_empty_marker_digest_v1() -> DraftPieceDigestV1 {
    lp_hash(&[EMPTY_MARKERS])
}

pub fn canonical_empty_root_digest_v1() -> DraftPieceDigestV1 {
    lp_hash(&[EMPTY_ROOT])
}

pub fn canonical_empty_marker_identity_index_digest_v1() -> DraftPieceDigestV1 {
    lp_hash(&[EMPTY_MARKER_INDEX])
}

pub fn canonical_empty_draft_root_operation_id_v1(
    draft_id: SyndicDraftId,
) -> DraftPieceOperationIdV1 {
    let digest = lp_hash(&[EMPTY_ROOT_OPERATION, draft_id.as_bytes()]);
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    DraftPieceOperationIdV1::from_bytes(bytes)
}

pub fn canonical_empty_draft_piece_root_v1(
    draft_id: SyndicDraftId,
    _revision: DraftRevision,
    operation_id: DraftPieceOperationIdV1,
) -> DraftPieceRootRecordV1 {
    let sequence = DraftPieceSummaryV1::new(
        0,
        0,
        0,
        0,
        0,
        canonical_empty_marker_digest_v1(),
        0,
        canonical_empty_root_digest_v1(),
    );
    let index = DraftMarkerIdentityIndexSummaryV1::new(
        0,
        0,
        canonical_empty_marker_identity_index_digest_v1(),
    );
    let key = DraftPieceRootKeyV1::direct_canonical_empty(draft_id, operation_id);
    let marker_commitment = canonical_empty_draft_marker_commitment_v1();
    let combined = combined_root_digest(sequence, index, marker_commitment);
    DraftPieceRootRecordV1::new(DraftPieceRootReferenceV1::new_authenticated(
        key,
        None,
        sequence,
        None,
        index,
        None,
        0,
        marker_commitment,
        combined,
    ))
}

fn lp_hash(parts: &[&[u8]]) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

pub(crate) fn combined_root_digest(
    sequence: DraftPieceSummaryV1,
    index: DraftMarkerIdentityIndexSummaryV1,
    marker_commitment: DraftMarkerCommitmentV1,
) -> DraftPieceDigestV1 {
    let mut sequence_bytes = Vec::with_capacity(105);
    sequence_bytes.extend_from_slice(&sequence.logical_utf8_bytes().to_be_bytes());
    sequence_bytes.extend_from_slice(&sequence.newline_count().to_be_bytes());
    sequence_bytes.extend_from_slice(&sequence.logical_line_count().to_be_bytes());
    sequence_bytes.extend_from_slice(&sequence.piece_count().to_be_bytes());
    sequence_bytes.extend_from_slice(&sequence.marker_count().to_be_bytes());
    sequence_bytes.extend_from_slice(sequence.marker_digest().as_bytes());
    sequence_bytes.push(sequence.height());
    sequence_bytes.extend_from_slice(sequence.root_digest().as_bytes());
    let mut index_bytes = Vec::with_capacity(41);
    index_bytes.extend_from_slice(&index.record_count().to_be_bytes());
    index_bytes.push(index.height());
    index_bytes.extend_from_slice(index.root_digest().as_bytes());
    let mut marker_commitment_bytes = Vec::with_capacity(49);
    marker_commitment_bytes.extend_from_slice(&marker_commitment.tree_root_digest());
    marker_commitment_bytes.extend_from_slice(&marker_commitment.marker_count().to_be_bytes());
    match marker_commitment.maximum_image_label() {
        Some(label) => {
            marker_commitment_bytes.push(1);
            marker_commitment_bytes.extend_from_slice(&label.get().to_be_bytes());
        }
        None => marker_commitment_bytes.push(0),
    }
    lp_hash(&[
        COMBINED_ROOT,
        &sequence_bytes,
        &index_bytes,
        &marker_commitment_bytes,
    ])
}

pub(crate) fn draft_piece_root_reference_is_locally_exact_v1(
    root: DraftPieceRootReferenceV1,
) -> bool {
    root.summary().text_summary().is_canonical()
        && root.summary().marker_count() == root.marker_index_summary().record_count()
        && root.summary().marker_count() == root.marker_commitment().marker_count()
        && (root.marker_commitment().marker_count() == 0)
            == root.marker_commitment().maximum_image_label().is_none()
        && (root.marker_commitment().marker_count() != 0
            || root.marker_commitment() == canonical_empty_draft_marker_commitment_v1())
        && (root.marker_commitment().marker_count() == 0) == root.marker_order_root().is_none()
        && (root.marker_commitment().marker_count() != 0 || root.marker_order_height() == 0)
        && (root.marker_commitment().marker_count() == 0 || root.marker_order_height() != 0)
        && root.combined_digest()
            == combined_root_digest(
                root.summary(),
                root.marker_index_summary(),
                root.marker_commitment(),
            )
}

pub(crate) fn draft_piece_build_roots_are_locally_exact_v1(roots: DraftPieceBuildRootsV1) -> bool {
    let sequence = roots.sequence_summary();
    let index = roots.marker_index_summary();
    let commitment = roots.marker_commitment();
    let empty_sequence = DraftPieceSummaryV1::new(
        0,
        0,
        0,
        0,
        0,
        canonical_empty_marker_digest_v1(),
        0,
        canonical_empty_root_digest_v1(),
    );
    let empty_index = DraftMarkerIdentityIndexSummaryV1::new(
        0,
        0,
        canonical_empty_marker_identity_index_digest_v1(),
    );
    sequence.text_summary().is_canonical()
        && sequence.marker_count() == index.record_count()
        && sequence.marker_count() == commitment.marker_count()
        && (sequence.piece_count() == 0) == roots.sequence_root().is_none()
        && (sequence.piece_count() != 0 || sequence == empty_sequence)
        && (sequence.piece_count() == 0 || sequence.height() != 0)
        && (sequence.marker_count() != 0
            || sequence.marker_digest() == canonical_empty_marker_digest_v1())
        && (index.record_count() == 0) == roots.marker_index_root().is_none()
        && (index.record_count() != 0 || index == empty_index)
        && (index.record_count() == 0 || index.height() != 0)
        && (commitment.marker_count() == 0) == roots.marker_order_root().is_none()
        && (commitment.marker_count() == 0) == commitment.maximum_image_label().is_none()
        && (commitment.marker_count() != 0
            || commitment == canonical_empty_draft_marker_commitment_v1())
        && (commitment.marker_count() != 0 || roots.marker_order_height() == 0)
        && (commitment.marker_count() == 0 || roots.marker_order_height() != 0)
}

pub(crate) fn validate_marker_order_root_record(
    record: DraftMarkerOrderRecordV1,
    roots: DraftPieceBuildRootsV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let Some(root_id) = roots.marker_order_root() else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    };
    let commitment = roots.marker_commitment();
    let DraftMarkerOrderRecordV1::Internal {
        key,
        height,
        children,
        digest,
    } = record
    else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    };
    if key.id() != root_id
        || height != roots.marker_order_height()
        || height == 0
        || height > DRAFT_PIECE_MAX_HEIGHT
        || children.is_empty()
        || children.len() > DRAFT_PIECE_MAX_CHILDREN
        || digest != marker_order_node_digest(height, &children)
        || digest.as_bytes() != &commitment.tree_root_digest()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let marker_count = children
        .iter()
        .try_fold(0_u64, |count, child| {
            count.checked_add(child.marker_count())
        })
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let maximum = children
        .iter()
        .filter_map(|child| child.maximum_image_label())
        .max();
    if marker_count != commitment.marker_count() || maximum != commitment.maximum_image_label() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(())
}

fn digest_bytes(domain: &[u8], parts: &[&[u8]]) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update(domain);
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

fn marker_digest(
    marker: DraftPieceMarkerV1,
    text_summary: DraftPieceTextSummaryV1,
) -> DraftPieceDigestV1 {
    digest_bytes(
        MARKER_LEAF,
        &[
            marker.marker_id().as_bytes(),
            &marker.order_key().to_be_bytes(),
            &marker.label().get().to_be_bytes(),
            &[marker.asset_id().version() as u8],
            &marker.asset_id().digest(),
            &marker.asset_id().length().get().to_be_bytes(),
            &text_summary.logical_utf8_bytes().to_be_bytes(),
            &text_summary.newline_count().to_be_bytes(),
            &text_summary.logical_line_count().to_be_bytes(),
        ],
    )
}

pub(crate) fn leaf_digest(
    value: &DraftPieceLeafValueV1,
    text_summary: DraftPieceTextSummaryV1,
) -> DraftPieceDigestV1 {
    match value {
        DraftPieceLeafValueV1::Text(text) => digest_bytes(
            TEXT_LEAF,
            &[
                text.as_bytes(),
                &text_summary.logical_utf8_bytes().to_be_bytes(),
                &text_summary.newline_count().to_be_bytes(),
                &text_summary.logical_line_count().to_be_bytes(),
            ],
        ),
        DraftPieceLeafValueV1::Marker(marker) => marker_digest(*marker, text_summary),
    }
}

pub(crate) fn marker_fold<'a>(
    digests: impl IntoIterator<Item = &'a DraftPieceDigestV1>,
) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update(MARKER_FOLD);
    let mut count = 0_u64;
    for value in digests {
        digest.update(value.as_bytes());
        count += 1;
    }
    if count == 0 {
        canonical_empty_marker_digest_v1()
    } else {
        digest.update(count.to_be_bytes());
        DraftPieceDigestV1::from_bytes(digest.finalize().into())
    }
}

pub(crate) fn record_id(
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
    ordinal: u64,
    digest: DraftPieceDigestV1,
) -> DraftPieceRecordIdV1 {
    let value = digest_bytes(
        RECORD_ID,
        &[
            draft_id.as_bytes(),
            session_id.as_bytes(),
            operation_id.as_bytes(),
            &ordinal.to_be_bytes(),
            digest.as_bytes(),
        ],
    );
    let mut id = [0; 16];
    id.copy_from_slice(&value.as_bytes()[..16]);
    DraftPieceRecordIdV1::from_bytes(id)
}

pub(crate) fn child_for_leaf(record: &DraftPieceLeafRecordV1) -> DraftPieceChildV1 {
    match record.value() {
        DraftPieceLeafValueV1::Text(text) => DraftPieceChildV1::new(
            record.key().id(),
            record.digest(),
            text.len() as u64,
            record.text_summary().newline_count(),
            record.text_summary().logical_line_count(),
            1,
            0,
            canonical_empty_marker_digest_v1(),
            DraftCompositeSearchKeyV1::BeforeMarkers(0),
            DraftCompositeSearchKeyV1::AfterMarkers(text.len() as u64),
        ),
        DraftPieceLeafValueV1::Marker(marker) => {
            let key = DraftCompositeSearchKeyV1::Marker {
                anchor: 0,
                order_key: marker.order_key(),
                marker_id: marker.marker_id(),
            };
            DraftPieceChildV1::new(
                record.key().id(),
                record.digest(),
                0,
                0,
                0,
                1,
                1,
                record.digest(),
                key,
                key,
            )
        }
    }
}

fn aggregate_children(
    children: &[DraftPieceChildV1],
) -> Result<(DraftPieceTextSummaryV1, u64, u64), DraftPieceRejectedReasonV1> {
    let mut text = 0_u64;
    let mut newlines = 0_u64;
    let mut pieces = 0_u64;
    let mut markers = 0_u64;
    let mut prefix = 0_u64;
    let mut previous_last_marker: Option<DraftCompositeSearchKeyV1> = None;
    for child in children {
        if child.piece_count() == 0
            || child.marker_count() > child.piece_count()
            || child.first().anchor() != 0
            || child.last().anchor() != child.logical_utf8_bytes()
            || child.first() > child.last()
            || (child.logical_utf8_bytes() == 0 && child.marker_count() != child.piece_count())
            || !child.text_summary().is_canonical()
        {
            return Err(DraftPieceRejectedReasonV1::TreeLimit);
        }
        let translated_first = checked_offset_key(child.first(), prefix)?;
        if let (Some(previous), DraftCompositeSearchKeyV1::Marker { .. }) =
            (previous_last_marker, translated_first)
        {
            if previous.anchor() == translated_first.anchor()
                && (previous >= translated_first
                    || same_marker_order_slot(previous, translated_first))
            {
                return Err(DraftPieceRejectedReasonV1::TreeLimit);
            }
        }
        previous_last_marker = match checked_offset_key(child.last(), prefix)? {
            marker @ DraftCompositeSearchKeyV1::Marker { .. } => Some(marker),
            _ => None,
        };
        prefix = prefix
            .checked_add(child.logical_utf8_bytes())
            .ok_or(DraftPieceRejectedReasonV1::AggregateOverflow)?;
        text = text
            .checked_add(child.logical_utf8_bytes())
            .ok_or(DraftPieceRejectedReasonV1::AggregateOverflow)?;
        newlines = newlines
            .checked_add(child.newline_count())
            .ok_or(DraftPieceRejectedReasonV1::AggregateOverflow)?;
        pieces = pieces
            .checked_add(child.piece_count())
            .ok_or(DraftPieceRejectedReasonV1::AggregateOverflow)?;
        markers = markers
            .checked_add(child.marker_count())
            .ok_or(DraftPieceRejectedReasonV1::AggregateOverflow)?;
    }
    let lines = if text == 0 {
        if newlines != 0 {
            return Err(DraftPieceRejectedReasonV1::TreeLimit);
        }
        0
    } else {
        newlines
            .checked_add(1)
            .ok_or(DraftPieceRejectedReasonV1::AggregateOverflow)?
    };
    Ok((
        DraftPieceTextSummaryV1::new(text, newlines, lines),
        pieces,
        markers,
    ))
}

fn same_marker_order_slot(
    left: DraftCompositeSearchKeyV1,
    right: DraftCompositeSearchKeyV1,
) -> bool {
    matches!(
        (left, right),
        (
            DraftCompositeSearchKeyV1::Marker { anchor: left_anchor, order_key: left_order, .. },
            DraftCompositeSearchKeyV1::Marker { anchor: right_anchor, order_key: right_order, .. },
        ) if left_anchor == right_anchor && left_order == right_order
    )
}

pub(crate) fn node_digest(height: u8, children: &[DraftPieceChildV1]) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update(NODE);
    digest.update([height]);
    digest.update((children.len() as u64).to_be_bytes());
    for child in children {
        digest.update(child.id().as_bytes());
        digest.update(child.digest().as_bytes());
        digest.update(child.logical_utf8_bytes().to_be_bytes());
        digest.update(child.newline_count().to_be_bytes());
        digest.update(child.logical_line_count().to_be_bytes());
        digest.update(child.piece_count().to_be_bytes());
        digest.update(child.marker_count().to_be_bytes());
        digest.update(child.marker_digest().as_bytes());
        digest.update(search_key_bytes(child.first()));
        digest.update(search_key_bytes(child.last()));
    }
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

fn search_key_bytes(key: DraftCompositeSearchKeyV1) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(33);
    match key {
        DraftCompositeSearchKeyV1::BeforeMarkers(anchor) => {
            bytes.push(0);
            bytes.extend_from_slice(&anchor.to_be_bytes());
        }
        DraftCompositeSearchKeyV1::Marker {
            anchor,
            order_key,
            marker_id,
        } => {
            bytes.push(1);
            bytes.extend_from_slice(&anchor.to_be_bytes());
            bytes.extend_from_slice(&order_key.to_be_bytes());
            bytes.extend_from_slice(marker_id.as_bytes());
        }
        DraftCompositeSearchKeyV1::AfterMarkers(anchor) => {
            bytes.push(2);
            bytes.extend_from_slice(&anchor.to_be_bytes());
        }
    }
    bytes
}

pub(crate) fn checked_offset_key(
    key: DraftCompositeSearchKeyV1,
    offset: u64,
) -> Result<DraftCompositeSearchKeyV1, DraftPieceRejectedReasonV1> {
    let add = |anchor: u64| {
        anchor
            .checked_add(offset)
            .ok_or(DraftPieceRejectedReasonV1::AggregateOverflow)
    };
    Ok(match key {
        DraftCompositeSearchKeyV1::BeforeMarkers(anchor) => {
            DraftCompositeSearchKeyV1::BeforeMarkers(add(anchor)?)
        }
        DraftCompositeSearchKeyV1::Marker {
            anchor,
            order_key,
            marker_id,
        } => DraftCompositeSearchKeyV1::Marker {
            anchor: add(anchor)?,
            order_key,
            marker_id,
        },
        DraftCompositeSearchKeyV1::AfterMarkers(anchor) => {
            DraftCompositeSearchKeyV1::AfterMarkers(add(anchor)?)
        }
    })
}

pub(crate) fn child_for_node(
    record: &DraftPieceNodeRecordV1,
) -> Result<DraftPieceChildV1, DraftPieceRejectedReasonV1> {
    let (text, pieces, markers) = aggregate_children(record.children())?;
    let marker_digests: Vec<_> = record
        .children()
        .iter()
        .filter(|child| child.marker_count() != 0)
        .map(|child| child.marker_digest())
        .collect();
    let marker_digest = marker_fold(marker_digests.iter());
    let first = record
        .children()
        .first()
        .ok_or(DraftPieceRejectedReasonV1::TreeLimit)?
        .first();
    let mut prefix = 0_u64;
    let mut last = first;
    for child in record.children() {
        last = checked_offset_key(child.last(), prefix)?;
        prefix = prefix
            .checked_add(child.logical_utf8_bytes())
            .ok_or(DraftPieceRejectedReasonV1::AggregateOverflow)?;
    }
    Ok(DraftPieceChildV1::new(
        record.key().id(),
        record.digest(),
        text.logical_utf8_bytes(),
        text.newline_count(),
        text.logical_line_count(),
        pieces,
        markers,
        marker_digest,
        first,
        last,
    ))
}

pub(crate) fn root_digest(
    summary: DraftPieceSummaryV1,
    node_digest: DraftPieceDigestV1,
) -> DraftPieceDigestV1 {
    digest_bytes(
        ROOT,
        &[
            &[summary.height()],
            &summary.logical_utf8_bytes().to_be_bytes(),
            &summary.newline_count().to_be_bytes(),
            &summary.logical_line_count().to_be_bytes(),
            &summary.piece_count().to_be_bytes(),
            &summary.marker_count().to_be_bytes(),
            summary.marker_digest().as_bytes(),
            node_digest.as_bytes(),
        ],
    )
}

pub(crate) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(75_000).expect("draft-piece point limit is nonzero")
}

fn hash_marker_position(digest: &mut Sha256, position: DraftCompositePositionV1) {
    digest.update(position.utf8_offset().to_be_bytes());
    match position.gap() {
        DraftCompositeGapWitnessV1::Unambiguous => digest.update([0]),
        DraftCompositeGapWitnessV1::BeforeAll => digest.update([1]),
        DraftCompositeGapWitnessV1::Between {
            left_order_key,
            left_marker_id,
            right_order_key,
            right_marker_id,
        } => {
            digest.update([2]);
            digest.update(left_order_key.to_be_bytes());
            digest.update(left_marker_id.as_bytes());
            digest.update(right_order_key.to_be_bytes());
            digest.update(right_marker_id.as_bytes());
        }
        DraftCompositeGapWitnessV1::AfterAll => digest.update([3]),
    }
}

pub fn draft_piece_fragment_chain_link_v1(
    preceding: DraftPieceDigestV1,
    ordinal: u64,
    replacement: &DraftPieceReplacementV1,
) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update(FRAGMENT_CHAIN_LINK);
    digest.update(preceding.as_bytes());
    digest.update(ordinal.to_be_bytes());
    digest.update([u8::from(replacement.is_continuation())]);
    hash_position(&mut digest, replacement.start());
    hash_position(&mut digest, replacement.end());
    digest.update((replacement.inserted().len() as u64).to_be_bytes());
    for piece in replacement.inserted() {
        match piece {
            DraftPieceV1::Text(text) => {
                digest.update([0]);
                digest.update((text.len() as u64).to_be_bytes());
                digest.update(text.as_bytes());
            }
            DraftPieceV1::Marker(marker) => {
                digest.update([1]);
                digest.update(marker.marker_id().as_bytes());
                digest.update(marker.order_key().to_be_bytes());
                digest.update(marker.label().get().to_be_bytes());
                hash_asset_id(&mut digest, marker.asset_id());
            }
        }
    }
    match replacement.marker_effect() {
        None => digest.update([0]),
        Some(effect) => {
            digest.update([1]);
            hash_marker_effect(&mut digest, effect);
        }
    }
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

pub fn canonical_empty_draft_piece_fragment_chain_v1() -> DraftPieceDigestV1 {
    digest_bytes(FRAGMENT_CHAIN_EMPTY, &[])
}

fn terminal_first_session_closure_is_exact(
    settlement: &DraftPieceSettlementV1,
    noncommit: &DraftPieceNoncommitClosureV1,
) -> bool {
    let observed = noncommit.observed_session();
    let Some(source_generation) =
        canonical_edit_command_source_generation(settlement.canonical_header())
    else {
        return false;
    };
    let source = DraftEditorCandidateSessionV1::from_parts(
        observed.thread_id(),
        observed.draft_id(),
        observed.session_id(),
        observed.open_operation_id(),
        source_generation,
        observed.durable_base_selector_revision(),
        observed.durable_base_root(),
        observed.durable_base_history(),
        observed.published_candidate_generation(),
        observed.published_selector_revision(),
        observed.published_root(),
        observed.published_history(),
        observed.newest_candidate_generation(),
        observed.newest_root(),
        observed.newest_history(),
        observed.dirty_generation(),
        observed.logical_extent(),
        observed.lifecycle(),
        None,
    );
    let custody = DraftEditorActiveOperationV1::building(
        settlement.key().operation_id(),
        settlement.proposal_digest(),
        settlement.predecessor_candidate_generation(),
        settlement.predecessor_root(),
        settlement.predecessor_history(),
        settlement.terminal_receipt(),
    );
    let Some(claimed) = source.with_active_operation(custody) else {
        return false;
    };
    let Some(cleared) = claimed.clear_active_operation(&custody) else {
        return false;
    };
    source.is_coherent()
        && source.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
        && source.active_operation().is_none()
        && source.newest_candidate_generation() == settlement.predecessor_candidate_generation()
        && source.newest_root() == settlement.predecessor_root()
        && source.newest_history() == settlement.predecessor_history()
        && noncommit.observed_history().reference() == source.newest_history()
        && claimed.session_generation() == source_generation.checked_add(1).unwrap_or(0)
        && claimed.active_operation() == Some(&custody)
        && cleared.session_generation() == source_generation.checked_add(2).unwrap_or(0)
        && cleared == *observed
}

pub(crate) fn settlement_closure_is_exact(settlement: &DraftPieceSettlementV1) -> bool {
    let header = DraftPieceEditHeaderV1::new(
        settlement.key().draft_id(),
        settlement.key().session_id(),
        settlement.predecessor_candidate_generation(),
        settlement.predecessor_root(),
        settlement.predecessor_history(),
        settlement.key().operation_id(),
        settlement.predecessor_caret(),
        settlement.predecessor_selection(),
        settlement.caret(),
        settlement.selection(),
        settlement.fragment_count(),
        settlement.fragment_chain(),
    );
    if settlement.predecessor_root().key().draft_id() != settlement.key().draft_id()
        || !canonical_edit_command_is_exact(settlement.canonical_header(), header)
        || settlement.proposal_digest() != canonical_proposal_digest(settlement.canonical_header())
    {
        return false;
    }
    let Some(source) = settlement.terminal_source() else {
        return settlement.build_digest().is_none()
            && settlement.terminal_receipt().key().transition_ordinal() == 1
            && settlement.terminal_receipt().key().draft_id() == settlement.key().draft_id()
            && settlement.terminal_receipt().key().session_id() == settlement.key().session_id()
            && settlement.terminal_receipt().key().operation_id()
                == settlement.key().operation_id()
            && matches!(
                settlement.outcome(),
                DraftPieceSettlementOutcomeV1::Rejected(_)
                    | DraftPieceSettlementOutcomeV1::Cancelled
                    | DraftPieceSettlementOutcomeV1::Error(_)
            )
            && matches!(
                settlement.closure(),
                DraftPieceSettlementClosureV1::Noncommit(noncommit)
                    if terminal_first_session_closure_is_exact(settlement, noncommit)
                        && noncommit.proposed_successor().is_none()
                        && noncommit.occupied_identity().is_none()
            );
    };
    if source.draft_id() != settlement.key().draft_id()
        || source.session_id() != settlement.key().session_id()
        || source.operation_id() != settlement.key().operation_id()
        || source.proposal_digest() != settlement.proposal_digest()
        || source.predecessor_candidate_generation()
            != settlement.predecessor_candidate_generation()
        || source.predecessor_root() != settlement.predecessor_root()
        || source.predecessor_history() != settlement.predecessor_history()
        || source.fragment_count() != settlement.fragment_count()
        || source.fragment_chain() != settlement.fragment_chain()
        || source.predecessor_caret() != settlement.predecessor_caret()
        || source.predecessor_selection() != settlement.predecessor_selection()
        || source.caret() != settlement.caret()
        || source.selection() != settlement.selection()
        || source.build_digest() != settlement.build_digest()
        || source.canonical_header() != settlement.canonical_header()
        || !build_record_is_exact(source)
        || !matches!(
            source.lifecycle(),
            DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
        )
        || source
            .progress_receipt()
            .key()
            .transition_ordinal()
            .checked_add(1)
            != Some(settlement.terminal_receipt().key().transition_ordinal())
        || settlement.terminal_receipt().key().draft_id() != settlement.key().draft_id()
        || settlement.terminal_receipt().key().session_id() != settlement.key().session_id()
        || settlement.terminal_receipt().key().operation_id() != settlement.key().operation_id()
    {
        return false;
    }
    match (settlement.outcome(), settlement.closure()) {
        (
            DraftPieceSettlementOutcomeV1::Committed {
                candidate_generation,
                successor,
                history,
                caret,
                selection,
            },
            DraftPieceSettlementClosureV1::Committed(adoption),
        ) => {
            source.lifecycle() == DraftPieceBuildLifecycleV1::Complete
                && source.successor() == Some(*successor)
                && source.build_digest().is_some()
                && *caret == settlement.caret()
                && *selection == settlement.selection()
                && adoption.predecessor_session().draft_id() == settlement.key().draft_id()
                && adoption.predecessor_session().session_id() == settlement.key().session_id()
                && adoption.predecessor_session().newest_candidate_generation()
                    == source.predecessor_candidate_generation()
                && adoption.predecessor_session().newest_root() == source.predecessor_root()
                && adoption.predecessor_session().lifecycle()
                    == DraftEditorCandidateSessionLifecycleV1::Active
                && matches!(
                    adoption.predecessor_session().active_operation(),
                    Some(custody)
                        if custody.operation_id() == source.operation_id()
                            && custody.proposal_digest() == Some(source.proposal_digest())
                            && custody.predecessor_candidate_generation()
                                == source.predecessor_candidate_generation()
                            && custody.predecessor_root() == source.predecessor_root()
                            && custody.predecessor_history() == source.predecessor_history()
                            && custody.build_receipt() == Some(source.progress_receipt())
                )
                && adoption.predecessor_history().reference() == source.predecessor_history()
                && adoption.predecessor_history().reference()
                    == adoption.predecessor_session().newest_history()
                && ordinary_draft_edit_history_adoption_is_locally_exact(
                    adoption.predecessor_history(),
                    adoption.transition(),
                    adoption.adopted_history(),
                    settlement.predecessor_caret(),
                    settlement.predecessor_selection(),
                    settlement.caret(),
                    settlement.selection(),
                    settlement.key().operation_id(),
                )
                && adoption
                    .predecessor_session()
                    .adopted(*successor, *history)
                    .as_ref()
                    == Some(adoption.adopted_session())
                && adoption.adopted_session().active_operation().is_none()
                && adoption.adopted_session().newest_candidate_generation() == *candidate_generation
                && *candidate_generation
                    == source
                        .predecessor_candidate_generation()
                        .checked_add(1)
                        .unwrap_or(0)
                && adoption.adopted_session().newest_root() == *successor
                && adoption.adopted_session().newest_history() == *history
                && adoption.adopted_session().session_generation()
                    == adoption
                        .predecessor_session()
                        .session_generation()
                        .checked_add(1)
                        .unwrap_or(0)
                && adoption.adopted_session().dirty_generation()
                    == adoption
                        .predecessor_session()
                        .dirty_generation()
                        .checked_add(1)
                        .unwrap_or(0)
                && adoption.adopted_root().reference() == *successor
                && adoption.adopted_history().reference() == *history
        }
        (
            DraftPieceSettlementOutcomeV1::Conflict {
                current_candidate_generation,
                current_root,
                current_history,
            },
            DraftPieceSettlementClosureV1::Noncommit(noncommit),
        ) => {
            source.lifecycle() == DraftPieceBuildLifecycleV1::Complete
                && source.successor().is_some()
                && noncommit.observed_session().draft_id() == settlement.key().draft_id()
                && noncommit.observed_session().session_id() == settlement.key().session_id()
                && noncommit.observed_session().newest_candidate_generation()
                    == *current_candidate_generation
                && noncommit.observed_session().newest_root() == *current_root
                && noncommit.observed_session().newest_history() == *current_history
                && noncommit.observed_history().reference() == *current_history
                && noncommit.observed_session().active_operation().is_none()
                && noncommit.proposed_successor() == source.successor()
                && noncommit.occupied_identity().is_none()
        }
        (
            DraftPieceSettlementOutcomeV1::Rejected(_)
            | DraftPieceSettlementOutcomeV1::Cancelled
            | DraftPieceSettlementOutcomeV1::Error(
                DraftPieceErrorReasonV1::OccupiedIdentity
                | DraftPieceErrorReasonV1::UnsettledOperation
                | DraftPieceErrorReasonV1::MissingRecord
                | DraftPieceErrorReasonV1::CorruptRecord
                | DraftPieceErrorReasonV1::ResourceLimit
                | DraftPieceErrorReasonV1::HistoryCapacityUnavailable,
            ),
            DraftPieceSettlementClosureV1::Noncommit(noncommit),
        ) => {
            noncommit.observed_session().draft_id() == settlement.key().draft_id()
                && noncommit.observed_session().session_id() == settlement.key().session_id()
                && noncommit.observed_session().active_operation().is_none()
                && noncommit.proposed_successor() == source.successor()
                && noncommit.occupied_identity().is_none()
        }
        (
            DraftPieceSettlementOutcomeV1::Error(
                DraftPieceErrorReasonV1::OccupiedIdentityNoncommit,
            ),
            DraftPieceSettlementClosureV1::Noncommit(noncommit),
        ) => {
            let Some(proof) = noncommit.occupied_identity() else {
                return false;
            };
            let OccupiedIdentityDifferenceV1::Root {
                key,
                requested,
                occupied,
            } = proof.difference()
            else {
                return false;
            };
            source.lifecycle() == DraftPieceBuildLifecycleV1::Complete
                && source.successor() == Some(requested.reference())
                && noncommit.observed_session().draft_id() == settlement.key().draft_id()
                && noncommit.observed_session().session_id() == settlement.key().session_id()
                && noncommit.observed_session().active_operation().is_none()
                && noncommit.proposed_successor() == source.successor()
                && proof.key() == settlement.key()
                && proof.requested_proposal_digest() == settlement.proposal_digest()
                && proof.occupied_proposal_digest() == settlement.proposal_digest()
                && *key == requested.reference().key()
                && *key == occupied.reference().key()
                && requested != occupied
        }
        _ => false,
    }
}

pub(crate) fn settlement_terminal_build_is_exact(
    settlement: &DraftPieceSettlementV1,
    stored: Option<&DraftPieceBuildRecordV1>,
) -> bool {
    let lifecycle = match settlement.outcome() {
        DraftPieceSettlementOutcomeV1::Committed { .. } => DraftPieceBuildLifecycleV1::Committed,
        DraftPieceSettlementOutcomeV1::Rejected(_) => DraftPieceBuildLifecycleV1::Rejected,
        DraftPieceSettlementOutcomeV1::Conflict { .. } => DraftPieceBuildLifecycleV1::Conflict,
        DraftPieceSettlementOutcomeV1::Cancelled => DraftPieceBuildLifecycleV1::Cancelled,
        DraftPieceSettlementOutcomeV1::Error(_) => DraftPieceBuildLifecycleV1::Error,
    };
    let Some(source) = settlement.terminal_source() else {
        let Some(stored) = stored else { return false };
        let origin = DraftPieceBuildBoundaryV1::new(0, 0);
        return build_record_is_exact(stored)
            && stored.draft_id() == settlement.key().draft_id()
            && stored.session_id() == settlement.key().session_id()
            && stored.operation_id() == settlement.key().operation_id()
            && stored.predecessor_candidate_generation()
                == settlement.predecessor_candidate_generation()
            && stored.predecessor_root() == settlement.predecessor_root()
            && stored.proposal_digest() == settlement.proposal_digest()
            && stored.caret() == settlement.caret()
            && stored.selection() == settlement.selection()
            && stored.fragment_count() == settlement.fragment_count()
            && stored.fragment_chain() == settlement.fragment_chain()
            && stored.canonical_header() == settlement.canonical_header()
            && stored.staged_fragment_count() == 0
            && stored.staged_fragment_chain() == canonical_empty_draft_piece_fragment_chain_v1()
            && stored.working_roots()
                == DraftPieceBuildRootsV1::from_root(settlement.predecessor_root())
            && stored.base_frontier() == origin
            && stored.successor_frontier() == origin
            && stored.next_record_ordinal() == 1
            && stored.frontier()
                == DraftPieceBuildFrontierV1::Receiving {
                    next_ordinal: 1,
                    chain: canonical_empty_draft_piece_fragment_chain_v1(),
                }
            && stored.progress_receipt() == settlement.terminal_receipt()
            && stored.successor().is_none()
            && stored.build_digest().is_none()
            && stored.lifecycle() == lifecycle;
    };
    let Some(stored) = stored else { return false };
    let expected = authenticated_build_record(
        DraftPieceBuildRecordV1::new(
            source.draft_id(),
            source.session_id(),
            source.predecessor_candidate_generation(),
            source.predecessor_root(),
            source.predecessor_history(),
            source.operation_id(),
            source.predecessor_caret(),
            source.predecessor_selection(),
            source.caret(),
            source.selection(),
            source.fragment_count(),
            source.fragment_chain(),
            source.canonical_header().to_vec(),
            source.staged_fragment_count(),
            source.staged_fragment_chain(),
            source.proposal_digest(),
            source.working_roots(),
            source.base_frontier(),
            source.successor_frontier(),
            source.next_record_ordinal(),
            source.frontier(),
            stored.progress_receipt(),
            source.successor(),
            source.build_digest(),
            lifecycle,
        )
        .with_durable_continuation(source.durable_continuation())
        .with_marker_effect_continuation(source.marker_effect_continuation()),
    );
    stored == &expected
        && stored.progress_receipt() == settlement.terminal_receipt()
        && source
            .progress_receipt()
            .key()
            .transition_ordinal()
            .checked_add(1)
            == Some(stored.progress_receipt().key().transition_ordinal())
}

pub(crate) fn canonical_edit_header_bytes(header: DraftPieceEditHeaderV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(header.draft_id().as_bytes());
    bytes.extend_from_slice(header.session_id().as_bytes());
    bytes.extend_from_slice(&header.predecessor_candidate_generation().to_be_bytes());
    let root = super::codec::canonical_root_reference_bytes(header.predecessor_root());
    bytes.extend_from_slice(&(root.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&root);
    let history = super::canonical_history_reference_bytes(header.predecessor_history());
    bytes.extend_from_slice(&(history.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&history);
    bytes.extend_from_slice(header.operation_id().as_bytes());
    for position in [
        header.predecessor_caret(),
        header.predecessor_selection(),
        header.caret(),
        header.selection(),
    ] {
        let position = super::codec::canonical_position_bytes(position);
        bytes.extend_from_slice(&(position.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&position);
    }
    bytes.extend_from_slice(&header.fragment_count().to_be_bytes());
    bytes.extend_from_slice(header.fragment_chain().as_bytes());
    bytes
}

pub(crate) fn canonical_edit_command_bytes(
    header: DraftPieceEditHeaderV1,
    source_session_generation: u64,
) -> Vec<u8> {
    let mut bytes = canonical_edit_header_bytes(header);
    bytes.extend_from_slice(&source_session_generation.to_be_bytes());
    bytes
}

fn canonical_edit_command_is_exact(bytes: &[u8], header: DraftPieceEditHeaderV1) -> bool {
    let header = canonical_edit_header_bytes(header);
    bytes.len() == header.len() + 8
        && bytes.starts_with(&header)
        && bytes[header.len()..]
            .try_into()
            .map(u64::from_be_bytes)
            .is_ok_and(|generation| generation != 0)
}

pub(crate) fn canonical_edit_command_source_generation(bytes: &[u8]) -> Option<u64> {
    let suffix = bytes.get(bytes.len().checked_sub(8)?..)?;
    let generation = u64::from_be_bytes(suffix.try_into().ok()?);
    (generation != 0).then_some(generation)
}

pub(crate) fn canonical_proposal_digest(bytes: &[u8]) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update(PROPOSAL);
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

pub(crate) fn build_record_is_exact(build: &DraftPieceBuildRecordV1) -> bool {
    let header = DraftPieceEditHeaderV1::new(
        build.draft_id(),
        build.session_id(),
        build.predecessor_candidate_generation(),
        build.predecessor_root(),
        build.predecessor_history(),
        build.operation_id(),
        build.predecessor_caret(),
        build.predecessor_selection(),
        build.caret(),
        build.selection(),
        build.fragment_count(),
        build.fragment_chain(),
    );
    let terminal_receiving = build.staged_fragment_count() <= build.fragment_count()
        && matches!(
            build.lifecycle(),
            DraftPieceBuildLifecycleV1::Rejected
                | DraftPieceBuildLifecycleV1::Cancelled
                | DraftPieceBuildLifecycleV1::Error
        )
        && matches!(
            build.frontier(), DraftPieceBuildFrontierV1::Receiving { next_ordinal, chain }
                if next_ordinal == build.staged_fragment_count().saturating_add(1)
                    && chain == build.staged_fragment_chain()
        );
    if (build.fragment_count() == 0 && !terminal_receiving)
        || build.predecessor_root().key().draft_id() != build.draft_id()
        || build.predecessor_history().root() != build.predecessor_root()
        || build.predecessor_history().candidate_generation()
            != build.predecessor_candidate_generation()
        || build.predecessor_history().key().draft_id() != build.draft_id()
        || !canonical_edit_command_is_exact(build.canonical_header(), header)
        || build.proposal_digest() != canonical_proposal_digest(build.canonical_header())
        || build.staged_fragment_count() > build.fragment_count()
        || build.base_frontier().rank() > build.predecessor_root().summary().piece_count()
        || build.successor_frontier().rank()
            > build.working_roots().sequence_summary().piece_count()
        || build.next_record_ordinal() == 0
        || build.progress_receipt().key().draft_id() != build.draft_id()
        || build.progress_receipt().key().session_id() != build.session_id()
        || build.progress_receipt().key().operation_id() != build.operation_id()
        || build.progress_receipt().key().transition_ordinal() == 0
        || !build
            .marker_effect_continuation()
            .is_locally_exact(DraftPieceSettlementKeyV1::new(
                build.draft_id(),
                build.session_id(),
                build.operation_id(),
            ))
        || build.durable_continuation().is_some_and(|continuation| {
            let identity = continuation.finished().identity();
            !continuation.is_locally_exact()
                || identity.draft_id() != build.draft_id()
                || identity.session_id() != build.session_id()
                || identity.operation_id().as_piece_operation() != build.operation_id()
        })
    {
        return false;
    }
    if build.staged_fragment_count() == 0
        && build.staged_fragment_chain() != canonical_empty_draft_piece_fragment_chain_v1()
    {
        return false;
    }
    let complete_shape = build.frontier() == DraftPieceBuildFrontierV1::Complete;
    let marker = build.marker_effect_continuation();
    let scan = marker.scan();
    if scan.completed_effect_count() > build.fragment_count()
        || marker.active().is_some_and(|active| {
            active.source_roots() != build.working_roots()
                || active.fragment_key().ordinal() != scan.next_fragment_ordinal()
        })
    {
        return false;
    }
    match (build.successor(), build.build_digest(), complete_shape) {
        (Some(successor), Some(digest), true) => {
            if successor.key()
                != DraftPieceRootKeyV1::editor_candidate(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                )
                || build.working_roots() != DraftPieceBuildRootsV1::from_root(successor)
                || digest != draft_piece_build_digest_v1(build.proposal_digest(), successor)
            {
                return false;
            }
        }
        (None, None, false) => {}
        _ => return false,
    }
    match build.frontier() {
        DraftPieceBuildFrontierV1::Receiving {
            next_ordinal,
            chain,
        } => {
            if marker != DraftPieceMarkerEffectContinuationV1::canonical_empty()
                || next_ordinal != build.staged_fragment_count().saturating_add(1)
                || chain != build.staged_fragment_chain()
                || (!terminal_receiving && build.staged_fragment_count() >= build.fragment_count())
            {
                return false;
            }
        }
        DraftPieceBuildFrontierV1::Planning { fragment_ordinal }
        | DraftPieceBuildFrontierV1::Removing {
            fragment_ordinal, ..
        }
        | DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal, ..
        }
        | DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal, ..
        } => {
            if fragment_ordinal != scan.next_fragment_ordinal()
                || matches!(build.frontier(), DraftPieceBuildFrontierV1::Planning { .. })
                    && marker.active().is_some()
                || build.staged_fragment_count() != build.fragment_count()
                || build.staged_fragment_chain() != build.fragment_chain()
                || fragment_ordinal == 0
                || fragment_ordinal > build.fragment_count()
            {
                return false;
            }
        }
        DraftPieceBuildFrontierV1::CrossValidating | DraftPieceBuildFrontierV1::Complete => {
            let exact_endpoint = match (scan.scanned_endpoint(), build.fragment_count()) {
                (Some(endpoint), count) => {
                    endpoint.key().ordinal() == count
                        && endpoint.chain() == build.fragment_chain()
                        && count.checked_add(1) == Some(scan.next_fragment_ordinal())
                }
                (None, 0) => scan.next_fragment_ordinal() == 1,
                _ => false,
            };
            if marker.active().is_some()
                || !exact_endpoint
                || build.staged_fragment_count() != build.fragment_count()
                || build.staged_fragment_chain() != build.fragment_chain()
            {
                return false;
            }
        }
    }
    match build.lifecycle() {
        DraftPieceBuildLifecycleV1::Open => !complete_shape,
        DraftPieceBuildLifecycleV1::Complete => complete_shape,
        DraftPieceBuildLifecycleV1::Committed | DraftPieceBuildLifecycleV1::Conflict => {
            complete_shape
        }
        DraftPieceBuildLifecycleV1::Rejected
        | DraftPieceBuildLifecycleV1::Cancelled
        | DraftPieceBuildLifecycleV1::Error => true,
    }
}

pub(crate) fn authenticated_build_record(
    build: DraftPieceBuildRecordV1,
) -> DraftPieceBuildRecordV1 {
    build
}

pub(crate) fn authenticated_build_transition(
    build: DraftPieceBuildRecordV1,
    previous: Option<DraftPieceBuildProgressReceiptReferenceV1>,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<(DraftPieceBuildRecordV1, DraftPieceBuildProgressReceiptV1), ()> {
    let ordinal = match previous {
        Some(previous) => previous
            .key()
            .transition_ordinal()
            .checked_add(1)
            .ok_or(())?,
        None => 1,
    };
    let build = authenticated_build_record(build);
    let key = DraftPieceBuildProgressReceiptKeyV1::new(
        build.draft_id(),
        build.session_id(),
        build.operation_id(),
        ordinal,
    );
    let reference = DraftPieceBuildProgressReceiptReferenceV1::new(
        key,
        draft_piece_build_progress_receipt_digest_v1(&build, previous, fragment_endpoint, key),
    );
    let build = build.with_progress_receipt(reference);
    let receipt = DraftPieceBuildProgressReceiptV1::new(
        reference,
        previous,
        fragment_endpoint,
        build.working_roots(),
        build.base_frontier(),
        build.successor_frontier(),
        build.next_record_ordinal(),
        build.frontier(),
        build.successor(),
        build.build_digest(),
        build.lifecycle(),
    )
    .with_durable_continuation(build.durable_continuation())
    .with_marker_effect_continuation(build.marker_effect_continuation());
    Ok((build, receipt))
}

pub(crate) fn draft_piece_build_progress_receipt_digest_v1(
    build: &DraftPieceBuildRecordV1,
    previous: Option<DraftPieceBuildProgressReceiptReferenceV1>,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
    key: DraftPieceBuildProgressReceiptKeyV1,
) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update(b"syndic/draft-piece-build-progress-receipt/v3");
    digest.update(key.draft_id().as_bytes());
    digest.update(key.session_id().as_bytes());
    digest.update(key.operation_id().as_bytes());
    digest.update(key.transition_ordinal().to_be_bytes());
    match previous {
        Some(previous) => {
            digest.update([1]);
            let key = previous.key();
            digest.update(key.draft_id().as_bytes());
            digest.update(key.session_id().as_bytes());
            digest.update(key.operation_id().as_bytes());
            digest.update(key.transition_ordinal().to_be_bytes());
            digest.update(previous.digest().as_bytes());
        }
        None => digest.update([0]),
    }
    match fragment_endpoint {
        Some(endpoint) => {
            digest.update([1]);
            let key = endpoint.key();
            digest.update(key.draft_id().as_bytes());
            digest.update(key.session_id().as_bytes());
            digest.update(key.operation_id().as_bytes());
            digest.update(key.ordinal().to_be_bytes());
            digest.update(endpoint.digest().as_bytes());
            digest.update(endpoint.chain().as_bytes());
        }
        None => digest.update([0]),
    }
    hash_build_roots(&mut digest, build.working_roots());
    hash_build_boundary(&mut digest, build.base_frontier());
    hash_build_boundary(&mut digest, build.successor_frontier());
    digest.update(build.next_record_ordinal().to_be_bytes());
    hash_build_frontier(&mut digest, build.frontier());
    hash_durable_continuation(&mut digest, build.durable_continuation());
    hash_marker_effect_continuation(&mut digest, build.marker_effect_continuation());
    match build.successor() {
        Some(root) => {
            digest.update([1]);
            digest.update(root.combined_digest().as_bytes());
        }
        None => digest.update([0]),
    }
    match build.build_digest() {
        Some(value) => {
            digest.update([1]);
            digest.update(value.as_bytes());
        }
        None => digest.update([0]),
    }
    digest.update([build.lifecycle() as u8]);
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

pub(crate) fn draft_piece_fragment_digest_v1(
    fragment: &DraftPieceBuildFragmentV1,
) -> DraftPieceDigestV1 {
    draft_piece_fragment_chain_link_v1(
        canonical_empty_draft_piece_fragment_chain_v1(),
        fragment.key().ordinal(),
        fragment.replacement(),
    )
}

pub(crate) fn canonical_fragment_endpoint(
    fragment: &DraftPieceBuildFragmentV1,
) -> DraftPieceCanonicalFragmentEndpointV1 {
    DraftPieceCanonicalFragmentEndpointV1::new(
        fragment.key(),
        draft_piece_fragment_digest_v1(fragment),
        fragment.chain_digest(),
    )
}

pub(crate) fn progress_receipt_is_exact(receipt: &DraftPieceBuildProgressReceiptV1) -> bool {
    let key = receipt.key();
    if key.transition_ordinal() == 0
        || match receipt.previous() {
            Some(previous) => {
                let previous_key = previous.key();
                previous_key.draft_id() != key.draft_id()
                    || previous_key.session_id() != key.session_id()
                    || previous_key.operation_id() != key.operation_id()
                    || previous_key.transition_ordinal().checked_add(1)
                        != Some(key.transition_ordinal())
            }
            None => key.transition_ordinal() != 1,
        }
        || receipt.fragment_endpoint().is_some_and(|endpoint| {
            let fragment = endpoint.key();
            !fragment.is_locally_valid()
                || fragment.draft_id() != key.draft_id()
                || fragment.session_id() != key.session_id()
                || fragment.operation_id() != key.operation_id()
        })
        || receipt.durable_continuation().is_some_and(|continuation| {
            let identity = continuation.finished().identity();
            !continuation.is_locally_exact()
                || identity.draft_id() != key.draft_id()
                || identity.session_id() != key.session_id()
                || identity.operation_id().as_piece_operation() != key.operation_id()
        })
        || !receipt
            .marker_effect_continuation()
            .is_locally_exact(DraftPieceSettlementKeyV1::new(
                key.draft_id(),
                key.session_id(),
                key.operation_id(),
            ))
        || receipt.successor().is_some() != receipt.build_digest().is_some()
    {
        return false;
    }
    receipt.reference().digest() == progress_receipt_digest_from_value(receipt)
}

fn progress_receipt_digest_from_value(
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> DraftPieceDigestV1 {
    let key = receipt.key();
    let mut digest = Sha256::new();
    digest.update(b"syndic/draft-piece-build-progress-receipt/v3");
    digest.update(key.draft_id().as_bytes());
    digest.update(key.session_id().as_bytes());
    digest.update(key.operation_id().as_bytes());
    digest.update(key.transition_ordinal().to_be_bytes());
    match receipt.previous() {
        Some(previous) => {
            digest.update([1]);
            let key = previous.key();
            digest.update(key.draft_id().as_bytes());
            digest.update(key.session_id().as_bytes());
            digest.update(key.operation_id().as_bytes());
            digest.update(key.transition_ordinal().to_be_bytes());
            digest.update(previous.digest().as_bytes());
        }
        None => digest.update([0]),
    }
    match receipt.fragment_endpoint() {
        Some(endpoint) => {
            digest.update([1]);
            let key = endpoint.key();
            digest.update(key.draft_id().as_bytes());
            digest.update(key.session_id().as_bytes());
            digest.update(key.operation_id().as_bytes());
            digest.update(key.ordinal().to_be_bytes());
            digest.update(endpoint.digest().as_bytes());
            digest.update(endpoint.chain().as_bytes());
        }
        None => digest.update([0]),
    }
    hash_build_roots(&mut digest, receipt.working_roots());
    hash_build_boundary(&mut digest, receipt.base_frontier());
    hash_build_boundary(&mut digest, receipt.successor_frontier());
    digest.update(receipt.next_record_ordinal().to_be_bytes());
    hash_build_frontier(&mut digest, receipt.frontier());
    hash_durable_continuation(&mut digest, receipt.durable_continuation());
    hash_marker_effect_continuation(&mut digest, receipt.marker_effect_continuation());
    match receipt.successor() {
        Some(root) => {
            digest.update([1]);
            digest.update(root.combined_digest().as_bytes());
        }
        None => digest.update([0]),
    }
    match receipt.build_digest() {
        Some(value) => {
            digest.update([1]);
            digest.update(value.as_bytes());
        }
        None => digest.update([0]),
    }
    digest.update([receipt.lifecycle() as u8]);
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

#[cfg(feature = "test-faults")]
pub(crate) fn recompute_progress_receipt_digest(
    receipt: DraftPieceBuildProgressReceiptV1,
) -> DraftPieceBuildProgressReceiptV1 {
    let reference = DraftPieceBuildProgressReceiptReferenceV1::new(
        receipt.key(),
        progress_receipt_digest_from_value(&receipt),
    );
    DraftPieceBuildProgressReceiptV1::new(
        reference,
        receipt.previous(),
        receipt.fragment_endpoint(),
        receipt.working_roots(),
        receipt.base_frontier(),
        receipt.successor_frontier(),
        receipt.next_record_ordinal(),
        receipt.frontier(),
        receipt.successor(),
        receipt.build_digest(),
        receipt.lifecycle(),
    )
    .with_durable_continuation(receipt.durable_continuation())
    .with_marker_effect_continuation(receipt.marker_effect_continuation())
}

pub(crate) fn progress_receipt_matches_build(
    receipt: &DraftPieceBuildProgressReceiptV1,
    build: &DraftPieceBuildRecordV1,
) -> bool {
    receipt.key().draft_id() == build.draft_id()
        && receipt.key().session_id() == build.session_id()
        && receipt.key().operation_id() == build.operation_id()
        && receipt.reference() == build.progress_receipt()
        && receipt.working_roots() == build.working_roots()
        && receipt.base_frontier() == build.base_frontier()
        && receipt.successor_frontier() == build.successor_frontier()
        && receipt.next_record_ordinal() == build.next_record_ordinal()
        && receipt.frontier() == build.frontier()
        && receipt.durable_continuation() == build.durable_continuation()
        && receipt.marker_effect_continuation() == build.marker_effect_continuation()
        && receipt.successor() == build.successor()
        && receipt.build_digest() == build.build_digest()
        && receipt.lifecycle() == build.lifecycle()
        && receipt.fragment_endpoint().is_none() == (build.staged_fragment_count() == 0)
        && receipt.fragment_endpoint().is_none_or(|endpoint| {
            endpoint.key()
                == DraftPieceBuildFragmentKeyV1::new(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                    build.staged_fragment_count(),
                )
                && endpoint.chain() == build.staged_fragment_chain()
        })
        && receipt.reference().digest()
            == draft_piece_build_progress_receipt_digest_v1(
                build,
                receipt.previous(),
                receipt.fragment_endpoint(),
                receipt.reference().key(),
            )
        && match receipt.previous() {
            Some(previous) => {
                previous.key().draft_id() == receipt.key().draft_id()
                    && previous.key().session_id() == receipt.key().session_id()
                    && previous.key().operation_id() == receipt.key().operation_id()
                    && previous.key().transition_ordinal().checked_add(1)
                        == Some(receipt.key().transition_ordinal())
            }
            None => receipt.key().transition_ordinal() == 1,
        }
}

pub(crate) fn marker_effect_progress_transition_is_exact(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    scanned_fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    let previous_continuation = previous.marker_effect_continuation();
    let current_continuation = current.marker_effect_continuation();
    let previous_scan = previous_continuation.scan();
    let current_scan = current_continuation.scan();
    if current_scan == previous_scan {
        return current_continuation.source_logical_frontier()
            == previous_continuation.source_logical_frontier()
            && current_continuation.successor_logical_frontier()
                == previous_continuation.successor_logical_frontier();
    }
    let Some(fragment) = scanned_fragment else {
        return false;
    };
    let endpoint = canonical_fragment_endpoint(fragment);
    if current_scan.scanned_endpoint() != Some(endpoint)
        || fragment.key().ordinal() != previous_scan.next_fragment_ordinal()
        || current_scan.next_fragment_ordinal()
            != previous_scan
                .next_fragment_ordinal()
                .checked_add(1)
                .unwrap_or(0)
        || current_continuation.active().is_some()
    {
        return false;
    }
    let inserted_bytes =
        fragment
            .replacement()
            .inserted()
            .iter()
            .try_fold(0_u64, |bytes, piece| match piece {
                DraftPieceV1::Text(text) => u64::try_from(text.len())
                    .ok()
                    .and_then(|length| bytes.checked_add(length)),
                DraftPieceV1::Marker(_) => Some(bytes),
            });
    let expected_source = if fragment.replacement().is_continuation() {
        Some(previous_continuation.source_logical_frontier())
    } else {
        Some(fragment.replacement().end().utf8_offset())
    };
    let expected_successor_start = if fragment.replacement().is_continuation() {
        Some(previous_continuation.successor_logical_frontier())
    } else {
        fragment
            .replacement()
            .start()
            .utf8_offset()
            .checked_sub(previous_continuation.source_logical_frontier())
            .and_then(|offset| {
                previous_continuation
                    .successor_logical_frontier()
                    .checked_add(offset)
            })
    };
    if expected_source != Some(current_continuation.source_logical_frontier())
        || expected_successor_start
            .zip(inserted_bytes)
            .and_then(|(start, bytes)| start.checked_add(bytes))
            != Some(current_continuation.successor_logical_frontier())
    {
        return false;
    }
    match previous_continuation.active() {
        Some(active) => {
            let Some(count) = previous_scan.completed_effect_count().checked_add(1) else {
                return false;
            };
            fragment.replacement().marker_effect() == Some(active.effect())
                && active.fragment_key() == fragment.key()
                && active.fragment_digest() == endpoint.digest()
                && active.source_roots() == previous.working_roots()
                && current_scan.completed_effect_count() == count
                && current_scan.effect_chain()
                    == draft_piece_marker_effect_chain_link_v1(
                        previous_scan.effect_chain(),
                        fragment.key(),
                        endpoint.digest(),
                        count,
                        current.working_roots(),
                    )
        }
        None => {
            fragment.replacement().marker_effect().is_none()
                && current_scan.completed_effect_count() == previous_scan.completed_effect_count()
                && current_scan.effect_chain() == previous_scan.effect_chain()
        }
    }
}

fn hash_optional_record_id(digest: &mut Sha256, value: Option<DraftPieceRecordIdV1>) {
    match value {
        Some(value) => {
            digest.update([1]);
            digest.update(value.as_bytes());
        }
        None => digest.update([0]),
    }
}

fn hash_build_boundary(digest: &mut Sha256, value: DraftPieceBuildBoundaryV1) {
    digest.update(value.rank().to_be_bytes());
    digest.update(value.inner().to_be_bytes());
}

fn hash_build_frontier(digest: &mut Sha256, value: DraftPieceBuildFrontierV1) {
    match value {
        DraftPieceBuildFrontierV1::Receiving {
            next_ordinal,
            chain,
        } => {
            digest.update([0]);
            digest.update(next_ordinal.to_be_bytes());
            digest.update(chain.as_bytes());
        }
        DraftPieceBuildFrontierV1::Planning { fragment_ordinal } => {
            digest.update([1]);
            digest.update(fragment_ordinal.to_be_bytes());
        }
        DraftPieceBuildFrontierV1::Removing {
            fragment_ordinal,
            next_rank,
            end_rank,
            removed_markers,
            base_end,
            successor_start,
            successor_end,
        } => {
            digest.update([2]);
            digest.update(fragment_ordinal.to_be_bytes());
            digest.update(next_rank.to_be_bytes());
            digest.update(end_rank.to_be_bytes());
            digest.update(removed_markers.to_be_bytes());
            for boundary in [base_end, successor_start, successor_end] {
                hash_build_boundary(digest, boundary);
            }
        }
        DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal,
            base_end,
            successor_start,
            successor_end,
        } => {
            digest.update([3]);
            digest.update(fragment_ordinal.to_be_bytes());
            for boundary in [base_end, successor_start, successor_end] {
                hash_build_boundary(digest, boundary);
            }
        }
        DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal,
            next_piece,
            next_byte,
            base_end,
            successor_end,
        } => {
            digest.update([4]);
            digest.update(fragment_ordinal.to_be_bytes());
            digest.update(next_piece.to_be_bytes());
            digest.update(next_byte.to_be_bytes());
            hash_build_boundary(digest, base_end);
            hash_build_boundary(digest, successor_end);
        }
        DraftPieceBuildFrontierV1::CrossValidating => digest.update([5]),
        DraftPieceBuildFrontierV1::Complete => digest.update([6]),
    }
}

fn hash_lane(digest: &mut Sha256, lane: DraftMutationStagingLaneFrontierV1) {
    digest.update(lane.next_cursor().to_be_bytes());
    digest.update(lane.next_ordinal().to_be_bytes());
    digest.update(lane.item_total().to_be_bytes());
    digest.update(lane.canonical_byte_total().to_be_bytes());
    digest.update(lane.cumulative_identity().as_bytes());
}

fn hash_build_roots(digest: &mut Sha256, roots: DraftPieceBuildRootsV1) {
    hash_optional_record_id(digest, roots.sequence_root());
    let summary = roots.sequence_summary();
    digest.update(summary.logical_utf8_bytes().to_be_bytes());
    digest.update(summary.newline_count().to_be_bytes());
    digest.update(summary.logical_line_count().to_be_bytes());
    digest.update(summary.piece_count().to_be_bytes());
    digest.update(summary.marker_count().to_be_bytes());
    digest.update(summary.marker_digest().as_bytes());
    digest.update([summary.height()]);
    digest.update(summary.root_digest().as_bytes());
    hash_optional_record_id(digest, roots.marker_index_root());
    let index = roots.marker_index_summary();
    digest.update(index.record_count().to_be_bytes());
    digest.update([index.height()]);
    digest.update(index.root_digest().as_bytes());
    hash_optional_record_id(digest, roots.marker_order_root());
    digest.update([roots.marker_order_height()]);
    let commitment = roots.marker_commitment();
    digest.update(commitment.tree_root_digest());
    digest.update(commitment.marker_count().to_be_bytes());
    match commitment.maximum_image_label() {
        Some(label) => {
            digest.update([1]);
            digest.update(label.get().to_be_bytes());
        }
        None => digest.update([0]),
    }
}

fn hash_position(digest: &mut Sha256, position: DraftCompositePositionV1) {
    digest.update(position.utf8_offset().to_be_bytes());
    match position.gap() {
        DraftCompositeGapWitnessV1::Unambiguous => digest.update([0]),
        DraftCompositeGapWitnessV1::BeforeAll => digest.update([1]),
        DraftCompositeGapWitnessV1::Between {
            left_order_key,
            left_marker_id,
            right_order_key,
            right_marker_id,
        } => {
            digest.update([2]);
            digest.update(left_order_key.to_be_bytes());
            digest.update(left_marker_id.as_bytes());
            digest.update(right_order_key.to_be_bytes());
            digest.update(right_marker_id.as_bytes());
        }
        DraftCompositeGapWitnessV1::AfterAll => digest.update([3]),
    }
}

fn hash_charges(digest: &mut Sha256, charges: DraftPieceMarkerEffectChargesV1) {
    digest.update(charges.logical_utf8_bytes().to_be_bytes());
    digest.update(charges.marker_count().to_be_bytes());
    digest.update(charges.encoded_bytes().to_be_bytes());
}

fn hash_occurrence(digest: &mut Sha256, occurrence: DraftMarkerIdentityOccurrenceV1) {
    digest.update(occurrence.marker_id().as_bytes());
    digest.update(occurrence.label().get().to_be_bytes());
    hash_asset_id(digest, occurrence.asset_id());
    digest.update(occurrence.order_key().to_be_bytes());
    digest.update(occurrence.sequence_leaf_id().as_bytes());
    digest.update(occurrence.sequence_leaf_digest().as_bytes());
}

fn hash_removal(digest: &mut Sha256, removal: DraftPieceMarkerRemovalProofV1) {
    hash_marker_position(digest, removal.position());
    hash_occurrence(digest, removal.occurrence());
}

fn hash_insertion(digest: &mut Sha256, insertion: DraftPieceMarkerInsertionV1) {
    digest.update(insertion.anchor().to_be_bytes());
    let marker = insertion.marker();
    digest.update(marker.marker_id().as_bytes());
    digest.update(marker.order_key().to_be_bytes());
    digest.update(marker.label().get().to_be_bytes());
    hash_asset_id(digest, marker.asset_id());
    hash_charges(digest, insertion.charges());
}

fn hash_asset_id(digest: &mut Sha256, asset_id: beryl_model::AssetId) {
    digest.update([asset_id.version() as u8]);
    digest.update(asset_id.digest());
    digest.update(asset_id.length().get().to_be_bytes());
}

fn hash_marker_effect(digest: &mut Sha256, effect: DraftPieceMarkerEffectV1) {
    match effect {
        DraftPieceMarkerEffectV1::Insert(insertion) => {
            digest.update([0]);
            hash_insertion(digest, insertion);
        }
        DraftPieceMarkerEffectV1::Remove { removal, charges } => {
            digest.update([1]);
            hash_removal(digest, removal);
            hash_charges(digest, charges);
        }
        DraftPieceMarkerEffectV1::Move { removal, insertion } => {
            digest.update([2]);
            hash_removal(digest, removal);
            hash_insertion(digest, insertion);
        }
        DraftPieceMarkerEffectV1::SameIdReplacement { removal, insertion } => {
            digest.update([3]);
            hash_removal(digest, removal);
            hash_insertion(digest, insertion);
        }
    }
}

pub(crate) fn draft_piece_marker_effect_chain_link_v1(
    prior: DraftPieceDigestV1,
    fragment_key: DraftPieceBuildFragmentKeyV1,
    fragment_digest: DraftPieceDigestV1,
    count: u64,
    roots: DraftPieceBuildRootsV1,
) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update(b"syndic/draft-marker-effect-chain/v1");
    digest.update(prior.as_bytes());
    digest.update(fragment_key.draft_id().as_bytes());
    digest.update(fragment_key.session_id().as_bytes());
    digest.update(fragment_key.operation_id().as_bytes());
    digest.update(fragment_key.ordinal().to_be_bytes());
    digest.update(fragment_digest.as_bytes());
    digest.update(count.to_be_bytes());
    hash_build_roots(&mut digest, roots);
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

fn hash_durable_continuation(
    digest: &mut Sha256,
    continuation: Option<DraftPieceDurableBuildContinuationV1>,
) {
    let Some(continuation) = continuation else {
        digest.update([0]);
        return;
    };
    digest.update([1]);
    let finished = continuation.finished();
    let identity = finished.identity();
    digest.update(identity.draft_id().as_bytes());
    digest.update(identity.session_id().as_bytes());
    digest.update(identity.operation_id().as_bytes());
    digest.update(finished.head_digest().as_bytes());
    let receipt = finished.receipt();
    digest.update(receipt.transition_ordinal().to_be_bytes());
    digest.update(receipt.digest().as_bytes());
    hash_lane(digest, finished.source());
    hash_lane(digest, finished.proposal());
    hash_lane(digest, continuation.source());
    hash_lane(digest, continuation.proposal());
    digest.update([match continuation.phase() {
        DraftPieceBuildStagingPhaseV1::Source => 0,
        DraftPieceBuildStagingPhaseV1::Proposal => 1,
        DraftPieceBuildStagingPhaseV1::Structure => 2,
    }]);
}

fn hash_marker_effect_continuation(
    digest: &mut Sha256,
    continuation: DraftPieceMarkerEffectContinuationV1,
) {
    digest.update(continuation.source_logical_frontier().to_be_bytes());
    digest.update(continuation.successor_logical_frontier().to_be_bytes());
    let scan = continuation.scan();
    digest.update(scan.next_fragment_ordinal().to_be_bytes());
    match scan.scanned_endpoint() {
        Some(endpoint) => {
            digest.update([1]);
            let key = endpoint.key();
            digest.update(key.draft_id().as_bytes());
            digest.update(key.session_id().as_bytes());
            digest.update(key.operation_id().as_bytes());
            digest.update(key.ordinal().to_be_bytes());
            digest.update(endpoint.digest().as_bytes());
            digest.update(endpoint.chain().as_bytes());
        }
        None => digest.update([0]),
    }
    digest.update(scan.completed_effect_count().to_be_bytes());
    digest.update(scan.effect_chain().as_bytes());
    match continuation.active() {
        Some(active) => {
            digest.update([1]);
            let key = active.fragment_key();
            digest.update(key.draft_id().as_bytes());
            digest.update(key.session_id().as_bytes());
            digest.update(key.operation_id().as_bytes());
            digest.update(key.ordinal().to_be_bytes());
            digest.update(active.fragment_digest().as_bytes());
            hash_marker_effect(digest, active.effect());
            hash_build_roots(digest, active.source_roots());
            hash_build_roots(digest, active.working_roots());
            digest.update(active.source_frontier().to_be_bytes());
            digest.update(active.successor_frontier().to_be_bytes());
            digest.update([match active.phase() {
                DraftPieceActiveMarkerPhaseV1::Removing => 0,
                DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap => 1,
                DraftPieceActiveMarkerPhaseV1::Inserting => 2,
                DraftPieceActiveMarkerPhaseV1::Publishing => 3,
            }]);
        }
        None => digest.update([0]),
    }
}

pub(crate) fn draft_piece_build_digest_v1(
    proposal_digest: DraftPieceDigestV1,
    successor: DraftPieceRootReferenceV1,
) -> DraftPieceDigestV1 {
    let domain = b"syndic/draft-piece-build/v3";
    let mut digest = Sha256::new();
    digest.update((domain.len() as u64).to_be_bytes());
    digest.update(domain);
    for part in [
        proposal_digest.as_bytes().as_slice(),
        successor.combined_digest().as_bytes().as_slice(),
    ] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

pub fn canonical_draft_piece_fragment_chain_v1(
    replacements: &[DraftPieceReplacementV1],
) -> DraftPieceDigestV1 {
    replacements.iter().enumerate().fold(
        canonical_empty_draft_piece_fragment_chain_v1(),
        |chain, (ordinal, replacement)| {
            draft_piece_fragment_chain_link_v1(chain, ordinal as u64 + 1, replacement)
        },
    )
}

pub(crate) fn validate_fragment(
    replacement: &DraftPieceReplacementV1,
) -> Result<(), DraftPieceRejectedReasonV1> {
    if replacement.inserted().len() > DRAFT_PIECE_PAGE_MAX_RECORDS {
        return Err(DraftPieceRejectedReasonV1::TooManyReplacements);
    }
    if replacement.is_continuation() && replacement.marker_effect().is_some() {
        return Err(DraftPieceRejectedReasonV1::TooManyReplacements);
    }
    let inserted_markers: Vec<_> = replacement
        .inserted()
        .iter()
        .filter_map(|piece| match piece {
            DraftPieceV1::Marker(marker) => Some(*marker),
            DraftPieceV1::Text(_) => None,
        })
        .collect();
    let expected_insertion = match replacement.marker_effect() {
        Some(DraftPieceMarkerEffectV1::Insert(insertion))
        | Some(DraftPieceMarkerEffectV1::Move { insertion, .. })
        | Some(DraftPieceMarkerEffectV1::SameIdReplacement { insertion, .. }) => {
            Some(insertion.marker())
        }
        Some(DraftPieceMarkerEffectV1::Remove { .. }) | None => None,
    };
    if match expected_insertion {
        Some(marker) => inserted_markers.as_slice() != [marker],
        None => !inserted_markers.is_empty(),
    } || (replacement.marker_effect().is_some()
        && (replacement.start() != replacement.end()
            || replacement
                .inserted()
                .iter()
                .any(|piece| matches!(piece, DraftPieceV1::Text(_)))))
    {
        return Err(DraftPieceRejectedReasonV1::DuplicateMarkerIdentity);
    }
    let mut bytes = 0_usize;
    for piece in replacement.inserted() {
        match piece {
            DraftPieceV1::Text(text) if text.is_empty() => {
                return Err(DraftPieceRejectedReasonV1::EmptyTextLeaf);
            }
            DraftPieceV1::Text(text) => {
                bytes = bytes
                    .checked_add(text.len())
                    .ok_or(DraftPieceRejectedReasonV1::InsertedPayloadTooLarge)?;
            }
            DraftPieceV1::Marker(_) => {}
        }
    }
    if bytes > DRAFT_PIECE_PAGE_MAX_BYTES {
        return Err(DraftPieceRejectedReasonV1::InsertedPayloadTooLarge);
    }
    Ok(())
}
