use beryl_backend::ThreadSummary;
use beryl_model::{
    conversation::{ConversationThreadId, RegisteredConversationThread},
    workspace::WorkspaceId,
};

use super::ConversationSurfaceState;

pub(in crate::shell) fn registered_thread_from_summary(
    execution_target: &WorkspaceId,
    summary: &ThreadSummary,
) -> RegisteredConversationThread {
    let mut thread = RegisteredConversationThread::new(
        ConversationThreadId::new(summary.id.clone()),
        execution_target.clone(),
        summary.preview.clone(),
        summary.name.clone(),
        summary.created_at,
        summary.updated_at,
    );
    if let Some(parent_thread_id) =
        summary
            .forked_from_id
            .as_ref()
            .map(|id| id.trim())
            .filter(|parent_thread_id| {
                !parent_thread_id.is_empty() && *parent_thread_id != summary.id.as_str()
            })
    {
        thread = thread
            .with_branch_parent_thread_id(ConversationThreadId::new(parent_thread_id.to_string()));
    }
    thread
}

pub(in crate::shell) fn normalized_thread_name(name: Option<&str>) -> Option<String> {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

pub(in crate::shell) fn first_real_branch_user_input_fragment_text<'a>(
    surface: &'a ConversationSurfaceState,
    thread: &RegisteredConversationThread,
) -> Option<&'a str> {
    let turns = surface.execution_details.turns();
    let start_index = if let Some(bootstrap_turn_id) = thread.branch_bootstrap_turn_id() {
        turns
            .iter()
            .position(|turn| turn.turn_id.as_deref() == Some(bootstrap_turn_id.as_str()))?
            .saturating_add(1)
    } else if let Some(source_turn_id) = thread.branch_source_turn_id() {
        turns
            .iter()
            .position(|turn| turn.turn_id.as_deref() == Some(source_turn_id.as_str()))?
            .saturating_add(2)
    } else {
        1
    };

    turns
        .iter()
        .skip(start_index)
        .find_map(|turn| turn.first_user_input_fragment_text())
}
