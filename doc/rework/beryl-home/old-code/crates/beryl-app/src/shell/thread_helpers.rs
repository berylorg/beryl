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
    RegisteredConversationThread::new(
        ConversationThreadId::new(summary.id.clone()),
        execution_target.clone(),
        summary.preview.clone(),
        summary.created_at,
        summary.updated_at,
    )
}

pub(in crate::shell) fn first_real_branch_user_input_fragment_text<'a>(
    surface: &'a ConversationSurfaceState,
    thread: &RegisteredConversationThread,
) -> Option<&'a str> {
    let turns = surface.active_turn_state.turns();
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
