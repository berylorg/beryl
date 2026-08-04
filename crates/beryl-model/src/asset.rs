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

/// Exact marker evidence retained by one sealed Syndic content object.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SealedContentMarkerSummary {
    content_id: SyndicContentId,
    content_digest: SyndicContentDigest,
    marker_digest: [u8; 32],
    marker_count: u64,
    maximum_image_label: Option<ImageLabelOrdinal>,
}

impl SealedContentMarkerSummary {
    /// Constructs one internally consistent exact marker summary.
    pub const fn new(
        content_id: SyndicContentId,
        content_digest: SyndicContentDigest,
        marker_digest: [u8; 32],
        marker_count: u64,
        maximum_image_label: Option<ImageLabelOrdinal>,
    ) -> Result<Self, AssetProofError> {
        if (marker_count == 0) != maximum_image_label.is_none() {
            return Err(AssetProofError::MarkerMaximumMismatch);
        }
        Ok(Self {
            content_id,
            content_digest,
            marker_digest,
            marker_count,
            maximum_image_label,
        })
    }

    /// Returns the exact sealed content identity.
    #[must_use]
    pub const fn content_id(self) -> SyndicContentId {
        self.content_id
    }

    /// Returns the full ordered-content digest.
    #[must_use]
    pub const fn content_digest(self) -> SyndicContentDigest {
        self.content_digest
    }

    /// Returns the digest of the ordered marker-id and label sequence.
    #[must_use]
    pub const fn marker_digest(self) -> [u8; 32] {
        self.marker_digest
    }

    /// Returns the number of ordered markers.
    #[must_use]
    pub const fn marker_count(self) -> u64 {
        self.marker_count
    }

    /// Returns the greatest image label present, or none for marker-free content.
    #[must_use]
    pub const fn maximum_image_label(self) -> Option<ImageLabelOrdinal> {
        self.maximum_image_label
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

/// Compact cross-domain proof for one sealed immutable asset-reference set.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SealedAssetReferenceSetProof {
    set_id: AssetReferenceSetId,
    source: SealedContentMarkerSummary,
    entry_frontier: u64,
    asset_chain_digest: AssetReferenceSetDigest,
}

impl SealedAssetReferenceSetProof {
    /// Constructs proof only when the sealed frontier covers every source marker exactly.
    pub const fn new(
        set_id: AssetReferenceSetId,
        source: SealedContentMarkerSummary,
        entry_frontier: u64,
        asset_chain_digest: AssetReferenceSetDigest,
    ) -> Result<Self, AssetProofError> {
        if entry_frontier != source.marker_count() {
            return Err(AssetProofError::EntryFrontierMismatch);
        }
        Ok(Self {
            set_id,
            source,
            entry_frontier,
            asset_chain_digest,
        })
    }

    /// Returns the immutable set identity.
    #[must_use]
    pub const fn set_id(self) -> AssetReferenceSetId {
        self.set_id
    }

    /// Returns the exact source content-marker summary.
    #[must_use]
    pub const fn source(self) -> SealedContentMarkerSummary {
        self.source
    }

    /// Returns the exact sealed entry frontier.
    #[must_use]
    pub const fn entry_frontier(self) -> u64 {
        self.entry_frontier
    }

    /// Returns the digest of the ordered marker-label-asset chain.
    #[must_use]
    pub const fn asset_chain_digest(self) -> AssetReferenceSetDigest {
        self.asset_chain_digest
    }
}

/// Why exact shared content-marker or asset-set evidence was inconsistent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetProofError {
    /// Marker-free content has no maximum label and marker-bearing content has one.
    MarkerMaximumMismatch,
    /// A sealed set must contain exactly one entry for every source marker.
    EntryFrontierMismatch,
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
        }
    }
}

impl std::error::Error for AssetProofError {}

/// Returns the canonical digest for an empty ordered content-marker sequence.
#[must_use]
pub fn content_marker_digest_seed() -> [u8; 32] {
    Sha256::digest(b"beryl.syndic.content-markers.v2\0").into()
}

/// Advances the canonical marker digest by one exact marker-id and label pair.
#[must_use]
pub fn advance_content_marker_digest(
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
