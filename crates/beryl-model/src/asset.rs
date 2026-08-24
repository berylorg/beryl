use std::{fmt, num::NonZeroU64};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{SyndicContentDigest, SyndicContentId, SyndicDraftMarkerId};

/// Exact digest algorithm and identity-layout version for a durable asset.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(u8)]
pub enum AssetIdentityVersion {
    /// Version 1 identifies bytes by SHA-256 plus exact nonzero byte length.
    Sha256V1 = 1,
}

/// Stable content identity for one Beryl-home asset.
///
/// Product features treat this value as opaque. Storage and sidecar boundaries
/// may inspect its versioned digest and length to prove byte identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AssetId {
    version: AssetIdentityVersion,
    digest: [u8; 32],
    length: NonZeroU64,
}

impl AssetId {
    /// Constructs the first supported content identity from an admitted digest and length.
    #[must_use]
    pub const fn sha256_v1(digest: [u8; 32], length: NonZeroU64) -> Self {
        Self {
            version: AssetIdentityVersion::Sha256V1,
            digest,
            length,
        }
    }

    /// Returns the exact identity-layout and digest-algorithm version.
    #[must_use]
    pub const fn version(self) -> AssetIdentityVersion {
        self.version
    }

    /// Returns the exact SHA-256 digest bytes.
    #[must_use]
    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }

    /// Returns the exact nonzero byte length included in the identity.
    #[must_use]
    pub const fn length(self) -> NonZeroU64 {
        self.length
    }
}

/// Final nonzero per-thread label allocated to one durable image marker.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImageLabelOrdinal(NonZeroU64);

impl ImageLabelOrdinal {
    /// First valid image label (`A`).
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    /// Constructs an exact admitted label ordinal, rejecting reserved zero.
    pub fn new(value: u64) -> Result<Self, ImageLabelOrdinalError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(ImageLabelOrdinalError::Zero)
    }

    /// Returns the integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Advances one label without wrapping.
    pub fn checked_next(self) -> Result<Self, ImageLabelOrdinalError> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(ImageLabelOrdinalError::Exhausted)
    }
}

impl fmt::Display for ImageLabelOrdinal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Bijective base-26: 1=A, 26=Z, 27=AA. A u64 needs at most 14 digits.
        let mut value = self.get();
        let mut encoded = [0_u8; 14];
        let mut start = encoded.len();
        while value != 0 {
            value -= 1;
            start -= 1;
            encoded[start] = b'A' + u8::try_from(value % 26).expect("base-26 digit fits u8");
            value /= 26;
        }
        formatter.write_str(std::str::from_utf8(&encoded[start..]).expect("labels are ASCII"))
    }
}

/// Why an image-label ordinal could not be constructed or advanced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageLabelOrdinalError {
    /// Zero is reserved as the absence of a label.
    Zero,
    /// The maximum representable ordinal cannot advance.
    Exhausted,
}

impl fmt::Display for ImageLabelOrdinalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("image-label ordinal must be nonzero"),
            Self::Exhausted => formatter.write_str("image-label ordinal is exhausted"),
        }
    }
}

impl std::error::Error for ImageLabelOrdinalError {}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SequentialMarkerSummaryV1 {
    marker_digest: [u8; 32],
    marker_count: u64,
    maximum_image_label: Option<ImageLabelOrdinal>,
}

impl SequentialMarkerSummaryV1 {
    pub const fn new(
        marker_digest: [u8; 32],
        marker_count: u64,
        maximum_image_label: Option<ImageLabelOrdinal>,
    ) -> Result<Self, AssetProofError> {
        if (marker_count == 0) != maximum_image_label.is_none() {
            return Err(AssetProofError::MarkerMaximumMismatch);
        }
        Ok(Self {
            marker_digest,
            marker_count,
            maximum_image_label,
        })
    }

    #[must_use]
    pub const fn marker_digest(self) -> [u8; 32] {
        self.marker_digest
    }

    #[must_use]
    pub const fn marker_count(self) -> u64 {
        self.marker_count
    }

    #[must_use]
    pub const fn maximum_image_label(self) -> Option<ImageLabelOrdinal> {
        self.maximum_image_label
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrderedMarkerAssetSummaryV1 {
    marker_asset_digest: [u8; 32],
    marker_count: u64,
}

impl OrderedMarkerAssetSummaryV1 {
    #[must_use]
    pub const fn new(marker_asset_digest: [u8; 32], marker_count: u64) -> Self {
        Self {
            marker_asset_digest,
            marker_count,
        }
    }

    #[must_use]
    pub const fn marker_asset_digest(self) -> [u8; 32] {
        self.marker_asset_digest
    }

    #[must_use]
    pub const fn marker_count(self) -> u64 {
        self.marker_count
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerCommitmentV1 {
    tree_root_digest: [u8; 32],
    marker_count: u64,
    maximum_image_label: Option<ImageLabelOrdinal>,
}

impl DraftMarkerCommitmentV1 {
    pub const fn new(
        tree_root_digest: [u8; 32],
        marker_count: u64,
        maximum_image_label: Option<ImageLabelOrdinal>,
    ) -> Result<Self, AssetProofError> {
        if (marker_count == 0) != maximum_image_label.is_none() {
            return Err(AssetProofError::MarkerMaximumMismatch);
        }
        Ok(Self {
            tree_root_digest,
            marker_count,
            maximum_image_label,
        })
    }

    #[must_use]
    pub const fn tree_root_digest(self) -> [u8; 32] {
        self.tree_root_digest
    }

    #[must_use]
    pub const fn marker_count(self) -> u64 {
        self.marker_count
    }

    #[must_use]
    pub const fn maximum_image_label(self) -> Option<ImageLabelOrdinal> {
        self.maximum_image_label
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SealedContentMarkerSummary {
    content_id: SyndicContentId,
    content_digest: SyndicContentDigest,
    sequential: SequentialMarkerSummaryV1,
}

impl SealedContentMarkerSummary {
    #[must_use]
    pub const fn new(
        content_id: SyndicContentId,
        content_digest: SyndicContentDigest,
        sequential: SequentialMarkerSummaryV1,
    ) -> Self {
        Self {
            content_id,
            content_digest,
            sequential,
        }
    }

    #[must_use]
    pub const fn content_id(self) -> SyndicContentId {
        self.content_id
    }

    #[must_use]
    pub const fn content_digest(self) -> SyndicContentDigest {
        self.content_digest
    }

    #[must_use]
    pub const fn sequential(self) -> SequentialMarkerSummaryV1 {
        self.sequential
    }
}

/// Stable owner-neutral identity of one immutable asset-reference set.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetReferenceSetId([u8; 16]);

impl AssetReferenceSetId {
    /// Constructs a set identity from boundary-owned stable bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the stable identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Exact digest of a source-bound ordered asset-reference entry chain.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetReferenceSetDigest([u8; 32]);

impl AssetReferenceSetDigest {
    /// Constructs a digest from bytes calculated by the asset storage boundary.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SealedAssetReferenceSetProof {
    set_id: AssetReferenceSetId,
    sequential: SequentialMarkerSummaryV1,
    ordered_assets: OrderedMarkerAssetSummaryV1,
    entry_frontier: u64,
    asset_chain_digest: AssetReferenceSetDigest,
}

impl SealedAssetReferenceSetProof {
    pub const fn new(
        set_id: AssetReferenceSetId,
        sequential: SequentialMarkerSummaryV1,
        ordered_assets: OrderedMarkerAssetSummaryV1,
        entry_frontier: u64,
        asset_chain_digest: AssetReferenceSetDigest,
    ) -> Result<Self, AssetProofError> {
        if entry_frontier != sequential.marker_count() {
            return Err(AssetProofError::EntryFrontierMismatch);
        }
        if entry_frontier != ordered_assets.marker_count() {
            return Err(AssetProofError::OrderedMarkerAssetCountMismatch);
        }
        Ok(Self {
            set_id,
            sequential,
            ordered_assets,
            entry_frontier,
            asset_chain_digest,
        })
    }

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetProofError {
    MarkerMaximumMismatch,
    EntryFrontierMismatch,
    OrderedMarkerAssetCountMismatch,
}

impl fmt::Display for AssetProofError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MarkerMaximumMismatch => formatter.write_str(
                "marker count and optional maximum image label do not describe the same marker set",
            ),
            Self::EntryFrontierMismatch => formatter.write_str(
                "sealed asset-reference frontier does not equal the source marker count",
            ),
            Self::OrderedMarkerAssetCountMismatch => formatter.write_str(
                "sealed asset-reference frontier does not equal the ordered marker-asset count",
            ),
        }
    }
}

impl std::error::Error for AssetProofError {}

#[must_use]
pub fn sequential_marker_digest_seed() -> [u8; 32] {
    Sha256::digest(b"beryl.syndic.content-markers.v2\0").into()
}

#[must_use]
pub fn advance_sequential_marker_digest(
    previous: [u8; 32],
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.content-marker-entry.v2\0");
    hash.update(previous);
    hash.update(marker_id.as_bytes());
    hash.update(label.get().to_be_bytes());
    hash.finalize().into()
}

#[must_use]
pub fn ordered_marker_asset_digest_seed() -> [u8; 32] {
    Sha256::digest(b"beryl.marker-asset-associations.v1\0").into()
}

#[must_use]
pub fn advance_ordered_marker_asset_digest(
    previous: [u8; 32],
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"beryl.marker-asset-association-entry.v1\0");
    hash.update(previous);
    hash.update(marker_id.as_bytes());
    hash.update(label.get().to_be_bytes());
    hash.update([asset_id.version() as u8]);
    hash.update(asset_id.digest());
    hash.update(asset_id.length().get().to_be_bytes());
    hash.finalize().into()
}
