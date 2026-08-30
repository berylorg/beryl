use std::{error::Error, fmt, num::NonZeroU64, sync::Arc};

use beryl_home_store::{
    FixedDigestHomeProofProtocol, ProofCompositionError, ProofReceiptError, ReadError,
};
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId};
use sha2::{Digest, Sha256};

use crate::draft_piece::{
    DraftEditorCandidateSessionIdV1, DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionOwnerV1,
    DraftPieceRootBuildIdentityV1, DraftPieceRootReferenceV1, DraftPieceSettlementKeyV1,
};

pub(super) const PAGE_MAX_ASSOCIATIONS: usize = 256;
pub(super) const PAGE_MAX_EVIDENCE_BYTES: usize = 65_536;
const PAGE_DOMAIN: &[u8] = b"syndic/draft-marker-label-readiness-page/v1";
const SOURCE_ENTRY_TAG: u8 = 0;
const CANDIDATE_SELECTOR_TAG: u8 = 0;
const CUT_SELECTOR_TAG: u8 = 1;
const ROOT_REFERENCE_BYTES: usize = 327;

pub(super) type PageProtocol = FixedDigestHomeProofProtocol<0x53444d5244595631, 0x5244595041474531>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerReadinessCandidateSourceV1 {
    pub(super) draft_id: SyndicDraftId,
    pub(super) session_id: DraftEditorCandidateSessionIdV1,
    pub(super) candidate_generation: u64,
    pub(super) root: DraftPieceRootReferenceV1,
    pub(super) marker_id: SyndicDraftMarkerId,
}

impl DraftMarkerReadinessCandidateSourceV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        candidate_generation: u64,
        root: DraftPieceRootReferenceV1,
        marker_id: SyndicDraftMarkerId,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            candidate_generation,
            root,
            marker_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerReadinessCutSourceV1 {
    pub(super) settlement: DraftPieceSettlementKeyV1,
    pub(super) successor_generation: u64,
    pub(super) successor_root: DraftPieceRootReferenceV1,
    pub(super) marker_id: SyndicDraftMarkerId,
}

impl DraftMarkerReadinessCutSourceV1 {
    pub const fn new(
        settlement: DraftPieceSettlementKeyV1,
        successor_generation: u64,
        successor_root: DraftPieceRootReferenceV1,
        marker_id: SyndicDraftMarkerId,
    ) -> Self {
        Self {
            settlement,
            successor_generation,
            successor_root,
            marker_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerReadinessSourceSelectorV1 {
    Candidate(DraftMarkerReadinessCandidateSourceV1),
    Cut(DraftMarkerReadinessCutSourceV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerReadinessSourceAssociationV1 {
    pub(super) target_marker_id: SyndicDraftMarkerId,
    pub(super) selector: DraftMarkerReadinessSourceSelectorV1,
}

impl DraftMarkerReadinessSourceAssociationV1 {
    pub const fn new(
        target_marker_id: SyndicDraftMarkerId,
        selector: DraftMarkerReadinessSourceSelectorV1,
    ) -> Self {
        Self {
            target_marker_id,
            selector,
        }
    }
}

#[derive(Debug)]
pub enum DraftMarkerReadinessSourceErrorV1 {
    Read(ReadError),
    PreflightRead(crate::SyndicReadError),
    Rejected,
    Build,
    Seal,
    Compose(ProofCompositionError),
    Receipt(ProofReceiptError),
}

impl fmt::Display for DraftMarkerReadinessSourceErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(
                formatter,
                "draft-marker readiness source read failed: {error}"
            ),
            Self::PreflightRead(error) => write!(
                formatter,
                "draft-marker readiness preflight read failed: {error}"
            ),
            Self::Rejected => formatter.write_str("draft-marker readiness source was rejected"),
            Self::Build => {
                formatter.write_str("draft-marker readiness proof command could not be built")
            }
            Self::Seal => {
                formatter.write_str("draft-marker readiness proof command could not be sealed")
            }
            Self::Compose(error) => write!(
                formatter,
                "draft-marker readiness proof composition failed: {error}"
            ),
            Self::Receipt(error) => write!(
                formatter,
                "draft-marker readiness proof receipt failed: {error}"
            ),
        }
    }
}

impl Error for DraftMarkerReadinessSourceErrorV1 {}

#[derive(Clone)]
pub(crate) struct CanonicalEntry {
    pub(crate) target_marker_id: SyndicDraftMarkerId,
    pub(crate) selector: DraftMarkerReadinessSourceSelectorV1,
    pub(crate) label: ImageLabelOrdinal,
    pub(crate) asset_id: AssetId,
}

impl CanonicalEntry {
    pub(super) fn selector_tag(&self) -> u8 {
        selector_tag(self.selector)
    }

    pub(super) fn evidence_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(SOURCE_ENTRY_TAG);
        match self.selector {
            DraftMarkerReadinessSourceSelectorV1::Candidate(source) => {
                bytes.push(CANDIDATE_SELECTOR_TAG);
                bytes.extend_from_slice(source.draft_id.as_bytes());
                bytes.extend_from_slice(source.session_id.as_bytes());
                bytes.extend_from_slice(&source.candidate_generation.to_le_bytes());
                bytes.extend_from_slice(&fixed_root_reference_bytes(source.root));
                bytes.extend_from_slice(source.marker_id.as_bytes());
            }
            DraftMarkerReadinessSourceSelectorV1::Cut(source) => {
                bytes.push(CUT_SELECTOR_TAG);
                bytes.extend_from_slice(source.settlement.draft_id().as_bytes());
                bytes.extend_from_slice(source.settlement.session_id().as_bytes());
                bytes.extend_from_slice(source.settlement.operation_id().as_bytes());
                bytes.extend_from_slice(&source.successor_generation.to_le_bytes());
                bytes.extend_from_slice(&fixed_root_reference_bytes(source.successor_root));
                bytes.extend_from_slice(source.marker_id.as_bytes());
            }
        }
        bytes.extend_from_slice(&self.label.get().to_le_bytes());
        bytes.push(self.asset_id.version() as u8);
        bytes.extend_from_slice(&self.asset_id.digest());
        bytes.extend_from_slice(&self.asset_id.length().get().to_le_bytes());
        bytes
    }
}

fn fixed_root_reference_bytes(root: DraftPieceRootReferenceV1) -> [u8; ROOT_REFERENCE_BYTES] {
    let mut bytes = Vec::with_capacity(ROOT_REFERENCE_BYTES);
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
    fixed_id(&mut bytes, root.root_node());
    let summary = root.summary();
    bytes.extend_from_slice(&summary.logical_utf8_bytes().to_le_bytes());
    bytes.extend_from_slice(&summary.newline_count().to_le_bytes());
    bytes.extend_from_slice(&summary.logical_line_count().to_le_bytes());
    bytes.extend_from_slice(&summary.piece_count().to_le_bytes());
    bytes.extend_from_slice(&summary.marker_count().to_le_bytes());
    bytes.extend_from_slice(summary.marker_digest().as_bytes());
    bytes.push(summary.height());
    bytes.extend_from_slice(summary.root_digest().as_bytes());
    fixed_id(&mut bytes, root.marker_index_root());
    let index = root.marker_index_summary();
    bytes.extend_from_slice(&index.record_count().to_le_bytes());
    bytes.push(index.height());
    bytes.extend_from_slice(index.root_digest().as_bytes());
    fixed_id(&mut bytes, root.marker_order_root());
    bytes.push(root.marker_order_height());
    let commitment = root.marker_commitment();
    bytes.extend_from_slice(&commitment.tree_root_digest());
    bytes.extend_from_slice(&commitment.marker_count().to_le_bytes());
    match commitment.maximum_image_label() {
        Some(label) => bytes.extend_from_slice(&label.get().to_le_bytes()),
        None => bytes.extend_from_slice(&0_u64.to_le_bytes()),
    }
    bytes.extend_from_slice(root.combined_digest().as_bytes());
    bytes
        .try_into()
        .expect("draft-marker readiness root reference has fixed width")
}

fn fixed_id(bytes: &mut Vec<u8>, id: Option<crate::draft_piece::DraftPieceRecordIdV1>) {
    match id {
        Some(id) => {
            bytes.push(1);
            bytes.extend_from_slice(id.as_bytes());
        }
        None => {
            bytes.push(0);
            bytes.extend_from_slice(&[0; 16]);
        }
    }
}

pub(crate) struct SealedDraftMarkerReadinessSourcePageV1 {
    pub(crate) owner: DraftMarkerAdmissionOwnerV1,
    pub(crate) page: DraftMarkerAdmissionCommandIdV1,
    pub(crate) ordinal: NonZeroU64,
    pub(crate) eof: bool,
    pub(crate) expected: [u8; 32],
    pub(crate) entries: Box<[CanonicalEntry]>,
}

pub struct SourceInput {
    pub(super) page: Arc<SealedDraftMarkerReadinessSourcePageV1>,
}

pub(super) fn selector_tag(selector: DraftMarkerReadinessSourceSelectorV1) -> u8 {
    match selector {
        DraftMarkerReadinessSourceSelectorV1::Candidate(_) => CANDIDATE_SELECTOR_TAG,
        DraftMarkerReadinessSourceSelectorV1::Cut(_) => CUT_SELECTOR_TAG,
    }
}

pub(super) fn page_correlation(
    ordinal: NonZeroU64,
    eof: bool,
    entries: &[CanonicalEntry],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(PAGE_DOMAIN);
    hasher.update(ordinal.get().to_le_bytes());
    hasher.update([u8::from(eof)]);
    hasher.update((entries.len() as u64).to_le_bytes());
    for entry in entries {
        hasher.update(entry.evidence_bytes());
    }
    hasher.finalize().into()
}
