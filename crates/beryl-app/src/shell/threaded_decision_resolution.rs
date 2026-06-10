mod graph_update;
mod state;
mod tool;

use beryl_backend::ThreadStatus;
use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
    threaded_decision::{
        ThreadedDecisionRecord, ThreadedDecisionRecordId, ThreadedDecisionStateError,
        ThreadedDecisionStatus,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use gpui::{Context, Window};
use tracing::warn;

use super::{
    ShellView, SurfaceNotice,
    execution_detail::UserInputFragment,
    graph::GraphMutationUpdate,
    graph_worker::{GraphUpdate, GraphWorkerTask},
    thread_title::TurnThreadTitleMode,
    token_usage_snapshot,
    turn_worker::spawn_turn_worker,
    turn_worker::{shell_dynamic_tool_request_channel, spawn_thread_activation_worker},
};

#[derive(Clone, Debug)]
pub(super) struct QueuedDecisionResolutionJob {
    workspace_id: BerylWorkspaceId,
    record_id: ThreadedDecisionRecordId,
}

#[derive(Clone, Debug)]
pub(super) struct PendingDecisionHandoffTurn {
    record_id: ThreadedDecisionRecordId,
}

pub(super) struct DecisionResolutionGraphTask {
    record_id: ThreadedDecisionRecordId,
    handoff_turn_id: ConversationTurnId,
    provenance: MutationProvenance,
    receiver: GraphWorkerTask,
}

impl QueuedDecisionResolutionJob {
    fn new(workspace_id: BerylWorkspaceId, record_id: ThreadedDecisionRecordId) -> Self {
        Self {
            workspace_id,
            record_id,
        }
    }
}

impl DecisionResolutionGraphTask {
    fn new(
        record_id: ThreadedDecisionRecordId,
        handoff_turn_id: ConversationTurnId,
        provenance: MutationProvenance,
        receiver: GraphWorkerTask,
    ) -> Self {
        Self {
            record_id,
            handoff_turn_id,
            provenance,
            receiver,
        }
    }

    fn try_recv(&self) -> Result<GraphUpdate, std::sync::mpsc::TryRecvError> {
        self.receiver.try_recv()
    }

    fn disconnected_update(&self) -> GraphMutationUpdate {
        self.receiver.disconnected_update(
            "Beryl lost the background task that was completing the decision checklist item.",
        )
    }
}

impl ShellView {
    pub(super) fn has_queued_decision_resolution_jobs_for_child(&self, thread_id: &str) -> bool {
        self.pending_decision_resolution_jobs.iter().any(|job| {
            self.workspace_shell_state().is_some_and(|loaded| {
                loaded
                    .threaded_decision_state
                    .record(&job.record_id)
                    .and_then(ThreadedDecisionRecord::child_thread_id)
                    .is_some_and(|child_thread_id| child_thread_id.as_str() == thread_id)
            })
        }) || self.workspace_shell_state().is_some_and(|loaded| {
            loaded
                .threaded_decision_state
                .records()
                .iter()
                .any(|record| {
                    record.status() == ThreadedDecisionStatus::PendingResolution
                        && record
                            .child_thread_id()
                            .is_some_and(|child_thread_id| child_thread_id.as_str() == thread_id)
                })
        })
    }

    pub(super) fn begin_next_ready_decision_resolution_handoff(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.decision_resolution_graph_receiver.is_some()
            || self
                .decision_resolution_parent_activation_record_id
                .is_some()
            || self.pending_decision_handoff_turn.is_some()
        {
            return false;
        }

        self.hydrate_ready_decision_resolution_jobs();
        let Some(job) = self.pending_decision_resolution_jobs.front().cloned() else {
            return false;
        };
        if !self
            .loaded_workspace()
            .is_some_and(|loaded| loaded.workspace.id() == &job.workspace_id)
        {
            self.pending_decision_resolution_jobs.pop_front();
            return true;
        }

        let Some(record) = self.decision_resolution_record(&job.record_id) else {
            self.pending_decision_resolution_jobs.pop_front();
            return true;
        };
        match record.status() {
            ThreadedDecisionStatus::PendingResolution => {
                self.begin_pending_decision_parent_handoff(job, record, window, cx)
            }
            ThreadedDecisionStatus::HandoffStarted if record.handoff_turn_id().is_some() => {
                self.begin_known_handoff_checklist_update(job, record, cx)
            }
            _ => {
                self.pending_decision_resolution_jobs.pop_front();
                true
            }
        }
    }

    pub(super) fn note_decision_handoff_turn_started(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pending) = self.pending_decision_handoff_turn.take() else {
            return false;
        };
        let parent_thread_id = ConversationThreadId::new(thread_id.to_string());
        let handoff_turn_id = ConversationTurnId::new(turn_id.to_string());
        let provenance = MutationProvenance::new(
            "beryl",
            token_usage_snapshot::current_unix_millis(),
            MutationSource::conversation_turn(parent_thread_id.clone(), handoff_turn_id.clone()),
            Some(100),
        )
        .expect("conversation-turn provenance is valid");

        let record_matches = self
            .decision_resolution_record(&pending.record_id)
            .is_some_and(|record| record.parent_thread_id() == &parent_thread_id);
        if !record_matches {
            self.pending_decision_handoff_turn = Some(pending);
            return false;
        }

        let changed = self.workspace_shell_state_mut().is_some_and(|loaded| {
            loaded
                .threaded_decision_state
                .mark_handoff_started(
                    &pending.record_id,
                    Some(handoff_turn_id.clone()),
                    provenance.clone(),
                )
                .unwrap_or_else(|error| {
                    warn!(error = %error, "failed to record threaded-decision handoff turn id");
                    false
                })
        });
        if changed {
            self.persist_current_threaded_decision_state();
        }

        if let Some(workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        {
            self.queue_decision_resolution_job(workspace_id, pending.record_id.clone());
        }
        self.begin_next_ready_decision_resolution_handoff(window, cx);
        true
    }

    pub(super) fn cancel_pending_decision_handoff_turn_after_worker_stop(&mut self) -> bool {
        let Some(pending) = self.pending_decision_handoff_turn.take() else {
            return false;
        };
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Decision handoff uncertain",
                format!(
                    "Beryl started a decision handoff attempt for record {}, but did not receive a parent turn id. It will not retry automatically.",
                    pending.record_id.as_str()
                ),
            ));
        }
        true
    }

    pub(super) fn finish_decision_resolution_parent_activation(
        &mut self,
        activation_succeeded: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(record_id) = self.decision_resolution_parent_activation_record_id.take() else {
            return false;
        };
        if activation_succeeded {
            return self.begin_next_ready_decision_resolution_handoff(window, cx);
        }

        self.remove_queued_decision_resolution_job(&record_id);
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Decision handoff unavailable",
                "Beryl could not reopen the exact parent thread, so the decision remains pending resolution.",
            ));
        }
        true
    }

    fn begin_pending_decision_parent_handoff(
        &mut self,
        job: QueuedDecisionResolutionJob,
        record: ThreadedDecisionRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.turn_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.status_operation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
        {
            return false;
        }

        let Some((execution_target, parent_label, automatic_title_generation_allowed)) =
            self.parent_thread_activation_details(&record)
        else {
            self.remove_queued_decision_resolution_job(&job.record_id);
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Decision handoff unavailable",
                    "The bound parent thread is no longer registered.",
                ));
            }
            return true;
        };

        if self
            .conversation_surface()
            .and_then(|surface| surface.selected_thread_id())
            != Some(record.parent_thread_id().as_str())
        {
            return self.begin_decision_resolution_parent_activation(
                job,
                record,
                execution_target,
                parent_label,
                window,
                cx,
            );
        }

        let parent_idle = self.conversation_surface().is_some_and(|surface| {
            matches!(
                surface.selected_thread_status.as_ref(),
                Some(ThreadStatus::Idle)
            )
        });
        if !parent_idle {
            self.remove_queued_decision_resolution_job(&job.record_id);
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Decision handoff unavailable",
                    "The exact parent thread is not idle, so Beryl did not start the handoff turn.",
                ));
            }
            return true;
        }

        let Some(handoff_message) = self.normalized_parent_handoff_message(&record) else {
            self.remove_queued_decision_resolution_job(&job.record_id);
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Decision handoff unavailable",
                    "Beryl could not build the parent handoff message because the decision checklist item is missing.",
                ));
            }
            return true;
        };

        let Some(connector) = self.backend_client_connector_for_execution_target(&execution_target)
        else {
            self.remove_queued_decision_resolution_job(&job.record_id);
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Decision handoff unavailable",
                    "Beryl does not have an active managed backend for the bound parent thread.",
                ));
            }
            return true;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return false;
        };

        let provenance = MutationProvenance::new(
            "beryl",
            token_usage_snapshot::current_unix_millis(),
            MutationSource::workspace_action("decision_handoff_start")
                .expect("workspace action provenance is valid"),
            Some(100),
        )
        .expect("workspace action provenance is valid");
        let state_result = self.workspace_shell_state_mut().map(|loaded| {
            loaded.threaded_decision_state.mark_handoff_started(
                record.record_id(),
                None,
                provenance,
            )
        });
        match state_result {
            Some(Ok(changed)) => {
                if changed {
                    self.persist_current_threaded_decision_state();
                }
            }
            Some(Err(error)) => {
                self.report_decision_resolution_state_error(error);
                self.remove_queued_decision_resolution_job(&job.record_id);
                return true;
            }
            None => return false,
        }

        let parent_thread_id = record.parent_thread_id().as_str().to_string();
        let Some(beryl_workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return false;
        };
        let turn_options = {
            let Some(surface) = self.conversation_surface() else {
                return false;
            };
            self.turn_options_with_current_developer_instructions(
                Some(parent_thread_id.as_str()),
                surface.pending_turn_start_options(Some(parent_thread_id.as_str())),
            )
        };
        let handoff_fragment = UserInputFragment::text(handoff_message);
        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_turn_for_thread(parent_thread_id.as_str(), handoff_fragment.clone());
        }
        self.notify_transcript_panel(cx);
        let (shell_tool_sender, shell_tool_receiver) = shell_dynamic_tool_request_channel();
        self.shell_tool_receiver = Some(shell_tool_receiver);
        self.pending_decision_handoff_turn = Some(PendingDecisionHandoffTurn {
            record_id: record.record_id().clone(),
        });
        self.turn_receiver = Some(spawn_turn_worker(
            persistence,
            connector,
            beryl_workspace_id,
            execution_target,
            Some(parent_thread_id),
            TurnThreadTitleMode::automatic_if_allowed(automatic_title_generation_allowed),
            vec![handoff_fragment],
            turn_options,
            Some(shell_tool_sender),
            self.bootstrap.probe_timeout(),
        ));
        self.pending_decision_resolution_jobs.pop_front();
        self.schedule_poll_if_needed(window, cx);
        true
    }

    fn begin_decision_resolution_parent_activation(
        &mut self,
        job: QueuedDecisionResolutionJob,
        record: ThreadedDecisionRecord,
        execution_target: WorkspaceId,
        label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(connector) = self.backend_client_connector_for_execution_target(&execution_target)
        else {
            self.remove_queued_decision_resolution_job(&job.record_id);
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Decision handoff unavailable",
                    "Beryl does not have an active managed backend for the bound parent thread.",
                ));
            }
            return true;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return false;
        };
        let Some(beryl_workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return false;
        };
        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_thread_activation(label.clone());
        }
        self.composer_image_label_validation_receiver = None;
        self.composer_image_label_scan_receiver = None;
        self.decision_resolution_parent_activation_record_id = Some(record.record_id().clone());
        self.thread_activation_receiver = Some(spawn_thread_activation_worker(
            persistence,
            connector,
            beryl_workspace_id,
            execution_target,
            record.parent_thread_id().as_str().to_string(),
            label,
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
        true
    }

    fn report_decision_resolution_state_error(&mut self, error: ThreadedDecisionStateError) {
        warn!(error = %error, "threaded-decision resolution state update failed");
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Decision resolution unavailable",
                error.to_string(),
            ));
        }
    }
}
