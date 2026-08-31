use std::num::NonZeroU64;

use beryl_home_store::RecordVersion;
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId};
use sha2::{Digest, Sha256};

use crate::DraftEditorCandidateSessionIdV1;
use crate::codec::parts::{Decoder, Encoder};
use crate::codec::{CodecError, ExactCodec, Family, SMALL_MAX, ScanKey, invalid};

use super::*;

const CAPACITY_DOMAIN: &[u8] = b"syndic/draft-marker-label-admission-capacity/v1";
const HEAD_DOMAIN: &[u8] = b"syndic/draft-marker-label-admission-head/v1";
const NODE_DOMAIN: &[u8] = b"syndic/draft-marker-label-admission-node/v1";
const RECEIPT_DOMAIN: &[u8] = b"syndic/draft-marker-label-admission-receipt/v1";
const SOURCE_EMPTY_DOMAIN: &[u8] = b"syndic/draft-marker-label-source-order-root/v1/empty";
const TARGET_EMPTY_DOMAIN: &[u8] = b"syndic/draft-marker-label-target-id-root/v1/empty";

pub(crate) struct DraftMarkerAdmissionCapacityFamily;
pub(crate) struct DraftMarkerAdmissionHeadsFamily;
pub(crate) struct DraftMarkerAdmissionNodesFamily;
pub(crate) struct DraftMarkerAdmissionReceiptsFamily;

pub(crate) type DraftMarkerAdmissionCapacityCodec = ExactCodec<DraftMarkerAdmissionCapacityFamily>;
pub(crate) type DraftMarkerAdmissionHeadsCodec = ExactCodec<DraftMarkerAdmissionHeadsFamily>;
pub(crate) type DraftMarkerAdmissionNodesCodec = ExactCodec<DraftMarkerAdmissionNodesFamily>;
pub(crate) type DraftMarkerAdmissionReceiptsCodec = ExactCodec<DraftMarkerAdmissionReceiptsFamily>;

fn digest(domain: &[u8], bytes: &[u8]) -> DraftMarkerAdmissionDigestV1 {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    DraftMarkerAdmissionDigestV1::from_bytes(hasher.finalize().into())
}

pub(crate) fn empty_root_digest(tree: DraftMarkerAdmissionTreeV1) -> DraftMarkerAdmissionDigestV1 {
    digest(
        match tree {
            DraftMarkerAdmissionTreeV1::SourceOrder => SOURCE_EMPTY_DOMAIN,
            DraftMarkerAdmissionTreeV1::TargetId => TARGET_EMPTY_DOMAIN,
        },
        &[],
    )
}

fn enc_owner(e: &mut Encoder, owner: DraftMarkerAdmissionOwnerV1) {
    e.fixed16(owner.draft_id().as_bytes());
    e.fixed16(owner.session_id().as_bytes());
    e.fixed16(owner.operation_id().as_bytes());
}

fn dec_owner(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionOwnerV1, CodecError> {
    Ok(DraftMarkerAdmissionOwnerV1::new(
        SyndicDraftId::from_bytes(d.fixed16()?),
        DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
        DraftMarkerAdmissionOperationIdV1::from_bytes(d.fixed16()?),
    ))
}

fn enc_limits(e: &mut Encoder, limits: DraftMarkerAdmissionLimitsV1) {
    e.u64(limits.max_heads());
    e.u64(limits.max_associations());
    e.u64(limits.max_encoded_bytes());
}

fn dec_limits(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionLimitsV1, CodecError> {
    Ok(DraftMarkerAdmissionLimitsV1::new(
        d.u64()?,
        d.u64()?,
        d.u64()?,
    ))
}

fn enc_charge(e: &mut Encoder, charge: DraftMarkerAdmissionRetainedChargeV1) {
    e.u64(charge.heads());
    e.u64(charge.associations());
    e.u64(charge.encoded_bytes());
}

fn dec_charge(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionRetainedChargeV1, CodecError> {
    Ok(DraftMarkerAdmissionRetainedChargeV1::new(
        d.u64()?,
        d.u64()?,
        d.u64()?,
    ))
}

fn enc_tree(e: &mut Encoder, tree: DraftMarkerAdmissionTreeV1) {
    e.u8(match tree {
        DraftMarkerAdmissionTreeV1::SourceOrder => 0,
        DraftMarkerAdmissionTreeV1::TargetId => 1,
    });
}

fn dec_tree(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionTreeV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftMarkerAdmissionTreeV1::SourceOrder),
        1 => Ok(DraftMarkerAdmissionTreeV1::TargetId),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-marker admission tree",
            tag,
        }),
    }
}

fn enc_kind(e: &mut Encoder, kind: DraftMarkerAdmissionNodeKindV1) {
    e.u8(match kind {
        DraftMarkerAdmissionNodeKindV1::Internal => 0,
        DraftMarkerAdmissionNodeKindV1::Leaf => 1,
    });
}

fn dec_kind(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionNodeKindV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftMarkerAdmissionNodeKindV1::Internal),
        1 => Ok(DraftMarkerAdmissionNodeKindV1::Leaf),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-marker admission node kind",
            tag,
        }),
    }
}

fn enc_node_key(e: &mut Encoder, key: DraftMarkerAdmissionNodeKeyV1) {
    enc_owner(e, key.owner());
    enc_kind(e, key.kind());
    e.fixed16(key.node_id().as_bytes());
}

fn dec_node_key(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionNodeKeyV1, CodecError> {
    Ok(DraftMarkerAdmissionNodeKeyV1::new(
        dec_owner(d)?,
        dec_kind(d)?,
        DraftMarkerAdmissionNodeIdV1::from_bytes(d.fixed16()?),
    ))
}

fn enc_source_key(e: &mut Encoder, key: DraftMarkerAdmissionSourceKeyV1) {
    e.u64(key.source_label().get());
    e.fixed16(key.target_marker_id().as_bytes());
}

fn dec_source_key(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionSourceKeyV1, CodecError> {
    Ok(DraftMarkerAdmissionSourceKeyV1::new(
        ImageLabelOrdinal::new(d.u64()?)
            .map_err(|error| invalid("draft-marker source label", error))?,
        SyndicDraftMarkerId::from_bytes(d.fixed16()?),
    ))
}

fn enc_asset(e: &mut Encoder, asset: AssetId) {
    e.u8(asset.version() as u8);
    e.fixed32(&asset.digest());
    e.u64(asset.length().get());
}

fn dec_asset(d: &mut Decoder<'_>) -> Result<AssetId, CodecError> {
    match d.u8()? {
        1 => Ok(AssetId::sha256_v1(
            d.fixed32()?,
            NonZeroU64::new(d.u64()?)
                .ok_or(CodecError::InvalidLength("draft-marker admission asset"))?,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-marker admission asset",
            tag,
        }),
    }
}

fn enc_assignment_prior_source(
    e: &mut Encoder,
    prior_source: Option<(ImageLabelOrdinal, AssetId)>,
) {
    match prior_source {
        None => e.u8(0),
        Some((label, asset)) => {
            e.u8(1);
            e.u64(label.get());
            enc_asset(e, asset);
        }
    }
}

fn dec_assignment_prior_source(
    d: &mut Decoder<'_>,
) -> Result<Option<(ImageLabelOrdinal, AssetId)>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some((
            ImageLabelOrdinal::new(d.u64()?)
                .map_err(|error| invalid("draft-marker prior source label", error))?,
            dec_asset(d)?,
        ))),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-marker prior source option",
            tag,
        }),
    }
}

fn enc_envelope(e: &mut Encoder, envelope: DraftMarkerAdmissionEnvelopeV1) {
    match envelope {
        DraftMarkerAdmissionEnvelopeV1::SourceOrder { first, last } => {
            e.u8(0);
            enc_source_key(e, first);
            enc_source_key(e, last);
        }
        DraftMarkerAdmissionEnvelopeV1::TargetId { first, last } => {
            e.u8(1);
            e.fixed16(first.as_bytes());
            e.fixed16(last.as_bytes());
        }
    }
}

fn dec_envelope(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionEnvelopeV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftMarkerAdmissionEnvelopeV1::SourceOrder {
            first: dec_source_key(d)?,
            last: dec_source_key(d)?,
        }),
        1 => Ok(DraftMarkerAdmissionEnvelopeV1::TargetId {
            first: SyndicDraftMarkerId::from_bytes(d.fixed16()?),
            last: SyndicDraftMarkerId::from_bytes(d.fixed16()?),
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-marker admission envelope",
            tag,
        }),
    }
}

fn enc_child(e: &mut Encoder, child: DraftMarkerAdmissionChildV1) {
    enc_node_key(e, child.key());
    e.fixed32(child.digest().as_bytes());
    e.u64(child.count());
    enc_envelope(e, child.envelope());
}

fn dec_child(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionChildV1, CodecError> {
    Ok(DraftMarkerAdmissionChildV1::new(
        dec_node_key(d)?,
        DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?),
        d.u64()?,
        dec_envelope(d)?,
    ))
}

pub(crate) fn enc_root(e: &mut Encoder, root: DraftMarkerAdmissionRootV1) {
    enc_tree(e, root.tree());
    match root.node() {
        None => e.u8(0),
        Some(node) => {
            e.u8(1);
            enc_node_key(e, node);
        }
    }
    e.u8(root.height());
    e.fixed32(root.digest().as_bytes());
    e.u64(root.count());
}

pub(crate) fn dec_root(d: &mut Decoder<'_>) -> Result<DraftMarkerAdmissionRootV1, CodecError> {
    let tree = dec_tree(d)?;
    let node = match d.u8()? {
        0 => None,
        1 => Some(dec_node_key(d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-marker admission root option",
                tag,
            });
        }
    };
    let value = DraftMarkerAdmissionRootV1::from_parts(
        tree,
        node,
        d.u8()?,
        DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?),
        d.u64()?,
    );
    value
        .validate_shape()
        .map_err(|error| invalid("draft-marker admission root", error))?;
    Ok(value)
}

fn enc_capacity_without_digest(
    revision: NonZeroU64,
    charge: DraftMarkerAdmissionRetainedChargeV1,
    limits: DraftMarkerAdmissionLimitsV1,
) -> Vec<u8> {
    let mut e = Encoder::new();
    e.u64(revision.get());
    enc_charge(&mut e, charge);
    enc_limits(&mut e, limits);
    e.finish()
}

pub(crate) fn capacity_digest(
    revision: NonZeroU64,
    charge: DraftMarkerAdmissionRetainedChargeV1,
    limits: DraftMarkerAdmissionLimitsV1,
) -> DraftMarkerAdmissionDigestV1 {
    digest(
        CAPACITY_DOMAIN,
        &enc_capacity_without_digest(revision, charge, limits),
    )
}

fn encode_capacity(value: &DraftMarkerAdmissionCapacityV1) -> Result<Vec<u8>, CodecError> {
    value
        .validate()
        .map_err(|error| invalid("draft-marker admission capacity", error))?;
    let mut bytes = enc_capacity_without_digest(value.revision(), value.charge(), value.limits());
    bytes.extend_from_slice(value.digest().as_bytes());
    Ok(bytes)
}

fn decode_capacity(bytes: &[u8]) -> Result<DraftMarkerAdmissionCapacityV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = DraftMarkerAdmissionCapacityV1::from_parts(
        NonZeroU64::new(d.u64()?).ok_or(CodecError::InvalidLength(
            "draft-marker admission capacity revision",
        ))?,
        dec_charge(&mut d)?,
        dec_limits(&mut d)?,
        DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?),
    );
    d.finish()?;
    value
        .validate()
        .map_err(|error| invalid("draft-marker admission capacity", error))?;
    Ok(value)
}

fn enc_head_without_digest(e: &mut Encoder, parts: &DraftMarkerAdmissionHeadPartsV1) {
    enc_owner(e, parts.owner);
    e.u64(parts.revision.get());
    e.u64(parts.home_generation.get());
    e.u8(match parts.lifecycle {
        DraftMarkerAdmissionLifecycleV1::Ingesting => 0,
        DraftMarkerAdmissionLifecycleV1::Assigning => 1,
        DraftMarkerAdmissionLifecycleV1::Ready => 2,
        DraftMarkerAdmissionLifecycleV1::Staging => 3,
        DraftMarkerAdmissionLifecycleV1::Building => 4,
        DraftMarkerAdmissionLifecycleV1::TerminalCleanup => 5,
        DraftMarkerAdmissionLifecycleV1::Settled => 6,
    });
    e.fixed32(parts.request_commitment.as_bytes());
    e.fixed32(parts.custody_commitment.as_bytes());
    e.u64(parts.next_page_ordinal.get());
    e.u64(parts.ingestion_association_cursor);
    e.u8(u8::from(parts.evidence_eof));
    enc_command_option(e, parts.selected_receipt);
    enc_root(e, parts.source_root);
    enc_root(e, parts.target_root);
    e.fixed32(parts.occurrence_commitment.as_bytes());
    e.u64(parts.unassigned_count);
    match parts.assignment_continuation {
        None => e.u8(0),
        Some(DraftMarkerAdmissionAssignmentContinuationV1::Reuse { prior_source }) => {
            e.u8(1);
            enc_assignment_prior_source(e, prior_source);
        }
        Some(DraftMarkerAdmissionAssignmentContinuationV1::Allocate {
            range,
            next_allocation,
            prior_source,
        }) => {
            e.u8(2);
            e.u64(range.first().get());
            e.u64(range.last().get());
            e.u64(next_allocation.get());
            enc_assignment_prior_source(e, prior_source);
        }
    }
    e.u64(parts.remaining_builder_count);
    enc_charge(e, parts.charge);
    enc_limits(e, parts.limits);
    match parts.cleanup_cursor {
        None => e.u8(0),
        Some(cursor) => {
            e.u8(1);
            enc_tree(e, cursor.tree());
            match cursor.after() {
                None => e.u8(0),
                Some(key) => {
                    e.u8(1);
                    enc_node_key(e, key);
                }
            }
        }
    }
}

pub(crate) fn head_digest(
    parts: &DraftMarkerAdmissionHeadPartsV1,
) -> Result<DraftMarkerAdmissionDigestV1, DraftMarkerAdmissionSchemaErrorV1> {
    let mut e = Encoder::new();
    enc_head_without_digest(&mut e, parts);
    if e.len() + 32 > SMALL_MAX {
        return Err(DraftMarkerAdmissionSchemaErrorV1::ValueTooLarge);
    }
    Ok(digest(HEAD_DOMAIN, &e.finish()))
}

fn head_parts(value: &DraftMarkerAdmissionHeadV1) -> DraftMarkerAdmissionHeadPartsV1 {
    DraftMarkerAdmissionHeadPartsV1 {
        owner: value.owner(),
        revision: value.revision(),
        home_generation: value.home_generation(),
        lifecycle: value.lifecycle(),
        request_commitment: value.request_commitment(),
        custody_commitment: value.custody_commitment(),
        next_page_ordinal: value.next_page_ordinal(),
        ingestion_association_cursor: value.ingestion_association_cursor(),
        evidence_eof: value.evidence_eof(),
        selected_receipt: value.selected_receipt(),
        source_root: value.source_root(),
        target_root: value.target_root(),
        occurrence_commitment: value.occurrence_commitment(),
        unassigned_count: value.unassigned_count(),
        assignment_continuation: value.assignment_continuation(),
        remaining_builder_count: value.remaining_builder_count(),
        charge: value.charge(),
        limits: value.limits(),
        cleanup_cursor: value.cleanup_cursor(),
        digest: value.digest(),
    }
}

fn encode_head(value: &DraftMarkerAdmissionHeadV1) -> Result<Vec<u8>, CodecError> {
    value
        .validate()
        .map_err(|error| invalid("draft-marker admission head", error))?;
    let parts = head_parts(value);
    let mut e = Encoder::new();
    enc_head_without_digest(&mut e, &parts);
    e.fixed32(value.digest().as_bytes());
    Ok(e.finish())
}

fn decode_head(bytes: &[u8]) -> Result<DraftMarkerAdmissionHeadV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let owner = dec_owner(&mut d)?;
    let revision = NonZeroU64::new(d.u64()?).ok_or(CodecError::InvalidLength(
        "draft-marker admission head revision",
    ))?;
    let home_generation = NonZeroU64::new(d.u64()?).ok_or(CodecError::InvalidLength(
        "draft-marker admission home generation",
    ))?;
    let lifecycle = match d.u8()? {
        0 => DraftMarkerAdmissionLifecycleV1::Ingesting,
        1 => DraftMarkerAdmissionLifecycleV1::Assigning,
        2 => DraftMarkerAdmissionLifecycleV1::Ready,
        3 => DraftMarkerAdmissionLifecycleV1::Staging,
        4 => DraftMarkerAdmissionLifecycleV1::Building,
        5 => DraftMarkerAdmissionLifecycleV1::TerminalCleanup,
        6 => DraftMarkerAdmissionLifecycleV1::Settled,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-marker admission lifecycle",
                tag,
            });
        }
    };
    let request_commitment = DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?);
    let custody_commitment = DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?);
    let next_page_ordinal = NonZeroU64::new(d.u64()?).ok_or(CodecError::InvalidLength(
        "draft-marker admission page ordinal",
    ))?;
    let ingestion_association_cursor = d.u64()?;
    let evidence_eof = dec_bool(&mut d, "draft-marker admission EOF")?;
    let selected_receipt = dec_command_option(&mut d)?;
    let source_root = dec_root(&mut d)?;
    let target_root = dec_root(&mut d)?;
    let occurrence_commitment = DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?);
    let unassigned_count = d.u64()?;
    let assignment_continuation = match d.u8()? {
        0 => None,
        1 => Some(DraftMarkerAdmissionAssignmentContinuationV1::reuse(
            dec_assignment_prior_source(&mut d)?,
        )),
        2 => {
            let reserved_first = ImageLabelOrdinal::new(d.u64()?)
                .map_err(|error| invalid("draft-marker reservation first", error))?;
            let reserved_last = ImageLabelOrdinal::new(d.u64()?)
                .map_err(|error| invalid("draft-marker reservation last", error))?;
            let next_allocation = ImageLabelOrdinal::new(d.u64()?)
                .map_err(|error| invalid("draft-marker next allocation", error))?;
            let prior_source = dec_assignment_prior_source(&mut d)?;
            let range = DraftMarkerLabelAllocationRangeV1::new(reserved_first, reserved_last)
                .map_err(|error| invalid("draft-marker reservation range", error))?;
            Some(
                DraftMarkerAdmissionAssignmentContinuationV1::allocate(
                    range,
                    next_allocation,
                    prior_source,
                )
                .map_err(|error| invalid("draft-marker assignment continuation", error))?,
            )
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-marker assignment cursor",
                tag,
            });
        }
    };
    let remaining_builder_count = d.u64()?;
    let charge = dec_charge(&mut d)?;
    let limits = dec_limits(&mut d)?;
    let cleanup_cursor = match d.u8()? {
        0 => None,
        1 => {
            let tree = dec_tree(&mut d)?;
            let after = match d.u8()? {
                0 => None,
                1 => Some(dec_node_key(&mut d)?),
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "draft-marker cleanup cursor",
                        tag,
                    });
                }
            };
            Some(DraftMarkerAdmissionCleanupCursorV1::new(tree, after))
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-marker cleanup option",
                tag,
            });
        }
    };
    let digest = DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?);
    d.finish()?;
    let value = DraftMarkerAdmissionHeadV1::from_parts(DraftMarkerAdmissionHeadPartsV1 {
        owner,
        revision,
        home_generation,
        lifecycle,
        request_commitment,
        custody_commitment,
        next_page_ordinal,
        ingestion_association_cursor,
        evidence_eof,
        selected_receipt,
        source_root,
        target_root,
        occurrence_commitment,
        unassigned_count,
        assignment_continuation,
        remaining_builder_count,
        charge,
        limits,
        cleanup_cursor,
        digest,
    });
    value
        .validate()
        .map_err(|error| invalid("draft-marker admission head", error))?;
    Ok(value)
}

fn enc_node_without_digest(
    e: &mut Encoder,
    key: DraftMarkerAdmissionNodeKeyV1,
    tree: DraftMarkerAdmissionTreeV1,
    payload: &DraftMarkerAdmissionNodePayloadV1,
) {
    enc_node_key(e, key);
    enc_tree(e, tree);
    match payload {
        DraftMarkerAdmissionNodePayloadV1::Internal { height, children } => {
            e.u8(0);
            e.u8(*height);
            e.u32(children.len() as u32);
            for child in children.iter() {
                enc_child(e, *child);
            }
        }
        DraftMarkerAdmissionNodePayloadV1::SourceLeaf {
            source_key,
            evidence,
            asset_id,
        } => {
            e.u8(1);
            enc_source_key(e, *source_key);
            e.bytes(evidence.as_bytes());
            enc_asset(e, *asset_id);
        }
        DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
            target_marker_id,
            page,
            evidence,
            source_label,
            asset_id,
            disposition,
        } => {
            e.u8(2);
            e.fixed16(target_marker_id.as_bytes());
            e.fixed16(page.command_id().as_bytes());
            e.u64(page.page_ordinal().get());
            e.bytes(evidence.as_bytes());
            e.u64(source_label.get());
            enc_asset(e, *asset_id);
            match disposition {
                DraftMarkerAdmissionTargetDispositionV1::Unassigned => e.u8(0),
                DraftMarkerAdmissionTargetDispositionV1::Assigned(label) => {
                    e.u8(1);
                    e.u64(label.get());
                }
            }
        }
    }
}

pub(crate) fn node_digest(
    key: DraftMarkerAdmissionNodeKeyV1,
    tree: DraftMarkerAdmissionTreeV1,
    payload: &DraftMarkerAdmissionNodePayloadV1,
) -> Result<DraftMarkerAdmissionDigestV1, DraftMarkerAdmissionSchemaErrorV1> {
    let mut e = Encoder::new();
    enc_node_without_digest(&mut e, key, tree, payload);
    if e.len() + 32 > SMALL_MAX {
        return Err(DraftMarkerAdmissionSchemaErrorV1::ValueTooLarge);
    }
    Ok(digest(NODE_DOMAIN, &e.finish()))
}

fn encode_node(value: &DraftMarkerAdmissionNodeV1) -> Result<Vec<u8>, CodecError> {
    value
        .validate()
        .map_err(|error| invalid("draft-marker admission node", error))?;
    let mut e = Encoder::new();
    enc_node_without_digest(&mut e, value.key(), value.tree(), value.payload());
    e.fixed32(value.digest().as_bytes());
    Ok(e.finish())
}

fn decode_node(bytes: &[u8]) -> Result<DraftMarkerAdmissionNodeV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_node_key(&mut d)?;
    let tree = dec_tree(&mut d)?;
    let payload = match d.u8()? {
        0 => {
            let height = d.u8()?;
            let count = usize::try_from(d.u32()?)
                .map_err(|_| CodecError::InvalidLength("draft-marker node children"))?;
            if count == 0 || count > DRAFT_MARKER_ADMISSION_TREE_FANOUT {
                return Err(CodecError::InvalidLength("draft-marker node children"));
            }
            let mut children = Vec::with_capacity(count);
            for _ in 0..count {
                children.push(dec_child(&mut d)?);
            }
            DraftMarkerAdmissionNodePayloadV1::Internal {
                height,
                children: children.into_boxed_slice(),
            }
        }
        1 => DraftMarkerAdmissionNodePayloadV1::SourceLeaf {
            source_key: dec_source_key(&mut d)?,
            evidence: DraftMarkerAdmissionEvidenceV1::new(
                d.bytes("draft-marker source evidence")?.to_vec(),
            )
            .map_err(|error| invalid("draft-marker source evidence", error))?,
            asset_id: dec_asset(&mut d)?,
        },
        2 => {
            let target_marker_id = SyndicDraftMarkerId::from_bytes(d.fixed16()?);
            let page = DraftMarkerAdmissionPageIdentityV1::new(
                DraftMarkerAdmissionCommandIdV1::from_bytes(d.fixed16()?),
                NonZeroU64::new(d.u64()?)
                    .ok_or(CodecError::InvalidLength("draft-marker page ordinal"))?,
            );
            let evidence = DraftMarkerAdmissionEvidenceV1::new(
                d.bytes("draft-marker target evidence")?.to_vec(),
            )
            .map_err(|error| invalid("draft-marker target evidence", error))?;
            let source_label = ImageLabelOrdinal::new(d.u64()?)
                .map_err(|error| invalid("draft-marker target source label", error))?;
            let asset_id = dec_asset(&mut d)?;
            let disposition = match d.u8()? {
                0 => DraftMarkerAdmissionTargetDispositionV1::Unassigned,
                1 => DraftMarkerAdmissionTargetDispositionV1::Assigned(
                    ImageLabelOrdinal::new(d.u64()?)
                        .map_err(|error| invalid("draft-marker assigned label", error))?,
                ),
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "draft-marker target disposition",
                        tag,
                    });
                }
            };
            DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                target_marker_id,
                page,
                evidence,
                source_label,
                asset_id,
                disposition,
            }
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-marker admission node payload",
                tag,
            });
        }
    };
    let stored_digest = DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?);
    d.finish()?;
    let value = DraftMarkerAdmissionNodeV1::from_parts(key, tree, payload, stored_digest);
    value
        .validate()
        .map_err(|error| invalid("draft-marker admission node", error))?;
    Ok(value)
}

fn enc_receipt_without_digest(e: &mut Encoder, parts: &DraftMarkerAdmissionReplayReceiptPartsV1) {
    enc_owner(e, parts.owner);
    e.fixed16(parts.command_id.as_bytes());
    e.u64(parts.page_ordinal.get());
    e.fixed32(parts.request_commitment.as_bytes());
    e.bytes(&parts.source_head_bytes);
    e.bytes(&parts.target_head_bytes);
    enc_root(e, parts.source_before);
    enc_root(e, parts.source_after);
    enc_root(e, parts.target_before);
    enc_root(e, parts.target_after);
    e.u32(parts.retained_predecessor_nodes.len() as u32);
    for node in parts.retained_predecessor_nodes.iter() {
        enc_child(e, *node);
    }
    e.u8(match parts.transition {
        DraftMarkerAdmissionReceiptTransitionV1::Ingestion => 0,
        DraftMarkerAdmissionReceiptTransitionV1::Assignment => 1,
        DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup => 2,
    });
}

pub(crate) fn receipt_digest(
    parts: &DraftMarkerAdmissionReplayReceiptPartsV1,
) -> Result<DraftMarkerAdmissionDigestV1, DraftMarkerAdmissionSchemaErrorV1> {
    let mut e = Encoder::new();
    enc_receipt_without_digest(&mut e, parts);
    if e.len() + 32 > SMALL_MAX {
        return Err(DraftMarkerAdmissionSchemaErrorV1::ValueTooLarge);
    }
    Ok(digest(RECEIPT_DOMAIN, &e.finish()))
}

fn receipt_parts(
    value: &DraftMarkerAdmissionReplayReceiptV1,
) -> DraftMarkerAdmissionReplayReceiptPartsV1 {
    DraftMarkerAdmissionReplayReceiptPartsV1 {
        owner: value.owner(),
        command_id: value.command_id(),
        page_ordinal: value.page_ordinal(),
        request_commitment: value.request_commitment(),
        source_head_bytes: value.source_head_bytes().into(),
        target_head_bytes: value.target_head_bytes().into(),
        source_before: value.source_before(),
        source_after: value.source_after(),
        target_before: value.target_before(),
        target_after: value.target_after(),
        retained_predecessor_nodes: value.retained_predecessor_nodes().into(),
        transition: value.transition(),
        digest: value.digest(),
    }
}

fn encode_receipt(value: &DraftMarkerAdmissionReplayReceiptV1) -> Result<Vec<u8>, CodecError> {
    value
        .validate()
        .map_err(|error| invalid("draft-marker admission receipt", error))?;
    let parts = receipt_parts(value);
    let mut e = Encoder::new();
    enc_receipt_without_digest(&mut e, &parts);
    e.fixed32(value.digest().as_bytes());
    Ok(e.finish())
}

fn decode_receipt(bytes: &[u8]) -> Result<DraftMarkerAdmissionReplayReceiptV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let owner = dec_owner(&mut d)?;
    let command_id = DraftMarkerAdmissionCommandIdV1::from_bytes(d.fixed16()?);
    let page_ordinal =
        NonZeroU64::new(d.u64()?).ok_or(CodecError::InvalidLength("draft-marker receipt page"))?;
    let request_commitment = DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?);
    let source_head_bytes: Box<[u8]> = d.bytes("draft-marker source head")?.into();
    let target_head_bytes: Box<[u8]> = d.bytes("draft-marker target head")?.into();
    let source_before = dec_root(&mut d)?;
    let source_after = dec_root(&mut d)?;
    let target_before = dec_root(&mut d)?;
    let target_after = dec_root(&mut d)?;
    let count = usize::try_from(d.u32()?)
        .map_err(|_| CodecError::InvalidLength("draft-marker retained nodes"))?;
    if count > usize::from(DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT) * 2 {
        return Err(CodecError::InvalidLength("draft-marker retained nodes"));
    }
    let mut retained_predecessor_nodes = Vec::with_capacity(count);
    for _ in 0..count {
        retained_predecessor_nodes.push(dec_child(&mut d)?);
    }
    let transition = match d.u8()? {
        0 => DraftMarkerAdmissionReceiptTransitionV1::Ingestion,
        1 => DraftMarkerAdmissionReceiptTransitionV1::Assignment,
        2 => DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-marker receipt transition",
                tag,
            });
        }
    };
    let stored_digest = DraftMarkerAdmissionDigestV1::from_bytes(d.fixed32()?);
    d.finish()?;
    let value =
        DraftMarkerAdmissionReplayReceiptV1::from_parts(DraftMarkerAdmissionReplayReceiptPartsV1 {
            owner,
            command_id,
            page_ordinal,
            request_commitment,
            source_head_bytes,
            target_head_bytes,
            source_before,
            source_after,
            target_before,
            target_after,
            retained_predecessor_nodes: retained_predecessor_nodes.into_boxed_slice(),
            transition,
            digest: stored_digest,
        });
    value
        .validate()
        .map_err(|error| invalid("draft-marker admission receipt", error))?;
    Ok(value)
}

fn enc_command_option(e: &mut Encoder, value: Option<DraftMarkerAdmissionCommandIdV1>) {
    match value {
        None => e.u8(0),
        Some(value) => {
            e.u8(1);
            e.fixed16(value.as_bytes());
        }
    }
}

fn dec_command_option(
    d: &mut Decoder<'_>,
) -> Result<Option<DraftMarkerAdmissionCommandIdV1>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(DraftMarkerAdmissionCommandIdV1::from_bytes(
            d.fixed16()?,
        ))),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-marker command option",
            tag,
        }),
    }
}

fn dec_bool(d: &mut Decoder<'_>, kind: &'static str) -> Result<bool, CodecError> {
    match d.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        tag => Err(CodecError::InvalidTag { kind, tag }),
    }
}

fn encode_capacity_key(_: &DraftMarkerAdmissionCapacityKeyV1) -> Result<Vec<u8>, CodecError> {
    Ok(vec![0])
}
fn decode_capacity_key(bytes: &[u8]) -> Result<DraftMarkerAdmissionCapacityKeyV1, CodecError> {
    if bytes == [0] {
        Ok(DraftMarkerAdmissionCapacityKeyV1)
    } else {
        Err(CodecError::InvalidLength(
            "draft-marker admission capacity key",
        ))
    }
}
fn encode_owner_key(key: &DraftMarkerAdmissionOwnerV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_owner(&mut e, *key);
    Ok(e.finish())
}
fn decode_owner_key(bytes: &[u8]) -> Result<DraftMarkerAdmissionOwnerV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let v = dec_owner(&mut d)?;
    d.finish()?;
    Ok(v)
}
fn encode_node_family_key(key: &DraftMarkerAdmissionNodeKeyV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_node_key(&mut e, *key);
    Ok(e.finish())
}
fn decode_node_family_key(bytes: &[u8]) -> Result<DraftMarkerAdmissionNodeKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let v = dec_node_key(&mut d)?;
    d.finish()?;
    Ok(v)
}
fn encode_receipt_key(key: &DraftMarkerAdmissionReceiptKeyV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_owner(&mut e, key.owner());
    e.fixed16(key.command_id().as_bytes());
    Ok(e.finish())
}
fn decode_receipt_key(bytes: &[u8]) -> Result<DraftMarkerAdmissionReceiptKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = DraftMarkerAdmissionReceiptKeyV1::new(
        dec_owner(&mut d)?,
        DraftMarkerAdmissionCommandIdV1::from_bytes(d.fixed16()?),
    );
    d.finish()?;
    Ok(value)
}

impl ScanKey for DraftMarkerAdmissionCapacityKeyV1 {
    fn first() -> Self {
        Self
    }
    fn last() -> Self {
        Self
    }
}
impl ScanKey for DraftMarkerAdmissionOwnerV1 {
    fn first() -> Self {
        Self::new(
            SyndicDraftId::from_bytes([0; 16]),
            DraftEditorCandidateSessionIdV1::from_bytes([0; 16]),
            DraftMarkerAdmissionOperationIdV1::from_bytes([0; 16]),
        )
    }
    fn last() -> Self {
        Self::new(
            SyndicDraftId::from_bytes([u8::MAX; 16]),
            DraftEditorCandidateSessionIdV1::from_bytes([u8::MAX; 16]),
            DraftMarkerAdmissionOperationIdV1::from_bytes([u8::MAX; 16]),
        )
    }
}
impl ScanKey for DraftMarkerAdmissionNodeKeyV1 {
    fn first() -> Self {
        Self::new(
            DraftMarkerAdmissionOwnerV1::first(),
            DraftMarkerAdmissionNodeKindV1::Internal,
            DraftMarkerAdmissionNodeIdV1::from_bytes([0; 16]),
        )
    }
    fn last() -> Self {
        Self::new(
            DraftMarkerAdmissionOwnerV1::last(),
            DraftMarkerAdmissionNodeKindV1::Leaf,
            DraftMarkerAdmissionNodeIdV1::from_bytes([u8::MAX; 16]),
        )
    }
}
impl ScanKey for DraftMarkerAdmissionReceiptKeyV1 {
    fn first() -> Self {
        Self::new(
            DraftMarkerAdmissionOwnerV1::first(),
            DraftMarkerAdmissionCommandIdV1::from_bytes([0; 16]),
        )
    }
    fn last() -> Self {
        Self::new(
            DraftMarkerAdmissionOwnerV1::last(),
            DraftMarkerAdmissionCommandIdV1::from_bytes([u8::MAX; 16]),
        )
    }
}

macro_rules! family {
    ($family:ty,$key:ty,$value:ty,$name:literal,$key_size:expr,$enc_key:ident,$dec_key:ident,$enc_value:ident,$dec_value:ident) => {
        impl Family for $family {
            type Key = $key;
            type Value = $value;
            const NAME: &'static str = $name;
            const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = $key_size;
            const MAX_VALUE_BYTES: usize = SMALL_MAX;
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

family!(
    DraftMarkerAdmissionCapacityFamily,
    DraftMarkerAdmissionCapacityKeyV1,
    DraftMarkerAdmissionCapacityV1,
    "draft-marker-label-admission-capacity",
    1,
    encode_capacity_key,
    decode_capacity_key,
    encode_capacity,
    decode_capacity
);
family!(
    DraftMarkerAdmissionHeadsFamily,
    DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionHeadV1,
    "draft-marker-label-admission-heads",
    48,
    encode_owner_key,
    decode_owner_key,
    encode_head,
    decode_head
);
family!(
    DraftMarkerAdmissionNodesFamily,
    DraftMarkerAdmissionNodeKeyV1,
    DraftMarkerAdmissionNodeV1,
    "draft-marker-label-admission-nodes",
    65,
    encode_node_family_key,
    decode_node_family_key,
    encode_node,
    decode_node
);
family!(
    DraftMarkerAdmissionReceiptsFamily,
    DraftMarkerAdmissionReceiptKeyV1,
    DraftMarkerAdmissionReplayReceiptV1,
    "draft-marker-label-admission-receipts",
    64,
    encode_receipt_key,
    decode_receipt_key,
    encode_receipt,
    decode_receipt
);

fn record_charge<F: Family>(
    key: &F::Key,
    value: &F::Value,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    let key = F::encode_key(key).map_err(|_| DraftMarkerAdmissionSchemaErrorV1::ValueTooLarge)?;
    let value =
        F::encode_value(value).map_err(|_| DraftMarkerAdmissionSchemaErrorV1::ValueTooLarge)?;
    u64::try_from(key.len())
        .ok()
        .and_then(|key| {
            u64::try_from(value.len())
                .ok()
                .and_then(|value| key.checked_add(value))
        })
        .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
}

fn record_key_charge<F: Family>(key: &F::Key) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    let key = F::encode_key(key).map_err(|_| DraftMarkerAdmissionSchemaErrorV1::ValueTooLarge)?;
    u64::try_from(key.len()).map_err(|_| DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
}

pub(crate) fn encoded_capacity_record_charge(
    key: &DraftMarkerAdmissionCapacityKeyV1,
    value: &DraftMarkerAdmissionCapacityV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    record_charge::<DraftMarkerAdmissionCapacityFamily>(key, value)
}

pub(crate) fn encoded_capacity_key_charge(
    key: &DraftMarkerAdmissionCapacityKeyV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    record_key_charge::<DraftMarkerAdmissionCapacityFamily>(key)
}

pub(crate) fn encoded_head_record_charge(
    key: &DraftMarkerAdmissionOwnerV1,
    value: &DraftMarkerAdmissionHeadV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    record_charge::<DraftMarkerAdmissionHeadsFamily>(key, value)
}

pub(crate) fn encoded_head_key_charge(
    key: &DraftMarkerAdmissionOwnerV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    record_key_charge::<DraftMarkerAdmissionHeadsFamily>(key)
}
pub(crate) fn encoded_node_record_charge(
    key: &DraftMarkerAdmissionNodeKeyV1,
    value: &DraftMarkerAdmissionNodeV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    record_charge::<DraftMarkerAdmissionNodesFamily>(key, value)
}

pub(crate) fn encoded_node_key_charge(
    key: &DraftMarkerAdmissionNodeKeyV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    let key = DraftMarkerAdmissionNodesFamily::encode_key(key)
        .map_err(|_| DraftMarkerAdmissionSchemaErrorV1::ValueTooLarge)?;
    u64::try_from(key.len()).map_err(|_| DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
}
pub(crate) fn encoded_receipt_record_charge(
    key: &DraftMarkerAdmissionReceiptKeyV1,
    value: &DraftMarkerAdmissionReplayReceiptV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    record_charge::<DraftMarkerAdmissionReceiptsFamily>(key, value)
}

pub(crate) fn encoded_receipt_key_charge(
    key: &DraftMarkerAdmissionReceiptKeyV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    record_key_charge::<DraftMarkerAdmissionReceiptsFamily>(key)
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionCodecFixtureV1 {
    Capacity(DraftMarkerAdmissionCapacityV1),
    Head(DraftMarkerAdmissionHeadV1),
    Node(DraftMarkerAdmissionNodeV1),
    Receipt(DraftMarkerAdmissionReplayReceiptV1),
}

#[cfg(feature = "test-faults")]
pub fn draft_marker_admission_codec_accepts(value: DraftMarkerAdmissionCodecFixtureV1) -> bool {
    match value {
        DraftMarkerAdmissionCodecFixtureV1::Capacity(value) => encode_capacity(&value)
            .and_then(|bytes| decode_capacity(&bytes))
            .is_ok(),
        DraftMarkerAdmissionCodecFixtureV1::Head(value) => encode_head(&value)
            .and_then(|bytes| decode_head(&bytes))
            .is_ok(),
        DraftMarkerAdmissionCodecFixtureV1::Node(value) => encode_node(&value)
            .and_then(|bytes| decode_node(&bytes))
            .is_ok(),
        DraftMarkerAdmissionCodecFixtureV1::Receipt(value) => encode_receipt(&value)
            .and_then(|bytes| decode_receipt(&bytes))
            .is_ok(),
    }
}

#[cfg(feature = "test-faults")]
pub fn draft_marker_admission_corrupted_value_rejected(
    value: DraftMarkerAdmissionCodecFixtureV1,
) -> bool {
    fn corrupt(mut bytes: Vec<u8>) -> Option<Vec<u8>> {
        let last = bytes.last_mut()?;
        *last ^= 1;
        Some(bytes)
    }
    match value {
        DraftMarkerAdmissionCodecFixtureV1::Capacity(value) => encode_capacity(&value)
            .ok()
            .and_then(corrupt)
            .is_some_and(|bytes| decode_capacity(&bytes).is_err()),
        DraftMarkerAdmissionCodecFixtureV1::Head(value) => encode_head(&value)
            .ok()
            .and_then(corrupt)
            .is_some_and(|bytes| decode_head(&bytes).is_err()),
        DraftMarkerAdmissionCodecFixtureV1::Node(value) => encode_node(&value)
            .ok()
            .and_then(corrupt)
            .is_some_and(|bytes| decode_node(&bytes).is_err()),
        DraftMarkerAdmissionCodecFixtureV1::Receipt(value) => encode_receipt(&value)
            .ok()
            .and_then(corrupt)
            .is_some_and(|bytes| decode_receipt(&bytes).is_err()),
    }
}
