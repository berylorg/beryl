use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_backend::{ManagedBackendClientConnector, ThreadStatus, ThreadSummary, TurnStreamEvent};
use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    workspace::WorkspaceId,
};

use crate::branch_bootstrap_core::{
    BranchBootstrapBackend, BranchBootstrapError, BranchBootstrapHistoryCompletion,
    bootstrap_dynamic_tool_unavailable_response,
    prove_branch_thread_completed_bootstrap_from_history,
    prove_branch_thread_durable_with_bootstrap_turn, start_branch_bootstrap_turn_only,
};

use super::super::{
    transcript_branch_core::{
        ForegroundTranscriptBranchPublication, TranscriptBranchOutcome, prepare_transcript_branch,
    },
    transcript_branch_menu_state::TranscriptBranchRequest,
    turn_worker::{
        POST_COMPLETION_GRACE, TURN_STREAM_IDLE_POLL_INTERVAL, TurnWorkerOutcome, TurnWorkerUpdate,
    },
};

pub(super) fn spawn_foreground_transcript_branch_worker(
    connector: ManagedBackendClientConnector,
    request: TranscriptBranchRequest,
    execution_target: WorkspaceId,
    timeout: Duration,
) -> Receiver<TurnWorkerUpdate> {
    let (sender, receiver) = mpsc::sync_channel(1024);
    thread::spawn(move || {
        run_foreground_transcript_branch_worker(
            connector,
            request,
            execution_target,
            timeout,
            sender,
        )
    });
    receiver
}

fn run_foreground_transcript_branch_worker(
    connector: ManagedBackendClientConnector,
    request: TranscriptBranchRequest,
    execution_target: WorkspaceId,
    timeout: Duration,
    sender: mpsc::SyncSender<TurnWorkerUpdate>,
) {
    let source_thread_id = request.target().source_thread_id().to_string();
    let source_turn_id = request.target().source_turn_id().to_string();
    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            let _ = sender.send(TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed {
                message: format!("Beryl could not connect to the managed backend: {error}"),
            }));
            return;
        }
    };

    let prepared = match prepare_transcript_branch(&mut session, request, timeout) {
        Ok(prepared) => prepared,
        Err(outcome) => {
            let _ = sender.send(TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed {
                message: failed_branch_message(outcome),
            }));
            return;
        }
    };
    let branch_thread_id = prepared.branch_thread_id().clone();
    let bootstrap_message = prepared.bootstrap_message().to_string();
    let title_seed = prepared.title_seed().to_string();
    let started = match start_branch_bootstrap_turn_only(
        &mut session,
        &branch_thread_id,
        &bootstrap_message,
        timeout,
    ) {
        Ok(started) => started,
        Err(error) => {
            let _ = sender.send(TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed {
                message: error.to_string(),
            }));
            return;
        }
    };
    let bootstrap_turn_id = started.bootstrap_turn_id().clone();
    let start = prepared.into_foreground_start(started.turn().clone(), bootstrap_turn_id.clone());
    if sender
        .send(TurnWorkerUpdate::ForegroundTranscriptBranchStarted(start))
        .is_err()
    {
        return;
    }

    let mut unexpected_dynamic_tool_request = None;
    let stream_result = stream_foreground_branch_bootstrap_events(
        &mut session,
        &branch_thread_id,
        &bootstrap_turn_id,
        &bootstrap_message,
        &mut unexpected_dynamic_tool_request,
        &sender,
        timeout,
    );
    let history_durable_summary = match stream_result {
        Ok(history_durable_summary) => history_durable_summary,
        Err(message) => {
            let _ = sender.send(TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed {
                message,
            }));
            return;
        }
    };

    if let Some(request) = unexpected_dynamic_tool_request {
        let _ = sender.send(TurnWorkerUpdate::ForegroundTranscriptBranchPublicationFinished(
            ForegroundTranscriptBranchPublication::Failed {
                source_thread_id,
                source_turn_id,
                message: format!(
                    "Bootstrap turn {} for branch thread {} requested a dynamic tool unexpectedly; Beryl returned an unavailable response and did not publish the branch. Request: {request}",
                    bootstrap_turn_id.as_str(),
                    branch_thread_id.as_str()
                ),
            },
        ));
    } else {
        let publication = match history_durable_summary {
            Some(durable_summary) => ForegroundTranscriptBranchPublication::Published {
                source_thread_id,
                source_turn_id,
                title_seed,
                durable_summary,
                bootstrap_turn_id: bootstrap_turn_id.clone(),
            },
            None => match prove_branch_thread_durable_with_bootstrap_turn(
                &mut session,
                &branch_thread_id,
                &bootstrap_turn_id,
                &bootstrap_message,
                timeout,
            ) {
                Ok(durable_summary) => ForegroundTranscriptBranchPublication::Published {
                    source_thread_id,
                    source_turn_id,
                    title_seed,
                    durable_summary,
                    bootstrap_turn_id: bootstrap_turn_id.clone(),
                },
                Err(error) => ForegroundTranscriptBranchPublication::Failed {
                    source_thread_id,
                    source_turn_id,
                    message: error.to_string(),
                },
            },
        };
        let _ = sender
            .send(TurnWorkerUpdate::ForegroundTranscriptBranchPublicationFinished(publication));
    }

    let _ = sender.send(TurnWorkerUpdate::Finished(TurnWorkerOutcome::Finished {
        execution_target,
        known_threads: None,
        active_thread_id: branch_thread_id.as_str().to_string(),
    }));
}

fn stream_foreground_branch_bootstrap_events<B>(
    backend: &mut B,
    branch_thread_id: &ConversationThreadId,
    bootstrap_turn_id: &ConversationTurnId,
    bootstrap_message: &str,
    unexpected_dynamic_tool_request: &mut Option<String>,
    sender: &mpsc::SyncSender<TurnWorkerUpdate>,
    timeout: Duration,
) -> Result<Option<ThreadSummary>, String>
where
    B: BranchBootstrapBackend,
{
    let mut saw_turn_completion = false;
    let mut saw_target_idle_before_completion = false;
    loop {
        let event_timeout = if saw_turn_completion {
            POST_COMPLETION_GRACE
        } else {
            TURN_STREAM_IDLE_POLL_INTERVAL
        };
        let event = match backend.next_turn_stream_event(event_timeout) {
            Ok(Some(TurnStreamEvent::ProtocolError { error })) => {
                return Err(format!(
                    "Beryl received a protocol error while streaming the branch bootstrap turn: {}",
                    error.message
                ));
            }
            Ok(Some(TurnStreamEvent::ApprovalRequested(request))) => {
                backend.deny_approval_request(&request).map_err(|error| {
                    BranchBootstrapError::BootstrapApprovalDenialFailed {
                        thread_id: branch_thread_id.clone(),
                        turn_id: bootstrap_turn_id.clone(),
                        error: error.to_string(),
                    }
                    .to_string()
                })?;
                return Err(BranchBootstrapError::BootstrapUnexpectedApprovalRequest {
                    thread_id: branch_thread_id.clone(),
                    turn_id: bootstrap_turn_id.clone(),
                    request: request.summary(),
                }
                .to_string());
            }
            Ok(Some(TurnStreamEvent::DynamicToolCallRequested(request))) => {
                unexpected_dynamic_tool_request.get_or_insert_with(|| request.summary());
                let response = bootstrap_dynamic_tool_unavailable_response(&request);
                backend
                    .respond_dynamic_tool_call(&request, &response)
                    .map_err(|error| {
                        BranchBootstrapError::BootstrapDynamicToolResponseFailed {
                            thread_id: branch_thread_id.clone(),
                            turn_id: bootstrap_turn_id.clone(),
                            error: error.to_string(),
                        }
                        .to_string()
                    })?;
                continue;
            }
            Ok(Some(event)) => event,
            Ok(None) if saw_turn_completion => break,
            Ok(None) if saw_target_idle_before_completion => {
                if let Some(completion) = foreground_history_completion(
                    backend,
                    branch_thread_id,
                    bootstrap_turn_id,
                    bootstrap_message,
                    timeout,
                    sender,
                    None,
                )? {
                    return Ok(Some(completion.thread().clone()));
                }
                continue;
            }
            Ok(None) => continue,
            Err(_) if saw_turn_completion => break,
            Err(error) => {
                return Err(format!(
                    "Beryl lost the execution stream for the branch bootstrap turn: {error}"
                ));
            }
        };

        if exact_bootstrap_completion(&event, branch_thread_id, bootstrap_turn_id) {
            saw_turn_completion = true;
        }

        if target_thread_idle(&event, branch_thread_id) && !saw_turn_completion {
            saw_target_idle_before_completion = true;
            if let Some(completion) = foreground_history_completion(
                backend,
                branch_thread_id,
                bootstrap_turn_id,
                bootstrap_message,
                timeout,
                sender,
                Some(event),
            )? {
                return Ok(Some(completion.thread().clone()));
            }
            continue;
        }

        let finish_after_event =
            saw_turn_completion && target_thread_idle_or_waiting(&event, branch_thread_id);
        emit_foreground_branch_event(sender, event)?;
        if finish_after_event {
            break;
        }
    }

    Ok(None)
}

fn foreground_history_completion<B>(
    backend: &mut B,
    branch_thread_id: &ConversationThreadId,
    bootstrap_turn_id: &ConversationTurnId,
    bootstrap_message: &str,
    timeout: Duration,
    sender: &mpsc::SyncSender<TurnWorkerUpdate>,
    pending_idle_event: Option<TurnStreamEvent>,
) -> Result<Option<BranchBootstrapHistoryCompletion>, String>
where
    B: BranchBootstrapBackend,
{
    let Some(completion) = prove_branch_thread_completed_bootstrap_from_history(
        backend,
        branch_thread_id,
        bootstrap_turn_id,
        bootstrap_message,
        timeout,
    )
    .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };

    emit_foreground_branch_event(
        sender,
        TurnStreamEvent::TurnCompleted {
            thread_id: branch_thread_id.as_str().to_string(),
            turn: completion.turn().clone(),
        },
    )?;
    if let Some(pending_idle_event) = pending_idle_event {
        emit_foreground_branch_event(sender, pending_idle_event)?;
    }
    Ok(Some(completion))
}

fn exact_bootstrap_completion(
    event: &TurnStreamEvent,
    branch_thread_id: &ConversationThreadId,
    bootstrap_turn_id: &ConversationTurnId,
) -> bool {
    matches!(
        event,
        TurnStreamEvent::TurnCompleted { thread_id, turn }
            if thread_id == branch_thread_id.as_str()
                && turn.id == bootstrap_turn_id.as_str()
    )
}

fn target_thread_idle(event: &TurnStreamEvent, branch_thread_id: &ConversationThreadId) -> bool {
    matches!(
        event,
        TurnStreamEvent::ThreadStatusChanged { thread_id, status }
            if thread_id == branch_thread_id.as_str()
                && matches!(status, ThreadStatus::Idle)
    )
}

fn target_thread_idle_or_waiting(
    event: &TurnStreamEvent,
    branch_thread_id: &ConversationThreadId,
) -> bool {
    matches!(
        event,
        TurnStreamEvent::ThreadStatusChanged { thread_id, status }
            if thread_id == branch_thread_id.as_str()
                && (matches!(status, ThreadStatus::Idle) || status.waiting_on_user_input())
    )
}

fn emit_foreground_branch_event(
    sender: &mpsc::SyncSender<TurnWorkerUpdate>,
    event: TurnStreamEvent,
) -> Result<(), String> {
    sender
        .send(TurnWorkerUpdate::Event(event))
        .map_err(|_| "Beryl stopped receiving foreground branch updates.".to_string())
}

fn failed_branch_message(outcome: TranscriptBranchOutcome) -> String {
    match outcome {
        TranscriptBranchOutcome::Failed { message, .. } => message,
        TranscriptBranchOutcome::Branched { .. } => {
            "Beryl received an unexpected branch preparation result.".to_string()
        }
    }
}
