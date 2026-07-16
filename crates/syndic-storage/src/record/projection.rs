use beryl_model::{
    ProjectionRevision, SyndicItemId, SyndicPathDigest, SyndicResourceId, SyndicThreadId,
    SyndicTurnId, ThreadRevision,
};

use crate::{
    AssistantMessagePhase, ContentReference, ProjectionLifecycle, ProviderItemKind,
    ProviderItemLifecycle, SourceEventSequence, SyndicRecordError, SyndicTimestamp,
    TranscriptGeneration, TurnItemOrdinal, UnsupportedHistoryReason,
};

mod build;
mod item;
mod resource;
mod source;

pub use build::*;
pub use item::*;
pub use resource::*;
pub use source::*;

/// Closed canonical item classification retained below transcript projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalItemKind {
    UserInput,
    AssistantMessage(AssistantMessagePhase),
    ProviderText(ProviderItemKind),
    Operational(ProviderItemKind),
    Activity(ProviderItemKind),
    GeneratedMedia,
    Unsupported(ProviderItemKind),
}

/// Closed typed payload retained by one canonical item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalItemPayload {
    UserInput {
        content: ContentReference,
        marker_count: u64,
    },
    Text(ContentReference),
    Activity,
    GeneratedMedia(SyndicResourceId),
    Unsupported(UnsupportedHistoryReason),
}

impl CanonicalItemPayload {
    #[must_use]
    pub const fn user_input(content: ContentReference, marker_count: u64) -> Self {
        Self::UserInput {
            content,
            marker_count,
        }
    }

    #[must_use]
    pub const fn text(content: ContentReference) -> Self {
        Self::Text(content)
    }

    #[must_use]
    pub const fn activity() -> Self {
        Self::Activity
    }

    #[must_use]
    pub const fn generated_media(resource: SyndicResourceId) -> Self {
        Self::GeneratedMedia(resource)
    }

    #[must_use]
    pub const fn unsupported(reason: UnsupportedHistoryReason) -> Self {
        Self::Unsupported(reason)
    }

    #[must_use]
    pub const fn content(&self) -> Option<ContentReference> {
        match self {
            Self::UserInput { content, .. } | Self::Text(content) => Some(*content),
            Self::Activity | Self::GeneratedMedia(_) | Self::Unsupported(_) => None,
        }
    }

    #[must_use]
    pub const fn marker_count(&self) -> u64 {
        match self {
            Self::UserInput { marker_count, .. } => *marker_count,
            Self::Text(_) | Self::Activity | Self::GeneratedMedia(_) | Self::Unsupported(_) => 0,
        }
    }
}

/// One canonical lightweight item with exact turn and source frontiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalItemRecord {
    id: SyndicItemId,
    turn_id: SyndicTurnId,
    ordinal: TurnItemOrdinal,
    revision: ProjectionRevision,
    source_event: Option<SourceEventSequence>,
    source_event_count: u64,
    cas_source: Option<CasItemSource>,
    provider_kind: ProviderItemKind,
    provider_lifecycle: ProviderItemLifecycle,
    disposition: ProviderItemDisposition,
    assistant_phase: Option<AssistantMessagePhase>,
    payload: CanonicalItemPayload,
}

impl CanonicalItemRecord {
    #[must_use]
    pub const fn local_user_input(
        id: SyndicItemId,
        turn_id: SyndicTurnId,
        ordinal: TurnItemOrdinal,
        revision: ProjectionRevision,
        content: ContentReference,
        marker_count: u64,
    ) -> Self {
        Self {
            id,
            turn_id,
            ordinal,
            revision,
            source_event: None,
            source_event_count: 0,
            cas_source: None,
            provider_kind: ProviderItemKind::UserMessage,
            provider_lifecycle: ProviderItemLifecycle::AwaitingCorrelation,
            disposition: ProviderItemDisposition::CorrelatedUserInput {
                content,
                marker_count,
            },
            assistant_phase: None,
            payload: CanonicalItemPayload::user_input(content, marker_count),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_source_state(
        id: SyndicItemId,
        turn_id: SyndicTurnId,
        ordinal: TurnItemOrdinal,
        revision: ProjectionRevision,
        source_event: Option<SourceEventSequence>,
        source_event_count: u64,
        cas_source: Option<CasItemSource>,
        provider_kind: ProviderItemKind,
        provider_lifecycle: ProviderItemLifecycle,
        disposition: ProviderItemDisposition,
        assistant_phase: Option<AssistantMessagePhase>,
        payload: CanonicalItemPayload,
    ) -> Result<Self, SyndicRecordError> {
        let value = Self {
            id,
            turn_id,
            ordinal,
            revision,
            source_event,
            source_event_count,
            cas_source,
            provider_kind,
            provider_lifecycle,
            disposition,
            assistant_phase,
            payload,
        };
        value.validate_shape()?;
        Ok(value)
    }
    #[must_use]
    pub const fn id(&self) -> SyndicItemId {
        self.id
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn ordinal(&self) -> TurnItemOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn revision(&self) -> ProjectionRevision {
        self.revision
    }
    #[must_use]
    pub const fn kind(&self) -> CanonicalItemKind {
        if matches!(self.disposition, ProviderItemDisposition::Unsupported(_)) {
            return CanonicalItemKind::Unsupported(self.provider_kind);
        }
        match self.provider_kind {
            ProviderItemKind::UserMessage => CanonicalItemKind::UserInput,
            ProviderItemKind::AgentMessage => {
                CanonicalItemKind::AssistantMessage(match self.assistant_phase {
                    Some(phase) => phase,
                    None => AssistantMessagePhase::Unknown,
                })
            }
            ProviderItemKind::StandaloneImageGeneration => CanonicalItemKind::GeneratedMedia,
            kind if matches!(self.disposition, ProviderItemDisposition::ActivityOnly) => {
                CanonicalItemKind::Activity(kind)
            }
            ProviderItemKind::HookPrompt | ProviderItemKind::Plan | ProviderItemKind::Reasoning => {
                CanonicalItemKind::ProviderText(self.provider_kind)
            }
            kind => CanonicalItemKind::Operational(kind),
        }
    }
    #[must_use]
    pub const fn source_event(&self) -> Option<SourceEventSequence> {
        self.source_event
    }
    #[must_use]
    pub const fn source_event_count(&self) -> u64 {
        self.source_event_count
    }
    #[must_use]
    pub const fn cas_source(&self) -> Option<&CasItemSource> {
        self.cas_source.as_ref()
    }
    #[must_use]
    pub const fn provider_kind(&self) -> ProviderItemKind {
        self.provider_kind
    }
    #[must_use]
    pub const fn provider_lifecycle(&self) -> ProviderItemLifecycle {
        self.provider_lifecycle
    }
    #[must_use]
    pub const fn disposition(&self) -> ProviderItemDisposition {
        self.disposition
    }
    #[must_use]
    pub const fn assistant_phase(&self) -> Option<AssistantMessagePhase> {
        self.assistant_phase
    }
    #[must_use]
    pub const fn payload(&self) -> &CanonicalItemPayload {
        &self.payload
    }

    fn validate_shape(&self) -> Result<(), SyndicRecordError> {
        if (self.source_event_count == 0) != self.source_event.is_none()
            || !source::disposition_is_valid(self.provider_kind, self.disposition)
        {
            return Err(SyndicRecordError::InvalidProviderItemDisposition);
        }
        match self.provider_lifecycle {
            ProviderItemLifecycle::AwaitingCorrelation
                if self.provider_kind == ProviderItemKind::UserMessage
                    && self.cas_source.is_none()
                    && self.source_event_count == 0 => {}
            ProviderItemLifecycle::Started | ProviderItemLifecycle::Completed
                if self.cas_source.is_some() && self.source_event_count != 0 => {}
            _ => return Err(SyndicRecordError::InvalidProviderItemLifecycle),
        }
        let payload_matches = match (self.disposition, &self.payload) {
            (
                ProviderItemDisposition::CorrelatedUserInput {
                    content,
                    marker_count,
                },
                CanonicalItemPayload::UserInput {
                    content: actual,
                    marker_count: actual_markers,
                },
            ) => content == *actual && marker_count == *actual_markers,
            (ProviderItemDisposition::CanonicalText, CanonicalItemPayload::Text(_))
            | (ProviderItemDisposition::ActivityOnly, CanonicalItemPayload::Activity) => true,
            (
                ProviderItemDisposition::GeneratedMedia { resource_id },
                CanonicalItemPayload::GeneratedMedia(actual),
            ) => resource_id == *actual,
            (
                ProviderItemDisposition::Unsupported(reason),
                CanonicalItemPayload::Unsupported(actual),
            ) => reason == *actual,
            _ => false,
        };
        if !payload_matches
            || (self.provider_kind == ProviderItemKind::AgentMessage)
                != self.assistant_phase.is_some()
        {
            return Err(SyndicRecordError::InvalidProviderItemDisposition);
        }
        Ok(())
    }
}

/// Current bounded transcript-view frontier for one named thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptViewHeadRecord {
    thread_id: SyndicThreadId,
    generation: TranscriptGeneration,
    revision: ProjectionRevision,
    entry_count: u64,
    committed_tail: Option<SyndicTurnId>,
    selected_path_digest: SyndicPathDigest,
    lifecycle: ProjectionLifecycle,
}

impl TranscriptViewHeadRecord {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        generation: TranscriptGeneration,
        revision: ProjectionRevision,
        entry_count: u64,
        committed_tail: Option<SyndicTurnId>,
        selected_path_digest: SyndicPathDigest,
        lifecycle: ProjectionLifecycle,
    ) -> Self {
        Self {
            thread_id,
            generation,
            revision,
            entry_count,
            committed_tail,
            selected_path_digest,
            lifecycle,
        }
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn generation(&self) -> TranscriptGeneration {
        self.generation
    }
    #[must_use]
    pub const fn revision(&self) -> ProjectionRevision {
        self.revision
    }
    #[must_use]
    pub const fn entry_count(&self) -> u64 {
        self.entry_count
    }
    #[must_use]
    pub const fn committed_tail(&self) -> Option<SyndicTurnId> {
        self.committed_tail
    }
    #[must_use]
    pub const fn selected_path_digest(&self) -> SyndicPathDigest {
        self.selected_path_digest
    }
    #[must_use]
    pub const fn lifecycle(&self) -> ProjectionLifecycle {
        self.lifecycle
    }
}

/// Compact history-derived facts used by Beryl-home catalog joins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistorySummaryRecord {
    thread_id: SyndicThreadId,
    thread_revision: ThreadRevision,
    committed_tail: Option<SyndicTurnId>,
    selected_path_digest: SyndicPathDigest,
    complete: bool,
    last_activity_at: SyndicTimestamp,
}

impl HistorySummaryRecord {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        thread_revision: ThreadRevision,
        committed_tail: Option<SyndicTurnId>,
        selected_path_digest: SyndicPathDigest,
        complete: bool,
        last_activity_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            thread_revision,
            committed_tail,
            selected_path_digest,
            complete,
            last_activity_at,
        }
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn thread_revision(&self) -> ThreadRevision {
        self.thread_revision
    }
    #[must_use]
    pub const fn committed_tail(&self) -> Option<SyndicTurnId> {
        self.committed_tail
    }
    #[must_use]
    pub const fn selected_path_digest(&self) -> SyndicPathDigest {
        self.selected_path_digest
    }
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }
    #[must_use]
    pub const fn last_activity_at(&self) -> SyndicTimestamp {
        self.last_activity_at
    }
}
