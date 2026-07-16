use super::*;

pub(crate) fn validate_text(
    text: &ProviderTextV1,
    prior_frontier: u64,
) -> Result<(), ProviderItemValidationError> {
    if let ProviderTextV1::Reused(reference) = text
        && reference.end() > prior_frontier
    {
        return Err(ProviderItemValidationError::TextReferenceBeyondFrontier {
            start: reference.start(),
            end: reference.end(),
            frontier: prior_frontier,
        });
    }
    Ok(())
}

pub(crate) fn validate_structured_value(
    value: &ProviderStructuredValueV1,
    prior_frontier: u64,
    depth: usize,
) -> Result<(), ProviderItemValidationError> {
    match value {
        ProviderStructuredValueV1::Null
        | ProviderStructuredValueV1::Boolean(_)
        | ProviderStructuredValueV1::Number(_) => Ok(()),
        ProviderStructuredValueV1::String(text) => validate_text(text, prior_frontier),
        ProviderStructuredValueV1::List(values) => {
            let next = checked_depth(depth)?;
            for value in values {
                validate_structured_value(value, prior_frontier, next)?;
            }
            Ok(())
        }
        ProviderStructuredValueV1::Object(entries) => {
            let next = checked_depth(depth)?;
            for entry in entries {
                validate_structured_value(&entry.value, prior_frontier, next)?;
            }
            Ok(())
        }
    }
}

fn checked_depth(depth: usize) -> Result<usize, ProviderItemValidationError> {
    let next =
        depth
            .checked_add(1)
            .ok_or(ProviderItemValidationError::StructuredDepthExceeded {
                maximum: PROVIDER_STRUCTURED_VALUE_MAX_DEPTH,
            })?;
    if next > PROVIDER_STRUCTURED_VALUE_MAX_DEPTH {
        return Err(ProviderItemValidationError::StructuredDepthExceeded {
            maximum: PROVIDER_STRUCTURED_VALUE_MAX_DEPTH,
        });
    }
    Ok(next)
}

pub(crate) fn validate_frame(
    frame: &ProviderItemFrameV1,
    prior_frontier: u64,
) -> Result<(), ProviderItemValidationError> {
    match frame.observation() {
        ProviderItemObservationV1::Started { item, .. } => {
            if item.kind().permits_completion_only() {
                return Err(ProviderItemValidationError::CompletionOnlyItemStarted);
            }
            validate_item(item, prior_frontier)
        }
        ProviderItemObservationV1::Completed { item, .. } => {
            validate_item(item, prior_frontier)?;
            validate_completed_status(item)
        }
        ProviderItemObservationV1::Delta(delta) => validate_delta(delta, prior_frontier),
    }
}

pub(crate) fn validate_item(
    item: &ProviderItemV1,
    prior_frontier: u64,
) -> Result<(), ProviderItemValidationError> {
    match item {
        ProviderItemV1::UserMessage(value) => {
            if value.submitted.content.encoding() != crate::ContentEncoding::ComposerV1 {
                return Err(ProviderItemValidationError::SubmittedContentMustBeComposer);
            }
            validate_opt_text(&value.client_id, prior_frontier)
        }
        ProviderItemV1::HookPrompt(value) => {
            for fragment in &value.fragments {
                validate_text(&fragment.text, prior_frontier)?;
                validate_text(&fragment.hook_run_id, prior_frontier)?;
            }
            Ok(())
        }
        ProviderItemV1::AgentMessage(value) => validate_agent_message(value, prior_frontier),
        ProviderItemV1::Plan(value) => validate_text(&value.text, prior_frontier),
        ProviderItemV1::Reasoning(value) => validate_texts(&value.summary, prior_frontier),
        ProviderItemV1::CommandExecution(value) => validate_command(value, prior_frontier),
        ProviderItemV1::FileChange(value) => validate_changes(&value.changes, prior_frontier),
        ProviderItemV1::McpToolCall(value) => validate_mcp(value, prior_frontier),
        ProviderItemV1::DynamicToolCall(value) => validate_dynamic(value, prior_frontier),
        ProviderItemV1::CollabAgentToolCall(value) => validate_collab(value, prior_frontier),
        ProviderItemV1::SubAgentActivity(value) => validate_text(&value.agent_path, prior_frontier),
        ProviderItemV1::WebSearch(value) => validate_web_search(value, prior_frontier),
        ProviderItemV1::ImageView(value) => validate_text(&value.path, prior_frontier),
        ProviderItemV1::Sleep(_) | ProviderItemV1::ContextCompaction => Ok(()),
        ProviderItemV1::StandaloneImageGeneration(value) => {
            validate_opt_text(&value.revised_prompt, prior_frontier)?;
            validate_opt_text(&value.saved_path, prior_frontier)
        }
        ProviderItemV1::EnteredReviewMode(value) => validate_text(&value.review, prior_frontier),
        ProviderItemV1::ExitedReviewMode(value) => validate_text(&value.review, prior_frontier),
    }
}

fn validate_agent_message(
    value: &ProviderAgentMessageV1,
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    validate_text(&value.text, prior)?;
    if let Some(citation) = &value.memory_citation {
        for entry in &citation.entries {
            validate_text(&entry.path, prior)?;
            validate_text(&entry.note, prior)?;
        }
        validate_texts(&citation.thread_ids, prior)?;
    }
    Ok(())
}

fn validate_command(
    value: &ProviderCommandExecutionV1,
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    validate_text(&value.command, prior)?;
    validate_text(&value.cwd, prior)?;
    validate_opt_text(&value.process_id, prior)?;
    for action in &value.command_actions {
        match action {
            ProviderCommandActionV1::Read {
                command,
                name,
                path,
            } => {
                validate_text(command, prior)?;
                validate_text(name, prior)?;
                validate_text(path, prior)?;
            }
            ProviderCommandActionV1::ListFiles { command, path } => {
                validate_text(command, prior)?;
                validate_opt_text(path, prior)?;
            }
            ProviderCommandActionV1::Search {
                command,
                query,
                path,
            } => {
                validate_text(command, prior)?;
                validate_opt_text(query, prior)?;
                validate_opt_text(path, prior)?;
            }
            ProviderCommandActionV1::Unknown { command } => validate_text(command, prior)?,
        }
    }
    validate_opt_text(&value.aggregated_output, prior)
}

pub(crate) fn validate_changes(
    changes: &[ProviderFileUpdateChangeV1],
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    for change in changes {
        validate_text(&change.path, prior)?;
        validate_text(&change.diff, prior)?;
        if let ProviderPatchChangeKindV1::Update { move_path } = &change.kind {
            validate_opt_text(move_path, prior)?;
        }
    }
    Ok(())
}

fn validate_mcp(
    value: &ProviderMcpToolCallV1,
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    validate_text(&value.server, prior)?;
    validate_text(&value.tool, prior)?;
    value.arguments.validate(prior)?;
    if let Some(context) = &value.app_context {
        validate_text(&context.connector_id, prior)?;
        validate_opt_text(&context.link_id, prior)?;
        validate_opt_text(&context.resource_uri, prior)?;
        validate_opt_text(&context.app_name, prior)?;
        validate_opt_text(&context.template_id, prior)?;
        validate_opt_text(&context.action_name, prior)?;
    }
    validate_opt_text(&value.mcp_app_resource_uri, prior)?;
    validate_opt_text(&value.plugin_id, prior)?;
    if let Some(result) = &value.result {
        for content in &result.content {
            content.validate(prior)?;
        }
        validate_opt_structured(&result.structured_content, prior)?;
        validate_opt_structured(&result.meta, prior)?;
    }
    if let Some(error) = &value.error {
        validate_text(&error.message, prior)?;
    }
    Ok(())
}

fn validate_dynamic(
    value: &ProviderDynamicToolCallV1,
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    validate_opt_text(&value.namespace, prior)?;
    validate_text(&value.tool, prior)?;
    value.arguments.validate(prior)?;
    if let Some(items) = &value.content_items {
        for item in items {
            match item {
                ProviderDynamicToolOutputV1::InputText { text } => validate_text(text, prior)?,
                ProviderDynamicToolOutputV1::InputImageLocator { locator } => {
                    locator.validate()?;
                }
                ProviderDynamicToolOutputV1::InputImageAsset { .. } => {}
            }
        }
    }
    Ok(())
}

fn validate_collab(
    value: &ProviderCollabAgentToolCallV1,
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    validate_opt_text(&value.prompt, prior)?;
    validate_opt_text(&value.model, prior)?;
    validate_opt_text(&value.reasoning_effort, prior)?;
    for state in &value.agents_states {
        validate_text(&state.agent, prior)?;
        validate_opt_text(&state.state.message, prior)?;
    }
    Ok(())
}

fn validate_web_search(
    value: &ProviderWebSearchV1,
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    validate_text(&value.query, prior)?;
    match &value.action {
        None | Some(ProviderWebSearchActionV1::Other) => Ok(()),
        Some(ProviderWebSearchActionV1::Search { query, queries }) => {
            validate_opt_text(query, prior)?;
            if let Some(queries) = queries {
                validate_texts(queries, prior)?;
            }
            Ok(())
        }
        Some(ProviderWebSearchActionV1::OpenPage { url }) => validate_opt_text(url, prior),
        Some(ProviderWebSearchActionV1::FindInPage { url, pattern }) => {
            validate_opt_text(url, prior)?;
            validate_opt_text(pattern, prior)
        }
    }
}

pub(crate) fn validate_delta(
    delta: &ProviderItemDeltaV1,
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    match delta {
        ProviderItemDeltaV1::AgentMessage { delta }
        | ProviderItemDeltaV1::Plan { delta }
        | ProviderItemDeltaV1::ReasoningSummaryText { delta, .. }
        | ProviderItemDeltaV1::CommandExecutionOutput { delta }
        | ProviderItemDeltaV1::FileChangeOutput { delta } => validate_text(delta, prior),
        ProviderItemDeltaV1::McpToolCallProgress { message } => validate_text(message, prior),
        ProviderItemDeltaV1::FileChangePatchUpdated { changes } => validate_changes(changes, prior),
        ProviderItemDeltaV1::ReasoningSummaryPartAdded { .. }
        | ProviderItemDeltaV1::ReasoningTextObserved { .. } => Ok(()),
    }
}

pub(crate) fn validate_completed_status(
    item: &ProviderItemV1,
) -> Result<(), ProviderItemValidationError> {
    let in_progress = match item {
        ProviderItemV1::CommandExecution(value) => {
            value.status == ProviderCommandStatusV1::InProgress
        }
        ProviderItemV1::FileChange(value) => value.status == ProviderPatchStatusV1::InProgress,
        ProviderItemV1::McpToolCall(value) => value.status == ProviderToolCallStatusV1::InProgress,
        ProviderItemV1::DynamicToolCall(value) => {
            value.status == ProviderToolCallStatusV1::InProgress
        }
        ProviderItemV1::CollabAgentToolCall(value) => {
            value.status == ProviderCollabToolStatusV1::InProgress
        }
        ProviderItemV1::StandaloneImageGeneration(value) => {
            value.status == ProviderImageGenerationStatusV1::InProgress
        }
        _ => false,
    };
    if in_progress {
        Err(ProviderItemValidationError::CompletionStatusInProgress)
    } else {
        Ok(())
    }
}

fn validate_texts(texts: &[ProviderTextV1], prior: u64) -> Result<(), ProviderItemValidationError> {
    for text in texts {
        validate_text(text, prior)?;
    }
    Ok(())
}

fn validate_opt_text(
    text: &Option<ProviderTextV1>,
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    match text {
        Some(text) => validate_text(text, prior),
        None => Ok(()),
    }
}

fn validate_opt_structured(
    value: &Option<ProviderStructuredValueV1>,
    prior: u64,
) -> Result<(), ProviderItemValidationError> {
    match value {
        Some(value) => value.validate(prior),
        None => Ok(()),
    }
}
