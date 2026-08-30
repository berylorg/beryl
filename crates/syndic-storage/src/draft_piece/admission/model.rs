use std::num::NonZeroU64;

use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId};

use super::{
    DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS, DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
    DRAFT_MARKER_ADMISSION_MAX_HEADS,
};
use crate::DraftEditorCandidateSessionIdV1;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerAdmissionOperationIdV1([u8; 16]);

impl DraftMarkerAdmissionOperationIdV1 {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerAdmissionNodeIdV1([u8; 16]);

impl DraftMarkerAdmissionNodeIdV1 {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerAdmissionCommandIdV1([u8; 16]);

impl DraftMarkerAdmissionCommandIdV1 {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerAdmissionDigestV1([u8; 32]);

impl DraftMarkerAdmissionDigestV1 {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerAdmissionOwnerV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftMarkerAdmissionOperationIdV1,
}

impl DraftMarkerAdmissionOwnerV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftMarkerAdmissionOperationIdV1,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            operation_id,
        }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }

    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }

    pub const fn operation_id(self) -> DraftMarkerAdmissionOperationIdV1 {
        self.operation_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionCapacityKeyV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionReceiptKeyV1 {
    owner: DraftMarkerAdmissionOwnerV1,
    command_id: DraftMarkerAdmissionCommandIdV1,
}

impl DraftMarkerAdmissionReceiptKeyV1 {
    pub const fn new(
        owner: DraftMarkerAdmissionOwnerV1,
        command_id: DraftMarkerAdmissionCommandIdV1,
    ) -> Self {
        Self { owner, command_id }
    }

    pub const fn owner(self) -> DraftMarkerAdmissionOwnerV1 {
        self.owner
    }

    pub const fn command_id(self) -> DraftMarkerAdmissionCommandIdV1 {
        self.command_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionLimitsV1 {
    max_heads: u64,
    max_associations: u64,
    max_encoded_bytes: u64,
}

impl DraftMarkerAdmissionLimitsV1 {
    pub const PRODUCTION: Self = Self {
        max_heads: DRAFT_MARKER_ADMISSION_MAX_HEADS,
        max_associations: DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
        max_encoded_bytes: DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
    };

    pub const fn new(max_heads: u64, max_associations: u64, max_encoded_bytes: u64) -> Self {
        Self {
            max_heads,
            max_associations,
            max_encoded_bytes,
        }
    }

    pub const fn max_heads(self) -> u64 {
        self.max_heads
    }

    pub const fn max_associations(self) -> u64 {
        self.max_associations
    }

    pub const fn max_encoded_bytes(self) -> u64 {
        self.max_encoded_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionRetainedChargeV1 {
    heads: u64,
    associations: u64,
    encoded_bytes: u64,
}

impl DraftMarkerAdmissionRetainedChargeV1 {
    pub const ZERO: Self = Self::new(0, 0, 0);

    pub const fn new(heads: u64, associations: u64, encoded_bytes: u64) -> Self {
        Self {
            heads,
            associations,
            encoded_bytes,
        }
    }

    pub const fn heads(self) -> u64 {
        self.heads
    }

    pub const fn associations(self) -> u64 {
        self.associations
    }

    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self::new(
            self.heads.checked_add(other.heads)?,
            self.associations.checked_add(other.associations)?,
            self.encoded_bytes.checked_add(other.encoded_bytes)?,
        ))
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        Some(Self::new(
            self.heads.checked_sub(other.heads)?,
            self.associations.checked_sub(other.associations)?,
            self.encoded_bytes.checked_sub(other.encoded_bytes)?,
        ))
    }

    pub const fn fits(self, limits: DraftMarkerAdmissionLimitsV1) -> bool {
        self.heads <= limits.max_heads
            && self.associations <= limits.max_associations
            && self.encoded_bytes <= limits.max_encoded_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionTreeV1 {
    SourceOrder,
    TargetId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DraftMarkerAdmissionNodeKindV1 {
    Internal,
    Leaf,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerAdmissionNodeKeyV1 {
    owner: DraftMarkerAdmissionOwnerV1,
    kind: DraftMarkerAdmissionNodeKindV1,
    node_id: DraftMarkerAdmissionNodeIdV1,
}

impl DraftMarkerAdmissionNodeKeyV1 {
    pub const fn new(
        owner: DraftMarkerAdmissionOwnerV1,
        kind: DraftMarkerAdmissionNodeKindV1,
        node_id: DraftMarkerAdmissionNodeIdV1,
    ) -> Self {
        Self {
            owner,
            kind,
            node_id,
        }
    }

    pub const fn owner(self) -> DraftMarkerAdmissionOwnerV1 {
        self.owner
    }

    pub const fn kind(self) -> DraftMarkerAdmissionNodeKindV1 {
        self.kind
    }

    pub const fn node_id(self) -> DraftMarkerAdmissionNodeIdV1 {
        self.node_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionSourceKeyV1 {
    source_label: ImageLabelOrdinal,
    target_marker_id: SyndicDraftMarkerId,
}

impl DraftMarkerAdmissionSourceKeyV1 {
    pub const fn new(
        source_label: ImageLabelOrdinal,
        target_marker_id: SyndicDraftMarkerId,
    ) -> Self {
        Self {
            source_label,
            target_marker_id,
        }
    }

    pub const fn source_label(self) -> ImageLabelOrdinal {
        self.source_label
    }

    pub const fn target_marker_id(self) -> SyndicDraftMarkerId {
        self.target_marker_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionEnvelopeV1 {
    SourceOrder {
        first: DraftMarkerAdmissionSourceKeyV1,
        last: DraftMarkerAdmissionSourceKeyV1,
    },
    TargetId {
        first: SyndicDraftMarkerId,
        last: SyndicDraftMarkerId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionChildV1 {
    key: DraftMarkerAdmissionNodeKeyV1,
    digest: DraftMarkerAdmissionDigestV1,
    count: u64,
    envelope: DraftMarkerAdmissionEnvelopeV1,
}

impl DraftMarkerAdmissionChildV1 {
    pub const fn new(
        key: DraftMarkerAdmissionNodeKeyV1,
        digest: DraftMarkerAdmissionDigestV1,
        count: u64,
        envelope: DraftMarkerAdmissionEnvelopeV1,
    ) -> Self {
        Self {
            key,
            digest,
            count,
            envelope,
        }
    }

    pub const fn key(self) -> DraftMarkerAdmissionNodeKeyV1 {
        self.key
    }

    pub const fn digest(self) -> DraftMarkerAdmissionDigestV1 {
        self.digest
    }

    pub const fn count(self) -> u64 {
        self.count
    }

    pub const fn envelope(self) -> DraftMarkerAdmissionEnvelopeV1 {
        self.envelope
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionPageIdentityV1 {
    command_id: DraftMarkerAdmissionCommandIdV1,
    page_ordinal: NonZeroU64,
}

impl DraftMarkerAdmissionPageIdentityV1 {
    pub const fn new(
        command_id: DraftMarkerAdmissionCommandIdV1,
        page_ordinal: NonZeroU64,
    ) -> Self {
        Self {
            command_id,
            page_ordinal,
        }
    }

    pub const fn command_id(self) -> DraftMarkerAdmissionCommandIdV1 {
        self.command_id
    }

    pub const fn page_ordinal(self) -> NonZeroU64 {
        self.page_ordinal
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionEvidenceV1(Box<[u8]>);

impl DraftMarkerAdmissionEvidenceV1 {
    pub fn new(bytes: impl Into<Box<[u8]>>) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        let bytes = bytes.into();
        if bytes.is_empty() || bytes.len() > super::DRAFT_MARKER_ADMISSION_MAX_EVIDENCE_BYTES {
            return Err(DraftMarkerAdmissionSchemaErrorV1::EvidenceLength);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionTargetDispositionV1 {
    Unassigned,
    Assigned(ImageLabelOrdinal),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionNodePayloadV1 {
    Internal {
        height: u8,
        children: Box<[DraftMarkerAdmissionChildV1]>,
    },
    SourceLeaf {
        source_key: DraftMarkerAdmissionSourceKeyV1,
        evidence: DraftMarkerAdmissionEvidenceV1,
        asset_id: AssetId,
    },
    TargetLeaf {
        target_marker_id: SyndicDraftMarkerId,
        page: DraftMarkerAdmissionPageIdentityV1,
        evidence: DraftMarkerAdmissionEvidenceV1,
        source_label: ImageLabelOrdinal,
        asset_id: AssetId,
        disposition: DraftMarkerAdmissionTargetDispositionV1,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionNodeV1 {
    key: DraftMarkerAdmissionNodeKeyV1,
    tree: DraftMarkerAdmissionTreeV1,
    payload: DraftMarkerAdmissionNodePayloadV1,
    digest: DraftMarkerAdmissionDigestV1,
}

impl DraftMarkerAdmissionNodeV1 {
    pub(crate) const fn from_parts(
        key: DraftMarkerAdmissionNodeKeyV1,
        tree: DraftMarkerAdmissionTreeV1,
        payload: DraftMarkerAdmissionNodePayloadV1,
        digest: DraftMarkerAdmissionDigestV1,
    ) -> Self {
        Self {
            key,
            tree,
            payload,
            digest,
        }
    }

    pub const fn key(&self) -> DraftMarkerAdmissionNodeKeyV1 {
        self.key
    }

    pub const fn tree(&self) -> DraftMarkerAdmissionTreeV1 {
        self.tree
    }

    pub const fn payload(&self) -> &DraftMarkerAdmissionNodePayloadV1 {
        &self.payload
    }

    pub const fn digest(&self) -> DraftMarkerAdmissionDigestV1 {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionRootV1 {
    tree: DraftMarkerAdmissionTreeV1,
    node: Option<DraftMarkerAdmissionNodeKeyV1>,
    height: u8,
    digest: DraftMarkerAdmissionDigestV1,
    count: u64,
}

impl DraftMarkerAdmissionRootV1 {
    pub(crate) const fn from_parts(
        tree: DraftMarkerAdmissionTreeV1,
        node: Option<DraftMarkerAdmissionNodeKeyV1>,
        height: u8,
        digest: DraftMarkerAdmissionDigestV1,
        count: u64,
    ) -> Self {
        Self {
            tree,
            node,
            height,
            digest,
            count,
        }
    }

    pub const fn tree(self) -> DraftMarkerAdmissionTreeV1 {
        self.tree
    }

    pub const fn node(self) -> Option<DraftMarkerAdmissionNodeKeyV1> {
        self.node
    }

    pub const fn height(self) -> u8 {
        self.height
    }

    pub const fn digest(self) -> DraftMarkerAdmissionDigestV1 {
        self.digest
    }

    pub const fn count(self) -> u64 {
        self.count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionLifecycleV1 {
    Ingesting,
    Assigning,
    Ready,
    Building,
    TerminalCleanup,
    Settled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionCleanupCursorV1 {
    tree: DraftMarkerAdmissionTreeV1,
    after: Option<DraftMarkerAdmissionNodeKeyV1>,
}

impl DraftMarkerAdmissionCleanupCursorV1 {
    pub const fn new(
        tree: DraftMarkerAdmissionTreeV1,
        after: Option<DraftMarkerAdmissionNodeKeyV1>,
    ) -> Self {
        Self { tree, after }
    }

    pub const fn tree(self) -> DraftMarkerAdmissionTreeV1 {
        self.tree
    }

    pub const fn after(self) -> Option<DraftMarkerAdmissionNodeKeyV1> {
        self.after
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionAssignmentContinuationV1 {
    reserved_first: ImageLabelOrdinal,
    reserved_last: ImageLabelOrdinal,
    next_allocation: ImageLabelOrdinal,
    prior_source: Option<(ImageLabelOrdinal, AssetId)>,
}

impl DraftMarkerAdmissionAssignmentContinuationV1 {
    pub fn new(
        reserved_first: ImageLabelOrdinal,
        reserved_last: ImageLabelOrdinal,
        next_allocation: ImageLabelOrdinal,
        prior_source: Option<(ImageLabelOrdinal, AssetId)>,
    ) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        if reserved_first > reserved_last
            || next_allocation < reserved_first
            || next_allocation > reserved_last
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
        }
        Ok(Self {
            reserved_first,
            reserved_last,
            next_allocation,
            prior_source,
        })
    }

    pub const fn reserved_first(self) -> ImageLabelOrdinal {
        self.reserved_first
    }

    pub const fn reserved_last(self) -> ImageLabelOrdinal {
        self.reserved_last
    }

    pub const fn next_allocation(self) -> ImageLabelOrdinal {
        self.next_allocation
    }

    pub const fn prior_source(self) -> Option<(ImageLabelOrdinal, AssetId)> {
        self.prior_source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionHeadV1 {
    owner: DraftMarkerAdmissionOwnerV1,
    revision: NonZeroU64,
    home_generation: NonZeroU64,
    lifecycle: DraftMarkerAdmissionLifecycleV1,
    request_commitment: DraftMarkerAdmissionDigestV1,
    custody_commitment: DraftMarkerAdmissionDigestV1,
    next_page_ordinal: NonZeroU64,
    ingestion_association_cursor: u64,
    evidence_eof: bool,
    selected_receipt: Option<DraftMarkerAdmissionCommandIdV1>,
    source_root: DraftMarkerAdmissionRootV1,
    target_root: DraftMarkerAdmissionRootV1,
    occurrence_commitment: DraftMarkerAdmissionDigestV1,
    unassigned_count: u64,
    assignment_continuation: Option<DraftMarkerAdmissionAssignmentContinuationV1>,
    remaining_builder_count: u64,
    charge: DraftMarkerAdmissionRetainedChargeV1,
    limits: DraftMarkerAdmissionLimitsV1,
    cleanup_cursor: Option<DraftMarkerAdmissionCleanupCursorV1>,
    digest: DraftMarkerAdmissionDigestV1,
}

impl DraftMarkerAdmissionHeadV1 {
    pub(crate) fn from_parts(parts: DraftMarkerAdmissionHeadPartsV1) -> Self {
        Self {
            owner: parts.owner,
            revision: parts.revision,
            home_generation: parts.home_generation,
            lifecycle: parts.lifecycle,
            request_commitment: parts.request_commitment,
            custody_commitment: parts.custody_commitment,
            next_page_ordinal: parts.next_page_ordinal,
            ingestion_association_cursor: parts.ingestion_association_cursor,
            evidence_eof: parts.evidence_eof,
            selected_receipt: parts.selected_receipt,
            source_root: parts.source_root,
            target_root: parts.target_root,
            occurrence_commitment: parts.occurrence_commitment,
            unassigned_count: parts.unassigned_count,
            assignment_continuation: parts.assignment_continuation,
            remaining_builder_count: parts.remaining_builder_count,
            charge: parts.charge,
            limits: parts.limits,
            cleanup_cursor: parts.cleanup_cursor,
            digest: parts.digest,
        }
    }

    pub const fn owner(&self) -> DraftMarkerAdmissionOwnerV1 {
        self.owner
    }
    pub const fn revision(&self) -> NonZeroU64 {
        self.revision
    }
    pub const fn home_generation(&self) -> NonZeroU64 {
        self.home_generation
    }
    pub const fn lifecycle(&self) -> DraftMarkerAdmissionLifecycleV1 {
        self.lifecycle
    }
    pub const fn request_commitment(&self) -> DraftMarkerAdmissionDigestV1 {
        self.request_commitment
    }
    pub const fn custody_commitment(&self) -> DraftMarkerAdmissionDigestV1 {
        self.custody_commitment
    }
    pub const fn next_page_ordinal(&self) -> NonZeroU64 {
        self.next_page_ordinal
    }
    pub const fn ingestion_association_cursor(&self) -> u64 {
        self.ingestion_association_cursor
    }
    pub const fn evidence_eof(&self) -> bool {
        self.evidence_eof
    }
    pub const fn selected_receipt(&self) -> Option<DraftMarkerAdmissionCommandIdV1> {
        self.selected_receipt
    }
    pub const fn source_root(&self) -> DraftMarkerAdmissionRootV1 {
        self.source_root
    }
    pub const fn target_root(&self) -> DraftMarkerAdmissionRootV1 {
        self.target_root
    }
    pub const fn occurrence_commitment(&self) -> DraftMarkerAdmissionDigestV1 {
        self.occurrence_commitment
    }
    pub const fn unassigned_count(&self) -> u64 {
        self.unassigned_count
    }
    pub const fn assignment_continuation(
        &self,
    ) -> Option<DraftMarkerAdmissionAssignmentContinuationV1> {
        self.assignment_continuation
    }
    pub const fn remaining_builder_count(&self) -> u64 {
        self.remaining_builder_count
    }
    pub const fn charge(&self) -> DraftMarkerAdmissionRetainedChargeV1 {
        self.charge
    }
    pub const fn limits(&self) -> DraftMarkerAdmissionLimitsV1 {
        self.limits
    }
    pub const fn cleanup_cursor(&self) -> Option<DraftMarkerAdmissionCleanupCursorV1> {
        self.cleanup_cursor
    }
    pub const fn digest(&self) -> DraftMarkerAdmissionDigestV1 {
        self.digest
    }
}

pub(crate) struct DraftMarkerAdmissionHeadPartsV1 {
    pub owner: DraftMarkerAdmissionOwnerV1,
    pub revision: NonZeroU64,
    pub home_generation: NonZeroU64,
    pub lifecycle: DraftMarkerAdmissionLifecycleV1,
    pub request_commitment: DraftMarkerAdmissionDigestV1,
    pub custody_commitment: DraftMarkerAdmissionDigestV1,
    pub next_page_ordinal: NonZeroU64,
    pub ingestion_association_cursor: u64,
    pub evidence_eof: bool,
    pub selected_receipt: Option<DraftMarkerAdmissionCommandIdV1>,
    pub source_root: DraftMarkerAdmissionRootV1,
    pub target_root: DraftMarkerAdmissionRootV1,
    pub occurrence_commitment: DraftMarkerAdmissionDigestV1,
    pub unassigned_count: u64,
    pub assignment_continuation: Option<DraftMarkerAdmissionAssignmentContinuationV1>,
    pub remaining_builder_count: u64,
    pub charge: DraftMarkerAdmissionRetainedChargeV1,
    pub limits: DraftMarkerAdmissionLimitsV1,
    pub cleanup_cursor: Option<DraftMarkerAdmissionCleanupCursorV1>,
    pub digest: DraftMarkerAdmissionDigestV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionCapacityV1 {
    revision: NonZeroU64,
    charge: DraftMarkerAdmissionRetainedChargeV1,
    limits: DraftMarkerAdmissionLimitsV1,
    digest: DraftMarkerAdmissionDigestV1,
}

impl DraftMarkerAdmissionCapacityV1 {
    pub(crate) const fn from_parts(
        revision: NonZeroU64,
        charge: DraftMarkerAdmissionRetainedChargeV1,
        limits: DraftMarkerAdmissionLimitsV1,
        digest: DraftMarkerAdmissionDigestV1,
    ) -> Self {
        Self {
            revision,
            charge,
            limits,
            digest,
        }
    }

    pub const fn revision(&self) -> NonZeroU64 {
        self.revision
    }
    pub const fn charge(&self) -> DraftMarkerAdmissionRetainedChargeV1 {
        self.charge
    }
    pub const fn limits(&self) -> DraftMarkerAdmissionLimitsV1 {
        self.limits
    }
    pub const fn digest(&self) -> DraftMarkerAdmissionDigestV1 {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionReceiptTransitionV1 {
    Ingestion,
    Assignment,
    TerminalCleanup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMarkerAdmissionReplayReceiptV1 {
    owner: DraftMarkerAdmissionOwnerV1,
    command_id: DraftMarkerAdmissionCommandIdV1,
    page_ordinal: NonZeroU64,
    request_commitment: DraftMarkerAdmissionDigestV1,
    source_head_bytes: Box<[u8]>,
    target_head_bytes: Box<[u8]>,
    source_before: DraftMarkerAdmissionRootV1,
    source_after: DraftMarkerAdmissionRootV1,
    target_before: DraftMarkerAdmissionRootV1,
    target_after: DraftMarkerAdmissionRootV1,
    retained_predecessor_nodes: Box<[DraftMarkerAdmissionChildV1]>,
    transition: DraftMarkerAdmissionReceiptTransitionV1,
    digest: DraftMarkerAdmissionDigestV1,
}

impl DraftMarkerAdmissionReplayReceiptV1 {
    pub(crate) fn from_parts(parts: DraftMarkerAdmissionReplayReceiptPartsV1) -> Self {
        Self {
            owner: parts.owner,
            command_id: parts.command_id,
            page_ordinal: parts.page_ordinal,
            request_commitment: parts.request_commitment,
            source_head_bytes: parts.source_head_bytes,
            target_head_bytes: parts.target_head_bytes,
            source_before: parts.source_before,
            source_after: parts.source_after,
            target_before: parts.target_before,
            target_after: parts.target_after,
            retained_predecessor_nodes: parts.retained_predecessor_nodes,
            transition: parts.transition,
            digest: parts.digest,
        }
    }

    pub const fn owner(&self) -> DraftMarkerAdmissionOwnerV1 {
        self.owner
    }
    pub const fn command_id(&self) -> DraftMarkerAdmissionCommandIdV1 {
        self.command_id
    }
    pub const fn page_ordinal(&self) -> NonZeroU64 {
        self.page_ordinal
    }
    pub const fn request_commitment(&self) -> DraftMarkerAdmissionDigestV1 {
        self.request_commitment
    }
    pub fn source_head_bytes(&self) -> &[u8] {
        &self.source_head_bytes
    }
    pub fn target_head_bytes(&self) -> &[u8] {
        &self.target_head_bytes
    }
    pub const fn source_before(&self) -> DraftMarkerAdmissionRootV1 {
        self.source_before
    }
    pub const fn source_after(&self) -> DraftMarkerAdmissionRootV1 {
        self.source_after
    }
    pub const fn target_before(&self) -> DraftMarkerAdmissionRootV1 {
        self.target_before
    }
    pub const fn target_after(&self) -> DraftMarkerAdmissionRootV1 {
        self.target_after
    }
    pub fn retained_predecessor_nodes(&self) -> &[DraftMarkerAdmissionChildV1] {
        &self.retained_predecessor_nodes
    }
    pub const fn transition(&self) -> DraftMarkerAdmissionReceiptTransitionV1 {
        self.transition
    }
    pub const fn digest(&self) -> DraftMarkerAdmissionDigestV1 {
        self.digest
    }
}

pub(crate) struct DraftMarkerAdmissionReplayReceiptPartsV1 {
    pub owner: DraftMarkerAdmissionOwnerV1,
    pub command_id: DraftMarkerAdmissionCommandIdV1,
    pub page_ordinal: NonZeroU64,
    pub request_commitment: DraftMarkerAdmissionDigestV1,
    pub source_head_bytes: Box<[u8]>,
    pub target_head_bytes: Box<[u8]>,
    pub source_before: DraftMarkerAdmissionRootV1,
    pub source_after: DraftMarkerAdmissionRootV1,
    pub target_before: DraftMarkerAdmissionRootV1,
    pub target_after: DraftMarkerAdmissionRootV1,
    pub retained_predecessor_nodes: Box<[DraftMarkerAdmissionChildV1]>,
    pub transition: DraftMarkerAdmissionReceiptTransitionV1,
    pub digest: DraftMarkerAdmissionDigestV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionSchemaErrorV1 {
    ArithmeticOverflow,
    CapacityExceeded,
    CommandTooLarge,
    DigestMismatch,
    EvidenceLength,
    InvalidCount,
    InvalidEnvelope,
    InvalidHead,
    InvalidRoot,
    InvalidTree,
    NodeFanout,
    TreeHeight,
    ValueTooLarge,
}

impl std::fmt::Display for DraftMarkerAdmissionSchemaErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid draft-marker admission schema: {self:?}")
    }
}

impl std::error::Error for DraftMarkerAdmissionSchemaErrorV1 {}
