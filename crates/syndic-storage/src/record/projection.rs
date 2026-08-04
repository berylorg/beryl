use beryl_model::{
    ProjectionRevision, SealedAssetReferenceSetProof, SyndicItemId, SyndicPathDigest,
    SyndicResourceId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};

use crate::{
    AssistantMessagePhase, ContentReference, ProjectionLifecycle, ProviderFrameHistorySupportV1,
    ProviderFrameObservationSummaryV1, ProviderItemKind, ProviderItemLifecycle,
    ProviderNarrativeCompletionDisposition, SealedProviderFrameReference, SourceEventSequence,
    SyndicRecordError, SyndicTimestamp, TranscriptGeneration, TurnItemOrdinal,
};

mod build;
mod item;
mod presentation;
mod resource;
mod source;
mod text_source;

pub use build::*;
pub use item::*;
pub use presentation::*;
pub use resource::*;
pub use source::*;
pub use text_source::*;

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
    assistant_phase: Option<AssistantMessagePhase>,
    provider: Option<SealedProviderFrameReference>,
    narrative_completion: Option<ProviderNarrativeCompletionDisposition>,
    presentation: CanonicalItemPresentation,
}

impl CanonicalItemRecord {
    #[must_use]
    pub fn local_user_input(
        id: SyndicItemId,
        turn_id: SyndicTurnId,
        ordinal: TurnItemOrdinal,
        revision: ProjectionRevision,
        content: ContentReference,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
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
            assistant_phase: None,
            provider: None,
            narrative_completion: None,
            presentation: CanonicalItemPresentation::user_input(content, asset_reference_set),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_provider_state(
        id: SyndicItemId,
        turn_id: SyndicTurnId,
        ordinal: TurnItemOrdinal,
        revision: ProjectionRevision,
        source_event: SourceEventSequence,
        source_event_count: u64,
        cas_source: CasItemSource,
        assistant_phase: Option<AssistantMessagePhase>,
        provider: SealedProviderFrameReference,
        narrative_completion: Option<ProviderNarrativeCompletionDisposition>,
        presentation: CanonicalItemPresentation,
    ) -> Result<Self, SyndicRecordError> {
        let provider_kind = provider.frame().item_kind();
        let provider_lifecycle = lifecycle_for_observation(provider.observation());
        let value = Self {
            id,
            turn_id,
            ordinal,
            revision,
            source_event: Some(source_event),
            source_event_count,
            cas_source: Some(cas_source),
            provider_kind,
            provider_lifecycle,
            assistant_phase,
            provider: Some(provider),
            narrative_completion,
            presentation,
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
    pub fn kind(&self) -> CanonicalItemKind {
        match &self.presentation {
            CanonicalItemPresentation::UserInput { .. } => CanonicalItemKind::UserInput,
            CanonicalItemPresentation::Narrative
                if self.provider_kind == ProviderItemKind::AgentMessage =>
            {
                CanonicalItemKind::AssistantMessage(match self.assistant_phase {
                    Some(phase) => phase,
                    None => AssistantMessagePhase::Unknown,
                })
            }
            CanonicalItemPresentation::Narrative => {
                CanonicalItemKind::ProviderText(self.provider_kind)
            }
            CanonicalItemPresentation::Operational => {
                CanonicalItemKind::Operational(self.provider_kind)
            }
            CanonicalItemPresentation::Activity => CanonicalItemKind::Activity(self.provider_kind),
            CanonicalItemPresentation::GeneratedMedia { .. } => CanonicalItemKind::GeneratedMedia,
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
    pub const fn assistant_phase(&self) -> Option<AssistantMessagePhase> {
        self.assistant_phase
    }
    #[must_use]
    pub const fn provider(&self) -> Option<&SealedProviderFrameReference> {
        self.provider.as_ref()
    }
    #[must_use]
    pub const fn narrative_completion(&self) -> Option<ProviderNarrativeCompletionDisposition> {
        self.narrative_completion
    }
    #[must_use]
    pub const fn presentation(&self) -> &CanonicalItemPresentation {
        &self.presentation
    }
    #[must_use]
    pub const fn presentation_content(&self) -> Option<ContentReference> {
        self.presentation.content()
    }
    #[must_use]
    pub const fn projection_source(&self) -> Option<ProjectionTextSource> {
        match &self.presentation {
            CanonicalItemPresentation::UserInput { content, .. } => {
                Some(ProjectionTextSource::composer(*content))
            }
            CanonicalItemPresentation::Narrative => match &self.provider {
                Some(provider) => match provider.narrative() {
                    Some(narrative) => Some(ProjectionTextSource::provider_narrative(narrative)),
                    None => None,
                },
                None => None,
            },
            CanonicalItemPresentation::Operational
            | CanonicalItemPresentation::Activity
            | CanonicalItemPresentation::GeneratedMedia { .. } => None,
        }
    }
    #[must_use]
    pub const fn provider_content(&self) -> Option<ContentReference> {
        match &self.provider {
            Some(provider) => Some(provider.content()),
            None => None,
        }
    }
    #[must_use]
    pub const fn history_support(&self) -> ProviderFrameHistorySupportV1 {
        match &self.provider {
            Some(provider) => provider.history_support(),
            None => ProviderFrameHistorySupportV1::Supported,
        }
    }
    #[must_use]
    pub const fn is_history_blocking(&self) -> bool {
        !self.history_support().is_supported()
            || matches!(
                self.narrative_completion,
                Some(ProviderNarrativeCompletionDisposition::Mismatch { .. })
            )
    }

    fn validate_shape(&self) -> Result<(), SyndicRecordError> {
        if (self.source_event_count == 0) != self.source_event.is_none() {
            return Err(SyndicRecordError::InvalidProviderItemDisposition);
        }
        match self.provider_lifecycle {
            ProviderItemLifecycle::AwaitingCorrelation
                if self.provider_kind == ProviderItemKind::UserMessage
                    && self.cas_source.is_none()
                    && self.source_event_count == 0
                    && self.provider.is_none() => {}
            ProviderItemLifecycle::Started | ProviderItemLifecycle::Completed
                if self.cas_source.is_some()
                    && self.source_event_count != 0
                    && self.provider.is_some() => {}
            _ => return Err(SyndicRecordError::InvalidProviderItemLifecycle),
        }
        if !presentation_is_valid(self.provider_kind, &self.presentation)
            || (self.provider_kind == ProviderItemKind::AgentMessage)
                != self.assistant_phase.is_some()
            || (self.provider_kind.requires_narrative()
                && self.provider_lifecycle == ProviderItemLifecycle::Completed)
                != self.narrative_completion.is_some()
        {
            return Err(SyndicRecordError::InvalidProviderItemDisposition);
        }
        if let (Some(source), Some(provider)) = (&self.cas_source, &self.provider)
            && (source.item_id() != provider.frame().item_id()
                || self.provider_kind != provider.frame().item_kind()
                || self.provider_lifecycle != lifecycle_for_observation(provider.observation()))
        {
            return Err(SyndicRecordError::SourceIdentityMismatch);
        }
        if let Some(disposition) = self.narrative_completion {
            let provider = self
                .provider
                .as_ref()
                .ok_or(SyndicRecordError::InvalidProviderItemDisposition)?;
            let narrative = provider
                .narrative()
                .ok_or(SyndicRecordError::InvalidProviderItemDisposition)?;
            let live_bytes = narrative.logical_utf8_bytes();
            let completion_bytes = provider.frame().logical_utf8_bytes();
            let valid = match disposition {
                ProviderNarrativeCompletionDisposition::Equal => live_bytes == completion_bytes,
                ProviderNarrativeCompletionDisposition::Mismatch { utf8_byte_offset } => {
                    utf8_byte_offset <= live_bytes.min(completion_bytes)
                        && (live_bytes != completion_bytes || utf8_byte_offset < live_bytes)
                }
            };
            if !valid {
                return Err(SyndicRecordError::InvalidProviderItemDisposition);
            }
        }
        Ok(())
    }
}

const fn lifecycle_for_observation(
    observation: ProviderFrameObservationSummaryV1,
) -> ProviderItemLifecycle {
    match observation {
        ProviderFrameObservationSummaryV1::Started(_)
        | ProviderFrameObservationSummaryV1::Delta => ProviderItemLifecycle::Started,
        ProviderFrameObservationSummaryV1::Completed(_) => ProviderItemLifecycle::Completed,
    }
}

const fn presentation_is_valid(
    kind: ProviderItemKind,
    presentation: &CanonicalItemPresentation,
) -> bool {
    match kind {
        ProviderItemKind::UserMessage => {
            matches!(presentation, CanonicalItemPresentation::UserInput { .. })
        }
        ProviderItemKind::AgentMessage | ProviderItemKind::Plan => {
            matches!(presentation, CanonicalItemPresentation::Narrative)
        }
        ProviderItemKind::CommandExecution
        | ProviderItemKind::FileChange
        | ProviderItemKind::McpToolCall
        | ProviderItemKind::DynamicToolCall => {
            matches!(presentation, CanonicalItemPresentation::Operational)
        }
        ProviderItemKind::HookPrompt
        | ProviderItemKind::Reasoning
        | ProviderItemKind::CollabAgentToolCall
        | ProviderItemKind::SubAgentActivity
        | ProviderItemKind::WebSearch
        | ProviderItemKind::ImageView
        | ProviderItemKind::Sleep
        | ProviderItemKind::EnteredReviewMode
        | ProviderItemKind::ExitedReviewMode
        | ProviderItemKind::ContextCompaction => {
            matches!(presentation, CanonicalItemPresentation::Activity)
        }
        ProviderItemKind::StandaloneImageGeneration => {
            matches!(
                presentation,
                CanonicalItemPresentation::GeneratedMedia { .. }
            )
        }
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
    revision: ProjectionRevision,
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
        revision: ProjectionRevision,
        thread_revision: ThreadRevision,
        committed_tail: Option<SyndicTurnId>,
        selected_path_digest: SyndicPathDigest,
        complete: bool,
        last_activity_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            revision,
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
    pub const fn revision(&self) -> ProjectionRevision {
        self.revision
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
