//! Destination-owned unpublished provider-observation staging.

mod canonical;
mod compaction_marker;
mod compiler;
mod cursor;
mod grammar;
mod grammar_tags;
mod issue;
mod schema;
mod staging;
mod validator;
mod validator_types;

pub(crate) use crate::{
    ProviderObservationBuildLifecycle, ProviderObservationBuildRecord,
    ProviderObservationChunkPayload, ProviderObservationChunkRecord, ProviderObservationDigest,
};
pub(crate) use canonical::{CanonicalObservationError, CanonicalObservationState};
pub(crate) use issue::{
    ProviderObservationIssueEvidenceError, classify_provider_observation_issue,
    validate_provider_observation_issue_evidence,
};
pub(crate) use schema::{ListKind, ObjectSchema};
pub(crate) use staging::replay_chunk;

pub use compaction_marker::{
    ProviderCompactionMarker, ProviderCompactionMarkerStager, ProviderCompactionMarkerStagingError,
};
pub use compiler::{
    InspectedProviderObservation, PreparedProviderObservationFrame,
    ProviderObservationFramePreparationError, ProviderObservationFramePreparationPlan,
    ProviderObservationFrameSemanticError, ProviderObservationFrameStageError,
    ProviderObservationFrameStageOutcome, inspect_provider_observation,
    prepare_provider_observation_frame, stage_provider_observation_frame,
};
pub use cursor::{
    BoundProviderObservation, ProviderObservationCursor, ProviderObservationCursorError,
    ProviderObservationPage, ProviderObservationRoute, ProviderObservationRouteMismatch,
    SealedProviderObservationHandle,
};
pub use grammar::{
    ProviderContainer, ProviderDeltaKind, ProviderEnumValue, ProviderField, ProviderFiniteF64,
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationItemKind,
    ProviderObservationItemLifecycle, ProviderScalar, ProviderStructuredPosition,
    ProviderValueContext,
};
pub use staging::{
    PROVIDER_OBSERVATION_CHUNK_MAX_BYTES, ProviderObservationSealCustodyGuard,
    ProviderObservationSealOutcome, ProviderObservationStageBatch,
    ProviderObservationStageBatchError, ProviderObservationStageBatchState,
    ProviderObservationStageCallback, ProviderObservationStageOutcome, ProviderObservationStager,
    ProviderObservationStagingBytes, ProviderObservationStagingError,
};
pub use validator::ProviderObservationValidatorError;
pub(crate) use validator::{
    PROVIDER_OBSERVATION_MAX_FRAME_DEPTH, ProviderObservationValidatorState,
};
pub(crate) use validator_types::{
    ProviderIdentityValidatorState, ProviderObservationElementKind, ProviderObservationFrame,
    Utf8ValidatorState,
};

pub(crate) const PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES: usize =
    ProviderIdentityValidatorState::MAX_BYTES as usize;
