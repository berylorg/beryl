use std::io;

use bounded_json::ParseFailure;

use super::super::schema;
use crate::{
    ProviderDeltaKind, ProviderItemKind, ProviderItemLifecycle, ProviderObservationSchemaError,
    incoming_json::DecodeReaderError,
    turn::{StreamedUserMessageCorrelationError, UserMessageEchoLifecycle},
};

pub(super) fn find_field(
    fields: &'static [schema::FieldSpec],
    name: &str,
) -> Result<(usize, schema::FieldSpec), ProviderObservationSchemaError> {
    fields
        .iter()
        .copied()
        .enumerate()
        .find(|(_, field)| field.name == name)
        .ok_or(ProviderObservationSchemaError::UnknownField)
}

pub(super) fn require_fields(
    fields: &[schema::FieldSpec],
    seen: u64,
) -> Result<(), ProviderObservationSchemaError> {
    if fields
        .iter()
        .enumerate()
        .any(|(index, field)| field.required && seen & (1_u64 << index) == 0)
    {
        Err(ProviderObservationSchemaError::MissingField)
    } else {
        Ok(())
    }
}

pub(super) fn delta_has_text(kind: ProviderDeltaKind) -> bool {
    matches!(
        kind,
        ProviderDeltaKind::AgentMessage
            | ProviderDeltaKind::Plan
            | ProviderDeltaKind::ReasoningSummaryText
            | ProviderDeltaKind::ReasoningTextObserved
            | ProviderDeltaKind::CommandExecutionOutput
            | ProviderDeltaKind::FileChangeOutput
    )
}

pub(super) fn required_delta_payload(kind: ProviderDeltaKind) -> u8 {
    match kind {
        ProviderDeltaKind::ReasoningSummaryPartAdded => 2,
        ProviderDeltaKind::ReasoningSummaryText | ProviderDeltaKind::ReasoningTextObserved => 3,
        ProviderDeltaKind::FileChangePatchUpdated => 4,
        ProviderDeltaKind::McpToolCallProgress => 8,
        _ => 1,
    }
}

pub(super) fn item_kind(value: &str) -> Option<ProviderItemKind> {
    Some(match value {
        "hookPrompt" => ProviderItemKind::HookPrompt,
        "agentMessage" => ProviderItemKind::AgentMessage,
        "plan" => ProviderItemKind::Plan,
        "reasoning" => ProviderItemKind::Reasoning,
        "commandExecution" => ProviderItemKind::CommandExecution,
        "fileChange" => ProviderItemKind::FileChange,
        "mcpToolCall" => ProviderItemKind::McpToolCall,
        "dynamicToolCall" => ProviderItemKind::DynamicToolCall,
        "collabAgentToolCall" => ProviderItemKind::CollabAgentToolCall,
        "subAgentActivity" => ProviderItemKind::SubAgentActivity,
        "webSearch" => ProviderItemKind::WebSearch,
        "imageView" => ProviderItemKind::ImageView,
        "sleep" => ProviderItemKind::Sleep,
        "imageGeneration" => ProviderItemKind::StandaloneImageGeneration,
        "enteredReviewMode" => ProviderItemKind::EnteredReviewMode,
        "exitedReviewMode" => ProviderItemKind::ExitedReviewMode,
        "contextCompaction" => ProviderItemKind::ContextCompaction,
        _ => return None,
    })
}

pub(super) fn user_lifecycle(lifecycle: ProviderItemLifecycle) -> UserMessageEchoLifecycle {
    match lifecycle {
        ProviderItemLifecycle::Started => UserMessageEchoLifecycle::Started,
        ProviderItemLifecycle::Completed => UserMessageEchoLifecycle::Completed,
    }
}

pub(super) fn known_input_type(value: &str) -> &'static str {
    match value {
        "text" => "text",
        "localImage" => "localImage",
        "image" => "image",
        "skill" => "skill",
        "mention" => "mention",
        _ => "unsupported",
    }
}

pub(super) fn unsupported(context: &'static str) -> StreamedUserMessageCorrelationError {
    StreamedUserMessageCorrelationError::UnsupportedNormalization { context }
}

pub(super) fn json_failure(failure: ParseFailure) -> DecodeReaderError {
    DecodeReaderError::Json(serde_json::Error::io(io::Error::new(
        io::ErrorKind::InvalidData,
        failure.to_string(),
    )))
}

pub(super) fn json_message(message: &'static str) -> DecodeReaderError {
    DecodeReaderError::Json(serde_json::Error::io(io::Error::new(
        io::ErrorKind::InvalidData,
        message,
    )))
}
