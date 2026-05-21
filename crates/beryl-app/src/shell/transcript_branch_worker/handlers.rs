use std::sync::mpsc;

use beryl_backend::{ThreadInfo, ThreadSummary};
use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId, WorkspaceConversationState},
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use gpui::{Context, Window};
use tracing::warn;

use super::super::{
    ShellState, ShellView,
    execution_detail::UserInputFragment,
    surface_notice::SurfaceNotice,
    thread_title::ThreadTitleCandidate,
    transcript_branch_core::{
        ForegroundTranscriptBranchPublication, ForegroundTranscriptBranchStart,
        TranscriptBranchActivationGate, TranscriptBranchOutcome, register_transcript_branch_thread,
        transcript_branch_activation_blocker,
    },
    transcript_branch_menu_state::TranscriptBranchAction,
    turn_worker::spawn_thread_activation_worker,
};
use super::TranscriptBranchUpdate;

impl ShellView {
    pub(in crate::shell) fn poll_transcript_branch_updates(
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
                durable_summary,
                bootstrap_turn_id,
            } => self.finish_successful_transcript_branch(
                action,
                source_thread_id,
                source_turn_id,
                title_seed,
                thread,
                durable_summary,
                bootstrap_turn_id,
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
        source_turn_id: String,
        title_seed: String,
        _: ThreadInfo,
        summary: ThreadSummary,
        bootstrap_turn_id: Option<ConversationTurnId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let branch_thread_id = ConversationThreadId::new(summary.id.clone());
        let source_thread_id = ConversationThreadId::new(source_thread_id);
        let source_turn_id = ConversationTurnId::new(source_turn_id);

        let Some((workspace_id, workspace_state, execution_target)) = self
            .publish_transcript_branch_metadata(
                source_thread_id,
                source_turn_id,
                branch_thread_id.clone(),
                title_seed,
                &summary,
                bootstrap_turn_id,
            )
        else {
            return;
        };

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

    fn publish_transcript_branch_metadata(
        &mut self,
        source_thread_id: ConversationThreadId,
        source_turn_id: ConversationTurnId,
        branch_thread_id: ConversationThreadId,
        title_seed: String,
        summary: &ThreadSummary,
        bootstrap_turn_id: Option<ConversationTurnId>,
    ) -> Option<(BerylWorkspaceId, WorkspaceConversationState, WorkspaceId)> {
        let registration = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                self.finish_failed_transcript_branch(
                    "Beryl created the branch, but the workspace is no longer loaded.".to_string(),
                );
                return None;
            };
            let workspace_id = loaded.workspace.id().clone();
            let result = register_transcript_branch_thread(
                &mut loaded.workspace_state,
                &source_thread_id,
                &source_turn_id,
                summary,
                bootstrap_turn_id,
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
                    return None;
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

        Some((workspace_id, workspace_state, execution_target))
    }

    pub(in crate::shell) fn begin_foreground_transcript_branch(
        &mut self,
        start: ForegroundTranscriptBranchStart,
        cx: &mut Context<Self>,
    ) -> bool {
        let execution_target = self.current_ready_execution_target_for_branch();
        let branch_thread_id = start.branch_thread_id().clone();
        let bootstrap_turn_id = start.bootstrap_turn_id().clone();

        if let Some(state) = self.foreground_transcript_branch.as_mut() {
            if state.source_thread_id() != start.source_thread_id()
                || state.source_turn_id() != start.source_turn_id()
                || state.action() != start.action()
            {
                self.finish_failed_foreground_transcript_branch(
                    "Beryl started a foreground branch, but it no longer matches the requested source turn.",
                );
                return true;
            }
            state.activate(branch_thread_id.clone(), bootstrap_turn_id.clone());
        }

        if let Some(surface) = self.conversation_surface_mut() {
            surface.load_thread_history(start.thread());
            surface.set_thread_session_metadata(start.session_metadata().clone());
            surface.begin_turn_for_thread(
                branch_thread_id.as_str(),
                UserInputFragment::text(start.bootstrap_message().to_string()),
            );
            let event = beryl_backend::TurnStreamEvent::TurnStarted {
                thread_id: branch_thread_id.as_str().to_string(),
                turn: start.bootstrap_turn().clone(),
            };
            surface.apply_stream_event(event, execution_target.as_ref());
        }
        self.notify_transcript_panel(cx);
        true
    }

    pub(in crate::shell) fn current_ready_execution_target_for_branch(
        &self,
    ) -> Option<WorkspaceId> {
        match &self.state {
            ShellState::Ready(ready) => Some(ready.execution_target.clone()),
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::BackendUnavailable(_)
            | ShellState::Blocked(_) => None,
        }
    }

    pub(in crate::shell) fn finish_foreground_transcript_branch_publication(
        &mut self,
        publication: ForegroundTranscriptBranchPublication,
    ) -> bool {
        match publication {
            ForegroundTranscriptBranchPublication::Published {
                source_thread_id,
                source_turn_id,
                title_seed,
                durable_summary,
                bootstrap_turn_id,
            } => {
                let branch_thread_id = ConversationThreadId::new(durable_summary.id.clone());
                if !self.foreground_transcript_branch_matches(
                    &source_thread_id,
                    &source_turn_id,
                    branch_thread_id.as_str(),
                    bootstrap_turn_id.as_str(),
                ) {
                    self.finish_failed_foreground_transcript_branch(
                        "Beryl finished a foreground branch, but the completed branch no longer matches the selected branch workflow.",
                    );
                    return true;
                }
                self.foreground_transcript_branch = None;
                let source_thread_id = ConversationThreadId::new(source_thread_id);
                let source_turn_id = ConversationTurnId::new(source_turn_id);
                let _ = self.publish_transcript_branch_metadata(
                    source_thread_id,
                    source_turn_id,
                    branch_thread_id,
                    title_seed,
                    &durable_summary,
                    Some(bootstrap_turn_id),
                );
                true
            }
            ForegroundTranscriptBranchPublication::Failed {
                source_thread_id,
                source_turn_id,
                message,
            } => {
                warn!(
                    source_thread_id = %source_thread_id,
                    source_turn_id = %source_turn_id,
                    error = %message,
                    "foreground transcript branch publication failed"
                );
                self.finish_failed_foreground_transcript_branch(message);
                true
            }
        }
    }

    pub(in crate::shell) fn foreground_transcript_branch_event_is_bootstrap_terminal(
        &self,
        event: &beryl_backend::TurnStreamEvent,
    ) -> bool {
        let beryl_backend::TurnStreamEvent::TurnCompleted { thread_id, turn } = event else {
            return false;
        };
        self.foreground_transcript_branch
            .as_ref()
            .is_some_and(|state| state.bootstrap_turn_matches(thread_id, &turn.id))
    }

    pub(in crate::shell) fn finish_failed_foreground_transcript_branch(
        &mut self,
        message: impl Into<String>,
    ) {
        let message = message.into();
        self.foreground_transcript_branch = None;
        self.finish_failed_transcript_branch(message);
    }

    fn foreground_transcript_branch_matches(
        &self,
        source_thread_id: &str,
        source_turn_id: &str,
        branch_thread_id: &str,
        bootstrap_turn_id: &str,
    ) -> bool {
        self.foreground_transcript_branch
            .as_ref()
            .is_some_and(|state| {
                state.source_thread_id() == source_thread_id
                    && state.source_turn_id() == source_turn_id
                    && state.bootstrap_turn_matches(branch_thread_id, bootstrap_turn_id)
            })
    }

    pub(in crate::shell) fn activate_transcript_branch(
        &mut self,
        workspace_id: BerylWorkspaceId,
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
            | ShellState::BackendUnavailable(_)
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
