use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Duration,
};

use beryl_backend::{
    ManagedBackendClientConnector, ManagedBackendSession, ThreadForkOptions, ThreadForkResponse,
    ThreadRollbackResponse, ThreadStatus, ThreadSummary, TurnInfo, TurnStartOptions,
    TurnStreamEvent,
};
use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use tracing::warn;

use crate::{
    BerylWorkspacePersistence,
    branch_bootstrap_core::{
        BranchBootstrapBackend, BranchBootstrapMessageInput,
        bootstrap_dynamic_tool_unavailable_response, branch_bootstrap_message,
        prove_branch_thread_durable_with_bootstrap_turn, turn_has_visible_bootstrap_message,
    },
};

use super::{
    resident_branch_edit::{self, ResidentBranchProof},
    syndic_ingestion::{self, SyndicLiveTurnIngestor},
    turn_input::UserInputFragment,
};

const BRANCH_BOOTSTRAP_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) trait ResidentBranchBackend: BranchBootstrapBackend {
    fn fork_thread_with_options(
        &mut self,
        thread_id: &str,
        options: ThreadForkOptions,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, Self::Error>;

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error>;
}

impl ResidentBranchBackend for ManagedBackendSession {
    fn fork_thread_with_options(
        &mut self,
        thread_id: &str,
        options: ThreadForkOptions,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, Self::Error> {
        ManagedBackendSession::fork_thread_with_options(self, thread_id, options, timeout)
    }

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error> {
        ManagedBackendSession::rollback_thread(self, thread_id, num_turns, timeout)
    }
}

pub(super) struct ResidentBranchTask {
    receiver: Receiver<ResidentBranchUpdate>,
}

pub(super) enum ResidentBranchUpdate {
    Finished(ResidentBranchOutcome),
}

pub(super) enum ResidentBranchOutcome {
    Created {
        source_thread_id: ConversationThreadId,
        source_turn_id: ConversationTurnId,
        title_seed: String,
        execution_target: WorkspaceId,
        thread_summary: ThreadSummary,
        bootstrap_turn_id: ConversationTurnId,
    },
    Failed {
        source_thread_id: String,
        message: String,
    },
}

impl ResidentBranchTask {
    pub(super) fn try_recv(&self) -> Result<ResidentBranchUpdate, TryRecvError> {
        self.receiver.try_recv()
    }
}

pub(super) fn spawn_resident_branch_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    workspace_id: BerylWorkspaceId,
    execution_target: WorkspaceId,
    syndic_storage_dir: PathBuf,
    proof: ResidentBranchProof,
    parent_thread_title: Option<String>,
    timeout: Duration,
) -> ResidentBranchTask {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let outcome = run_resident_branch_worker(
            persistence,
            connector,
            workspace_id,
            execution_target,
            syndic_storage_dir,
            proof,
            parent_thread_title,
            timeout,
        );
        let _ = sender.send(ResidentBranchUpdate::Finished(outcome));
    });
    ResidentBranchTask { receiver }
}

fn run_resident_branch_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    workspace_id: BerylWorkspaceId,
    execution_target: WorkspaceId,
    syndic_storage_dir: PathBuf,
    proof: ResidentBranchProof,
    parent_thread_title: Option<String>,
    timeout: Duration,
) -> ResidentBranchOutcome {
    match run_resident_branch_worker_result(
        persistence,
        connector,
        &workspace_id,
        &execution_target,
        syndic_storage_dir,
        &proof,
        parent_thread_title.as_deref(),
        timeout,
    ) {
        Ok(created) => created,
        Err(message) => ResidentBranchOutcome::Failed {
            source_thread_id: proof.source_thread_id,
            message,
        },
    }
}

fn run_resident_branch_worker_result(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    workspace_id: &BerylWorkspaceId,
    execution_target: &WorkspaceId,
    syndic_storage_dir: PathBuf,
    proof: &ResidentBranchProof,
    parent_thread_title: Option<&str>,
    timeout: Duration,
) -> Result<ResidentBranchOutcome, String> {
    let mut session = connector
        .connect_client(timeout)
        .map_err(|error| format!("Beryl could not connect to the managed backend: {error}"))?;
    run_resident_branch_backend_result(
        &mut session,
        persistence,
        workspace_id,
        execution_target,
        syndic_storage_dir,
        proof,
        parent_thread_title,
        timeout,
    )
}

pub(crate) fn run_resident_branch_backend_result<B>(
    session: &mut B,
    persistence: BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    execution_target: &WorkspaceId,
    syndic_storage_dir: PathBuf,
    proof: &ResidentBranchProof,
    parent_thread_title: Option<&str>,
    timeout: Duration,
) -> Result<ResidentBranchOutcome, String>
where
    B: ResidentBranchBackend,
{
    let fork = session
        .fork_thread_with_options(
            &proof.source_thread_id,
            ThreadForkOptions::metadata_only(),
            timeout,
        )
        .map_err(|error| {
            format!(
                "Beryl could not fork CAS thread {} for resident branch: {error}",
                proof.source_thread_id
            )
        })?;
    let branch_thread = fork.thread;
    let branch_summary = branch_thread.summary();
    let branch_thread_id = branch_summary.id.clone();
    validate_branch_thread(&branch_summary, execution_target)?;

    if proof.rollback_turns_after_target > 0 {
        session
            .rollback_thread(
                &branch_thread_id,
                proof.rollback_turns_after_target,
                timeout,
            )
            .map_err(|error| {
                format!(
                    "Beryl forked CAS branch {branch_thread_id} but could not roll it back by {} turn(s): {error}",
                    proof.rollback_turns_after_target
                )
            })?;
    }

    let runtime_target = execution_target.runtime_mode().display_name().to_string();
    resident_branch_edit::materialize_resident_branch_prefix(
        &syndic_storage_dir,
        workspace_id.as_str(),
        proof,
        &runtime_target,
        &branch_thread_id,
        branch_summary.name.as_deref(),
    )
    .map_err(|error| {
        format!(
            "Beryl forked CAS branch {branch_thread_id} but could not materialize its Syndic prefix: {error:?}"
        )
    })?;

    let source_thread_id = ConversationThreadId::new(proof.source_thread_id.clone());
    let source_turn_id = ConversationTurnId::new(proof.source_turn_id.clone());
    let branch_conversation_id = ConversationThreadId::new(branch_thread_id.clone());
    let bootstrap_message = branch_bootstrap_message(BranchBootstrapMessageInput {
        parent_thread_id: &source_thread_id,
        parent_thread_title,
        branch_context: None,
    });
    let bootstrap_fragment = UserInputFragment::text(bootstrap_message.clone());
    let bootstrap_admission = syndic_ingestion::admit_user_turn(
        &persistence,
        workspace_id,
        execution_target,
        Some(&branch_thread_id),
        std::slice::from_ref(&bootstrap_fragment),
    )
    .map_err(|error| {
        format!(
            "Beryl materialized CAS branch {branch_thread_id} but could not durably admit the bootstrap turn: {error}"
        )
    })?;
    let mut ingestor = SyndicLiveTurnIngestor::new(bootstrap_admission).map_err(|error| {
        format!("Beryl could not open Syndic capture for branch bootstrap: {error}")
    })?;
    ingestor
        .bind_cas_thread(&branch_thread_id)
        .map_err(|error| {
            format!("Beryl could not bind CAS branch {branch_thread_id} into Syndic: {error}")
        })?;

    let started_turn = session
        .start_turn_with_options(
            &branch_thread_id,
            &bootstrap_message,
            TurnStartOptions::default().without_developer_instructions_context(),
            timeout,
        )
        .map_err(|error| {
            let _ = ingestor.mark_local_failure(format!(
                "CAS rejected resident branch bootstrap turn: {error}"
            ));
            format!("Beryl could not start the branch bootstrap turn: {error}")
        })?
        .turn;
    let bootstrap_turn_id = bootstrap_turn_id(&branch_thread_id, &started_turn)?;
    ingestor
        .ingest_event(&TurnStreamEvent::TurnStarted {
            thread_id: branch_thread_id.clone(),
            turn: started_turn.clone(),
        })
        .map_err(|error| {
            format!("Beryl could not persist branch bootstrap turn start in Syndic: {error}")
        })?;
    let terminal_turn = wait_for_branch_bootstrap_terminal(
        &mut *session,
        &mut ingestor,
        &branch_thread_id,
        &bootstrap_turn_id,
        started_turn,
    )?;
    if terminal_turn.status != beryl_backend::TurnStatus::Completed {
        return Err(format!(
            "Beryl branch bootstrap turn {} ended with status {:?}.",
            bootstrap_turn_id.as_str(),
            terminal_turn.status
        ));
    }
    if !turn_has_visible_bootstrap_message(&terminal_turn, &bootstrap_message) {
        return Err(format!(
            "Beryl branch bootstrap turn {} did not contain the visible branch provenance message.",
            bootstrap_turn_id.as_str()
        ));
    }

    let durable_summary = prove_branch_thread_durable_with_bootstrap_turn(
        &mut *session,
        &branch_conversation_id,
        &bootstrap_turn_id,
        timeout,
    )
    .map_err(|error| error.to_string())?;

    Ok(ResidentBranchOutcome::Created {
        source_thread_id,
        source_turn_id,
        title_seed: proof.title_seed.clone(),
        execution_target: execution_target.clone(),
        thread_summary: durable_summary,
        bootstrap_turn_id,
    })
}

fn validate_branch_thread(
    summary: &ThreadSummary,
    execution_target: &WorkspaceId,
) -> Result<(), String> {
    if summary.cwd == execution_target.canonical_path() {
        return Ok(());
    }
    Err(format!(
        "The forked branch records working directory {}, but the expected workspace member is {}.",
        summary.cwd.display(),
        execution_target.canonical_path().display()
    ))
}

fn bootstrap_turn_id(thread_id: &str, turn: &TurnInfo) -> Result<ConversationTurnId, String> {
    let turn_id = turn.id.trim();
    if turn_id.is_empty() {
        return Err(format!(
            "CAS branch thread {thread_id} accepted the bootstrap turn without a turn id."
        ));
    }
    Ok(ConversationTurnId::new(turn_id.to_string()))
}

fn wait_for_branch_bootstrap_terminal<B>(
    session: &mut B,
    ingestor: &mut SyndicLiveTurnIngestor,
    branch_thread_id: &str,
    bootstrap_turn_id: &ConversationTurnId,
    started_turn: TurnInfo,
) -> Result<TurnInfo, String>
where
    B: ResidentBranchBackend,
{
    if started_turn.is_terminal() {
        ingestor
            .ingest_event(&TurnStreamEvent::TurnCompleted {
                thread_id: branch_thread_id.to_string(),
                turn: started_turn.clone(),
            })
            .map_err(|error| {
                format!("Beryl could not persist terminal branch bootstrap turn: {error}")
            })?;
        return Ok(started_turn);
    }

    loop {
        let event = match session.next_turn_stream_event(BRANCH_BOOTSTRAP_IDLE_TIMEOUT) {
            Ok(Some(event)) => event,
            Ok(None) => {
                let message =
                    "timed out waiting for live branch bootstrap completion event".to_string();
                let _ = ingestor.mark_stream_lost(message.clone());
                return Err(message);
            }
            Err(error) => {
                let message = format!("Beryl lost the branch bootstrap execution stream: {error}");
                let _ = ingestor.mark_stream_lost(message.clone());
                return Err(message);
            }
        };

        match event {
            TurnStreamEvent::ProtocolError { error } => {
                let message = format!(
                    "Beryl received a protocol error during branch bootstrap: {}",
                    error.message
                );
                let _ = ingestor.mark_stream_lost(message.clone());
                return Err(message);
            }
            TurnStreamEvent::ApprovalRequested(request) => {
                if let Err(error) = session.deny_approval_request(&request) {
                    let message =
                        format!("Beryl could not deny branch bootstrap approval: {error}");
                    let _ = ingestor.mark_local_failure(message.clone());
                    return Err(message);
                }
                let message = format!(
                    "Beryl branch bootstrap requested an approval unexpectedly: {}",
                    request.summary()
                );
                let _ = ingestor.mark_local_failure(message.clone());
                return Err(message);
            }
            TurnStreamEvent::DynamicToolCallRequested(request) => {
                let response = bootstrap_dynamic_tool_unavailable_response(&request);
                if let Err(error) = session.respond_dynamic_tool_call(&request, &response) {
                    let message = format!(
                        "Beryl could not reject branch bootstrap dynamic tool call: {error}"
                    );
                    let _ = ingestor.mark_local_failure(message.clone());
                    return Err(message);
                }
                let message = format!(
                    "Beryl branch bootstrap requested a dynamic tool unexpectedly: {}",
                    request.summary()
                );
                let _ = ingestor.mark_local_failure(message.clone());
                return Err(message);
            }
            TurnStreamEvent::TurnCompleted { thread_id, turn }
                if thread_id == branch_thread_id && turn.id == bootstrap_turn_id.as_str() =>
            {
                ingestor
                    .ingest_event(&TurnStreamEvent::TurnCompleted {
                        thread_id,
                        turn: turn.clone(),
                    })
                    .map_err(|error| {
                        format!(
                            "Beryl could not persist terminal branch bootstrap event in Syndic: {error}"
                        )
                    })?;
                return Ok(turn);
            }
            TurnStreamEvent::ThreadStatusChanged { thread_id, status }
                if thread_id == branch_thread_id && matches!(status, ThreadStatus::Idle) =>
            {
                let event = TurnStreamEvent::ThreadStatusChanged { thread_id, status };
                if let Err(error) = ingestor.ingest_event(&event) {
                    warn!(
                        error = %error,
                        "failed to persist idle branch bootstrap status before reporting failure"
                    );
                }
                let message = "branch bootstrap thread became idle before Beryl observed the live bootstrap completion event".to_string();
                let _ = ingestor.mark_stream_lost(message.clone());
                return Err(message);
            }
            event => {
                ingestor.ingest_event(&event).map_err(|error| {
                    format!("Beryl could not persist a branch bootstrap event in Syndic: {error}")
                })?;
            }
        }
    }
}
