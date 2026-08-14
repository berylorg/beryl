use std::collections::HashSet;

use beryl_backend::ProtocolPhase;

use super::execution_detail::{
    ExecutionItem, ReasoningDetail, TurnExecutionRecord, TurnNarrativeEntry,
};

pub(super) fn project_parent_narrative_turn(
    turn: &TurnExecutionRecord,
) -> Option<TurnExecutionRecord> {
    if turn.released_history_placeholder {
        return Some(turn.clone());
    }

    let items = turn
        .items
        .iter()
        .filter_map(project_parent_narrative_item)
        .collect::<Vec<_>>();
    let item_ids = items
        .iter()
        .map(|item| item.id().to_string())
        .collect::<HashSet<_>>();
    let has_user_prompt = turn.has_user_input_fragments();

    if !has_user_prompt && items.is_empty() {
        return None;
    }

    let mut projected = turn.clone();
    projected.items = items;
    projected.narrative_entries = turn
        .narrative_entries()
        .iter()
        .filter(|entry| match entry {
            TurnNarrativeEntry::UserInput { .. } => true,
            TurnNarrativeEntry::Item { item_id } => item_ids.contains(item_id),
        })
        .cloned()
        .collect();
    projected.error_message = None;
    projected.terminal_assistant_item_id =
        resolve_projected_terminal_assistant_item(&projected.items);
    Some(projected)
}

fn project_parent_narrative_item(item: &ExecutionItem) -> Option<ExecutionItem> {
    match item {
        ExecutionItem::AgentMessage(message)
            if matches!(
                message.phase,
                Some(ProtocolPhase::Commentary) | Some(ProtocolPhase::FinalAnswer) | None
            ) =>
        {
            (!message.text.is_empty()).then(|| ExecutionItem::AgentMessage(message.clone()))
        }
        ExecutionItem::Reasoning(reasoning) => {
            project_reasoning_summary(reasoning).map(ExecutionItem::Reasoning)
        }
        ExecutionItem::GeneratedImage(image) => Some(ExecutionItem::GeneratedImage(image.clone())),
        ExecutionItem::CommandExecution(_)
        | ExecutionItem::FileChange(_)
        | ExecutionItem::Generic(_)
        | ExecutionItem::AgentMessage(_) => None,
    }
}

fn project_reasoning_summary(reasoning: &ReasoningDetail) -> Option<ReasoningDetail> {
    let summary = reasoning
        .summary
        .iter()
        .filter(|part| !part.is_empty())
        .cloned()
        .collect::<Vec<_>>();

    if summary.is_empty() {
        return None;
    }

    Some(ReasoningDetail {
        id: reasoning.id.clone(),
        summary,
        content: Vec::new(),
        complete: reasoning.complete,
    })
}

fn resolve_projected_terminal_assistant_item(items: &[ExecutionItem]) -> Option<String> {
    items
        .iter()
        .rev()
        .find_map(|item| match item {
            ExecutionItem::AgentMessage(message)
                if message.phase == Some(ProtocolPhase::FinalAnswer) =>
            {
                Some(message.id.clone())
            }
            _ => None,
        })
        .or_else(|| {
            items.iter().rev().find_map(|item| match item {
                ExecutionItem::AgentMessage(message) => Some(message.id.clone()),
                _ => None,
            })
        })
}
