use std::{error::Error, fmt, num::NonZeroU64};

#[cfg(feature = "test-faults")]
use beryl_home_store::RecordCodec;
use beryl_home_store::{
    DomainMutation, DomainReader, HomeCandidateRecoveryAccess, HomeStore, MutationBuilder,
    MutationContribution, ReconciliationReservation, RecordVersion,
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
    read::access::ReadAccess,
};

use super::{
    DRAFT_PIECE_MAX_CHILDREN, DRAFT_PIECE_MAX_HEIGHT, DraftEditorCandidateSessionIdV1,
    DraftMarkerOrderCommitmentsFamily, DraftMarkerOrderRecordKeyV1, DraftMarkerOrderRecordKindV1,
    DraftMarkerOrderRecordV1, DraftPieceDigestV1, DraftPieceOperationIdV1, DraftPieceRecordIdV1,
    DraftPieceRootBuildIdentityV1, DraftPieceRootKeyV1, DraftPieceRootReferenceV1,
    DraftPieceRootsFamily, marker_order_leaf_digest, marker_order_node_digest,
};

mod preparation;
mod read;

use read::*;

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
    pub fn begin_draft_marker_seal(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMarkerSealBeginV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, BeginMutation { prepared })
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

    pub fn cancel_draft_marker_seal(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMarkerSealCancelV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, CancelMutation { prepared })
    }

    pub fn fail_draft_marker_seal(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMarkerSealFailV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, FailMutation { prepared })
    }

    pub fn supersede_draft_marker_seal(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMarkerSealSupersedeV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, SupersedeMutation { prepared })
    }
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
    type Prepared = Option<PreparedDraftMarkerSealBeginV1>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let source = point::<DraftPieceRootsFamily>(reader, &self.prepared.source.key())?.ok_or(
            SyndicMutationError::RequiredRecordMissing {
                family: "draft-piece-roots",
            },
        )?;
        if source.reference() != self.prepared.source {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let existing = point::<DraftMarkerSealsFamily>(reader, &self.prepared.initial.key)?;
        if let Some(existing) = &existing {
            if validate_record(&existing).is_err() || existing.key != self.prepared.initial.key {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        Ok(existing.is_none().then_some(self.prepared))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            mutations.put::<DraftMarkerSealsCodec>(&prepared.initial.key, &prepared.initial)?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for AdvanceMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<PreparedDraftMarkerSealAdvanceV1>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
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
        Ok((current == self.prepared.expected).then_some(self.prepared))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            mutations.put::<DraftMarkerSealsCodec>(&prepared.next.key, &prepared.next)?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for CancelMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<PreparedDraftMarkerSealCancelV1>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let current =
            validate_terminal_mutation(reader, &self.prepared.expected, &self.prepared.next)?;
        Ok(
            (current == self.prepared.expected && self.prepared.expected != self.prepared.next)
                .then_some(self.prepared),
        )
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            mutations.put::<DraftMarkerSealsCodec>(&prepared.next.key, &prepared.next)?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for FailMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<PreparedDraftMarkerSealFailV1>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let current =
            validate_terminal_mutation(reader, &self.prepared.expected, &self.prepared.next)?;
        Ok(
            (current == self.prepared.expected && self.prepared.expected != self.prepared.next)
                .then_some(self.prepared),
        )
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            mutations.put::<DraftMarkerSealsCodec>(&prepared.next.key, &prepared.next)?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for SupersedeMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<PreparedDraftMarkerSealSupersedeV1>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let current =
            validate_terminal_mutation(reader, &self.prepared.expected, &self.prepared.next)?;
        Ok(
            (current == self.prepared.expected && self.prepared.expected != self.prepared.next)
                .then_some(self.prepared),
        )
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            mutations.put::<DraftMarkerSealsCodec>(&prepared.next.key, &prepared.next)?;
        }
        Ok(())
    }
}

#[cfg(feature = "test-faults")]
impl DomainMutation<SyndicDomain> for MarkerSealCollisionMutation {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerSealsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<DraftMarkerSealsCodec>(&prepared.physical_key, &prepared.record)?;
        Ok(())
    }
}

fn validate_terminal_mutation(
    reader: &DomainReader<'_, SyndicDomain>,
    expected: &DraftMarkerSealRecordV1,
    next: &DraftMarkerSealRecordV1,
) -> Result<DraftMarkerSealRecordV1, SyndicMutationError> {
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
    Ok(current)
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
