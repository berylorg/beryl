use std::{error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, MutationBuildError, ReadError, SidecarError,
};
use beryl_model::{AssetId, DomainRevision};

use crate::{RecordRevision, ValueError};

use super::AssetReferenceOwner;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetValueError {
    InvalidMediaType,
    ByteBoundExceeded { maximum: u64, actual: u64 },
}

impl fmt::Display for AssetValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMediaType => {
                formatter.write_str("asset media type is invalid or oversized")
            }
            Self::ByteBoundExceeded { maximum, actual } => {
                write!(formatter, "asset has {actual} bytes, exceeding {maximum}")
            }
        }
    }
}

impl Error for AssetValueError {}

/// Why a bounded exact asset-reference batch description was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetReferenceBatchError {
    Empty,
    TooMany {
        maximum: usize,
        actual: usize,
    },
    DuplicateSource(AssetReferenceOwner),
    DuplicateDestination(AssetReferenceOwner),
    SourceDestinationOverlap(AssetReferenceOwner),
    ConflictingRecordRevision {
        asset_id: AssetId,
        first: RecordRevision,
        second: RecordRevision,
    },
    RecordRevisionExhausted {
        asset_id: AssetId,
    },
    Value(AssetValueError),
}

impl fmt::Display for AssetReferenceBatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("asset-reference batch must not be empty"),
            Self::TooMany { maximum, actual } => write!(
                formatter,
                "asset-reference batch has {actual} entries, exceeding {maximum}"
            ),
            Self::DuplicateSource(owner) => {
                write!(formatter, "asset-reference batch repeats source {owner:?}")
            }
            Self::DuplicateDestination(owner) => {
                write!(
                    formatter,
                    "asset-reference batch repeats destination {owner:?}"
                )
            }
            Self::SourceDestinationOverlap(owner) => write!(
                formatter,
                "asset-reference batch uses {owner:?} as both source and destination"
            ),
            Self::ConflictingRecordRevision {
                asset_id,
                first,
                second,
            } => write!(
                formatter,
                "asset-reference batch has conflicting revisions {} and {} for {asset_id:?}",
                first.get(),
                second.get()
            ),
            Self::RecordRevisionExhausted { asset_id } => write!(
                formatter,
                "asset-reference batch cannot advance exhausted metadata for {asset_id:?}"
            ),
            Self::Value(source) => source.fmt(formatter),
        }
    }
}

impl Error for AssetReferenceBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Value(source) => Some(source),
            _ => None,
        }
    }
}

impl From<AssetValueError> for AssetReferenceBatchError {
    fn from(source: AssetValueError) -> Self {
        Self::Value(source)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetAdmissionError {
    WrongNamespace,
    IdentityMismatch,
    ByteBoundExceeded {
        maximum: u64,
        actual: u64,
    },
    CreationRevisionMismatch {
        expected: DomainRevision,
        actual: DomainRevision,
    },
    RevisionExhausted,
}

impl fmt::Display for AssetAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongNamespace => {
                formatter.write_str("admitted sidecar is not in the image namespace")
            }
            Self::IdentityMismatch => formatter
                .write_str("admitted sidecar digest or length disagrees with the asset identity"),
            Self::ByteBoundExceeded { maximum, actual } => {
                write!(formatter, "asset has {actual} bytes, exceeding {maximum}")
            }
            Self::CreationRevisionMismatch { expected, actual } => write!(
                formatter,
                "asset creation revision must be {expected}, got {actual}"
            ),
            Self::RevisionExhausted => formatter.write_str("asset domain revision is exhausted"),
        }
    }
}

impl Error for AssetAdmissionError {}

#[derive(Debug)]
pub enum AssetMutationError {
    Read(ReadError),
    Build(MutationBuildError),
    Value(AssetValueError),
    StateValue(ValueError),
    AssetAlreadyExists(AssetId),
    AssetMissing(AssetId),
    ReferenceAlreadyExists(AssetReferenceOwner),
    ReferenceMissing(AssetReferenceOwner),
    ReferenceAssetMismatch,
    MetadataFactsMismatch,
    RecordRevisionConflict {
        expected: RecordRevision,
        current: RecordRevision,
    },
    ReferenceCountOverflow,
    ReferenceCountUnderflow,
}

impl fmt::Display for AssetMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::StateValue(source) => source.fmt(formatter),
            Self::AssetAlreadyExists(asset) => {
                write!(formatter, "asset metadata already exists for {asset:?}")
            }
            Self::AssetMissing(asset) => {
                write!(formatter, "asset metadata is missing for {asset:?}")
            }
            Self::ReferenceAlreadyExists(owner) => {
                write!(formatter, "asset reference already exists for {owner:?}")
            }
            Self::ReferenceMissing(owner) => {
                write!(formatter, "asset reference is missing for {owner:?}")
            }
            Self::ReferenceAssetMismatch => {
                formatter.write_str("asset reference points to a different asset")
            }
            Self::MetadataFactsMismatch => {
                formatter.write_str("asset media facts disagree with existing metadata")
            }
            Self::RecordRevisionConflict { expected, current } => write!(
                formatter,
                "asset metadata revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::ReferenceCountOverflow => {
                formatter.write_str("asset reference count is exhausted")
            }
            Self::ReferenceCountUnderflow => {
                formatter.write_str("asset reference count is already zero")
            }
        }
    }
}

impl Error for AssetMutationError {}

impl DomainCallbackError for AssetMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for AssetMutationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl From<MutationBuildError> for AssetMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl From<AssetValueError> for AssetMutationError {
    fn from(source: AssetValueError) -> Self {
        Self::Value(source)
    }
}

impl From<ValueError> for AssetMutationError {
    fn from(source: ValueError) -> Self {
        Self::StateValue(source)
    }
}

/// Why a bounded reference reconciliation read could not publish one coherent status.
#[derive(Debug)]
pub enum AssetReferenceStatusError {
    Read(ReadError),
    ConcurrentChange,
}

impl fmt::Display for AssetReferenceStatusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::ConcurrentChange => formatter
                .write_str("asset references changed concurrently during exact reconciliation"),
        }
    }
}

impl Error for AssetReferenceStatusError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::ConcurrentChange => None,
        }
    }
}

impl From<ReadError> for AssetReferenceStatusError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

#[derive(Debug)]
pub enum AssetValidationError {
    Read(ReadError),
    Sidecar(SidecarError),
    Invariant(&'static str),
}

impl fmt::Display for AssetValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Sidecar(source) => source.fmt(formatter),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for AssetValidationError {}

impl DomainCallbackError for AssetValidationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            Self::Sidecar(source) => Ok(DomainCallbackSource::Sidecar(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for AssetValidationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}
