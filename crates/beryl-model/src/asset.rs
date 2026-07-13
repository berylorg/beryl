use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

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
