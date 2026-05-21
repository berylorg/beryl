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

    let mut saw_target_idle = false;
    loop {
        let event = backend
            .next_turn_stream_event(idle_timeout)
            .map_err(|error| BranchBootstrapError::BootstrapStreamFailed {
                thread_id: thread_id.clone(),
                turn_id: bootstrap_turn_id.clone(),
                error: error.to_string(),
            })?;
        let Some(event) = event else {
            if saw_target_idle {
                if let Some(proof) = read_bootstrap_terminal_from_history(
                    backend,
                    thread_id,
                    bootstrap_turn_id,
                    idle_timeout,
                )? {
                    return Ok(proof);
                }
            }
            continue;
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
                saw_target_idle = true;
                if let Some(proof) = read_bootstrap_terminal_from_history(
                    backend,
                    thread_id,
                    bootstrap_turn_id,
                    idle_timeout,
                )? {
                    return Ok(proof);
                }
            }
            _ => {}
        }
    }
}

fn read_bootstrap_terminal_from_history<B>(
    backend: &mut B,
    thread_id: &ConversationThreadId,
    bootstrap_turn_id: &ConversationTurnId,
    timeout: Duration,
) -> Result<Option<BootstrapTerminalProof>, BranchBootstrapError>
where
    B: BranchBootstrapBackend,
{
    let thread = backend
        .read_thread_with_turns(thread_id.as_str(), timeout)
        .map_err(|error| BranchBootstrapError::DurabilityProofFailed {
            thread_id: thread_id.clone(),
            error: error.to_string(),
        })?;
    let _ = validate_durable_thread_summary(thread.summary(), thread_id)?;
    let Some(turn) = thread
        .turns
        .iter()
        .find(|turn| turn.id == bootstrap_turn_id.as_str())
        .cloned()
    else {
        return Ok(None);
    };

    if !turn.is_terminal() {
        return Ok(None);
    }

    Ok(Some(BootstrapTerminalProof::History { thread, turn }))
}

pub(crate) fn prove_branch_thread_durable_with_bootstrap_turn<B>(
    backend: &mut B,
    thread_id: &ConversationThreadId,
    bootstrap_turn_id: &ConversationTurnId,
    message: &str,
    timeout: Duration,
) -> Result<ThreadSummary, BranchBootstrapError>
where
    B: BranchBootstrapBackend,
{
    let thread = backend
        .read_thread_with_turns(thread_id.as_str(), timeout)
        .map_err(|error| BranchBootstrapError::DurabilityProofFailed {
            thread_id: thread_id.clone(),
            error: error.to_string(),
        })?;
    validate_thread_info_with_completed_bootstrap_turn(
        thread,
        thread_id,
        bootstrap_turn_id,
        message,
    )
}

pub(crate) fn prove_branch_thread_completed_bootstrap_from_history<B>(
    backend: &mut B,
    thread_id: &ConversationThreadId,
    bootstrap_turn_id: &ConversationTurnId,
    message: &str,
    timeout: Duration,
) -> Result<Option<BranchBootstrapHistoryCompletion>, BranchBootstrapError>
where
    B: BranchBootstrapBackend,
{
    let thread = backend
        .read_thread_with_turns(thread_id.as_str(), timeout)
        .map_err(|error| BranchBootstrapError::DurabilityProofFailed {
            thread_id: thread_id.clone(),
            error: error.to_string(),
        })?;
    let summary = validate_durable_thread_summary(thread.summary(), thread_id)?;
    let Some(turn) = thread
        .turns
        .iter()
        .find(|turn| turn.id == bootstrap_turn_id.as_str())
        .cloned()
    else {
        return Ok(None);
    };

    if !turn.is_terminal() {
        return Ok(None);
    }

    if turn.status != TurnStatus::Completed {
        return Err(BranchBootstrapError::BootstrapTurnNotCompletedInHistory {
            thread_id: thread_id.clone(),
            turn_id: bootstrap_turn_id.clone(),
            status: turn.status,
        });
    }

    if !turn_has_visible_bootstrap_message(&turn, message) {
        return Err(BranchBootstrapError::BootstrapTurnMissingVisibleMessage {
            thread_id: thread_id.clone(),
            turn_id: bootstrap_turn_id.clone(),
        });
    }

    Ok(Some(BranchBootstrapHistoryCompletion::new(summary, turn)))
}

pub(super) fn validate_thread_info_with_completed_bootstrap_turn(
    thread: ThreadInfo,
    thread_id: &ConversationThreadId,
    bootstrap_turn_id: &ConversationTurnId,
    message: &str,
) -> Result<ThreadSummary, BranchBootstrapError> {
    let summary = validate_durable_thread_summary(thread.summary(), thread_id)?;
    let Some(turn) = thread
        .turns
        .iter()
        .find(|turn| turn.id == bootstrap_turn_id.as_str())
    else {
        return Err(BranchBootstrapError::BootstrapTurnMissingFromHistory {
            thread_id: thread_id.clone(),
            turn_id: bootstrap_turn_id.clone(),
        });
    };

    if turn.status != TurnStatus::Completed {
        return Err(BranchBootstrapError::BootstrapTurnNotCompletedInHistory {
            thread_id: thread_id.clone(),
            turn_id: bootstrap_turn_id.clone(),
            status: turn.status,
        });
    }

    if !turn_has_visible_bootstrap_message(turn, message) {
        return Err(BranchBootstrapError::BootstrapTurnMissingVisibleMessage {
            thread_id: thread_id.clone(),
            turn_id: bootstrap_turn_id.clone(),
        });
    }

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

fn turn_has_visible_bootstrap_message(turn: &TurnInfo, message: &str) -> bool {
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
