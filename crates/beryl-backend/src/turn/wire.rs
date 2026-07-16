use std::io;

use beryl_model::{CasItemId, CasThreadId, CasTurnId};
use serde::Deserialize;
use serde_json::Value;

use crate::ThreadSummary;

use super::{
    CompletedTurn, CompletedTurnStatus, FileUpdateChange, ItemDelta, ItemDeltaPayload,
    ItemLifecycleTimestampMs, RateLimitSnapshot, ThreadItem, ThreadItemLifecycleContract,
    ThreadStatus, ThreadTokenUsage, TurnError, TurnStatus, TurnStreamEvent,
};

const THREAD_STATUS_CHANGED_METHOD: &str = "thread/status/changed";
const THREAD_STARTED_METHOD: &str = "thread/started";
const THREAD_CLOSED_METHOD: &str = "thread/closed";
const TURN_STARTED_METHOD: &str = "turn/started";
const TURN_COMPLETED_METHOD: &str = "turn/completed";
const ITEM_STARTED_METHOD: &str = "item/started";
const ITEM_COMPLETED_METHOD: &str = "item/completed";
const AGENT_MESSAGE_DELTA_METHOD: &str = "item/agentMessage/delta";
const PLAN_DELTA_METHOD: &str = "item/plan/delta";
const REASONING_SUMMARY_PART_ADDED_METHOD: &str = "item/reasoning/summaryPartAdded";
const REASONING_SUMMARY_TEXT_DELTA_METHOD: &str = "item/reasoning/summaryTextDelta";
const REASONING_TEXT_DELTA_METHOD: &str = "item/reasoning/textDelta";
const COMMAND_EXECUTION_OUTPUT_DELTA_METHOD: &str = "item/commandExecution/outputDelta";
const FILE_CHANGE_OUTPUT_DELTA_METHOD: &str = "item/fileChange/outputDelta";
const FILE_CHANGE_PATCH_UPDATED_METHOD: &str = "item/fileChange/patchUpdated";
const MCP_TOOL_CALL_PROGRESS_METHOD: &str = "item/mcpToolCall/progress";
const THREAD_NAME_UPDATED_METHOD: &str = "thread/name/updated";
const THREAD_TOKEN_USAGE_UPDATED_METHOD: &str = "thread/tokenUsage/updated";
const ACCOUNT_RATE_LIMITS_UPDATED_METHOD: &str = "account/rateLimits/updated";
const CODEX_EVENT_COLLAB_AGENT_SPAWN_END_METHOD: &str = "codex/event/collab_agent_spawn_end";

pub fn parse_turn_stream_event(
    method: &str,
    params: Option<Value>,
) -> Result<Option<TurnStreamEvent>, serde_json::Error> {
    let event = match method {
        THREAD_STARTED_METHOD => {
            let params = required_notification_params(method, params)?;
            let params: ThreadStartedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ThreadStarted {
                thread: params.thread,
            }
        }
        CODEX_EVENT_COLLAB_AGENT_SPAWN_END_METHOD => {
            let params = required_notification_params(method, params)?;
            let Some(event) = collab_agent_spawn_label_event(&params) else {
                return Ok(None);
            };
            event
        }
        THREAD_STATUS_CHANGED_METHOD => {
            let params = required_notification_params(method, params)?;
            let params: ThreadStatusChangedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ThreadStatusChanged {
                thread_id: params.thread_id,
                status: params.status,
            }
        }
        THREAD_CLOSED_METHOD => {
            let params = required_notification_params(method, params)?;
            let params: ThreadClosedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ThreadClosed {
                thread_id: params.thread_id,
            }
        }
        TURN_STARTED_METHOD => parse_turn_started(required_notification_params(method, params)?)?,
        TURN_COMPLETED_METHOD => {
            parse_turn_completed(required_notification_params(method, params)?)?
        }
        ITEM_STARTED_METHOD => parse_item_started(required_notification_params(method, params)?)?,
        ITEM_COMPLETED_METHOD => {
            parse_item_completed(required_notification_params(method, params)?)?
        }
        AGENT_MESSAGE_DELTA_METHOD => {
            parse_text_delta(required_notification_params(method, params)?, |delta| {
                ItemDeltaPayload::AgentMessage { delta }
            })?
        }
        PLAN_DELTA_METHOD => {
            parse_text_delta(required_notification_params(method, params)?, |delta| {
                ItemDeltaPayload::Plan { delta }
            })?
        }
        REASONING_SUMMARY_PART_ADDED_METHOD => {
            parse_reasoning_summary_part(required_notification_params(method, params)?)?
        }
        REASONING_SUMMARY_TEXT_DELTA_METHOD => {
            parse_reasoning_summary_text(required_notification_params(method, params)?)?
        }
        REASONING_TEXT_DELTA_METHOD => {
            parse_reasoning_text(required_notification_params(method, params)?)?
        }
        COMMAND_EXECUTION_OUTPUT_DELTA_METHOD => {
            parse_text_delta(required_notification_params(method, params)?, |delta| {
                ItemDeltaPayload::CommandExecutionOutput { delta }
            })?
        }
        FILE_CHANGE_OUTPUT_DELTA_METHOD => {
            parse_text_delta(required_notification_params(method, params)?, |delta| {
                ItemDeltaPayload::FileChangeOutput { delta }
            })?
        }
        FILE_CHANGE_PATCH_UPDATED_METHOD => {
            parse_file_change_patch(required_notification_params(method, params)?)?
        }
        MCP_TOOL_CALL_PROGRESS_METHOD => {
            parse_mcp_progress(required_notification_params(method, params)?)?
        }
        THREAD_TOKEN_USAGE_UPDATED_METHOD => {
            let params = required_notification_params(method, params)?;
            let params: ThreadTokenUsageUpdatedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::TokenUsageUpdated {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                token_usage: params.token_usage,
            }
        }
        ACCOUNT_RATE_LIMITS_UPDATED_METHOD => {
            let params = required_notification_params(method, params)?;
            let params: AccountRateLimitsUpdatedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::AccountRateLimitsUpdated {
                rate_limits: params.rate_limits,
            }
        }
        THREAD_NAME_UPDATED_METHOD => {
            let params = required_notification_params(method, params)?;
            let params: ThreadNameUpdatedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ThreadNameUpdated {
                thread_id: params.thread_id,
                thread_name: params.thread_name,
            }
        }
        _ => return Ok(None),
    };

    Ok(Some(event))
}

fn required_notification_params(
    method: &str,
    params: Option<Value>,
) -> Result<Value, serde_json::Error> {
    params.ok_or_else(|| invalid_notification(format!("{method} notification requires params")))
}

fn parse_turn_started(params: Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: TurnStartedNotification = serde_json::from_value(params)?;
    Ok(TurnStreamEvent::TurnStarted {
        thread_id: params.thread_id,
        turn_id: params.turn.id,
        status: params.turn.status,
    })
}

fn parse_turn_completed(params: Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: TurnCompletedNotification = serde_json::from_value(params)?;
    Ok(TurnStreamEvent::TurnCompleted {
        thread_id: params.thread_id,
        turn: CompletedTurn {
            id: params.turn.id,
            status: params.turn.status,
            error: params.turn.error,
        },
    })
}

fn parse_item_started(params: Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: ItemStartedNotification = serde_json::from_value(params)?;
    if params.item.lifecycle_contract() == ThreadItemLifecycleContract::CompletionOnly {
        return Err(invalid_notification(
            "completion-only subAgentActivity cannot appear in item/started",
        ));
    }
    Ok(TurnStreamEvent::ItemStarted {
        thread_id: params.thread_id,
        turn_id: params.turn_id,
        started_at_ms: params.started_at_ms,
        item: params.item,
    })
}

fn parse_item_completed(params: Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: ItemCompletedNotification = serde_json::from_value(params)?;
    Ok(TurnStreamEvent::ItemCompleted {
        thread_id: params.thread_id,
        turn_id: params.turn_id,
        completed_at_ms: params.completed_at_ms,
        item: params.item,
    })
}

fn parse_text_delta(
    params: Value,
    payload: impl FnOnce(String) -> ItemDeltaPayload,
) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: TextDeltaNotification = serde_json::from_value(params)?;
    Ok(item_delta_event(
        params.thread_id,
        params.turn_id,
        params.item_id,
        payload(params.delta),
    ))
}

fn parse_reasoning_summary_part(params: Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: ReasoningSummaryPartAddedNotification = serde_json::from_value(params)?;
    let summary_index = checked_index(params.summary_index, "summaryIndex")?;
    Ok(item_delta_event(
        params.thread_id,
        params.turn_id,
        params.item_id,
        ItemDeltaPayload::ReasoningSummaryPartAdded { summary_index },
    ))
}

fn parse_reasoning_summary_text(params: Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: ReasoningSummaryTextDeltaNotification = serde_json::from_value(params)?;
    let summary_index = checked_index(params.summary_index, "summaryIndex")?;
    Ok(item_delta_event(
        params.thread_id,
        params.turn_id,
        params.item_id,
        ItemDeltaPayload::ReasoningSummaryText {
            summary_index,
            delta: params.delta,
        },
    ))
}

fn parse_reasoning_text(params: Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: ReasoningTextDeltaNotification = serde_json::from_value(params)?;
    let content_index = checked_index(params.content_index, "contentIndex")?;
    let _raw_reasoning_text = params.delta;
    Ok(item_delta_event(
        params.thread_id,
        params.turn_id,
        params.item_id,
        ItemDeltaPayload::ReasoningTextObserved { content_index },
    ))
}

fn parse_file_change_patch(params: Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: FileChangePatchUpdatedNotification = serde_json::from_value(params)?;
    Ok(item_delta_event(
        params.thread_id,
        params.turn_id,
        params.item_id,
        ItemDeltaPayload::FileChangePatchUpdated {
            changes: params.changes,
        },
    ))
}

fn parse_mcp_progress(params: Value) -> Result<TurnStreamEvent, serde_json::Error> {
    let params: McpToolCallProgressNotification = serde_json::from_value(params)?;
    Ok(item_delta_event(
        params.thread_id,
        params.turn_id,
        params.item_id,
        ItemDeltaPayload::McpToolCallProgress {
            message: params.message,
        },
    ))
}

fn item_delta_event(
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    payload: ItemDeltaPayload,
) -> TurnStreamEvent {
    TurnStreamEvent::ItemDelta(ItemDelta::new(thread_id, turn_id, item_id, payload))
}

fn checked_index(index: i64, field: &'static str) -> Result<usize, serde_json::Error> {
    usize::try_from(index).map_err(|_| {
        invalid_notification(format!(
            "{field} must be a nonnegative index representable by this client"
        ))
    })
}

fn invalid_notification(message: impl Into<String>) -> serde_json::Error {
    serde_json::Error::io(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadStartedNotification {
    thread: ThreadSummary,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadStatusChangedNotification {
    thread_id: String,
    status: ThreadStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadClosedNotification {
    thread_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnStartedNotification {
    thread_id: CasThreadId,
    turn: TurnStartedWire,
}

#[derive(Deserialize)]
struct TurnStartedWire {
    id: CasTurnId,
    status: TurnStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnCompletedNotification {
    thread_id: CasThreadId,
    turn: TurnCompletedWire,
}

#[derive(Deserialize)]
struct TurnCompletedWire {
    id: CasTurnId,
    status: CompletedTurnStatus,
    error: Option<TurnError>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemStartedNotification {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    started_at_ms: ItemLifecycleTimestampMs,
    item: ThreadItem,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemCompletedNotification {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    completed_at_ms: ItemLifecycleTimestampMs,
    item: ThreadItem,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextDeltaNotification {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    delta: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReasoningSummaryPartAddedNotification {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    summary_index: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReasoningSummaryTextDeltaNotification {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    summary_index: i64,
    delta: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReasoningTextDeltaNotification {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    content_index: i64,
    delta: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileChangePatchUpdatedNotification {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    changes: Vec<FileUpdateChange>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpToolCallProgressNotification {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadTokenUsageUpdatedNotification {
    thread_id: String,
    turn_id: String,
    token_usage: ThreadTokenUsage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRateLimitsUpdatedNotification {
    rate_limits: RateLimitSnapshot,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadNameUpdatedNotification {
    thread_id: String,
    thread_name: Option<String>,
}

fn collab_agent_spawn_label_event(params: &Value) -> Option<TurnStreamEvent> {
    let msg = params.get("msg").unwrap_or(params);
    let thread_id = string_field_any(
        msg,
        &[
            "new_thread_id",
            "newThreadId",
            "new_agent_id",
            "newAgentId",
            "agent_id",
            "agentId",
        ],
    )?;
    let label = string_field_any(
        msg,
        &[
            "new_agent_nickname",
            "newAgentNickname",
            "agent_nickname",
            "agentNickname",
            "nickname",
        ],
    )?;

    Some(TurnStreamEvent::AgentLabelUpdated { thread_id, label })
}

fn string_field_any(value: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| {
        value
            .get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}
