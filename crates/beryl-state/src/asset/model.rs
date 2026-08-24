use std::num::NonZeroU64;

use beryl_model::{
    AssetId, AssetReferenceSetDigest, AssetReferenceSetId, DomainRevision, ImageLabelOrdinal,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    SyndicAcceptedInputId, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicProjectionId,
    SyndicRetryRecordId,
};

use crate::RecordRevision;

use super::{AssetValueError, MAX_MEDIA_TYPE_BYTES};

/// Validated media type retained with durable image metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetMediaType(Box<str>);

impl AssetMediaType {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AssetValueError> {
        let value = value.as_ref();
        let valid = !value.is_empty()
            && value.len() <= MAX_MEDIA_TYPE_BYTES
            && value.contains('/')
            && value.bytes().all(|byte| byte.is_ascii_graphic());
        valid
            .then(|| Self(value.into()))
            .ok_or(AssetValueError::InvalidMediaType)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated nonzero pixel dimensions, when bounded header parsing supplied them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetDimensions {
    width: NonZeroU64,
    height: NonZeroU64,
}

impl AssetDimensions {
    #[must_use]
    pub const fn new(width: NonZeroU64, height: NonZeroU64) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn width(self) -> NonZeroU64 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> NonZeroU64 {
        self.height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetSidecarState {
    Committed,
}

/// Immutable metadata for one home-wide content-addressed image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetMetadataRecord {
    pub(super) asset_id: AssetId,
    pub(super) media_type: AssetMediaType,
    pub(super) dimensions: Option<AssetDimensions>,
    pub(super) creation_revision: DomainRevision,
    pub(super) sidecar_state: AssetSidecarState,
    pub(super) revision: RecordRevision,
}

impl AssetMetadataRecord {
    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    #[must_use]
    pub const fn media_type(&self) -> &AssetMediaType {
        &self.media_type
    }

    #[must_use]
    pub const fn dimensions(&self) -> Option<AssetDimensions> {
        self.dimensions
    }

    #[must_use]
    pub const fn creation_revision(&self) -> DomainRevision {
        self.creation_revision
    }

    #[must_use]
    pub const fn sidecar_state(&self) -> AssetSidecarState {
        self.sidecar_state
    }

    #[must_use]
    pub const fn revision(&self) -> RecordRevision {
        self.revision
    }
}

/// Stable one-based position of one marker reference within a set.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetReferenceOrdinal(NonZeroU64);

impl AssetReferenceOrdinal {
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    pub fn new(value: u64) -> Result<Self, AssetValueError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(AssetValueError::ZeroReferenceOrdinal)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub fn checked_next(self) -> Result<Self, AssetValueError> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(AssetValueError::ReferenceOrdinalExhausted)
    }
}

/// Publication lifecycle of one owner-neutral reference-set build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetReferenceSetLifecycle {
    Building,
    Sealed,
}

/// Opaque authority for inspecting one unpublished reference-set build.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AssetReferenceSetStagingAuthority {
    pub(super) set_id: AssetReferenceSetId,
    pub(super) secret: [u8; 32],
}

impl AssetReferenceSetStagingAuthority {
    #[must_use]
    pub const fn new(set_id: AssetReferenceSetId, secret: [u8; 32]) -> Self {
        Self { set_id, secret }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetReferenceSetCompletion {
    Building(AssetReferenceSetManifest),
    Sealed(SealedAssetReferenceSetProof),
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetReferenceSetManifestCorruption {
    Lifecycle,
    Sequential,
    OrderedAssets,
    EntryFrontier,
    AssetChain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AssetReferenceSetCompletionEvidence {
    pub(super) set_id: AssetReferenceSetId,
    pub(super) authority_commitment: [u8; 32],
    pub(super) manifest_commitment: [u8; 32],
    pub(super) sealed_proof_commitment: Option<[u8; 32]>,
}

/// Compact manifest for one staged or sealed immutable marker-reference set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetReferenceSetManifest {
    pub(super) set_id: AssetReferenceSetId,
    pub(super) sequential: SequentialMarkerSummaryV1,
    pub(super) ordered_assets: OrderedMarkerAssetSummaryV1,
    pub(super) lifecycle: AssetReferenceSetLifecycle,
    pub(super) entry_frontier: u64,
    pub(super) asset_chain_digest: AssetReferenceSetDigest,
    pub(super) revision: RecordRevision,
}

impl AssetReferenceSetManifest {
    #[must_use]
    pub const fn set_id(&self) -> AssetReferenceSetId {
        self.set_id
    }

    #[must_use]
    pub const fn sequential(&self) -> SequentialMarkerSummaryV1 {
        self.sequential
    }

    #[must_use]
    pub const fn ordered_assets(&self) -> OrderedMarkerAssetSummaryV1 {
        self.ordered_assets
    }

    #[must_use]
    pub const fn lifecycle(&self) -> AssetReferenceSetLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn marker_count(&self) -> u64 {
        self.sequential.marker_count()
    }

    #[must_use]
    pub const fn marker_digest(&self) -> [u8; 32] {
        self.sequential.marker_digest()
    }

    #[must_use]
    pub const fn maximum_image_label(&self) -> Option<ImageLabelOrdinal> {
        self.sequential.maximum_image_label()
    }

    #[must_use]
    pub const fn entry_frontier(&self) -> u64 {
        self.entry_frontier
    }

    #[must_use]
    pub const fn asset_chain_digest(&self) -> AssetReferenceSetDigest {
        self.asset_chain_digest
    }

    #[must_use]
    pub const fn revision(&self) -> RecordRevision {
        self.revision
    }

    #[must_use]
    pub const fn build_proof(&self) -> AssetReferenceSetBuildProof {
        AssetReferenceSetBuildProof {
            set_id: self.set_id,
            sequential: self.sequential,
            ordered_assets: self.ordered_assets,
            entry_frontier: self.entry_frontier,
            asset_chain_digest: self.asset_chain_digest,
            revision: self.revision,
        }
    }
}

/// Exact revision-bound state required to resume or seal a staged build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetReferenceSetBuildProof {
    pub(super) set_id: AssetReferenceSetId,
    pub(super) sequential: SequentialMarkerSummaryV1,
    pub(super) ordered_assets: OrderedMarkerAssetSummaryV1,
    pub(super) entry_frontier: u64,
    pub(super) asset_chain_digest: AssetReferenceSetDigest,
    pub(super) revision: RecordRevision,
}

impl AssetReferenceSetBuildProof {
    #[must_use]
    pub const fn set_id(self) -> AssetReferenceSetId {
        self.set_id
    }

    #[must_use]
    pub const fn sequential(self) -> SequentialMarkerSummaryV1 {
        self.sequential
    }

    #[must_use]
    pub const fn ordered_assets(self) -> OrderedMarkerAssetSummaryV1 {
        self.ordered_assets
    }

    #[must_use]
    pub const fn entry_frontier(self) -> u64 {
        self.entry_frontier
    }

    #[must_use]
    pub const fn asset_chain_digest(self) -> AssetReferenceSetDigest {
        self.asset_chain_digest
    }
}

/// Whether an entry first introduced its label or repeated its exact first asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetLabelDisposition {
    First,
    Repeated {
        first_ordinal: AssetReferenceOrdinal,
    },
}

/// One immutable ordinal marker-to-asset entry in a reference set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetReferenceEntryRecord {
    pub(super) set_id: AssetReferenceSetId,
    pub(super) ordinal: AssetReferenceOrdinal,
    pub(super) marker_id: SyndicDraftMarkerId,
    pub(super) label: ImageLabelOrdinal,
    pub(super) asset_id: AssetId,
    pub(super) label_disposition: AssetLabelDisposition,
    pub(super) chain_digest: AssetReferenceSetDigest,
}

impl AssetReferenceEntryRecord {
    #[must_use]
    pub const fn set_id(&self) -> AssetReferenceSetId {
        self.set_id
    }

    #[must_use]
    pub const fn ordinal(&self) -> AssetReferenceOrdinal {
        self.ordinal
    }

    #[must_use]
    pub const fn marker_id(&self) -> SyndicDraftMarkerId {
        self.marker_id
    }

    #[must_use]
    pub const fn label(&self) -> ImageLabelOrdinal {
        self.label
    }

    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    #[must_use]
    pub const fn label_disposition(&self) -> AssetLabelDisposition {
        self.label_disposition
    }

    #[must_use]
    pub const fn chain_digest(&self) -> AssetReferenceSetDigest {
        self.chain_digest
    }
}

/// Compact durable owner identity; marker identities live only in set entries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AssetOwner {
    CurrentDraft(SyndicDraftId),
    AcceptedInput(SyndicAcceptedInputId),
    SubmittedTurnItem(SyndicItemId),
    RetryRecord(SyndicRetryRecordId),
    TranscriptProjection(SyndicProjectionId),
}

/// One compact owner head selecting an immutable sealed reference set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetOwnerHeadRecord {
    pub(super) owner: AssetOwner,
    pub(super) set: SealedAssetReferenceSetProof,
    pub(super) owner_revision: RecordRevision,
}

impl AssetOwnerHeadRecord {
    #[must_use]
    pub const fn owner(&self) -> AssetOwner {
        self.owner
    }

    #[must_use]
    pub const fn set(&self) -> SealedAssetReferenceSetProof {
        self.set
    }

    #[must_use]
    pub const fn owner_revision(&self) -> RecordRevision {
        self.owner_revision
    }

    /// Returns the exact current state used by an optional owner-head transition.
    #[must_use]
    pub const fn expectation(&self) -> AssetOwnerHeadExpectation {
        AssetOwnerHeadExpectation {
            set: self.set,
            owner_revision: self.owner_revision,
        }
    }
}

/// Exact present state expected by one owner-head transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetOwnerHeadExpectation {
    pub(super) set: SealedAssetReferenceSetProof,
    pub(super) owner_revision: RecordRevision,
}

impl AssetOwnerHeadExpectation {
    #[must_use]
    pub const fn set(self) -> SealedAssetReferenceSetProof {
        self.set
    }

    #[must_use]
    pub const fn owner_revision(self) -> RecordRevision {
        self.owner_revision
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct AssetEntryKey {
    pub(super) set_id: AssetReferenceSetId,
    pub(super) ordinal: AssetReferenceOrdinal,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct AssetMarkerKey {
    pub(super) set_id: AssetReferenceSetId,
    pub(super) marker_id: SyndicDraftMarkerId,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct AssetLabelFirstKey {
    pub(super) set_id: AssetReferenceSetId,
    pub(super) label: ImageLabelOrdinal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AssetLabelFirstRecord {
    pub(super) first_ordinal: AssetReferenceOrdinal,
    pub(super) asset_id: AssetId,
}
