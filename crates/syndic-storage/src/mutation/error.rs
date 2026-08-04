use std::{error::Error, fmt};

use beryl_home_store::{DomainCallbackError, DomainCallbackSource, MutationBuildError, ReadError};
use beryl_model::{
    AcceptedInputRevision, BindingRevision, CasNativeTurnCountError, ContentRevision,
    DraftRevision, InputGateRevision, RevisionError, ThreadRevision,
};

use crate::{SourceEventSequence, SyndicRecordError, SyndicValueError, TurnStateRevision};

/// Invalid pure construction of a thread-creation intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CreateThreadError {
    #[error("thread-from-tail creation requires a nonempty committed source tail")]
    EmptySourceTail,
    #[error("thread-from-tail creation requires a complete finalized source tail")]
    IncompleteSourceTail,
    #[error("thread creation timestamp precedes source-thread activity")]
    TimestampPrecedesSourceActivity,
}

/// Result of reconciling one natural thread/draft identity pair after replay or failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadCreationStatus {
    Absent,
    Exact,
    Collision,
}

/// Why one exact Syndic thread or draft mutation was rejected.
#[derive(Debug)]
pub enum SyndicMutationError {
    Read(ReadError),
    Build(MutationBuildError),
    Revision(RevisionError),
    NativeTurnCount(CasNativeTurnCountError),
    Record(SyndicRecordError),
    Value(SyndicValueError),
    IdentityCollision,
    ContentIdentityCollision,
    ContentManifestConflict,
    ContentChunkConflict,
    ContentNotComplete,
    ContentRevisionConflict {
        expected: ContentRevision,
        current: ContentRevision,
    },
    RequiredRecordMissing {
        family: &'static str,
    },
    SourceTailConflict,
    TimestampPrecedesSourceActivity,
    CurrentDraftConflict,
    ThreadRevisionConflict {
        expected: ThreadRevision,
        current: ThreadRevision,
    },
    DraftRevisionConflict {
        expected: DraftRevision,
        current: DraftRevision,
    },
    InputGateRevisionConflict {
        expected: InputGateRevision,
        current: InputGateRevision,
    },
    AcceptedInputRevisionConflict {
        expected: AcceptedInputRevision,
        current: AcceptedInputRevision,
    },
    BindingRevisionConflict {
        expected: BindingRevision,
        current: BindingRevision,
    },
    TurnStateRevisionConflict {
        expected: TurnStateRevision,
        current: TurnStateRevision,
    },
    LiveTurnConflict,
    TurnLifecycleConflict,
    TerminalTurnClosed,
    SourceEventAlreadyAdmitted,
    SourceEventCollision,
    SourceEventSequenceConflict {
        expected: SourceEventSequence,
        actual: SourceEventSequence,
    },
    SourceEventFrontierExhausted,
    SourceIdentityConflict,
    ProviderFrameBuildConflict,
    ProviderFrameValidationConflict,
    ProviderObservationIssueConflict,
    ProviderItemKindConflict,
    ProviderItemLifecycleConflict,
    TerminalItemAuditConflict,
    CanonicalItemConflict,
    ActivityQueryConflict,
    GeneratedMediaResourceCollision,
    CanonicalFinalizationConflict,
    TerminalHistoryCompletionConflict,
    ProjectionBuildConflict,
    ProjectionAlreadyCurrent,
    ProjectionIdentityCollision,
    TranscriptBuildConflict,
    TranscriptProjectionUnavailable,
    TranscriptAlreadyCurrent,
    TranscriptIdentityCollision,
    AssistantPhaseConflict,
    InputGateStateConflict,
    AcceptedInputDeliveryConflict,
    AcceptedInputPromotionConflict,
    BindingStateConflict,
    ExecutionBindingConflict,
    ThreadAttributesRevisionConflict {
        expected: crate::ThreadAttributesRevision,
        current: crate::ThreadAttributesRevision,
    },
    ThreadUsageRevisionConflict {
        expected: crate::ThreadUsageRevision,
        current: crate::ThreadUsageRevision,
    },
    ThreadCatalogSummaryConflict,
    GeneratedTitleAlreadyAccepted,
    ThreadArchiveStateConflict,
    UsageRouteConflict,
    UsageProviderOrdinalConflict,
    BindingPathConflict,
    CasThreadOwnershipConflict,
    CasThreadRetired,
    CasTurnOwnershipConflict,
    ActiveCasTurnCollision,
    ExecutionSnapshotCollision,
    ActiveSteeringRouteConflict,
    EmptySubmission,
    AdmissionIdentityCollision,
    AssetReferenceSetConflict,
    ReplacementEditAlreadyActive,
    ReplacementEditNotActive,
    ReplacementDraftNotEmpty,
    ReplacementTargetConflict,
    UnchangedPayload,
    TimestampRegressed,
}

impl fmt::Display for SyndicMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Revision(source) => source.fmt(formatter),
            Self::NativeTurnCount(source) => source.fmt(formatter),
            Self::Record(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::IdentityCollision => {
                formatter.write_str("thread creation natural identity collides with durable state")
            }
            Self::ContentIdentityCollision => {
                formatter.write_str("content identity collides with different durable state")
            }
            Self::ContentManifestConflict => {
                formatter.write_str("content construction frontier disagrees")
            }
            Self::ContentChunkConflict => {
                formatter.write_str("content chunk already exists at the requested ordinal")
            }
            Self::ContentNotComplete => {
                formatter.write_str("content cannot publish before its exact manifest is complete")
            }
            Self::ContentRevisionConflict { expected, current } => write!(
                formatter,
                "content revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::RequiredRecordMissing { family } => {
                write!(formatter, "required Syndic `{family}` record is missing")
            }
            Self::SourceTailConflict => {
                formatter.write_str("source thread no longer has the exact selected tail")
            }
            Self::TimestampPrecedesSourceActivity => {
                formatter.write_str("thread creation timestamp precedes source-thread activity")
            }
            Self::CurrentDraftConflict => {
                formatter.write_str("thread, current draft, and reverse index disagree")
            }
            Self::ThreadRevisionConflict { expected, current } => write!(
                formatter,
                "thread revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::DraftRevisionConflict { expected, current } => write!(
                formatter,
                "draft revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::InputGateRevisionConflict { expected, current } => write!(
                formatter,
                "input-gate revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::AcceptedInputRevisionConflict { expected, current } => write!(
                formatter,
                "accepted-input revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::BindingRevisionConflict { expected, current } => write!(
                formatter,
                "binding revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::TurnStateRevisionConflict { expected, current } => write!(
                formatter,
                "turn-state revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::LiveTurnConflict => {
                formatter.write_str("live event does not target the exact current turn")
            }
            Self::TurnLifecycleConflict => {
                formatter.write_str("turn lifecycle does not admit the requested live event")
            }
            Self::TerminalTurnClosed => {
                formatter.write_str("proven-terminal turn is closed to source events")
            }
            Self::SourceEventAlreadyAdmitted => {
                formatter.write_str("the exact source event is already durably admitted")
            }
            Self::SourceEventCollision => {
                formatter.write_str("source-event sequence collides with different durable data")
            }
            Self::SourceEventSequenceConflict { expected, actual } => write!(
                formatter,
                "source-event sequence conflict: expected {}, got {}",
                expected.get(),
                actual.get()
            ),
            Self::SourceEventFrontierExhausted => {
                formatter.write_str("source-event sequence frontier is exhausted")
            }
            Self::SourceIdentityConflict => {
                formatter.write_str("source event external identity correlation disagrees")
            }
            Self::ProviderFrameBuildConflict => {
                formatter.write_str("sealed provider-frame build disagrees with publication")
            }
            Self::ProviderFrameValidationConflict => formatter
                .write_str("staged provider frame failed exact structural validation"),
            Self::ProviderObservationIssueConflict => formatter.write_str(
                "provider-observation issue evidence or lifecycle conflict disagrees",
            ),
            Self::ProviderItemKindConflict => {
                formatter.write_str("source item kind disagrees with durable item authority")
            }
            Self::ProviderItemLifecycleConflict => formatter
                .write_str("source item lifecycle or durable disposition disagrees"),
            Self::TerminalItemAuditConflict => formatter.write_str(
                "successful terminal status requires every admitted item to be closed and supported",
            ),
            Self::CanonicalItemConflict => {
                formatter.write_str("canonical item identity or live frontier disagrees")
            }
            Self::ActivityQueryConflict => {
                formatter.write_str("activity-query projection frontier disagrees")
            }
            Self::GeneratedMediaResourceCollision => formatter
                .write_str("generated-media resource identity collides with durable state"),
            Self::CanonicalFinalizationConflict => {
                formatter.write_str("canonical item finalization frontier disagrees")
            }
            Self::TerminalHistoryCompletionConflict => formatter.write_str(
                "terminal history is not at the exact durable convergence fixed point",
            ),
            Self::ProjectionBuildConflict => {
                formatter.write_str("item projection build frontier disagrees")
            }
            Self::ProjectionAlreadyCurrent => {
                formatter.write_str("canonical item already has a current projection set")
            }
            Self::ProjectionIdentityCollision => {
                formatter.write_str("derived projection identity collides with different facts")
            }
            Self::TranscriptBuildConflict => {
                formatter.write_str("transcript build frontier disagrees")
            }
            Self::TranscriptProjectionUnavailable => formatter
                .write_str("a current item projection required by the transcript is unavailable"),
            Self::TranscriptAlreadyCurrent => {
                formatter.write_str("thread transcript projection is already current")
            }
            Self::TranscriptIdentityCollision => {
                formatter.write_str("derived transcript identity collides with different facts")
            }
            Self::AssistantPhaseConflict => {
                formatter.write_str("assistant message phase metadata disagrees")
            }
            Self::InputGateStateConflict => {
                formatter.write_str("input-gate state does not admit the requested mutation")
            }
            Self::AcceptedInputDeliveryConflict => formatter
                .write_str("accepted-input delivery state does not admit the requested transition"),
            Self::AcceptedInputPromotionConflict => formatter.write_str(
                "accepted-input authority does not admit the requested next-turn promotion",
            ),
            Self::BindingStateConflict => {
                formatter.write_str("binding state does not admit the requested transition")
            }
            Self::ExecutionBindingConflict => formatter
                .write_str("CAS execution copy disagrees with canonical thread execution"),
            Self::ThreadAttributesRevisionConflict { expected, current } => write!(
                formatter,
                "thread-attributes revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::ThreadUsageRevisionConflict { expected, current } => write!(
                formatter,
                "thread-usage revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::ThreadCatalogSummaryConflict => formatter
                .write_str("thread-catalog summary or exact canonical source changed"),
            Self::GeneratedTitleAlreadyAccepted => {
                formatter.write_str("thread already has an accepted generated title")
            }
            Self::ThreadArchiveStateConflict => formatter
                .write_str("thread archive state does not admit the requested transition"),
            Self::UsageRouteConflict => formatter
                .write_str("token usage does not name the exact current usable route"),
            Self::UsageProviderOrdinalConflict => {
                formatter.write_str("token usage provider-control ordinal is stale")
            }
            Self::BindingPathConflict => {
                formatter.write_str("binding path or represented prefix disagrees")
            }
            Self::CasThreadOwnershipConflict => {
                formatter.write_str("CAS thread is permanently reserved by another Syndic thread")
            }
            Self::CasThreadRetired => {
                formatter.write_str("CAS thread has been permanently retired from execution")
            }
            Self::CasTurnOwnershipConflict => {
                formatter.write_str("CAS turn is already correlated with different durable facts")
            }
            Self::ActiveCasTurnCollision => {
                formatter.write_str("execution snapshot already has a different active CAS turn")
            }
            Self::ExecutionSnapshotCollision => {
                formatter.write_str("execution snapshot identity collides with durable state")
            }
            Self::ActiveSteeringRouteConflict => {
                formatter.write_str("awaiting-steering routes disagree with active publication")
            }
            Self::EmptySubmission => formatter.write_str("an empty draft cannot be submitted"),
            Self::AdmissionIdentityCollision => {
                formatter.write_str("submission natural identity collides with durable state")
            }
            Self::AssetReferenceSetConflict => {
                formatter.write_str("sealed asset-reference proof disagrees with the content")
            }
            Self::ReplacementEditAlreadyActive => {
                formatter.write_str("the current draft already has replacement-edit intent")
            }
            Self::ReplacementEditNotActive => {
                formatter.write_str("the current draft has no replacement-edit intent")
            }
            Self::ReplacementDraftNotEmpty => {
                formatter.write_str("replacement editing requires an empty current draft")
            }
            Self::ReplacementTargetConflict => {
                formatter.write_str("replacement target or selected transcript proof changed")
            }
            Self::UnchangedPayload => {
                formatter.write_str("unchanged draft payload must not be written")
            }
            Self::TimestampRegressed => {
                formatter.write_str("draft update timestamp precedes current durable activity")
            }
        }
    }
}

impl Error for SyndicMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Build(source) => Some(source),
            Self::Revision(source) => Some(source),
            Self::NativeTurnCount(source) => Some(source),
            Self::Record(source) => Some(source),
            Self::Value(source) => Some(source),
            _ => None,
        }
    }
}

impl DomainCallbackError for SyndicMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for SyndicMutationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl From<MutationBuildError> for SyndicMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl From<RevisionError> for SyndicMutationError {
    fn from(source: RevisionError) -> Self {
        Self::Revision(source)
    }
}

impl From<CasNativeTurnCountError> for SyndicMutationError {
    fn from(source: CasNativeTurnCountError) -> Self {
        Self::NativeTurnCount(source)
    }
}

impl From<SyndicRecordError> for SyndicMutationError {
    fn from(source: SyndicRecordError) -> Self {
        Self::Record(source)
    }
}

impl From<SyndicValueError> for SyndicMutationError {
    fn from(source: SyndicValueError) -> Self {
        Self::Value(source)
    }
}
