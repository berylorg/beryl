use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse};
use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
    threaded_decision::{
        ThreadedDecisionOperationId, ThreadedDecisionOutcome, ThreadedDecisionRecord,
    },
};
use gpui::{Context, Window};

use crate::threaded_decision_dynamic_tools::{
    DecisionResolutionToolResult, RESOLVE_DECISION_BRANCH_TOOL, ThreadedDecisionDynamicToolError,
    decision_resolution_tool_success_response, threaded_decision_tool_failure_response,
};

use super::super::{ShellView, token_usage_snapshot};

impl ShellView {
    pub(in crate::shell) fn handle_decision_resolution_dynamic_tool_request(
        &mut self,
        request: &DynamicToolCallRequest,
        outcome: ThreadedDecisionOutcome,
        summary: String,
        handoff_message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DynamicToolCallResponse {
        let active_matches = self.conversation_surface().is_some_and(|surface| {
            let Some(active) = surface.execution_details.active_turn_identity() else {
                return false;
            };
            active.thread_id.as_deref() == Some(request.thread_id())
                && active.turn_id.as_deref() == Some(request.turn_id())
        });
        if !active_matches {
            return threaded_decision_tool_failure_response(
                request,
                ThreadedDecisionDynamicToolError::unavailable(
                    "The decision resolution tool must be called from the selected active child decision turn.",
                ),
            );
        }

        let child_thread_id = ConversationThreadId::new(request.thread_id().to_string());
        let Some(record) = self.loaded_workspace().and_then(|loaded| {
            loaded
                .threaded_decision_state
                .active_record_for_child_thread(&child_thread_id)
                .cloned()
        }) else {
            return threaded_decision_tool_failure_response(
                request,
                ThreadedDecisionDynamicToolError::unavailable(
                    "The selected active thread is not bound as an active decision child branch.",
                ),
            );
        };

        let parent_registered = self.loaded_workspace().is_some_and(|loaded| {
            loaded
                .workspace_state
                .thread_registration(record.parent_thread_id())
                .is_some()
        });
        if !parent_registered {
            return threaded_decision_tool_failure_response(
                request,
                ThreadedDecisionDynamicToolError::unavailable(
                    "The bound parent thread is no longer registered in this Beryl workspace.",
                ),
            );
        }

        let provenance = match MutationSource::dynamic_tool_call(
            child_thread_id.clone(),
            ConversationTurnId::new(request.turn_id().to_string()),
            RESOLVE_DECISION_BRANCH_TOOL,
            request.call_id(),
        )
        .and_then(|source| {
            MutationProvenance::new(
                "codex",
                token_usage_snapshot::current_unix_millis(),
                source,
                Some(100),
            )
        }) {
            Ok(provenance) => provenance,
            Err(error) => {
                return threaded_decision_tool_failure_response(
                    request,
                    ThreadedDecisionDynamicToolError::new("invalid_provenance", error.to_string()),
                );
            }
        };

        let timestamp = token_usage_snapshot::current_unix_millis();
        let Some(operation_id) = next_resolution_operation_id(&record, timestamp) else {
            return threaded_decision_tool_failure_response(
                request,
                ThreadedDecisionDynamicToolError::new(
                    "invalid_operation_id",
                    "Beryl could not build a durable resolution operation id.",
                ),
            );
        };

        let state_result = self.workspace_shell_state_mut().map(|loaded| {
            loaded.threaded_decision_state.mark_pending_resolution(
                record.record_id(),
                outcome,
                summary.clone(),
                handoff_message.clone(),
                operation_id,
                provenance.clone(),
            )
        });
        match state_result {
            Some(Ok(changed)) => {
                if changed {
                    self.persist_current_threaded_decision_state();
                }
            }
            Some(Err(error)) => {
                return threaded_decision_tool_failure_response(
                    request,
                    ThreadedDecisionDynamicToolError::unavailable(error.to_string()),
                );
            }
            None => {
                return threaded_decision_tool_failure_response(
                    request,
                    ThreadedDecisionDynamicToolError::unavailable(
                        "No Beryl workspace is loaded for decision resolution.",
                    ),
                );
            }
        }

        if let Some(workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        {
            self.queue_decision_resolution_job(workspace_id, record.record_id().clone());
        }
        self.begin_next_ready_decision_resolution_handoff(window, cx);
        cx.notify();

        decision_resolution_tool_success_response(DecisionResolutionToolResult {
            record_id: record.record_id().as_str().to_string(),
            checklist_item_id: record.checklist_item_id().as_str().to_string(),
            parent_thread_id: record.parent_thread_id().as_str().to_string(),
            child_thread_id: child_thread_id.as_str().to_string(),
            status: "queued_handoff",
            message: Some(
                "Beryl accepted the resolution and will create the parent handoff after the child turn finishes."
                    .to_string(),
            ),
        })
    }
}

fn next_resolution_operation_id(
    record: &ThreadedDecisionRecord,
    timestamp: u64,
) -> Option<ThreadedDecisionOperationId> {
    ThreadedDecisionOperationId::new(format!(
        "resolve_decision_branch_{}_{}",
        sanitize_id_part(record.record_id().as_str()),
        timestamp
    ))
    .ok()
}

fn sanitize_id_part(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            _ => '_',
        })
        .collect();
    if sanitized.is_empty() {
        "untitled".to_string()
    } else {
        sanitized
    }
}
