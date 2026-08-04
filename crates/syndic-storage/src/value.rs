mod compaction;
mod content;
mod context;
mod input_gate;
mod lifecycle;
mod ordering;
mod parent;
mod proof;
mod stop;

pub use beryl_model::ImageLabelOrdinal;

pub use compaction::*;
pub use content::{ContentChunkOrdinal, ContentEncoding, ContentLifecycle};
pub use context::{
    DISCUSSION_CONTEXT_MAX_BYTES, DiscussionContextDescriptor, DiscussionContextEnvelope,
    DiscussionContextRange, DiscussionContextSource, DiscussionContextText,
    DiscussionContextVersion,
};
pub use input_gate::{
    InputGateState, NextTurnReason, PendingSteeringTargetProof, SteeringTargetProof,
};
pub use lifecycle::{
    AcceptedInputLifecycle, AssistantMessagePhase, BindingLifecycle, ProjectionLifecycle,
    ProviderItemKind, ProviderItemLifecycle, ProviderObservationIssueReason, ProviderOperationKind,
    TurnEndStatus, TurnIncompleteReason, TurnKind, TurnLifecycle, TurnTerminalOutcome,
    UnsupportedHistoryReason,
};
pub use ordering::{
    AcceptedInputOrdinal, AcceptedRouteGeneration, AcceptedRouteRevision, ActivityQueryRevision,
    ActivityWorkPeriod, ComposerAtomOrdinal, ContentPieceOrdinal, ContextEnvelopeRevision,
    ImageLabelFrontier, InputMarkerOrdinal, ItemProjectionGeneration, ItemSourceEventOrdinal,
    ProjectionOrdinal, ProviderControlOrdinal, ProviderItemBuildRevision,
    ProviderNarrativeGeneration, ResourceOrdinal, SourceEventSequence, SyndicConnectionGeneration,
    SyndicTimestamp, ThreadAttributesRevision, ThreadLineageDepth, ThreadUsageRevision,
    TranscriptGeneration, TranscriptPosition, TurnDepth, TurnItemOrdinal, TurnStateRevision,
};
pub use parent::ConversationParent;
pub use proof::{
    CasLineageMode, CasLineageProof, CasRepresentedPrefixProof, CurrentTranscriptEntryProof,
    NativeCasLineage, RecoveredInjectionProof, RecoveryItemCount, RecoveryProjectionVersion,
    RecoveryUtf8ByteCount, SelectedPathProof,
};
pub use stop::{
    StopAbandonmentReason, StopAttemptNonce, StopCause, StopCauseFirstRevisions,
    StopCauseFirstRevisionsError, StopCauseSet, StopCauseSetError, StopDispatchClaimWitness,
    StopOperationId, StopOperationNonce, StopOperationRevision,
};

/// Why a pure Syndic value was rejected before persistence or provider work.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SyndicValueError {
    /// Required text was empty.
    #[error("{kind} must not be empty")]
    EmptyText { kind: &'static str },
    /// Text exceeded its exact UTF-8 byte budget.
    #[error("{kind} must not exceed {maximum} UTF-8 bytes, got {actual}")]
    TextTooLong {
        kind: &'static str,
        maximum: usize,
        actual: usize,
    },
    /// Text exceeded its exact Unicode scalar-value budget.
    #[error("{kind} must not exceed {maximum} Unicode scalar values, got {actual}")]
    TooManyScalars {
        kind: &'static str,
        maximum: usize,
        actual: usize,
    },
    /// Exact captured text contained a NUL byte.
    #[error("{kind} contains a NUL byte at offset {index}")]
    NulByte { kind: &'static str, index: usize },
    /// Exact captured text had whitespace outside its logical content.
    #[error("{kind} must not have surrounding whitespace")]
    SurroundingWhitespace { kind: &'static str },
    /// Exact captured text contained a Unicode control character.
    #[error("{kind} contains a control character at UTF-8 offset {index}")]
    ControlCharacter { kind: &'static str, index: usize },
    /// Exact captured text contained no alphanumeric character.
    #[error("{kind} must contain at least one alphanumeric character")]
    MissingAlphanumeric { kind: &'static str },
    /// A required positive counter was supplied as zero.
    #[error("{kind} must be positive when present")]
    ZeroPositiveValue { kind: &'static str },
    /// An end-exclusive source range was empty or reversed.
    #[error("source range must be non-empty and ordered, got {start}..{end}")]
    InvalidRange { start: u64, end: u64 },
    /// A source range exceeded the owning context budget.
    #[error("source range must not exceed {maximum} bytes, got {actual}")]
    RangeTooLong { maximum: u64, actual: u64 },
    /// Exact selected text did not match its admitted source range.
    #[error("context text has {text_bytes} bytes but its source range has {range_bytes}")]
    ContextLengthMismatch { text_bytes: u64, range_bytes: u64 },
    /// Zero is reserved as the absence of an ordered value.
    #[error("{kind} must be non-zero")]
    ZeroOrdinal { kind: &'static str },
    /// A monotonic ordered value cannot advance further.
    #[error("{kind} is exhausted")]
    OrdinalExhausted { kind: &'static str },
    /// Zero is invalid for a completed recovery count.
    #[error("{kind} must be non-zero")]
    ZeroCount { kind: &'static str },
    /// A recovery count exceeded the exact supported maximum.
    #[error("{kind} must not exceed {maximum}, got {actual}")]
    CountTooLarge {
        kind: &'static str,
        maximum: u64,
        actual: u64,
    },
    /// CAS lineage mechanism and establishment-prefix shape cannot both be true.
    #[error("invalid CAS lineage proof: {reason}")]
    InvalidLineageProof { reason: &'static str },
    /// A locally incomplete turn outcome must retain why captured history is incomplete.
    #[error("an incomplete turn outcome requires a typed history-incomplete reason")]
    IncompleteTurnRequiresReason,
}
