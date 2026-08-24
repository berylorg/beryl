use beryl_backend::{
    StreamedInputSourceError, StreamedInputSourceIdentity, StreamedInputSourceRevision,
};
use beryl_home_store::{ReadError, SidecarError};
use beryl_state::AssetReadError;
use syndic_storage::SyndicReadError;

use crate::cas_projection::input_replay::InputReplayPrepareError;

#[derive(Debug)]
pub(super) enum MarkerReplayError {
    Cancelled,
    HomeRead(ReadError),
    SyndicRead(SyndicReadError),
    AssetRead(AssetReadError),
    Sidecar(SidecarError),
    ReadUnavailable,
    InvalidSource,
    InvalidDescriptor,
    SourceIdentityMismatch {
        expected: StreamedInputSourceIdentity,
        actual: StreamedInputSourceIdentity,
    },
    RevisionDrift {
        expected: StreamedInputSourceRevision,
        actual: StreamedInputSourceRevision,
    },
    RuntimePathNotUnicode,
    RuntimePathUnmappable,
}

impl MarkerReplayError {
    pub(super) fn into_preparation(self) -> InputReplayPrepareError {
        match self {
            Self::Cancelled => InputReplayPrepareError::Cancelled,
            Self::HomeRead(source) => InputReplayPrepareError::HomeRead(source),
            Self::SyndicRead(source) => InputReplayPrepareError::SyndicRead(source),
            Self::AssetRead(source) => InputReplayPrepareError::AssetRead(source),
            Self::Sidecar(source) => InputReplayPrepareError::Sidecar(source),
            Self::ReadUnavailable => InputReplayPrepareError::ReadUnavailable,
            Self::InvalidSource => InputReplayPrepareError::AssetReferenceSetMismatch,
            Self::InvalidDescriptor => InputReplayPrepareError::DescriptorInvalid,
            Self::SourceIdentityMismatch { expected, actual } => {
                InputReplayPrepareError::SourceIdentityMismatch { expected, actual }
            }
            Self::RevisionDrift { expected, actual } => {
                InputReplayPrepareError::RevisionDrift { expected, actual }
            }
            Self::RuntimePathNotUnicode => InputReplayPrepareError::RuntimePathNotUnicode,
            Self::RuntimePathUnmappable => InputReplayPrepareError::RuntimePathUnmappable,
        }
    }

    pub(super) fn into_source(self) -> StreamedInputSourceError {
        match self {
            Self::Cancelled => StreamedInputSourceError::Cancelled,
            Self::HomeRead(_) | Self::ReadUnavailable => StreamedInputSourceError::ReadFailed,
            Self::SyndicRead(error) => map_syndic_read_error(error),
            Self::AssetRead(error) => map_asset_read_error(error),
            Self::Sidecar(_) => StreamedInputSourceError::VerifierUnavailable,
            Self::InvalidSource
            | Self::InvalidDescriptor
            | Self::RuntimePathNotUnicode
            | Self::RuntimePathUnmappable => StreamedInputSourceError::InvalidSource,
            Self::SourceIdentityMismatch { expected, actual } => {
                StreamedInputSourceError::SourceIdentityMismatch { expected, actual }
            }
            Self::RevisionDrift { expected, actual } => {
                StreamedInputSourceError::RevisionDrift { expected, actual }
            }
        }
    }
}

impl From<ReadError> for MarkerReplayError {
    fn from(source: ReadError) -> Self {
        Self::HomeRead(source)
    }
}

impl From<SyndicReadError> for MarkerReplayError {
    fn from(source: SyndicReadError) -> Self {
        Self::SyndicRead(source)
    }
}

impl From<AssetReadError> for MarkerReplayError {
    fn from(source: AssetReadError) -> Self {
        Self::AssetRead(source)
    }
}

impl From<SidecarError> for MarkerReplayError {
    fn from(source: SidecarError) -> Self {
        Self::Sidecar(source)
    }
}

fn map_asset_read_error(error: AssetReadError) -> StreamedInputSourceError {
    match error {
        AssetReadError::Read(_) => StreamedInputSourceError::ReadFailed,
        AssetReadError::ReferenceSetMissing(_)
        | AssetReadError::ReferenceSetNotBuilding(_)
        | AssetReadError::ReferenceSetNotSealed(_)
        | AssetReadError::CompletionMismatch(_)
        | AssetReadError::CompletionEvidenceMismatch(_)
        | AssetReadError::SealedProofMismatch(_) => StreamedInputSourceError::InvalidSource,
    }
}

fn map_syndic_read_error(error: SyndicReadError) -> StreamedInputSourceError {
    match error {
        SyndicReadError::Read(_) | SyndicReadError::ConcurrentChange { .. } => {
            StreamedInputSourceError::ReadFailed
        }
        SyndicReadError::Invariant(_)
        | SyndicReadError::ContentTextRequiresSealed
        | SyndicReadError::ContentTextContainsImageMarkers { .. }
        | SyndicReadError::InvalidContentTextOffset { .. }
        | SyndicReadError::InvalidContentTextSegmentCursor { .. }
        | SyndicReadError::InvalidContentTextSegmentOffset { .. }
        | SyndicReadError::InvalidContentTextReadLimit { .. }
        | SyndicReadError::ContentTextReadLimitTooSmall { .. }
        | SyndicReadError::InvalidResourceRange { .. }
        | SyndicReadError::InvalidResourceReadLimit { .. }
        | SyndicReadError::ResourceHasNoTextBacking
        | SyndicReadError::CaptureItemHasNoTextContent
        | SyndicReadError::StaleThreadLineage
        | SyndicReadError::InvalidThreadLineageCursor
        | SyndicReadError::StaleActivityQuery
        | SyndicReadError::InvalidActivityQueryCursor
        | SyndicReadError::ActivityQueryIsStale
        | SyndicReadError::StaleAcceptedRoute
        | SyndicReadError::InvalidAcceptedRouteCursor
        | SyndicReadError::StaleAcceptedReadySourceScan
        | SyndicReadError::InvalidAcceptedReadySourceCursor
        | SyndicReadError::StaleAcceptedReadyCandidateSource
        | SyndicReadError::InvalidAcceptedReadyCandidateCursor
        | SyndicReadError::StaleAcceptedNextSourceScan
        | SyndicReadError::InvalidAcceptedNextSourceCursor
        | SyndicReadError::StaleAcceptedNextCandidateSource
        | SyndicReadError::InvalidAcceptedNextCandidateSource
        | SyndicReadError::InvalidAcceptedNextCandidateCursor
        | SyndicReadError::InvalidDeliveryRecoveryStartupCursor
        | SyndicReadError::StaleRecoveredPendingScan
        | SyndicReadError::InvalidRecoveredPendingCursor
        | SyndicReadError::CatalogSummaryRevisionExhausted => {
            StreamedInputSourceError::InvalidSource
        }
    }
}
