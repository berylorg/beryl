use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_backend::{ManagedBackendClientConnector, ThreadInfo, ThreadSummary};
use beryl_model::{
    conversation::{ConversationThreadId, WorkspaceConversationState},
    workspace::WorkspaceId,
};
use gpui::{Context, Window};
use tracing::warn;

use super::{
    ShellState, ShellView, SurfaceNotice,
    thread_title::ThreadTitleCandidate,
    transcript_branch_core::{
        TranscriptBranchActivationGate, TranscriptBranchOutcome, register_transcript_branch_thread,
        run_transcript_branch, transcript_branch_activation_blocker,
    },
    transcript_branch_menu_state::{TranscriptBranchAction, TranscriptBranchRequest},
    turn_worker::spawn_thread_activation_worker,
};

pub(super) enum TranscriptBranchUpdate {
    Finished(TranscriptBranchOutcome),
}

pub(super) fn spawn_transcript_branch_worker(
    connector: ManagedBackendClientConnector,
    request: TranscriptBranchRequest,
    timeout: Duration,
) -> Receiver<TranscriptBranchUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let outcome = run_transcript_branch_worker(connector, request, timeout);
        let _ = sender.send(TranscriptBranchUpdate::Finished(outcome));
    });
    receiver
}

fn run_transcript_branch_worker(
    connector: ManagedBackendClientConnector,
    request: TranscriptBranchRequest,
    timeout: Duration,
) -> TranscriptBranchOutcome {
    let action = request.action();
    let source_thread_id = request.target().source_thread_id().to_string();
    let source_turn_id = request.target().source_turn_id().to_string();

    let mut session = match connector.connect_request_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            return TranscriptBranchOutcome::Failed {
                action,
                source_thread_id,
                source_turn_id,
                message: format!("Beryl could not connect to the managed backend: {error}"),
            };
        }
    };

    run_transcript_branch(&mut session, request, timeout)
}

impl ShellView {
    pub(super) fn poll_transcript_branch_updates(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(receiver) = self.transcript_branch_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(TranscriptBranchUpdate::Finished(outcome)) => {
                self.transcript_branch_receiver = None;
                self.finish_transcript_branch_worker(outcome, window, cx);
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.transcript_branch_receiver = None;
                self.handle_transcript_branch_worker_stopped();
                true
            }
        }
    }

    fn finish_transcript_branch_worker(
        &mut self,
        outcome: TranscriptBranchOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            TranscriptBranchOutcome::Branched {
                action,
                source_thread_id,
                source_turn_id,
                title_seed,
                thread,
            } => self.finish_successful_transcript_branch(
                action,
                source_thread_id,
                source_turn_id,
                title_seed,
                thread,
                window,
                cx,
            ),
            TranscriptBranchOutcome::Failed {
                action,
                source_thread_id,
                source_turn_id,
                message,
            } => {
                warn!(
                    ?action,
                    source_thread_id = %source_thread_id,
                    source_turn_id = %source_turn_id,
                    error = %message,
                    "transcript branch worker failed"
                );
                self.finish_failed_transcript_branch(message);
            }
        }
    }

    fn finish_successful_transcript_branch(
        &mut self,
        action: TranscriptBranchAction,
        source_thread_id: String,
        _: String,
        title_seed: String,
        thread: ThreadInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let summary = thread.summary();
        let branch_thread_id = ConversationThreadId::new(summary.id.clone());
        let source_thread_id = ConversationThreadId::new(source_thread_id);

        let registration = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                self.finish_failed_transcript_branch(
                    "Beryl created the branch, but the workspace is no longer loaded.".to_string(),
                );
                return;
            };
            let workspace_id = loaded.workspace.id().clone();
            let result = register_transcript_branch_thread(
                &mut loaded.workspace_state,
                &source_thread_id,
                &summary,
            );
            match result {
                Ok((execution_target, touched_manifest)) => (
                    workspace_id,
                    loaded.workspace_state.clone(),
                    execution_target,
                    touched_manifest,
                ),
                Err(message) => {
                    self.finish_failed_transcript_branch(message);
                    return;
                }
            }
        };
        let (workspace_id, workspace_state, execution_target, touched_manifest) = registration;

        if touched_manifest {
            self.persist_current_workspace_state(true);
        }
        self.mark_member_thread_inventory_refresh_needed();

        if let Some(candidate) =
            ThreadTitleCandidate::new(branch_thread_id.as_str().to_string(), title_seed)
        {
            let _ = self.repair_thread_title_from_candidate(execution_target.clone(), candidate);
        }

        match action {
            TranscriptBranchAction::Background => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Thread branched",
                        "Beryl created the branch in the background.",
                    ));
                }
            }
            TranscriptBranchAction::SwitchTo => {
                self.activate_transcript_branch(
                    workspace_id,
                    workspace_state,
                    execution_target,
                    branch_thread_id,
                    &summary,
                    window,
                    cx,
                );
            }
        }
    }

    fn activate_transcript_branch(
        &mut self,
        workspace_id: beryl_model::workspace::BerylWorkspaceId,
        workspace_state: WorkspaceConversationState,
        execution_target: WorkspaceId,
        branch_thread_id: ConversationThreadId,
        summary: &ThreadSummary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_execution_target = match &self.state {
            ShellState::Ready(ready) => Some(ready.execution_target.clone()),
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_) => None,
        };
        let connector = self.backend_client_connector();
        if let Some(blocker) =
            transcript_branch_activation_blocker(TranscriptBranchActivationGate {
                activation_in_progress: self.thread_activation_receiver.is_some(),
                workspace_ready: current_execution_target.is_some(),
                execution_target_matches_branch: current_execution_target
                    .as_ref()
                    .is_some_and(|target| target == &execution_target),
                backend_available: connector.is_some(),
            })
        {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Thread branch created",
                    blocker.notice_detail(),
                ));
            }
            return;
        }

        let connector = connector.expect("activation gate verified backend availability");

        let label = crate::member_thread_inventory::resolved_thread_title(
            &workspace_state,
            &branch_thread_id,
            &execution_target,
            &summary.preview,
            summary.name.as_deref(),
            summary.created_at,
            summary.updated_at,
        );
        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_thread_activation(label.clone());
        }
        self.composer_image_label_scan_receiver = None;
        self.notify_transcript_panel(cx);
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return;
        };
        self.thread_activation_receiver = Some(spawn_thread_activation_worker(
            persistence,
            connector,
            workspace_id,
            execution_target,
            branch_thread_id.as_str().to_string(),
            label,
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
    }

    fn finish_failed_transcript_branch(&mut self, message: String) {
        warn!(error = %message, "transcript branch failed");
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new("Thread branch failed", message.clone()));
        }

        self.block_if_backend_process_dead(
            "Managed backend disconnected during thread branching",
            "The backend process exited before Beryl could finish creating the branch.",
            &message,
        );
    }

    fn handle_transcript_branch_worker_stopped(&mut self) {
        let message = "Beryl lost the background task that was creating the branch.";
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new("Thread branch failed", message));
        }
        self.block_if_backend_process_dead(
            "Thread branch stopped unexpectedly",
            message,
            "Beryl preserved the current workspace surface, but it cannot continue until the managed backend for this workspace is relaunched.",
        );
    }
}
