use beryl_backend::{StreamedInputSourceIdentity, StreamedInputSourceRevision};
use beryl_home_store::{HomeGeneration, HomeHealthState, ReadError, SidecarError};
use beryl_model::{BerylHomeId, SyndicAcceptedInputId, SyndicContentId};
use beryl_state::AssetReadError;
use syndic_storage::SyndicReadError;

#[derive(Debug)]
pub(in crate::cas_projection) enum InputReplayPrepareError {
    Cancelled,
    HomeNotHealthy {
        state: HomeHealthState,
        expected_home_id: BerylHomeId,
        actual_home_id: BerylHomeId,
        expected_generation: HomeGeneration,
        actual_generation: Option<HomeGeneration>,
    },
    HealthyHomeGenerationMissing,
    HomeIdentityMismatch {
        expected: BerylHomeId,
        actual: BerylHomeId,
    },
    HomeGenerationMismatch {
        expected: HomeGeneration,
        actual: Option<HomeGeneration>,
        state: HomeHealthState,
    },
    HomeRead(ReadError),
    SyndicRead(SyndicReadError),
    AssetRead(AssetReadError),
    Sidecar(SidecarError),
    AcceptedInputMissing {
        input_id: SyndicAcceptedInputId,
    },
    AcceptedInputChanged {
        input_id: SyndicAcceptedInputId,
    },
    AcceptedInputContentMismatch {
        input_id: SyndicAcceptedInputId,
    },
    ContentMissing {
        content_id: SyndicContentId,
    },
    ContentChanged {
        content_id: SyndicContentId,
    },
    ReadUnavailable,
    AssetReferenceSetMissing,
    AssetOwnerHeadMissing,
    AssetReferenceSetMismatch,
    DescriptorInvalid,
    EmptyInput,
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

impl From<ReadError> for InputReplayPrepareError {
    fn from(source: ReadError) -> Self {
        Self::HomeRead(source)
    }
}

impl From<SyndicReadError> for InputReplayPrepareError {
    fn from(source: SyndicReadError) -> Self {
        Self::SyndicRead(source)
    }
}

impl From<AssetReadError> for InputReplayPrepareError {
    fn from(source: AssetReadError) -> Self {
        Self::AssetRead(source)
    }
}

impl From<SidecarError> for InputReplayPrepareError {
    fn from(source: SidecarError) -> Self {
        Self::Sidecar(source)
    }
}
