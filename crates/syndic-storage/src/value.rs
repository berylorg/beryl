mod content;
mod context;
mod input_gate;
mod lifecycle;
mod ordering;
mod parent;
mod proof;

pub use content::{ContentChunkOrdinal, ContentEncoding, ContentLifecycle};
pub use context::{
    DISCUSSION_CONTEXT_MAX_BYTES, DiscussionContextDescriptor, DiscussionContextEnvelope,
    DiscussionContextRange, DiscussionContextSource, DiscussionContextText,
    DiscussionContextVersion,
};
pub use input_gate::{
    AcceptedInputDisposition, InputGateState, NextTurnReason, PendingSteeringTargetProof,
    SteeringTargetProof,
};
pub use lifecycle::{
    AcceptedInputLifecycle, AssistantMessagePhase, BindingLifecycle, ProjectionLifecycle,
    ProviderItemKind, ProviderItemLifecycle, ProviderOperationKind, TurnEndStatus,
    TurnIncompleteReason, TurnKind, TurnLifecycle, TurnTerminalOutcome, UnsupportedHistoryReason,
};
pub use ordering::{
    AcceptedInputOrdinal, ComposerAtomOrdinal, ContentPieceOrdinal, ContextEnvelopeRevision,
    ImageLabelOrdinal, InputMarkerOrdinal, ItemProjectionGeneration, ItemSourceEventOrdinal,
    ProjectionOrdinal, ResourceOrdinal, SourceEventSequence, SyndicTimestamp, TranscriptGeneration,
    TranscriptPosition, TurnDepth, TurnItemOrdinal, TurnStateRevision,
};
pub use parent::ConversationParent;
pub use proof::{
    CasLineageMode, CasLineageProof, CasRepresentedPrefixProof, CurrentTranscriptEntryProof,
    NativeCasLineage, RecoveredInjectionProof, RecoveryItemCount, RecoveryProjectionVersion,
    RecoveryUtf8ByteCount, SelectedPathProof,
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
    /// Exact captured text contained a NUL byte.
    #[error("{kind} contains a NUL byte at offset {index}")]
    NulByte { kind: &'static str, index: usize },
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
