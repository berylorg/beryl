use std::{error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, MutationBuildError, ReadError, SidecarError,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, DomainRevision, ImageLabelOrdinal, SyndicDraftMarkerId,
};

use super::{ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES, ASSET_REFERENCE_PAGE_MAX_ENTRIES, AssetOwner};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetValueError {
    InvalidMediaType,
    ZeroReferenceOrdinal,
    ReferenceOrdinalExhausted,
}

impl fmt::Display for AssetValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMediaType => {
                formatter.write_str("asset media type is invalid or oversized")
            }
            Self::ZeroReferenceOrdinal => {
                formatter.write_str("asset reference ordinal must be nonzero")
            }
            Self::ReferenceOrdinalExhausted => {
                formatter.write_str("asset reference ordinal is exhausted")
            }
        }
    }
}

impl Error for AssetValueError {}

/// Why an authority-bound staged or sealed asset-reference read could not be completed.
#[derive(Debug)]
pub enum AssetReadError {
    Read(ReadError),
    ReferenceSetMissing(AssetReferenceSetId),
    ReferenceSetNotBuilding(AssetReferenceSetId),
    ReferenceSetNotSealed(AssetReferenceSetId),
    StagingAuthorityMismatch(AssetReferenceSetId),
    SealedProofMismatch(AssetReferenceSetId),
}

impl fmt::Display for AssetReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::ReferenceSetMissing(set) => {
                write!(formatter, "asset reference set {set:?} is missing")
            }
            Self::ReferenceSetNotBuilding(set) => {
                write!(
                    formatter,
                    "asset reference set {set:?} is not an unpublished build"
                )
            }
            Self::ReferenceSetNotSealed(set) => {
                write!(formatter, "asset reference set {set:?} is not sealed")
            }
            Self::StagingAuthorityMismatch(set) => write!(
                formatter,
                "asset reference set {set:?} does not match the staging authority"
            ),
            Self::SealedProofMismatch(set) => write!(
                formatter,
                "asset reference set {set:?} does not match the complete sealed proof"
            ),
        }
    }
}

impl Error for AssetReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::ReferenceSetMissing(_)
            | Self::ReferenceSetNotBuilding(_)
            | Self::ReferenceSetNotSealed(_)
            | Self::StagingAuthorityMismatch(_)
            | Self::SealedProofMismatch(_) => None,
        }
    }
}

impl From<ReadError> for AssetReadError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

/// Why one fixed-capacity contiguous reference page was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetReferencePageError {
    Empty,
    TooMany { actual: usize },
    DuplicateMarker(SyndicDraftMarkerId),
}

impl fmt::Display for AssetReferencePageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("asset reference page must not be empty"),
            Self::TooMany { actual } => write!(
                formatter,
                "asset reference page has {actual} entries, exceeding the physical command bound {ASSET_REFERENCE_PAGE_MAX_ENTRIES}"
            ),
            Self::DuplicateMarker(marker) => {
                write!(formatter, "asset reference page repeats marker {marker:?}")
            }
        }
    }
}

impl Error for AssetReferencePageError {}

/// Why one compact owner-head update command was rejected before persistence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetOwnerHeadUpdateError {
    Empty,
    NoEffect,
    TooMany { actual: usize },
    DuplicateOwner(AssetOwner),
}

impl fmt::Display for AssetOwnerHeadUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("asset owner-head update must not be empty"),
            Self::NoEffect => formatter
                .write_str("asset owner-head update must contain at least one head mutation"),
            Self::TooMany { actual } => write!(
                formatter,
                "asset owner-head update has {actual} entries, exceeding the physical command bound {ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES}"
            ),
            Self::DuplicateOwner(owner) => {
                write!(formatter, "asset owner-head update repeats {owner:?}")
            }
        }
    }
}

impl Error for AssetOwnerHeadUpdateError {}

/// Why one bounded validation-only owner-head participant was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetOwnerHeadValidationError {
    Empty,
    TooMany { actual: usize },
    DuplicateOwner(AssetOwner),
}

impl fmt::Display for AssetOwnerHeadValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("asset owner-head validation must not be empty"),
            Self::TooMany { actual } => write!(
                formatter,
                "asset owner-head validation has {actual} entries, exceeding the physical command bound {ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES}"
            ),
            Self::DuplicateOwner(owner) => {
                write!(formatter, "asset owner-head validation repeats {owner:?}")
            }
        }
    }
}

impl Error for AssetOwnerHeadValidationError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetAdmissionError {
    WrongNamespace,
    IdentityMismatch,
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
    MetadataAlreadyExists(AssetId),
    MetadataMissing(AssetId),
    ReferenceSetAlreadyExists(AssetReferenceSetId),
    ReferenceSetMissing(AssetReferenceSetId),
    ReferenceSetNotBuilding(AssetReferenceSetId),
    ReferenceSetNotSealed(AssetReferenceSetId),
    BuildProofMismatch(AssetReferenceSetId),
    MarkerSummaryMismatch(AssetReferenceSetId),
    CountOverflow,
    ManifestRevisionExhausted(AssetReferenceSetId),
    EntryAlreadyExists,
    MarkerAlreadyExists(SyndicDraftMarkerId),
    LabelAssetMismatch { label: ImageLabelOrdinal },
    OwnerHeadMismatch(AssetOwner),
    OwnerRevisionExhausted(AssetOwner),
}

impl fmt::Display for AssetMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::MetadataAlreadyExists(asset) => {
                write!(formatter, "asset metadata already exists for {asset:?}")
            }
            Self::MetadataMissing(asset) => {
                write!(formatter, "asset metadata is missing for {asset:?}")
            }
            Self::ReferenceSetAlreadyExists(set) => {
                write!(formatter, "asset reference set {set:?} already exists")
            }
            Self::ReferenceSetMissing(set) => {
                write!(formatter, "asset reference set {set:?} is missing")
            }
            Self::ReferenceSetNotBuilding(set) => {
                write!(formatter, "asset reference set {set:?} is not building")
            }
            Self::ReferenceSetNotSealed(set) => {
                write!(formatter, "asset reference set {set:?} is not sealed")
            }
            Self::BuildProofMismatch(set) => {
                write!(
                    formatter,
                    "asset reference set {set:?} build proof is stale or collided"
                )
            }
            Self::MarkerSummaryMismatch(set) => write!(
                formatter,
                "asset reference set {set:?} does not exactly match its sealed content-marker summary"
            ),
            Self::CountOverflow => formatter.write_str("asset reference-set count is exhausted"),
            Self::ManifestRevisionExhausted(set) => {
                write!(
                    formatter,
                    "asset reference set {set:?} revision is exhausted"
                )
            }
            Self::EntryAlreadyExists => {
                formatter.write_str("asset reference entry ordinal already exists")
            }
            Self::MarkerAlreadyExists(marker) => {
                write!(
                    formatter,
                    "asset reference set already contains marker {marker:?}"
                )
            }
            Self::LabelAssetMismatch { label } => write!(
                formatter,
                "image label {label} already identifies a different asset in this reference set"
            ),
            Self::OwnerHeadMismatch(owner) => {
                write!(
                    formatter,
                    "asset owner head proof or revision disagrees for {owner:?}"
                )
            }
            Self::OwnerRevisionExhausted(owner) => {
                write!(formatter, "asset owner revision is exhausted for {owner:?}")
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

impl Error for AssetValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Sidecar(source) => Some(source),
            Self::Invariant(_) => None,
        }
    }
}

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
