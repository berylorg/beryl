use beryl_model::{CasItemId, CasThreadId, CasTurnId, SyndicItemId, SyndicTurnId};

use crate::{
    AssistantMessagePhase, ContentReference, ProviderItemKind, SourceEventSequence,
    SyndicRecordError, TurnEndStatus, UnsupportedHistoryReason,
};

use super::super::{MAX_LARGE_TEXT_BYTES, validate_text};

/// Exact external source tuple carried by a normalized event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CasTurnSource {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
}

impl CasTurnSource {
    #[must_use]
    pub const fn new(thread_id: CasThreadId, turn_id: CasTurnId) -> Self {
        Self { thread_id, turn_id }
    }
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }
    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }
}

/// Exact external source tuple carried by one canonical item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CasItemSource {
    turn: CasTurnSource,
    item_id: CasItemId,
}

impl CasItemSource {
    #[must_use]
    pub const fn new(turn: CasTurnSource, item_id: CasItemId) -> Self {
        Self { turn, item_id }
    }
    #[must_use]
    pub const fn turn(&self) -> &CasTurnSource {
        &self.turn
    }
    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }
}

/// Bounded exact UTF-8 carried by one coalesced source event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEventText(Box<str>);

impl SourceEventText {
    pub fn new(text: impl AsRef<str>) -> Result<Self, SyndicRecordError> {
        validate_text(
            "source-event text",
            text.as_ref(),
            MAX_LARGE_TEXT_BYTES,
            false,
        )
        .map(Self)
    }

    #[must_use]
    pub const fn as_str(&self) -> &str {
        &self.0
    }
}

/// Closed durable representation selected for one normalized provider item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderItemDisposition {
    CorrelatedUserInput {
        content: ContentReference,
        marker_count: u64,
    },
    CanonicalText,
    ActivityOnly,
    GeneratedMedia {
        resource_id: beryl_model::SyndicResourceId,
    },
    Unsupported(UnsupportedHistoryReason),
}

impl ProviderItemDisposition {
    #[must_use]
    pub const fn is_history_blocking(self) -> bool {
        matches!(self, Self::Unsupported(_))
    }
}

/// Exact normalized identity, kind, and durable representation of one provider item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceItemDescriptor {
    item_id: SyndicItemId,
    cas_item_id: CasItemId,
    kind: ProviderItemKind,
    disposition: ProviderItemDisposition,
}

impl SourceItemDescriptor {
    pub fn new(
        item_id: SyndicItemId,
        cas_item_id: CasItemId,
        kind: ProviderItemKind,
        disposition: ProviderItemDisposition,
    ) -> Result<Self, SyndicRecordError> {
        if !disposition_is_valid(kind, disposition) {
            return Err(SyndicRecordError::InvalidProviderItemDisposition);
        }
        Ok(Self {
            item_id,
            cas_item_id,
            kind,
            disposition,
        })
    }

    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn cas_item_id(&self) -> &CasItemId {
        &self.cas_item_id
    }
    #[must_use]
    pub const fn kind(&self) -> ProviderItemKind {
        self.kind
    }
    #[must_use]
    pub const fn disposition(&self) -> ProviderItemDisposition {
        self.disposition
    }
}

pub(crate) const fn disposition_is_valid(
    kind: ProviderItemKind,
    disposition: ProviderItemDisposition,
) -> bool {
    if matches!(disposition, ProviderItemDisposition::Unsupported(_)) {
        return true;
    }
    match kind {
        ProviderItemKind::UserMessage => {
            matches!(
                disposition,
                ProviderItemDisposition::CorrelatedUserInput { .. }
            )
        }
        ProviderItemKind::HookPrompt
        | ProviderItemKind::AgentMessage
        | ProviderItemKind::Plan
        | ProviderItemKind::CommandExecution
        | ProviderItemKind::FileChange => {
            matches!(disposition, ProviderItemDisposition::CanonicalText)
        }
        ProviderItemKind::Reasoning
        | ProviderItemKind::McpToolCall
        | ProviderItemKind::DynamicToolCall
        | ProviderItemKind::CollabAgentToolCall
        | ProviderItemKind::WebSearch
        | ProviderItemKind::Sleep => matches!(
            disposition,
            ProviderItemDisposition::CanonicalText | ProviderItemDisposition::ActivityOnly
        ),
        ProviderItemKind::SubAgentActivity
        | ProviderItemKind::ImageView
        | ProviderItemKind::EnteredReviewMode
        | ProviderItemKind::ExitedReviewMode
        | ProviderItemKind::ContextCompaction => {
            matches!(disposition, ProviderItemDisposition::ActivityOnly)
        }
        ProviderItemKind::StandaloneImageGeneration => {
            matches!(disposition, ProviderItemDisposition::GeneratedMedia { .. })
        }
    }
}

/// Closed normalized effect retained by one source event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceEventPayload {
    TurnActivated,
    ItemStarted {
        item: SourceItemDescriptor,
        assistant_phase: Option<AssistantMessagePhase>,
    },
    ItemDelta {
        item_id: SyndicItemId,
        cas_item_id: CasItemId,
        expected_kind: ProviderItemKind,
        text: SourceEventText,
    },
    ItemCompleted {
        item: SourceItemDescriptor,
        assistant_phase: Option<AssistantMessagePhase>,
    },
    TurnEnded(TurnEndStatus),
}

impl SourceEventPayload {
    #[must_use]
    pub const fn item_id(&self) -> Option<SyndicItemId> {
        match self {
            Self::ItemStarted { item, .. } | Self::ItemCompleted { item, .. } => {
                Some(item.item_id())
            }
            Self::ItemDelta { item_id, .. } => Some(*item_id),
            Self::TurnActivated | Self::TurnEnded(_) => None,
        }
    }

    #[must_use]
    pub const fn cas_item_id(&self) -> Option<&CasItemId> {
        match self {
            Self::ItemStarted { item, .. } | Self::ItemCompleted { item, .. } => {
                Some(item.cas_item_id())
            }
            Self::ItemDelta { cas_item_id, .. } => Some(cas_item_id),
            Self::TurnActivated | Self::TurnEnded(_) => None,
        }
    }
}

/// One normalized bounded live-source event in exact per-turn sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEventRecord {
    turn_id: SyndicTurnId,
    sequence: SourceEventSequence,
    source: Option<CasTurnSource>,
    payload: SourceEventPayload,
}

impl SourceEventRecord {
    pub fn new(
        turn_id: SyndicTurnId,
        sequence: SourceEventSequence,
        source: Option<CasTurnSource>,
        payload: SourceEventPayload,
    ) -> Result<Self, SyndicRecordError> {
        if payload.item_id().is_some() && source.is_none() {
            return Err(SyndicRecordError::SourceIdentityMismatch);
        }
        if let SourceEventPayload::ItemStarted {
            item,
            assistant_phase,
        }
        | SourceEventPayload::ItemCompleted {
            item,
            assistant_phase,
        } = &payload
            && (item.kind() == ProviderItemKind::AgentMessage) != assistant_phase.is_some()
        {
            return Err(SyndicRecordError::InvalidProviderItemDisposition);
        }
        Ok(Self {
            turn_id,
            sequence,
            source,
            payload,
        })
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn sequence(&self) -> SourceEventSequence {
        self.sequence
    }
    #[must_use]
    pub const fn source(&self) -> Option<&CasTurnSource> {
        self.source.as_ref()
    }
    #[must_use]
    pub const fn payload(&self) -> &SourceEventPayload {
        &self.payload
    }
}
