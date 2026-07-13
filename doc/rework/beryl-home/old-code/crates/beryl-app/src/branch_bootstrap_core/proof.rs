use super::*;

pub(super) fn wait_for_bootstrap_turn_terminal<B>(
    backend: &mut B,
    thread_id: &ConversationThreadId,
    bootstrap_turn_id: &ConversationTurnId,
    started_turn: TurnInfo,
    idle_timeout: Duration,
) -> Result<BootstrapTerminalProof, BranchBootstrapError>
where
    B: BranchBootstrapBackend,
{
    if started_turn.is_terminal() {
        return Ok(BootstrapTerminalProof::Streamed(started_turn));
    }

    loop {
        let event = backend
            .next_turn_stream_event(idle_timeout)
            .map_err(|error| BranchBootstrapError::BootstrapStreamFailed {
                thread_id: thread_id.clone(),
                turn_id: bootstrap_turn_id.clone(),
                error: error.to_string(),
            })?;
        let Some(event) = event else {
            return Err(BranchBootstrapError::BootstrapStreamFailed {
                thread_id: thread_id.clone(),
                turn_id: bootstrap_turn_id.clone(),
                error: "timed out waiting for live bootstrap completion event".to_string(),
            });
        };

        match event {
            TurnStreamEvent::ProtocolError { error } => {
                return Err(BranchBootstrapError::BootstrapStreamFailed {
                    thread_id: thread_id.clone(),
                    turn_id: bootstrap_turn_id.clone(),
                    error: error.message,
                });
            }
            TurnStreamEvent::ApprovalRequested(request) => {
                if let Err(error) = backend.deny_approval_request(&request) {
                    return Err(BranchBootstrapError::BootstrapApprovalDenialFailed {
                        thread_id: thread_id.clone(),
                        turn_id: bootstrap_turn_id.clone(),
                        error: error.to_string(),
                    });
                }
                return Err(BranchBootstrapError::BootstrapUnexpectedApprovalRequest {
                    thread_id: thread_id.clone(),
                    turn_id: bootstrap_turn_id.clone(),
                    request: request.summary(),
                });
            }
            TurnStreamEvent::DynamicToolCallRequested(request) => {
                let response = bootstrap_dynamic_tool_unavailable_response(&request);
                if let Err(error) = backend.respond_dynamic_tool_call(&request, &response) {
                    return Err(BranchBootstrapError::BootstrapDynamicToolResponseFailed {
                        thread_id: thread_id.clone(),
                        turn_id: bootstrap_turn_id.clone(),
                        error: error.to_string(),
                    });
                }
                return Err(
                    BranchBootstrapError::BootstrapUnexpectedDynamicToolRequest {
                        thread_id: thread_id.clone(),
                        turn_id: bootstrap_turn_id.clone(),
                        request: request.summary(),
                    },
                );
            }
            TurnStreamEvent::TurnCompleted {
                thread_id: event_thread_id,
                turn,
            } if event_thread_id == thread_id.as_str() && turn.id == bootstrap_turn_id.as_str() => {
                return Ok(BootstrapTerminalProof::Streamed(turn));
            }
            TurnStreamEvent::ThreadStatusChanged {
                thread_id: event_thread_id,
                status,
            } if event_thread_id == thread_id.as_str()
                && matches!(status, beryl_backend::ThreadStatus::Idle) =>
            {
                return Err(BranchBootstrapError::BootstrapStreamFailed {
                    thread_id: thread_id.clone(),
                    turn_id: bootstrap_turn_id.clone(),
                    error: "thread became idle before Beryl observed the live bootstrap completion event"
                        .to_string(),
                });
            }
            _ => {}
        }
    }
}

pub(crate) fn prove_branch_thread_durable_with_bootstrap_turn<B>(
    backend: &mut B,
    thread_id: &ConversationThreadId,
    bootstrap_turn_id: &ConversationTurnId,
    timeout: Duration,
) -> Result<ThreadSummary, BranchBootstrapError>
where
    B: BranchBootstrapBackend,
{
    let thread = backend
        .read_thread_metadata(thread_id.as_str(), timeout)
        .map_err(|error| BranchBootstrapError::DurabilityProofFailed {
            thread_id: thread_id.clone(),
            error: error.to_string(),
        })?;
    let summary = validate_durable_thread_summary(thread, thread_id)?;
    let _ = bootstrap_turn_id;
    Ok(summary)
}

pub(super) fn validate_durable_thread_summary(
    thread: ThreadSummary,
    thread_id: &ConversationThreadId,
) -> Result<ThreadSummary, BranchBootstrapError> {
    if thread.id != thread_id.as_str() {
        return Err(BranchBootstrapError::DurableThreadIdMismatch {
            expected_thread_id: thread_id.clone(),
            actual_thread_id: thread.id,
        });
    }

    if thread.ephemeral {
        return Err(BranchBootstrapError::DurableThreadMarkedEphemeral {
            thread_id: thread_id.clone(),
        });
    }

    Ok(thread)
}

pub(crate) fn turn_has_visible_bootstrap_message(turn: &TurnInfo, message: &str) -> bool {
    turn.items.iter().any(|item| {
        let ThreadItem::UserMessage(user_message) = item else {
            return false;
        };
        user_message.content.iter().any(|input| match input {
            UserInput::Text { text } => text.trim() == message.trim(),
            UserInput::Image { .. }
            | UserInput::LocalImage { .. }
            | UserInput::Skill { .. }
            | UserInput::Mention { .. } => false,
        })
    })
}

pub(crate) fn bootstrap_dynamic_tool_unavailable_response(
    request: &DynamicToolCallRequest,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(format!(
        "{{\"ok\":false,\"error\":{{\"kind\":\"branch_bootstrap_tool_unavailable\",\"message\":\"Beryl branch bootstrap turns do not run dynamic tools.\",\"tool\":\"{}\",\"callId\":\"{}\"}}}}",
        escape_json_string(request.tool()),
        escape_json_string(request.call_id())
    ))
}

fn escape_json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}
