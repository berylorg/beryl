use std::path::PathBuf;

use beryl_model::{CasThreadId, CasTurnId};
use serde::Deserialize;

use crate::{
    DynamicToolCallRequest, JsonRpcError, ThreadSummary,
    activity::{
        ToolActivityCollabAgentSpawnMetadata, ToolActivityEvent, ToolActivityFileChangeSummary,
        ToolActivityLifecycle, ToolActivitySource,
    },
};

use super::{
    ApprovalRequest, CollabAgentTool, ItemDelta, ItemDeltaPayload, RateLimitSnapshot, ThreadItem,
    ThreadStatus, ThreadTokenUsage, TurnStatus,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompletedTurnStatus {
    Completed,
    Interrupted,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalNonSteerableTurnKind {
    Review,
    Compact,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CodexErrorInfo {
    ContextWindowExceeded,
    SessionBudgetExceeded,
    UsageLimitExceeded,
    ServerOverloaded,
    CyberPolicy,
    HttpConnectionFailed {
        http_status_code: Option<u16>,
    },
    ResponseStreamConnectionFailed {
        http_status_code: Option<u16>,
    },
    InternalServerError,
    Unauthorized,
    BadRequest,
    ThreadRollbackFailed,
    SandboxError,
    ResponseStreamDisconnected {
        http_status_code: Option<u16>,
    },
    ResponseTooManyFailedAttempts {
        http_status_code: Option<u16>,
    },
    ActiveTurnNotSteerable {
        turn_kind: TerminalNonSteerableTurnKind,
    },
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnError {
    pub message: String,
    pub codex_error_info: Option<CodexErrorInfo>,
    #[serde(default)]
    pub additional_details: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletedTurn {
    pub id: CasTurnId,
    pub status: CompletedTurnStatus,
    pub error: Option<TurnError>,
}

/// An exact nonnegative item-lifecycle timestamp supplied by CAS, in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct ItemLifecycleTimestampMs(u64);

impl ItemLifecycleTimestampMs {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnStreamEvent {
    ThreadStarted {
        thread: ThreadSummary,
    },
    AgentLabelUpdated {
        thread_id: String,
        label: String,
    },
    ThreadStatusChanged {
        thread_id: String,
        status: ThreadStatus,
    },
    ThreadClosed {
        thread_id: String,
    },
    TurnStarted {
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        status: TurnStatus,
    },
    TurnCompleted {
        thread_id: CasThreadId,
        turn: CompletedTurn,
    },
    ItemStarted {
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        started_at_ms: ItemLifecycleTimestampMs,
        item: ThreadItem,
    },
    ItemCompleted {
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        completed_at_ms: ItemLifecycleTimestampMs,
        item: ThreadItem,
    },
    ItemDelta(ItemDelta),
    TokenUsageUpdated {
        thread_id: String,
        turn_id: String,
        token_usage: ThreadTokenUsage,
    },
    AccountRateLimitsUpdated {
        rate_limits: RateLimitSnapshot,
    },
    ThreadNameUpdated {
        thread_id: String,
        thread_name: Option<String>,
    },
    ApprovalRequested(ApprovalRequest),
    DynamicToolCallRequested(DynamicToolCallRequest),
    ProtocolError {
        error: JsonRpcError,
    },
}

impl TurnStreamEvent {
    #[must_use]
    pub fn activity(&self) -> Option<ToolActivityEvent> {
        match self {
            Self::ItemStarted {
                thread_id,
                turn_id,
                item,
                ..
            } => item_activity(thread_id, turn_id, item, ToolActivityLifecycle::Started),
            Self::ItemCompleted {
                thread_id,
                turn_id,
                item,
                ..
            } => item_activity(thread_id, turn_id, item, ToolActivityLifecycle::Completed),
            Self::ItemDelta(delta) => delta_activity(delta),
            _ => None,
        }
    }

    #[must_use]
    pub fn tool_activity(&self) -> Option<ToolActivityEvent> {
        self.activity()
            .filter(|activity| activity.source.is_operational_tool())
    }
}

fn item_activity(
    thread_id: &CasThreadId,
    turn_id: &CasTurnId,
    item: &ThreadItem,
    lifecycle: ToolActivityLifecycle,
) -> Option<ToolActivityEvent> {
    let source = ToolActivitySource::from_item_type(item.item_type())?;
    let mut activity = ToolActivityEvent::new(
        thread_id.as_str(),
        turn_id.as_str(),
        item.id().as_str(),
        item.item_type(),
        source,
        lifecycle,
    );

    match item {
        ThreadItem::Reasoning(item) => {
            activity = activity.with_reasoning_summary_text(joined_non_empty_text(&item.summary));
        }
        ThreadItem::CommandExecution(item) => {
            activity = activity
                .with_raw_command(Some(item.command.as_str()))
                .with_command_exec_process_id(item.process_id.as_deref())
                .with_raw_item_status(Some(item.status.as_wire_str()));
        }
        ThreadItem::FileChange(item) => {
            activity = activity
                .with_raw_item_status(Some(item.status.as_wire_str()))
                .with_file_change_summary(Some(file_change_summary(item)));
        }
        ThreadItem::McpToolCall(item) => {
            let resource_uri = item
                .app_context
                .as_ref()
                .and_then(|context| context.resource_uri.as_deref())
                .or(item.mcp_app_resource_uri.as_deref());
            activity = activity
                .with_raw_tool_name(Some(item.tool.as_str()))
                .with_raw_tool_server(Some(item.server.as_str()))
                .with_raw_resource_uri(resource_uri)
                .with_raw_item_status(Some(item.status.as_wire_str()));
        }
        ThreadItem::DynamicToolCall(item) => {
            activity = activity
                .with_raw_tool_name(Some(item.tool.as_str()))
                .with_raw_tool_namespace(item.namespace.as_deref())
                .with_raw_item_status(Some(item.status.as_wire_str()));
        }
        ThreadItem::CollabAgentToolCall(item) => {
            let receiver_thread_ids = item
                .receiver_thread_ids
                .iter()
                .map(ToString::to_string)
                .collect();
            activity = activity
                .with_raw_tool_name(Some(item.tool.as_wire_str()))
                .with_raw_item_status(Some(item.status.as_wire_str()))
                .with_receiver_thread_ids(receiver_thread_ids);
            if item.tool == CollabAgentTool::SpawnAgent {
                activity = activity.with_collab_agent_spawn_metadata(
                    ToolActivityCollabAgentSpawnMetadata::from_raw(
                        item.model.as_deref(),
                        item.reasoning_effort.as_deref(),
                    ),
                );
            }
        }
        ThreadItem::ImageGeneration(item) => {
            activity = activity.with_raw_item_status(Some(item.status.as_str()));
        }
        _ => {}
    }

    Some(activity)
}

fn delta_activity(delta: &ItemDelta) -> Option<ToolActivityEvent> {
    let (summary_index, summary_delta) = match delta.payload() {
        ItemDeltaPayload::ReasoningSummaryPartAdded { summary_index } => {
            (Some(*summary_index), None)
        }
        ItemDeltaPayload::ReasoningSummaryText {
            summary_index,
            delta,
        } => (Some(*summary_index), Some(delta.as_str())),
        _ => return None,
    };

    Some(
        ToolActivityEvent::new(
            delta.thread_id().as_str(),
            delta.turn_id().as_str(),
            delta.item_id().as_str(),
            delta.expected_item_kind().item_type(),
            ToolActivitySource::Reasoning,
            ToolActivityLifecycle::Updated,
        )
        .with_reasoning_summary_index(summary_index)
        .with_reasoning_summary_delta(summary_delta),
    )
}

fn file_change_summary(item: &super::FileChangeItem) -> ToolActivityFileChangeSummary {
    let mut paths = std::collections::BTreeSet::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;

    for change in &item.changes {
        paths.insert(change.path.clone());
        let (change_additions, change_deletions) = diff_line_counts(&change.diff);
        additions = additions.saturating_add(change_additions);
        deletions = deletions.saturating_add(change_deletions);
    }

    ToolActivityFileChangeSummary {
        file_count: paths.len(),
        additions,
        deletions,
        single_file_path: if paths.len() == 1 {
            paths.into_iter().next().map(PathBuf::from)
        } else {
            None
        },
    }
}

fn diff_line_counts(diff: &str) -> (usize, usize) {
    let mut additions = 0;
    let mut deletions = 0;
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            additions += 1;
        } else if line.starts_with('-') {
            deletions += 1;
        }
    }
    (additions, deletions)
}

fn joined_non_empty_text(parts: &[String]) -> Option<String> {
    let mut text = String::new();
    for part in parts {
        if !part.is_empty() {
            text.push_str(part);
        }
    }
    (!text.is_empty()).then_some(text)
}
