use std::num::NonZeroU64;

use super::SyndicValueError;

/// Stable one-based order of a bounded chunk inside one content object.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContentChunkOrdinal(NonZeroU64);

impl ContentChunkOrdinal {
    /// First valid chunk ordinal.
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    /// Constructs an exact ordinal, rejecting zero.
    pub fn new(value: u64) -> Result<Self, SyndicValueError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(SyndicValueError::ZeroOrdinal {
                kind: "content-chunk ordinal",
            })
    }

    /// Returns the integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Advances one step without wrapping.
    pub fn checked_next(self) -> Result<Self, SyndicValueError> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(SyndicValueError::OrdinalExhausted {
                kind: "content-chunk ordinal",
            })
    }
}

/// Exact logical encoding carried by one chunked content object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentEncoding {
    /// Canonical ordered composer atoms with unresolved durable draft markers.
    ComposerV1,
    /// Exact UTF-8 text owned by a canonical assistant or operational item.
    Utf8V1,
    /// Closed typed provider-item frames and frame-local logical views.
    ProviderItemV1,
}

/// Publication state of one content manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentLifecycle {
    /// Chunks may be appended, but no logical owner may reference this content.
    Building,
    /// The complete content-addressed manifest and chunks are immutable.
    Sealed,
    /// One canonical item owns this appendable UTF-8 frontier.
    Live,
    /// One canonical item's complete UTF-8 frontier is immutable.
    Finalized,
}

impl ContentLifecycle {
    /// Returns whether no later chunk or manifest mutation is permitted.
    #[must_use]
    pub const fn is_immutable(self) -> bool {
        matches!(self, Self::Sealed | Self::Finalized)
    }
}
