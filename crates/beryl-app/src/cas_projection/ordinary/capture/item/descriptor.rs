use beryl_backend::{ThreadItem, ThreadItemKind};
use beryl_model::{CasItemId, SyndicItemId, SyndicResourceId};
use sha2::{Digest, Sha256};
use syndic_storage::{
    AssistantMessagePhase, ProviderItemDisposition, ProviderItemKind, SourceItemDescriptor,
};

use super::super::LiveCapture;
use crate::cas_projection::ordinary::OrdinaryTurnExecutionError;

pub(super) struct ItemDescriptor<'a> {
    pub(super) source: SourceItemDescriptor,
    pub(super) assistant_phase: Option<AssistantMessagePhase>,
    pub(super) text: ItemText<'a>,
}

#[derive(Clone)]
pub(super) enum ItemText<'a> {
    Empty,
    One(Option<&'a str>),
    Hook(std::slice::Iter<'a, beryl_backend::HookPromptFragment>),
    Reasoning(std::slice::Iter<'a, String>),
    FileChanges(std::slice::Iter<'a, beryl_backend::FileUpdateChange>),
}

impl<'a> ItemText<'a> {
    pub(super) const fn one(text: &'a str) -> Self {
        Self::One(Some(text))
    }

    pub(super) fn file_changes(changes: &'a [beryl_backend::FileUpdateChange]) -> Self {
        Self::FileChanges(changes.iter())
    }
}

impl<'a> Iterator for ItemText<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::One(text) => text.take(),
            Self::Hook(fragments) => fragments.next().map(|fragment| fragment.text.as_str()),
            Self::Reasoning(summary) => summary.next().map(String::as_str),
            Self::FileChanges(changes) => changes.next().map(|change| change.diff.as_str()),
        }
    }
}

impl<'a> ItemDescriptor<'a> {
    pub(super) fn new(
        capture: &LiveCapture,
        item: &'a ThreadItem,
    ) -> Result<Self, OrdinaryTurnExecutionError> {
        let kind = provider_kind(item.kind());
        let item_id = if kind == ProviderItemKind::UserMessage {
            capture.submitted_item_id
        } else {
            syndic_item_id(capture, item.id())
        };
        let disposition = disposition(capture, item, item_id);
        let source = SourceItemDescriptor::new(item_id, item.id().clone(), kind, disposition)?;
        Ok(Self {
            source,
            assistant_phase: assistant_phase(item),
            text: item_text(item),
        })
    }
}

fn disposition(
    capture: &LiveCapture,
    item: &ThreadItem,
    item_id: SyndicItemId,
) -> ProviderItemDisposition {
    match item {
        ThreadItem::UserMessage(_) => ProviderItemDisposition::CorrelatedUserInput {
            content: capture.submitted_content,
            marker_count: 0,
        },
        ThreadItem::HookPrompt(_)
        | ThreadItem::AgentMessage(_)
        | ThreadItem::Plan(_)
        | ThreadItem::Reasoning(_)
        | ThreadItem::CommandExecution(_)
        | ThreadItem::FileChange(_)
        | ThreadItem::WebSearch(_) => ProviderItemDisposition::CanonicalText,
        ThreadItem::McpToolCall(_)
        | ThreadItem::DynamicToolCall(_)
        | ThreadItem::CollabAgentToolCall(_)
        | ThreadItem::SubAgentActivity(_)
        | ThreadItem::ImageView(_)
        | ThreadItem::Sleep(_)
        | ThreadItem::EnteredReviewMode(_)
        | ThreadItem::ExitedReviewMode(_)
        | ThreadItem::ContextCompaction(_) => ProviderItemDisposition::ActivityOnly,
        ThreadItem::ImageGeneration(_) => ProviderItemDisposition::GeneratedMedia {
            resource_id: syndic_resource_id(capture, item_id),
        },
    }
}

fn item_text(item: &ThreadItem) -> ItemText<'_> {
    match item {
        ThreadItem::UserMessage(_)
        | ThreadItem::McpToolCall(_)
        | ThreadItem::DynamicToolCall(_)
        | ThreadItem::CollabAgentToolCall(_)
        | ThreadItem::SubAgentActivity(_)
        | ThreadItem::ImageView(_)
        | ThreadItem::Sleep(_)
        | ThreadItem::ImageGeneration(_)
        | ThreadItem::EnteredReviewMode(_)
        | ThreadItem::ExitedReviewMode(_)
        | ThreadItem::ContextCompaction(_) => ItemText::Empty,
        ThreadItem::HookPrompt(prompt) => ItemText::Hook(prompt.fragments.iter()),
        ThreadItem::AgentMessage(message) => ItemText::one(&message.text),
        ThreadItem::Plan(plan) => ItemText::one(&plan.text),
        ThreadItem::Reasoning(reasoning) => ItemText::Reasoning(reasoning.summary.iter()),
        ThreadItem::CommandExecution(command) => {
            ItemText::One(command.aggregated_output.as_deref())
        }
        ThreadItem::FileChange(change) => ItemText::file_changes(&change.changes),
        ThreadItem::WebSearch(search) => ItemText::one(&search.query),
    }
}

fn assistant_phase(item: &ThreadItem) -> Option<AssistantMessagePhase> {
    let ThreadItem::AgentMessage(message) = item else {
        return None;
    };
    Some(match message.phase {
        Some(beryl_backend::ProtocolPhase::Commentary) => AssistantMessagePhase::Commentary,
        Some(beryl_backend::ProtocolPhase::FinalAnswer) => AssistantMessagePhase::FinalAnswer,
        None => AssistantMessagePhase::Unknown,
    })
}

pub(super) const fn provider_kind(kind: ThreadItemKind) -> ProviderItemKind {
    match kind {
        ThreadItemKind::UserMessage => ProviderItemKind::UserMessage,
        ThreadItemKind::HookPrompt => ProviderItemKind::HookPrompt,
        ThreadItemKind::AgentMessage => ProviderItemKind::AgentMessage,
        ThreadItemKind::Plan => ProviderItemKind::Plan,
        ThreadItemKind::Reasoning => ProviderItemKind::Reasoning,
        ThreadItemKind::CommandExecution => ProviderItemKind::CommandExecution,
        ThreadItemKind::FileChange => ProviderItemKind::FileChange,
        ThreadItemKind::McpToolCall => ProviderItemKind::McpToolCall,
        ThreadItemKind::DynamicToolCall => ProviderItemKind::DynamicToolCall,
        ThreadItemKind::CollabAgentToolCall => ProviderItemKind::CollabAgentToolCall,
        ThreadItemKind::SubAgentActivity => ProviderItemKind::SubAgentActivity,
        ThreadItemKind::WebSearch => ProviderItemKind::WebSearch,
        ThreadItemKind::ImageView => ProviderItemKind::ImageView,
        ThreadItemKind::Sleep => ProviderItemKind::Sleep,
        ThreadItemKind::ImageGeneration => ProviderItemKind::StandaloneImageGeneration,
        ThreadItemKind::EnteredReviewMode => ProviderItemKind::EnteredReviewMode,
        ThreadItemKind::ExitedReviewMode => ProviderItemKind::ExitedReviewMode,
        ThreadItemKind::ContextCompaction => ProviderItemKind::ContextCompaction,
    }
}

fn syndic_item_id(capture: &LiveCapture, item: &CasItemId) -> SyndicItemId {
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.live-item.v1");
    hash.update(capture.context.thread_id().as_bytes());
    hash.update(capture.context.turn_id().as_bytes());
    hash.update(capture.source.thread_id().as_str().as_bytes());
    hash.update([0]);
    hash.update(capture.source.turn_id().as_str().as_bytes());
    hash.update([0]);
    hash.update(item.as_str().as_bytes());
    truncated_identity(hash)
}

fn syndic_resource_id(capture: &LiveCapture, item: SyndicItemId) -> SyndicResourceId {
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.generated-media-resource.v1");
    hash.update(capture.context.thread_id().as_bytes());
    hash.update(capture.context.turn_id().as_bytes());
    hash.update(item.as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&digest[..16]);
    SyndicResourceId::from_bytes(identity)
}

fn truncated_identity(hash: Sha256) -> SyndicItemId {
    let digest: [u8; 32] = hash.finalize().into();
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&digest[..16]);
    SyndicItemId::from_bytes(identity)
}
