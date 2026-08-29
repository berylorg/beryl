use std::{error::Error, fmt, num::NonZeroU64};

#[cfg(feature = "test-faults")]
use beryl_home_store::RecordCodec;
use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation, RecordVersion,
};
use beryl_model::{
    AssetId, DomainRevision, DraftMarkerCommitmentV1, ImageLabelOrdinal,
    OrderedMarkerAssetSummaryV1, SequentialMarkerSummaryV1, SyndicDraftId, SyndicDraftMarkerId,
    advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};
use sha2::{Digest, Sha256};

use crate::{
    SyndicMutationError, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    codec::{
        CodecError, ExactCodec, Family, family_point_limit,
        parts::{Decoder, Encoder},
    },
    domain::SyndicDomain,
};

use super::{
    DRAFT_PIECE_MAX_CHILDREN, DRAFT_PIECE_MAX_HEIGHT, DraftEditorCandidateSessionIdV1,
    DraftMarkerOrderCommitmentsFamily, DraftMarkerOrderRecordKeyV1, DraftMarkerOrderRecordKindV1,
    DraftMarkerOrderRecordV1, DraftPieceDigestV1, DraftPieceOperationIdV1, DraftPieceRecordIdV1,
    DraftPieceRootBuildIdentityV1, DraftPieceRootKeyV1, DraftPieceRootReferenceV1,
    DraftPieceRootsFamily, marker_order_leaf_digest, marker_order_node_digest,
};

pub const DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS: usize = 256;

const SEAL_RECORD_DIGEST_DOMAIN: &[u8] = b"beryl.syndic.draft-marker-seal-record.v2\0";

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerSealOperationIdV1([u8; 16]);

impl DraftMarkerSealOperationIdV1 {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerSealKeyV1 {
    root_key: DraftPieceRootKeyV1,
    combined_digest: DraftPieceDigestV1,
    marker_order_root: Option<DraftPieceRecordIdV1>,
    commitment: DraftMarkerCommitmentV1,
    operation_id: DraftMarkerSealOperationIdV1,
}

impl DraftMarkerSealKeyV1 {
    pub const fn root_key(self) -> DraftPieceRootKeyV1 {
        self.root_key
    }

    pub const fn combined_digest(self) -> DraftPieceDigestV1 {
        self.combined_digest
    }

    pub const fn marker_order_root(self) -> Option<DraftPieceRecordIdV1> {
        self.marker_order_root
    }

    pub const fn commitment(self) -> DraftMarkerCommitmentV1 {
        self.commitment
    }

    pub const fn operation_id(self) -> DraftMarkerSealOperationIdV1 {
        self.operation_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealRequestV1 {
    source: DraftPieceRootReferenceV1,
    operation_id: DraftMarkerSealOperationIdV1,
}

impl DraftMarkerSealRequestV1 {
    pub const fn new(
        source: DraftPieceRootReferenceV1,
        operation_id: DraftMarkerSealOperationIdV1,
    ) -> Self {
        Self {
            source,
            operation_id,
        }
    }

    pub const fn source(self) -> DraftPieceRootReferenceV1 {
        self.source
    }

    pub const fn operation_id(self) -> DraftMarkerSealOperationIdV1 {
        self.operation_id
    }

    pub const fn key(self) -> DraftMarkerSealKeyV1 {
        DraftMarkerSealKeyV1 {
            root_key: self.source.key(),
            combined_digest: self.source.combined_digest(),
            marker_order_root: self.source.marker_order_root(),
            commitment: self.source.marker_commitment(),
            operation_id: self.operation_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealOrderedMarkerV1 {
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
}

impl DraftMarkerSealOrderedMarkerV1 {
    pub const fn marker_id(self) -> SyndicDraftMarkerId {
        self.marker_id
    }

    pub const fn label(self) -> ImageLabelOrdinal {
        self.label
    }

    pub const fn asset_id(self) -> AssetId {
        self.asset_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealPageReleaseV1 {
    key: DraftMarkerSealKeyV1,
    source_frontier: u64,
    target_frontier: u64,
}

impl DraftMarkerSealPageReleaseV1 {
    pub const fn key(self) -> DraftMarkerSealKeyV1 {
        self.key
    }

    pub const fn source_frontier(self) -> u64 {
        self.source_frontier
    }

    pub const fn target_frontier(self) -> u64 {
        self.target_frontier
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealPageV1 {
    markers: Vec<DraftMarkerSealOrderedMarkerV1>,
    release: DraftMarkerSealPageReleaseV1,
    exact_eof: bool,
}

impl DraftMarkerSealPageV1 {
    pub fn markers(&self) -> &[DraftMarkerSealOrderedMarkerV1] {
        &self.markers
    }

    pub const fn release(&self) -> DraftMarkerSealPageReleaseV1 {
        self.release
    }

    pub const fn exact_eof(&self) -> bool {
        self.exact_eof
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealFailureReasonV1 {
    Operational,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealLifecycleV1 {
    Open,
    Cancelled,
    Failed(DraftMarkerSealFailureReasonV1),
    Superseded(DraftMarkerSealOperationIdV1),
    Sealed {
        sequential: SequentialMarkerSummaryV1,
        ordered_assets: OrderedMarkerAssetSummaryV1,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealCustodyReleaseV1 {
    key: DraftMarkerSealKeyV1,
    completed_marker_count: u64,
}

impl DraftMarkerSealCustodyReleaseV1 {
    pub const fn key(self) -> DraftMarkerSealKeyV1 {
        self.key
    }

    pub const fn completed_marker_count(self) -> u64 {
        self.completed_marker_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealProofV1 {
    source: DraftPieceRootReferenceV1,
    commitment: DraftMarkerCommitmentV1,
    sequential: SequentialMarkerSummaryV1,
    ordered_assets: OrderedMarkerAssetSummaryV1,
}

impl DraftMarkerSealProofV1 {
    pub const fn source(self) -> DraftPieceRootReferenceV1 {
        self.source
    }

    pub const fn commitment(self) -> DraftMarkerCommitmentV1 {
        self.commitment
    }

    pub const fn sequential(self) -> SequentialMarkerSummaryV1 {
        self.sequential
    }

    pub const fn ordered_assets(self) -> OrderedMarkerAssetSummaryV1 {
        self.ordered_assets
    }

    pub(crate) const fn new_authenticated(
        source: DraftPieceRootReferenceV1,
        commitment: DraftMarkerCommitmentV1,
        sequential: SequentialMarkerSummaryV1,
        ordered_assets: OrderedMarkerAssetSummaryV1,
    ) -> Self {
        Self {
            source,
            commitment,
            sequential,
            ordered_assets,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealStatusV1 {
    Absent,
    Open {
        completed_marker_count: u64,
    },
    Cancelled(DraftMarkerSealCustodyReleaseV1),
    Failed {
        reason: DraftMarkerSealFailureReasonV1,
        release: DraftMarkerSealCustodyReleaseV1,
    },
    Superseded {
        successor: DraftMarkerSealOperationIdV1,
        release: DraftMarkerSealCustodyReleaseV1,
    },
    Sealed(DraftMarkerSealProofV1, DraftMarkerSealCustodyReleaseV1),
}

#[derive(Debug)]
pub enum DraftMarkerSealErrorV1 {
    Read(SyndicReadError),
    MissingSource,
    MissingSeal,
    IdentityCollision,
    Corruption,
    InvalidPageLimit,
    MarkerCountOverflow,
}

impl fmt::Display for DraftMarkerSealErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "marker seal read failed: {error}"),
            Self::MissingSource => formatter.write_str("captured marker seal source is absent"),
            Self::MissingSeal => formatter.write_str("draft marker seal is absent"),
            Self::IdentityCollision => formatter.write_str("draft marker seal identity collision"),
            Self::Corruption => formatter.write_str("draft marker seal state is corrupt"),
            Self::InvalidPageLimit => {
                formatter.write_str("draft marker seal page limit is invalid")
            }
            Self::MarkerCountOverflow => formatter.write_str("draft marker seal count overflow"),
        }
    }
}

impl Error for DraftMarkerSealErrorV1 {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SyndicReadError> for DraftMarkerSealErrorV1 {
    fn from(error: SyndicReadError) -> Self {
        Self::Read(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DraftMarkerSealCursorFrameV1 {
    record_id: DraftPieceRecordIdV1,
    digest: DraftPieceDigestV1,
    height: u8,
    next_child_index: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DraftMarkerSealCursorV1 {
    BeforeRoot,
    Positioned(Vec<DraftMarkerSealCursorFrameV1>),
    Eof(Vec<DraftMarkerSealCursorFrameV1>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DraftMarkerSealFrontierV1 {
    record_id: DraftPieceRecordIdV1,
    digest: DraftPieceDigestV1,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DraftMarkerSealRecordV1 {
    key: DraftMarkerSealKeyV1,
    cursor: DraftMarkerSealCursorV1,
    frontier: Option<DraftMarkerSealFrontierV1>,
    sequential_digest: [u8; 32],
    ordered_asset_digest: [u8; 32],
    completed_marker_count: u64,
    maximum_image_label: Option<ImageLabelOrdinal>,
    lifecycle: DraftMarkerSealLifecycleV1,
    record_digest: [u8; 32],
}

#[derive(Clone)]
pub struct PreparedDraftMarkerSealBeginV1 {
    initial: DraftMarkerSealRecordV1,
    source: DraftPieceRootReferenceV1,
}

impl PreparedDraftMarkerSealBeginV1 {
    pub const fn key(&self) -> DraftMarkerSealKeyV1 {
        self.initial.key
    }
}

#[derive(Clone)]
pub struct PreparedDraftMarkerSealAdvanceV1 {
    expected: DraftMarkerSealRecordV1,
    next: DraftMarkerSealRecordV1,
    page: DraftMarkerSealPageV1,
}

impl PreparedDraftMarkerSealAdvanceV1 {
    pub const fn key(&self) -> DraftMarkerSealKeyV1 {
        self.expected.key
    }

    pub const fn page(&self) -> &DraftMarkerSealPageV1 {
        &self.page
    }
}

#[derive(Clone)]
pub struct PreparedDraftMarkerSealCancelV1 {
    expected: DraftMarkerSealRecordV1,
    next: DraftMarkerSealRecordV1,
}

impl PreparedDraftMarkerSealCancelV1 {
    pub const fn key(&self) -> DraftMarkerSealKeyV1 {
        self.expected.key
    }

    pub fn release(&self) -> DraftMarkerSealCustodyReleaseV1 {
        release_for(&self.next)
    }
}

#[derive(Clone)]
pub struct PreparedDraftMarkerSealFailV1 {
    expected: DraftMarkerSealRecordV1,
    next: DraftMarkerSealRecordV1,
}

impl PreparedDraftMarkerSealFailV1 {
    pub const fn key(&self) -> DraftMarkerSealKeyV1 {
        self.expected.key
    }

    pub fn release(&self) -> DraftMarkerSealCustodyReleaseV1 {
        release_for(&self.next)
    }
}

#[derive(Clone)]
pub struct PreparedDraftMarkerSealSupersedeV1 {
    expected: DraftMarkerSealRecordV1,
    next: DraftMarkerSealRecordV1,
}

impl PreparedDraftMarkerSealSupersedeV1 {
    pub const fn key(&self) -> DraftMarkerSealKeyV1 {
        self.expected.key
    }

    pub fn release(&self) -> DraftMarkerSealCustodyReleaseV1 {
        release_for(&self.next)
    }
}

pub(crate) struct DraftMarkerSealsFamily;
pub(crate) type DraftMarkerSealsCodec = ExactCodec<DraftMarkerSealsFamily>;

impl Family for DraftMarkerSealsFamily {
    type Key = DraftMarkerSealKeyV1;
    type Value = DraftMarkerSealRecordV1;
    const NAME: &'static str = "draft-marker-seals";
    const RECORD_VERSION: RecordVersion = RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 256;
    const MAX_VALUE_BYTES: usize = 8_192;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(encode_key(key))
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        decode_key(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_record(value)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        decode_record(encoded)
    }
}

struct BeginMutation {
    prepared: PreparedDraftMarkerSealBeginV1,
}

struct AdvanceMutation {
    prepared: PreparedDraftMarkerSealAdvanceV1,
}

struct CancelMutation {
    prepared: PreparedDraftMarkerSealCancelV1,
}

struct FailMutation {
    prepared: PreparedDraftMarkerSealFailV1,
}

struct SupersedeMutation {
    prepared: PreparedDraftMarkerSealSupersedeV1,
}

#[cfg(feature = "test-faults")]
#[derive(Clone)]
struct MarkerSealCollisionMutation {
    physical_key: DraftMarkerSealKeyV1,
    record: DraftMarkerSealRecordV1,
}

impl SyndicStorage {
    pub fn prepare_draft_marker_seal_begin(
        &self,
        store: &HomeStore,
        request: DraftMarkerSealRequestV1,
    ) -> Result<PreparedDraftMarkerSealBeginV1, DraftMarkerSealErrorV1> {
        validate_source(self, store, request.source())?;
        let mut initial = DraftMarkerSealRecordV1 {
            key: request.key(),
            cursor: DraftMarkerSealCursorV1::BeforeRoot,
            frontier: None,
            sequential_digest: sequential_marker_digest_seed(),
            ordered_asset_digest: ordered_marker_asset_digest_seed(),
            completed_marker_count: 0,
            maximum_image_label: None,
            lifecycle: DraftMarkerSealLifecycleV1::Open,
            record_digest: [0; 32],
        };
        initial.record_digest = seal_record_digest(&initial);
        if let Some(existing) = self.point::<DraftMarkerSealsFamily>(
            store,
            request.key(),
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )? {
            validate_record(&existing)?;
            if existing.key != request.key() {
                return Err(DraftMarkerSealErrorV1::IdentityCollision);
            }
        }
        Ok(PreparedDraftMarkerSealBeginV1 {
            initial,
            source: request.source(),
        })
    }

    pub fn begin_draft_marker_seal(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMarkerSealBeginV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, BeginMutation { prepared })
    }

    pub fn prepare_draft_marker_seal_advance(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
    ) -> Result<Option<PreparedDraftMarkerSealAdvanceV1>, DraftMarkerSealErrorV1> {
        self.prepare_draft_marker_seal_advance_with_limit(
            store,
            key,
            DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS,
        )
    }

    pub fn prepare_draft_marker_seal_advance_with_limit(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
        marker_limit: usize,
    ) -> Result<Option<PreparedDraftMarkerSealAdvanceV1>, DraftMarkerSealErrorV1> {
        if marker_limit == 0 || marker_limit > DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS {
            return Err(DraftMarkerSealErrorV1::InvalidPageLimit);
        }
        let Some(expected) = self.point::<DraftMarkerSealsFamily>(
            store,
            key,
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )?
        else {
            return Err(DraftMarkerSealErrorV1::MissingSeal);
        };
        validate_record(&expected)?;
        if expected.key != key {
            return Err(DraftMarkerSealErrorV1::IdentityCollision);
        }
        let source = validate_key_source(self, store, key)?;
        validate_cursor_closure(self, store, &expected)?;
        if !matches!(expected.lifecycle, DraftMarkerSealLifecycleV1::Open) {
            return Ok(None);
        }

        let source_frontier = expected.completed_marker_count;
        let mut next = expected.clone();
        let mut markers = Vec::with_capacity(marker_limit);
        while markers.len() < marker_limit {
            let Some(frontier) = next_marker(self, store, &mut next)? else {
                break;
            };
            next.sequential_digest = advance_sequential_marker_digest(
                next.sequential_digest,
                frontier.marker_id,
                frontier.label,
            );
            next.ordered_asset_digest = advance_ordered_marker_asset_digest(
                next.ordered_asset_digest,
                frontier.marker_id,
                frontier.label,
                frontier.asset_id,
            );
            next.completed_marker_count = next
                .completed_marker_count
                .checked_add(1)
                .ok_or(DraftMarkerSealErrorV1::MarkerCountOverflow)?;
            next.maximum_image_label = Some(match next.maximum_image_label {
                Some(current) => current.max(frontier.label),
                None => frontier.label,
            });
            next.frontier = Some(frontier);
            markers.push(DraftMarkerSealOrderedMarkerV1 {
                marker_id: frontier.marker_id,
                label: frontier.label,
                asset_id: frontier.asset_id,
            });
        }

        close_exhausted_cursor(self, store, &mut next)?;
        let exact_eof = matches!(next.cursor, DraftMarkerSealCursorV1::Eof(_));
        if exact_eof {
            let commitment = key.commitment;
            if next.completed_marker_count != commitment.marker_count()
                || next.maximum_image_label != commitment.maximum_image_label()
                || source.marker_order_root() != key.marker_order_root
                || source.marker_commitment() != commitment
            {
                return Err(DraftMarkerSealErrorV1::Corruption);
            }
            let summary = SequentialMarkerSummaryV1::new(
                next.sequential_digest,
                next.completed_marker_count,
                next.maximum_image_label,
            )
            .map_err(|_| DraftMarkerSealErrorV1::Corruption)?;
            let ordered_assets = OrderedMarkerAssetSummaryV1::new(
                next.ordered_asset_digest,
                next.completed_marker_count,
            );
            next.lifecycle = DraftMarkerSealLifecycleV1::Sealed {
                sequential: summary,
                ordered_assets,
            };
        } else if markers.is_empty() {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        next.record_digest = seal_record_digest(&next);
        let page = DraftMarkerSealPageV1 {
            markers,
            release: DraftMarkerSealPageReleaseV1 {
                key,
                source_frontier,
                target_frontier: next.completed_marker_count,
            },
            exact_eof,
        };
        Ok(Some(PreparedDraftMarkerSealAdvanceV1 {
            expected,
            next,
            page,
        }))
    }

    pub fn advance_draft_marker_seal(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: &PreparedDraftMarkerSealAdvanceV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            AdvanceMutation {
                prepared: prepared.clone(),
            },
        )
    }

    pub fn prepare_draft_marker_seal_cancel(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
    ) -> Result<PreparedDraftMarkerSealCancelV1, DraftMarkerSealErrorV1> {
        let Some(expected) = self.point::<DraftMarkerSealsFamily>(
            store,
            key,
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )?
        else {
            return Err(DraftMarkerSealErrorV1::MissingSeal);
        };
        validate_record(&expected)?;
        if expected.key != key {
            return Err(DraftMarkerSealErrorV1::IdentityCollision);
        }
        validate_key_source(self, store, key)?;
        validate_cursor_closure(self, store, &expected)?;
        let mut next = expected.clone();
        match expected.lifecycle {
            DraftMarkerSealLifecycleV1::Open => {
                next.lifecycle = DraftMarkerSealLifecycleV1::Cancelled;
                next.record_digest = seal_record_digest(&next);
            }
            DraftMarkerSealLifecycleV1::Cancelled => {}
            _ => return Err(DraftMarkerSealErrorV1::IdentityCollision),
        }
        Ok(PreparedDraftMarkerSealCancelV1 { expected, next })
    }

    pub fn cancel_draft_marker_seal(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMarkerSealCancelV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, CancelMutation { prepared })
    }

    pub fn prepare_draft_marker_seal_fail(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
        reason: DraftMarkerSealFailureReasonV1,
    ) -> Result<PreparedDraftMarkerSealFailV1, DraftMarkerSealErrorV1> {
        let (expected, next) = prepare_terminal_transition(
            self,
            store,
            key,
            DraftMarkerSealLifecycleV1::Failed(reason),
        )?;
        Ok(PreparedDraftMarkerSealFailV1 { expected, next })
    }

    pub fn fail_draft_marker_seal(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMarkerSealFailV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, FailMutation { prepared })
    }

    pub fn prepare_draft_marker_seal_supersede(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
        successor: DraftMarkerSealOperationIdV1,
    ) -> Result<PreparedDraftMarkerSealSupersedeV1, DraftMarkerSealErrorV1> {
        if successor == key.operation_id {
            return Err(DraftMarkerSealErrorV1::IdentityCollision);
        }
        let (expected, next) = prepare_terminal_transition(
            self,
            store,
            key,
            DraftMarkerSealLifecycleV1::Superseded(successor),
        )?;
        Ok(PreparedDraftMarkerSealSupersedeV1 { expected, next })
    }

    pub fn supersede_draft_marker_seal(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMarkerSealSupersedeV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, SupersedeMutation { prepared })
    }

    pub fn draft_marker_seal_status(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
    ) -> Result<DraftMarkerSealStatusV1, DraftMarkerSealErrorV1> {
        let Some(record) = self.point::<DraftMarkerSealsFamily>(
            store,
            key,
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )?
        else {
            return Ok(DraftMarkerSealStatusV1::Absent);
        };
        validate_record(&record)?;
        if record.key != key {
            return Err(DraftMarkerSealErrorV1::IdentityCollision);
        }
        let source = validate_key_source(self, store, key)?;
        validate_cursor_closure(self, store, &record)?;
        let release = release_for(&record);
        Ok(match record.lifecycle {
            DraftMarkerSealLifecycleV1::Open => DraftMarkerSealStatusV1::Open {
                completed_marker_count: record.completed_marker_count,
            },
            DraftMarkerSealLifecycleV1::Cancelled => DraftMarkerSealStatusV1::Cancelled(release),
            DraftMarkerSealLifecycleV1::Failed(reason) => {
                DraftMarkerSealStatusV1::Failed { reason, release }
            }
            DraftMarkerSealLifecycleV1::Superseded(successor) => {
                DraftMarkerSealStatusV1::Superseded { successor, release }
            }
            DraftMarkerSealLifecycleV1::Sealed {
                sequential,
                ordered_assets,
            } => {
                if !matches!(record.cursor, DraftMarkerSealCursorV1::Eof(_))
                    || sequential.marker_digest() != record.sequential_digest
                    || sequential.marker_count() != record.completed_marker_count
                    || sequential.maximum_image_label() != record.maximum_image_label
                    || ordered_assets.marker_asset_digest() != record.ordered_asset_digest
                    || ordered_assets.marker_count() != record.completed_marker_count
                    || sequential.marker_count() != key.commitment.marker_count()
                    || sequential.maximum_image_label() != key.commitment.maximum_image_label()
                {
                    return Err(DraftMarkerSealErrorV1::Corruption);
                }
                DraftMarkerSealStatusV1::Sealed(
                    DraftMarkerSealProofV1::new_authenticated(
                        source,
                        key.commitment,
                        sequential,
                        ordered_assets,
                    ),
                    release,
                )
            }
        })
    }
}

fn prepare_terminal_transition(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftMarkerSealKeyV1,
    lifecycle: DraftMarkerSealLifecycleV1,
) -> Result<(DraftMarkerSealRecordV1, DraftMarkerSealRecordV1), DraftMarkerSealErrorV1> {
    let Some(expected) = storage.point::<DraftMarkerSealsFamily>(
        store,
        key,
        storage_point_limit::<DraftMarkerSealsFamily>(),
    )?
    else {
        return Err(DraftMarkerSealErrorV1::MissingSeal);
    };
    validate_record(&expected)?;
    if expected.key != key {
        return Err(DraftMarkerSealErrorV1::IdentityCollision);
    }
    validate_key_source(storage, store, key)?;
    validate_cursor_closure(storage, store, &expected)?;
    let mut next = expected.clone();
    if expected.lifecycle == lifecycle {
        return Ok((expected, next));
    }
    if !matches!(expected.lifecycle, DraftMarkerSealLifecycleV1::Open) {
        return Err(DraftMarkerSealErrorV1::IdentityCollision);
    }
    next.lifecycle = lifecycle;
    next.record_digest = seal_record_digest(&next);
    Ok((expected, next))
}

#[cfg(feature = "test-faults")]
pub fn inject_draft_marker_seal_natural_identity_collision_for_test(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftMarkerSealKeyV1,
    colliding_operation_id: DraftMarkerSealOperationIdV1,
) -> (DraftMarkerSealKeyV1, MutationContribution) {
    assert_ne!(key.operation_id, colliding_operation_id);
    let record = storage
        .point::<DraftMarkerSealsFamily>(
            store,
            key,
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )
        .expect("marker seal collision fixture reads")
        .expect("marker seal collision fixture record exists");
    let physical_key = DraftMarkerSealKeyV1 {
        operation_id: colliding_operation_id,
        ..key
    };
    let contribution = storage.handle.contribution(
        storage
            .revision(store)
            .expect("marker seal collision fixture revision reads"),
        MarkerSealCollisionMutation {
            physical_key,
            record,
        },
    );
    (physical_key, contribution)
}

#[cfg(feature = "test-faults")]
pub fn inject_draft_marker_seal_record_corruption_for_test(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftMarkerSealKeyV1,
) {
    let record = storage
        .point::<DraftMarkerSealsFamily>(
            store,
            key,
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )
        .expect("marker seal corruption fixture reads")
        .expect("marker seal corruption fixture record exists");
    let encoded_key = <DraftMarkerSealsCodec as RecordCodec<SyndicDomain>>::encode_key(&key)
        .expect("marker seal corruption fixture key encodes");
    let mut encoded_value =
        <DraftMarkerSealsCodec as RecordCodec<SyndicDomain>>::encode_value(&record)
            .expect("marker seal corruption fixture value encodes");
    *encoded_value
        .last_mut()
        .expect("marker seal corruption fixture value is nonempty") ^= 0x80;
    store
        .inject_persisted_corrupt_record::<SyndicDomain, DraftMarkerSealsCodec>(
            &storage.handle,
            &encoded_key,
            &encoded_value,
        )
        .expect("marker seal corruption fixture persists");
}

fn close_exhausted_cursor(
    storage: &SyndicStorage,
    store: &HomeStore,
    record: &mut DraftMarkerSealRecordV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    let DraftMarkerSealCursorV1::Positioned(frames) = &record.cursor else {
        return Ok(());
    };
    for frame in frames {
        let node = load_marker_record(
            storage,
            store,
            record.key.root_key.draft_id(),
            DraftMarkerOrderRecordKindV1::Internal,
            frame.record_id,
        )?;
        validate_internal(&node, frame.digest, frame.height)?;
        let children = node.children().ok_or(DraftMarkerSealErrorV1::Corruption)?;
        if usize::from(frame.next_child_index) < children.len() {
            return Ok(());
        }
        if usize::from(frame.next_child_index) > children.len() {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
    }
    let terminal_frames = frames.clone();
    record.cursor = DraftMarkerSealCursorV1::Eof(terminal_frames);
    Ok(())
}

fn release_for(record: &DraftMarkerSealRecordV1) -> DraftMarkerSealCustodyReleaseV1 {
    DraftMarkerSealCustodyReleaseV1 {
        key: record.key,
        completed_marker_count: record.completed_marker_count,
    }
}

fn validate_source(
    storage: &SyndicStorage,
    store: &HomeStore,
    source: DraftPieceRootReferenceV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    let Some(root) = storage.point::<DraftPieceRootsFamily>(
        store,
        source.key(),
        storage_point_limit::<DraftPieceRootsFamily>(),
    )?
    else {
        return Err(DraftMarkerSealErrorV1::MissingSource);
    };
    if root.reference() != source {
        return Err(DraftMarkerSealErrorV1::IdentityCollision);
    }
    let commitment = source.marker_commitment();
    if (commitment.marker_count() == 0) != source.marker_order_root().is_none()
        || commitment.marker_count() != source.summary().marker_count()
    {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(())
}

fn validate_key_source(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftMarkerSealKeyV1,
) -> Result<DraftPieceRootReferenceV1, DraftMarkerSealErrorV1> {
    let Some(root) = storage.point::<DraftPieceRootsFamily>(
        store,
        key.root_key,
        storage_point_limit::<DraftPieceRootsFamily>(),
    )?
    else {
        return Err(DraftMarkerSealErrorV1::MissingSource);
    };
    let source = root.reference();
    if source.combined_digest() != key.combined_digest
        || source.marker_order_root() != key.marker_order_root
        || source.marker_commitment() != key.commitment
    {
        return Err(DraftMarkerSealErrorV1::IdentityCollision);
    }
    validate_source(storage, store, source)?;
    Ok(source)
}

fn source_marker_order_height(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftMarkerSealKeyV1,
) -> Result<u8, DraftMarkerSealErrorV1> {
    let source = validate_key_source(storage, store, key)?;
    let height = source.marker_order_height();
    if height == 0 || height > DRAFT_PIECE_MAX_HEIGHT {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(height)
}

fn next_marker(
    storage: &SyndicStorage,
    store: &HomeStore,
    record: &mut DraftMarkerSealRecordV1,
) -> Result<Option<DraftMarkerSealFrontierV1>, DraftMarkerSealErrorV1> {
    match &record.cursor {
        DraftMarkerSealCursorV1::Eof(_) => return Ok(None),
        DraftMarkerSealCursorV1::BeforeRoot => {
            let Some(root_id) = record.key.marker_order_root else {
                record.cursor = DraftMarkerSealCursorV1::Eof(Vec::new());
                return Ok(None);
            };
            let expected_height = source_marker_order_height(storage, store, record.key)?;
            let (frontier, frames) = descend_to_leaf(
                storage,
                store,
                record.key.root_key.draft_id(),
                root_id,
                DraftPieceDigestV1::from_bytes(record.key.commitment.tree_root_digest()),
                expected_height,
                Vec::new(),
            )?;
            record.cursor = if frames.is_empty() {
                DraftMarkerSealCursorV1::Eof(Vec::new())
            } else {
                DraftMarkerSealCursorV1::Positioned(frames)
            };
            return Ok(Some(frontier));
        }
        DraftMarkerSealCursorV1::Positioned(_) => {}
    }

    let DraftMarkerSealCursorV1::Positioned(mut frames) = record.cursor.clone() else {
        unreachable!()
    };
    let terminal_frames = frames.clone();
    while let Some(frame) = frames.last_mut() {
        let node = load_marker_record(
            storage,
            store,
            record.key.root_key.draft_id(),
            DraftMarkerOrderRecordKindV1::Internal,
            frame.record_id,
        )?;
        validate_internal(&node, frame.digest, frame.height)?;
        let children = node.children().ok_or(DraftMarkerSealErrorV1::Corruption)?;
        let index = usize::from(frame.next_child_index);
        if index < children.len() {
            frame.next_child_index = frame
                .next_child_index
                .checked_add(1)
                .ok_or(DraftMarkerSealErrorV1::Corruption)?;
            let child = children[index];
            let expected_height = frame
                .height
                .checked_sub(1)
                .ok_or(DraftMarkerSealErrorV1::Corruption)?;
            let (frontier, next_frames) = descend_to_leaf(
                storage,
                store,
                record.key.root_key.draft_id(),
                child.id(),
                child.digest(),
                expected_height,
                frames,
            )?;
            record.cursor = if next_frames.is_empty() {
                DraftMarkerSealCursorV1::Eof(Vec::new())
            } else {
                DraftMarkerSealCursorV1::Positioned(next_frames)
            };
            return Ok(Some(frontier));
        }
        frames.pop();
    }
    record.cursor = DraftMarkerSealCursorV1::Eof(terminal_frames);
    Ok(None)
}

fn descend_to_leaf(
    storage: &SyndicStorage,
    store: &HomeStore,
    draft_id: SyndicDraftId,
    mut record_id: DraftPieceRecordIdV1,
    mut expected_digest: DraftPieceDigestV1,
    mut expected_height: u8,
    mut frames: Vec<DraftMarkerSealCursorFrameV1>,
) -> Result<(DraftMarkerSealFrontierV1, Vec<DraftMarkerSealCursorFrameV1>), DraftMarkerSealErrorV1>
{
    loop {
        if frames.len() >= usize::from(DRAFT_PIECE_MAX_HEIGHT) {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        let kind = if expected_height == 0 {
            DraftMarkerOrderRecordKindV1::Leaf
        } else {
            DraftMarkerOrderRecordKindV1::Internal
        };
        let record = load_marker_record(storage, store, draft_id, kind, record_id)?;
        if expected_height == 0 {
            validate_leaf(&record, expected_digest)?;
            let (marker_id, label, asset_id) =
                record.marker().ok_or(DraftMarkerSealErrorV1::Corruption)?;
            return Ok((
                DraftMarkerSealFrontierV1 {
                    record_id,
                    digest: expected_digest,
                    marker_id,
                    label,
                    asset_id,
                },
                frames,
            ));
        }
        validate_internal(&record, expected_digest, expected_height)?;
        let children = record
            .children()
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        let child = *children.first().ok_or(DraftMarkerSealErrorV1::Corruption)?;
        frames.push(DraftMarkerSealCursorFrameV1 {
            record_id,
            digest: expected_digest,
            height: expected_height,
            next_child_index: 1,
        });
        record_id = child.id();
        expected_digest = child.digest();
        expected_height = expected_height
            .checked_sub(1)
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
    }
}

fn load_marker_record(
    storage: &SyndicStorage,
    store: &HomeStore,
    draft_id: SyndicDraftId,
    kind: DraftMarkerOrderRecordKindV1,
    record_id: DraftPieceRecordIdV1,
) -> Result<DraftMarkerOrderRecordV1, DraftMarkerSealErrorV1> {
    storage
        .point::<DraftMarkerOrderCommitmentsFamily>(
            store,
            DraftMarkerOrderRecordKeyV1::new(draft_id, kind, record_id),
            storage_point_limit::<DraftMarkerOrderCommitmentsFamily>(),
        )?
        .ok_or(DraftMarkerSealErrorV1::Corruption)
}

fn validate_internal(
    record: &DraftMarkerOrderRecordV1,
    expected_digest: DraftPieceDigestV1,
    expected_height: u8,
) -> Result<(), DraftMarkerSealErrorV1> {
    let children = record
        .children()
        .ok_or(DraftMarkerSealErrorV1::Corruption)?;
    if expected_height == 0
        || record.height() != expected_height
        || record.digest() != expected_digest
        || children.is_empty()
        || children.len() > DRAFT_PIECE_MAX_CHILDREN
        || marker_order_node_digest(expected_height, children) != expected_digest
    {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    let mut count = 0u64;
    let mut maximum = None;
    for child in children {
        count = count
            .checked_add(child.marker_count())
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        let child_maximum = child
            .maximum_image_label()
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        maximum = Some(maximum.map_or(child_maximum, |value: ImageLabelOrdinal| {
            value.max(child_maximum)
        }));
    }
    if count == 0 || maximum.is_none() {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(())
}

fn validate_leaf(
    record: &DraftMarkerOrderRecordV1,
    expected_digest: DraftPieceDigestV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    let (marker_id, label, asset_id) = record.marker().ok_or(DraftMarkerSealErrorV1::Corruption)?;
    if record.height() != 0
        || record.digest() != expected_digest
        || marker_order_leaf_digest(marker_id, label, asset_id) != expected_digest
    {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(())
}

fn validate_cursor_closure(
    storage: &SyndicStorage,
    store: &HomeStore,
    record: &DraftMarkerSealRecordV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    validate_record(record)?;
    match &record.cursor {
        DraftMarkerSealCursorV1::BeforeRoot => {
            if record.frontier.is_some() || record.completed_marker_count != 0 {
                return Err(DraftMarkerSealErrorV1::Corruption);
            }
        }
        DraftMarkerSealCursorV1::Eof(frames) if record.completed_marker_count == 0 => {
            if !frames.is_empty()
                || record.key.marker_order_root.is_some()
                || record.frontier.is_some()
            {
                return Err(DraftMarkerSealErrorV1::Corruption);
            }
        }
        DraftMarkerSealCursorV1::Eof(frames) => {
            validate_cursor_path(storage, store, record, frames, true)?;
        }
        DraftMarkerSealCursorV1::Positioned(frames) => {
            validate_cursor_path(storage, store, record, frames, false)?;
        }
    }
    Ok(())
}

fn validate_cursor_path(
    storage: &SyndicStorage,
    store: &HomeStore,
    record: &DraftMarkerSealRecordV1,
    frames: &[DraftMarkerSealCursorFrameV1],
    require_eof: bool,
) -> Result<(), DraftMarkerSealErrorV1> {
    if frames.is_empty() {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    if frames.len() > usize::from(DRAFT_PIECE_MAX_HEIGHT) {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    let mut selected_child = None;
    for (index, frame) in frames.iter().enumerate() {
        let node = load_marker_record(
            storage,
            store,
            record.key.root_key.draft_id(),
            DraftMarkerOrderRecordKindV1::Internal,
            frame.record_id,
        )?;
        validate_internal(&node, frame.digest, frame.height)?;
        if index == 0
            && (Some(frame.record_id) != record.key.marker_order_root
                || frame.digest
                    != DraftPieceDigestV1::from_bytes(record.key.commitment.tree_root_digest()))
        {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        let children = node.children().ok_or(DraftMarkerSealErrorV1::Corruption)?;
        if require_eof && usize::from(frame.next_child_index) != children.len() {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        let selected_index = usize::from(frame.next_child_index)
            .checked_sub(1)
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        let child = *children
            .get(selected_index)
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        if let Some((id, digest, height)) = selected_child
            && (id != frame.record_id || digest != frame.digest || height != frame.height)
        {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        selected_child = Some((
            child.id(),
            child.digest(),
            frame
                .height
                .checked_sub(1)
                .ok_or(DraftMarkerSealErrorV1::Corruption)?,
        ));
    }
    let frontier = record.frontier.ok_or(DraftMarkerSealErrorV1::Corruption)?;
    if selected_child != Some((frontier.record_id, frontier.digest, 0)) {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    validate_frontier_leaf(storage, store, record)
}

fn validate_frontier_leaf(
    storage: &SyndicStorage,
    store: &HomeStore,
    record: &DraftMarkerSealRecordV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    let frontier = record.frontier.ok_or(DraftMarkerSealErrorV1::Corruption)?;
    let leaf = load_marker_record(
        storage,
        store,
        record.key.root_key.draft_id(),
        DraftMarkerOrderRecordKindV1::Leaf,
        frontier.record_id,
    )?;
    validate_leaf(&leaf, frontier.digest)?;
    if leaf.marker() != Some((frontier.marker_id, frontier.label, frontier.asset_id)) {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(())
}

fn validate_record(record: &DraftMarkerSealRecordV1) -> Result<(), DraftMarkerSealErrorV1> {
    if record.record_digest != seal_record_digest(record)
        || record.completed_marker_count > record.key.commitment.marker_count()
        || (record.completed_marker_count == 0) != record.maximum_image_label.is_none()
        || (record.completed_marker_count == 0) != record.frontier.is_none()
        || record.completed_marker_count == 0
            && (record.sequential_digest != sequential_marker_digest_seed()
                || record.ordered_asset_digest != ordered_marker_asset_digest_seed())
        || matches!(record.cursor, DraftMarkerSealCursorV1::BeforeRoot)
            && record.completed_marker_count != 0
        || matches!(
            record.lifecycle,
            DraftMarkerSealLifecycleV1::Superseded(successor)
                if successor == record.key.operation_id
        )
        || matches!(record.lifecycle, DraftMarkerSealLifecycleV1::Sealed { .. })
            != matches!(record.cursor, DraftMarkerSealCursorV1::Eof(_))
        || matches!(
            record.lifecycle,
            DraftMarkerSealLifecycleV1::Sealed {
                sequential,
                ordered_assets,
            } if sequential.marker_digest() != record.sequential_digest
                || sequential.marker_count() != record.completed_marker_count
                || sequential.maximum_image_label() != record.maximum_image_label
                || ordered_assets.marker_asset_digest() != record.ordered_asset_digest
                || ordered_assets.marker_count() != record.completed_marker_count
        )
    {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(())
}

impl DomainMutation<SyndicDomain> for BeginMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let source = point::<DraftPieceRootsFamily>(reader, &self.prepared.source.key())?.ok_or(
            SyndicMutationError::RequiredRecordMissing {
                family: "draft-piece-roots",
            },
        )?;
        if source.reference() != self.prepared.source {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if let Some(existing) = point::<DraftMarkerSealsFamily>(reader, &self.prepared.initial.key)?
        {
            if validate_record(&existing).is_err() || existing.key != self.prepared.initial.key {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if point::<DraftMarkerSealsFamily>(reader, &self.prepared.initial.key)?.is_none() {
            mutations
                .put::<DraftMarkerSealsCodec>(&self.prepared.initial.key, &self.prepared.initial)?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for AdvanceMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let current = point::<DraftMarkerSealsFamily>(reader, &self.prepared.expected.key)?.ok_or(
            SyndicMutationError::RequiredRecordMissing {
                family: "draft-marker-seals",
            },
        )?;
        if current != self.prepared.expected && current != self.prepared.next {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if validate_record(&current).is_err()
            || validate_record(&self.prepared.expected).is_err()
            || validate_record(&self.prepared.next).is_err()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let current = point::<DraftMarkerSealsFamily>(reader, &self.prepared.expected.key)?.ok_or(
            SyndicMutationError::RequiredRecordMissing {
                family: "draft-marker-seals",
            },
        )?;
        if current == self.prepared.expected {
            mutations.put::<DraftMarkerSealsCodec>(&self.prepared.next.key, &self.prepared.next)?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for CancelMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        validate_terminal_mutation(reader, &self.prepared.expected, &self.prepared.next)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        contribute_terminal_mutation(
            reader,
            mutations,
            &self.prepared.expected,
            &self.prepared.next,
        )
    }
}

impl DomainMutation<SyndicDomain> for FailMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        validate_terminal_mutation(reader, &self.prepared.expected, &self.prepared.next)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        contribute_terminal_mutation(
            reader,
            mutations,
            &self.prepared.expected,
            &self.prepared.next,
        )
    }
}

impl DomainMutation<SyndicDomain> for SupersedeMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        validate_terminal_mutation(reader, &self.prepared.expected, &self.prepared.next)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        contribute_terminal_mutation(
            reader,
            mutations,
            &self.prepared.expected,
            &self.prepared.next,
        )
    }
}

#[cfg(feature = "test-faults")]
impl DomainMutation<SyndicDomain> for MarkerSealCollisionMutation {
    type Error = SyndicMutationError;

    fn validate(&self, _: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<DraftMarkerSealsCodec>(&self.physical_key, &self.record)?;
        Ok(())
    }
}

fn validate_terminal_mutation(
    reader: &DomainReader<'_, SyndicDomain>,
    expected: &DraftMarkerSealRecordV1,
    next: &DraftMarkerSealRecordV1,
) -> Result<(), SyndicMutationError> {
    let current = point::<DraftMarkerSealsFamily>(reader, &expected.key)?.ok_or(
        SyndicMutationError::RequiredRecordMissing {
            family: "draft-marker-seals",
        },
    )?;
    if expected.key != next.key
        || current != *expected && current != *next
        || validate_record(&current).is_err()
        || validate_record(expected).is_err()
        || validate_record(next).is_err()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

fn contribute_terminal_mutation(
    reader: &DomainReader<'_, SyndicDomain>,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    expected: &DraftMarkerSealRecordV1,
    next: &DraftMarkerSealRecordV1,
) -> Result<(), SyndicMutationError> {
    let current = point::<DraftMarkerSealsFamily>(reader, &expected.key)?.ok_or(
        SyndicMutationError::RequiredRecordMissing {
            family: "draft-marker-seals",
        },
    )?;
    if current == *expected && expected != next {
        mutations.put::<DraftMarkerSealsCodec>(&next.key, next)?;
    }
    Ok(())
}

fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, SyndicMutationError> {
    reader
        .point::<ExactCodec<F>>(key, family_point_limit::<F>())
        .map_err(SyndicMutationError::from)
}

fn storage_point_limit<F: Family>() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(family_point_limit::<F>().max_bytes())
        .expect("marker seal point-read limit is nonzero")
}

fn encode_key(key: &DraftMarkerSealKeyV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_root_key(&mut e, key.root_key);
    e.fixed32(key.combined_digest.as_bytes());
    enc_record_id_option(&mut e, key.marker_order_root);
    enc_commitment(&mut e, key.commitment);
    e.fixed16(key.operation_id.as_bytes());
    e.finish()
}

fn decode_key(encoded: &[u8]) -> Result<DraftMarkerSealKeyV1, CodecError> {
    let mut d = Decoder::new(encoded);
    let key = DraftMarkerSealKeyV1 {
        root_key: dec_root_key(&mut d)?,
        combined_digest: DraftPieceDigestV1::from_bytes(d.fixed32()?),
        marker_order_root: dec_record_id_option(&mut d)?,
        commitment: dec_commitment(&mut d)?,
        operation_id: DraftMarkerSealOperationIdV1::from_bytes(d.fixed16()?),
    };
    d.finish()?;
    if (key.commitment.marker_count() == 0) != key.marker_order_root.is_none() {
        return Err(CodecError::InvalidLength("draft marker seal root"));
    }
    Ok(key)
}

fn encode_record(record: &DraftMarkerSealRecordV1) -> Result<Vec<u8>, CodecError> {
    validate_record(record).map_err(|_| CodecError::InvalidLength("draft marker seal record"))?;
    let mut e = Encoder::new();
    e.bytes(&encode_key(&record.key));
    enc_cursor(&mut e, &record.cursor);
    enc_frontier(&mut e, record.frontier);
    e.fixed32(&record.sequential_digest);
    e.fixed32(&record.ordered_asset_digest);
    e.u64(record.completed_marker_count);
    enc_label(&mut e, record.maximum_image_label);
    enc_lifecycle(&mut e, record.lifecycle);
    e.fixed32(&record.record_digest);
    Ok(e.finish())
}

fn decode_record(encoded: &[u8]) -> Result<DraftMarkerSealRecordV1, CodecError> {
    let mut d = Decoder::new(encoded);
    let key = decode_key(d.bytes("draft marker seal key")?)?;
    let record = DraftMarkerSealRecordV1 {
        key,
        cursor: dec_cursor(&mut d)?,
        frontier: dec_frontier(&mut d)?,
        sequential_digest: d.fixed32()?,
        ordered_asset_digest: d.fixed32()?,
        completed_marker_count: d.u64()?,
        maximum_image_label: dec_label(&mut d)?,
        lifecycle: dec_lifecycle(&mut d)?,
        record_digest: d.fixed32()?,
    };
    d.finish()?;
    validate_record(&record).map_err(|_| CodecError::InvalidLength("draft marker seal record"))?;
    Ok(record)
}

fn enc_root_key(e: &mut Encoder, key: DraftPieceRootKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    match key.build_identity() {
        DraftPieceRootBuildIdentityV1::DirectCanonicalEmpty { operation_id } => {
            e.u8(1);
            e.fixed16(operation_id.as_bytes());
        }
        DraftPieceRootBuildIdentityV1::EditorCandidate {
            session_id,
            operation_id,
        } => {
            e.u8(2);
            e.fixed16(session_id.as_bytes());
            e.fixed16(operation_id.as_bytes());
        }
    }
}

fn dec_root_key(d: &mut Decoder<'_>) -> Result<DraftPieceRootKeyV1, CodecError> {
    let draft_id = SyndicDraftId::from_bytes(d.fixed16()?);
    match d.u8()? {
        1 => Ok(DraftPieceRootKeyV1::direct_canonical_empty(
            draft_id,
            DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
        )),
        2 => Ok(DraftPieceRootKeyV1::editor_candidate(
            draft_id,
            DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
            DraftPieceOperationIdV1::from_bytes(d.fixed16()?),
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "draft marker seal build identity",
            tag,
        }),
    }
}

fn enc_record_id_option(e: &mut Encoder, value: Option<DraftPieceRecordIdV1>) {
    match value {
        None => e.u8(0),
        Some(value) => {
            e.u8(1);
            e.fixed16(value.as_bytes());
        }
    }
}

fn dec_record_id_option(d: &mut Decoder<'_>) -> Result<Option<DraftPieceRecordIdV1>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(DraftPieceRecordIdV1::from_bytes(d.fixed16()?))),
        tag => Err(CodecError::InvalidTag {
            kind: "draft marker seal record identity option",
            tag,
        }),
    }
}

fn enc_commitment(e: &mut Encoder, value: DraftMarkerCommitmentV1) {
    e.fixed32(&value.tree_root_digest());
    e.u64(value.marker_count());
    enc_label(e, value.maximum_image_label());
}

fn dec_commitment(d: &mut Decoder<'_>) -> Result<DraftMarkerCommitmentV1, CodecError> {
    DraftMarkerCommitmentV1::new(d.fixed32()?, d.u64()?, dec_label(d)?)
        .map_err(|_| CodecError::InvalidLength("draft marker commitment"))
}

fn enc_label(e: &mut Encoder, value: Option<ImageLabelOrdinal>) {
    e.u64(value.map_or(0, ImageLabelOrdinal::get));
}

fn dec_label(d: &mut Decoder<'_>) -> Result<Option<ImageLabelOrdinal>, CodecError> {
    let value = d.u64()?;
    if value == 0 {
        Ok(None)
    } else {
        ImageLabelOrdinal::new(value)
            .map(Some)
            .map_err(|_| CodecError::InvalidLength("draft marker seal label"))
    }
}

fn enc_asset_id(e: &mut Encoder, asset_id: AssetId) {
    e.u8(asset_id.version() as u8);
    e.fixed32(&asset_id.digest());
    e.u64(asset_id.length().get());
}

fn dec_asset_id(d: &mut Decoder<'_>) -> Result<AssetId, CodecError> {
    match d.u8()? {
        1 => Ok(AssetId::sha256_v1(
            d.fixed32()?,
            NonZeroU64::new(d.u64()?)
                .ok_or(CodecError::InvalidLength("draft marker seal asset length"))?,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "draft marker seal asset identity",
            tag,
        }),
    }
}

fn enc_cursor(e: &mut Encoder, cursor: &DraftMarkerSealCursorV1) {
    match cursor {
        DraftMarkerSealCursorV1::BeforeRoot => e.u8(1),
        DraftMarkerSealCursorV1::Positioned(frames) => {
            e.u8(2);
            enc_cursor_frames(e, frames);
        }
        DraftMarkerSealCursorV1::Eof(frames) => {
            e.u8(3);
            enc_cursor_frames(e, frames);
        }
    }
}

fn enc_cursor_frames(e: &mut Encoder, frames: &[DraftMarkerSealCursorFrameV1]) {
    e.u8(u8::try_from(frames.len()).expect("cursor height is bounded"));
    for frame in frames {
        e.fixed16(frame.record_id.as_bytes());
        e.fixed32(frame.digest.as_bytes());
        e.u8(frame.height);
        e.u8(frame.next_child_index);
    }
}

fn dec_cursor(d: &mut Decoder<'_>) -> Result<DraftMarkerSealCursorV1, CodecError> {
    match d.u8()? {
        1 => Ok(DraftMarkerSealCursorV1::BeforeRoot),
        2 => {
            let frames = dec_cursor_frames(d)?;
            if frames.is_empty() {
                return Err(CodecError::InvalidLength("draft marker seal cursor"));
            }
            Ok(DraftMarkerSealCursorV1::Positioned(frames))
        }
        3 => Ok(DraftMarkerSealCursorV1::Eof(dec_cursor_frames(d)?)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft marker seal cursor",
            tag,
        }),
    }
}

fn dec_cursor_frames(d: &mut Decoder<'_>) -> Result<Vec<DraftMarkerSealCursorFrameV1>, CodecError> {
    let count = usize::from(d.u8()?);
    if count > usize::from(DRAFT_PIECE_MAX_HEIGHT) {
        return Err(CodecError::InvalidLength("draft marker seal cursor"));
    }
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        frames.push(DraftMarkerSealCursorFrameV1 {
            record_id: DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
            digest: DraftPieceDigestV1::from_bytes(d.fixed32()?),
            height: d.u8()?,
            next_child_index: d.u8()?,
        });
    }
    Ok(frames)
}

fn enc_frontier(e: &mut Encoder, frontier: Option<DraftMarkerSealFrontierV1>) {
    match frontier {
        None => e.u8(0),
        Some(frontier) => {
            e.u8(1);
            e.fixed16(frontier.record_id.as_bytes());
            e.fixed32(frontier.digest.as_bytes());
            e.fixed16(frontier.marker_id.as_bytes());
            e.u64(frontier.label.get());
            enc_asset_id(e, frontier.asset_id);
        }
    }
}

fn dec_frontier(d: &mut Decoder<'_>) -> Result<Option<DraftMarkerSealFrontierV1>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(DraftMarkerSealFrontierV1 {
            record_id: DraftPieceRecordIdV1::from_bytes(d.fixed16()?),
            digest: DraftPieceDigestV1::from_bytes(d.fixed32()?),
            marker_id: SyndicDraftMarkerId::from_bytes(d.fixed16()?),
            label: ImageLabelOrdinal::new(d.u64()?)
                .map_err(|_| CodecError::InvalidLength("draft marker seal frontier label"))?,
            asset_id: dec_asset_id(d)?,
        })),
        tag => Err(CodecError::InvalidTag {
            kind: "draft marker seal frontier",
            tag,
        }),
    }
}

fn enc_lifecycle(e: &mut Encoder, lifecycle: DraftMarkerSealLifecycleV1) {
    match lifecycle {
        DraftMarkerSealLifecycleV1::Open => e.u8(1),
        DraftMarkerSealLifecycleV1::Cancelled => e.u8(2),
        DraftMarkerSealLifecycleV1::Failed(DraftMarkerSealFailureReasonV1::Operational) => e.u8(3),
        DraftMarkerSealLifecycleV1::Superseded(successor) => {
            e.u8(4);
            e.fixed16(successor.as_bytes());
        }
        DraftMarkerSealLifecycleV1::Sealed {
            sequential,
            ordered_assets,
        } => {
            e.u8(5);
            e.fixed32(&sequential.marker_digest());
            e.u64(sequential.marker_count());
            enc_label(e, sequential.maximum_image_label());
            e.fixed32(&ordered_assets.marker_asset_digest());
            e.u64(ordered_assets.marker_count());
        }
    }
}

fn dec_lifecycle(d: &mut Decoder<'_>) -> Result<DraftMarkerSealLifecycleV1, CodecError> {
    match d.u8()? {
        1 => Ok(DraftMarkerSealLifecycleV1::Open),
        2 => Ok(DraftMarkerSealLifecycleV1::Cancelled),
        3 => Ok(DraftMarkerSealLifecycleV1::Failed(
            DraftMarkerSealFailureReasonV1::Operational,
        )),
        4 => Ok(DraftMarkerSealLifecycleV1::Superseded(
            DraftMarkerSealOperationIdV1::from_bytes(d.fixed16()?),
        )),
        5 => Ok(DraftMarkerSealLifecycleV1::Sealed {
            sequential: SequentialMarkerSummaryV1::new(d.fixed32()?, d.u64()?, dec_label(d)?)
                .map_err(|_| CodecError::InvalidLength("draft marker seal summary"))?,
            ordered_assets: OrderedMarkerAssetSummaryV1::new(d.fixed32()?, d.u64()?),
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "draft marker seal lifecycle",
            tag,
        }),
    }
}

fn seal_record_digest(record: &DraftMarkerSealRecordV1) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(SEAL_RECORD_DIGEST_DOMAIN);
    hash.update(encode_key(&record.key));
    let mut e = Encoder::new();
    enc_cursor(&mut e, &record.cursor);
    enc_frontier(&mut e, record.frontier);
    e.fixed32(&record.sequential_digest);
    e.fixed32(&record.ordered_asset_digest);
    e.u64(record.completed_marker_count);
    enc_label(&mut e, record.maximum_image_label);
    enc_lifecycle(&mut e, record.lifecycle);
    hash.update(e.finish());
    hash.finalize().into()
}
