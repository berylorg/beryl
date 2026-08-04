use std::{
    error::Error,
    fs::File,
    num::NonZeroU64,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    HealthGateError, HomeGeneration, HomeStore, domain::StoreInstanceId, fault::FaultController,
    fault::FaultPoint, health::FailureSeverity,
};

mod io;
mod operations;
mod platform;

use io::*;

const SIDECAR_DIRECTORY: &str = "sidecars";
const MAX_NAMESPACE_BYTES: usize = 32;
const HASH_BYTES: usize = 32;
const COPY_BUFFER_BYTES: usize = 64 * 1_024;

/// Bounded stable physical namespace for one sidecar-owning domain.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SidecarNamespace(String);

impl SidecarNamespace {
    /// Validates one lowercase ASCII namespace component.
    pub fn new(value: impl Into<String>) -> Result<Self, SidecarNamespaceError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_NAMESPACE_BYTES
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(SidecarNamespaceError { value })
        }
    }

    /// Returns the stable namespace component.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Invalid physical sidecar namespace.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error(
    "invalid sidecar namespace `{value}`; use 1-{MAX_NAMESPACE_BYTES} lowercase ASCII letters, digits, `_`, or `-`"
)]
pub struct SidecarNamespaceError {
    value: String,
}

/// Exact SHA-256 content identity used by the physical sidecar namespace.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SidecarDigest([u8; HASH_BYTES]);

impl SidecarDigest {
    /// Reconstructs one digest from durable typed metadata.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; HASH_BYTES]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; HASH_BYTES] {
        self.0
    }
}

/// Durable physical identity of one content-addressed sidecar.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SidecarAddress {
    namespace: SidecarNamespace,
    digest: SidecarDigest,
    length: u64,
}

impl SidecarAddress {
    /// Reconstructs an address from domain-owned durable metadata.
    #[must_use]
    pub const fn new(namespace: SidecarNamespace, digest: SidecarDigest, length: u64) -> Self {
        Self {
            namespace,
            digest,
            length,
        }
    }

    /// Returns the stable namespace.
    #[must_use]
    pub const fn namespace(&self) -> &SidecarNamespace {
        &self.namespace
    }

    /// Returns the SHA-256 digest.
    #[must_use]
    pub const fn digest(&self) -> SidecarDigest {
        self.digest
    }

    /// Returns the exact byte length.
    #[must_use]
    pub const fn length(&self) -> u64 {
        self.length
    }
}

/// Explicit caller bound for sidecar admission or verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SidecarByteLimit(NonZeroU64);

impl SidecarByteLimit {
    /// Constructs a nonzero byte limit.
    #[must_use]
    pub const fn new(maximum: NonZeroU64) -> Self {
        Self(maximum)
    }

    /// Returns the maximum accepted bytes.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Durable sidecar publication retained through a following metadata commit.
pub struct AdmittedSidecar {
    address: SidecarAddress,
    path: PathBuf,
    _file: File,
    pub(crate) store: StoreInstanceId,
    pub(crate) generation: HomeGeneration,
}

impl std::fmt::Debug for AdmittedSidecar {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdmittedSidecar")
            .field("address", &self.address)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl AdmittedSidecar {
    /// Returns the durable content address to store in typed metadata.
    #[must_use]
    pub const fn address(&self) -> &SidecarAddress {
        &self.address
    }

    /// Returns the canonical Host path for later verified runtime projection.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Verified existing sidecar retained against replacement for this value's lifetime.
pub struct VerifiedSidecar {
    address: SidecarAddress,
    path: PathBuf,
    _file: File,
    generation: HomeGeneration,
}

impl std::fmt::Debug for VerifiedSidecar {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VerifiedSidecar")
            .field("address", &self.address)
            .field("path", &self.path)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl VerifiedSidecar {
    /// Returns the verified durable content address.
    #[must_use]
    pub const fn address(&self) -> &SidecarAddress {
        &self.address
    }

    /// Returns the canonical Host path retained by this verification.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the healthy generation that performed verification.
    #[must_use]
    pub const fn generation(&self) -> HomeGeneration {
        self.generation
    }
}

/// Bounded physical verifier available to typed domain reopen validators.
pub struct SidecarVerifier<'a> {
    home: &'a Path,
    faults: &'a FaultController,
}

impl<'a> SidecarVerifier<'a> {
    pub(crate) fn new(store: &'a HomeStore) -> Self {
        Self {
            home: store.canonical_path(),
            faults: &store.faults,
        }
    }

    /// Proves that one referenced final sidecar has the declared length and digest.
    pub fn verify(
        &self,
        address: &SidecarAddress,
        limit: SidecarByteLimit,
    ) -> Result<(), SidecarError> {
        ensure_bound(address.length, limit)?;
        let directories =
            retain_sidecar_directories(self.home, address, self.faults, false, false)?;
        open_and_verify_final(self.faults, &directories, address, None, None, false).map(drop)
    }
}

/// Concrete physical sidecar operation stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SidecarStage {
    CreateDirectory,
    FlushDirectory,
    CreateTemporary,
    WriteTemporary,
    FlushTemporary,
    RenameFinal,
    OpenFinal,
    ReadFinal,
    ConfirmHealth,
}

/// Why sidecar admission or verification did not produce a retained token.
#[derive(Debug, Error)]
pub enum SidecarError {
    /// The process-wide state gate is not accepting sidecar work.
    #[error(transparent)]
    HealthGate(#[from] HealthGateError),
    /// A panic poisoned the in-process generation lock.
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,
    /// Caller bytes or durable metadata exceed the explicit operation bound.
    #[error("sidecar has {actual} bytes, exceeding the explicit limit {maximum}")]
    BoundExceeded { maximum: u64, actual: u64 },
    /// An expected referenced sidecar does not exist.
    #[error("referenced sidecar is missing")]
    Missing,
    /// A digest-derived final path contains bytes other than its declared content.
    #[error("sidecar content does not match its declared digest and length")]
    ContentMismatch,
    /// A reserved sidecar path is not an ordinary local directory.
    #[error("sidecar directory layout is invalid")]
    InvalidLayout,
    /// A concrete file or directory operation failed.
    #[error("sidecar operation failed during {stage:?}: {source}")]
    Storage {
        stage: SidecarStage,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}
