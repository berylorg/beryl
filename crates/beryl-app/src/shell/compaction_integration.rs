//! Live compaction ownership and explicit resumption of interrupted input queues.
use super::{
    ConversationSurfaceState, ShellState, ShellView, SurfaceNotice, UserInputFragment,
    compaction_control::CompactionControl,
    compaction_observer::{
        self, Cancellation, ObserverTarget, ObserverTask, ObserverUpdate, Outcome,
        UnconfirmedReason, UpdateKind,
    },
    lifecycle_continuation::context_compaction_queue_failure_message,
    status_operation::{StatusModelListOutcome, StatusOperationUpdate},
};
use crate::compaction_diagnostics::{
    CompactionDiagnosticCategory as DiagnosticCategory, CompactionDiagnosticEventData,
    CompactionDiagnosticHandle, CompactionDiagnosticStage as DiagnosticStage,
    CompactionDiagnosticStart,
};
use beryl_backend::{ManagedBackendClientConnector, ThreadReadOptions, ThreadStatus};
use beryl_model::conversation::ConversationThreadId;
use gpui::Context;
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Instant,
};

/// One handle for the latest operation, retained while interrupted input is held.
/// Execution targets and their filesystem paths are never copied into diagnostics.
#[derive(Clone)]
pub(super) struct CompactionDiagnosticOperation {
    handle: CompactionDiagnosticHandle,
    started: Instant,
}

impl CompactionDiagnosticOperation {
    fn record(&self, stage: DiagnosticStage, category: DiagnosticCategory, turn: Option<&str>) {
        self.handle.record(
            stage,
            category,
            CompactionDiagnosticEventData {
                turn_identity: turn,
                elapsed: self.started.elapsed(),
                last_event_age: None,
            },
        );
    }
}

pub(super) enum StatusOperationTask {
    Models(Receiver<StatusOperationUpdate>),
    Compaction {
        control: CompactionControl,
        task: Option<ObserverTask>,
    },
    QueueCheck(QueueCheckTask),
}

pub(super) struct QueueCheckTask {
    pub(super) target: ObserverTarget,
    receiver: Receiver<Option<ThreadStatus>>,
    cancellation: Cancellation,
}
impl Drop for QueueCheckTask {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

pub(super) enum PolledStatusUpdate {
    Model(StatusModelListOutcome),
    Observer(ObserverUpdate),
    QueueStatus(ObserverTarget, Option<ThreadStatus>),
}

impl StatusOperationTask {
    pub(super) fn try_recv(&self) -> Result<PolledStatusUpdate, TryRecvError> {
        match self {
            Self::Models(receiver) => {
                receiver
                    .try_recv()
                    .map(|StatusOperationUpdate::ModelListFinished(outcome)| {
                        PolledStatusUpdate::Model(outcome)
                    })
            }
            Self::Compaction {
                task: Some(task), ..
            } => task.try_recv().map(PolledStatusUpdate::Observer),
            Self::Compaction { task: None, .. } => Err(TryRecvError::Empty),
            Self::QueueCheck(task) => task
                .receiver
                .try_recv()
                .map(|status| PolledStatusUpdate::QueueStatus(task.target.clone(), status)),
        }
    }
}

impl ShellView {
    pub(super) fn record_compaction_queue(&self, category: DiagnosticCategory) {
        if let Some(operation) = &self.compaction_diagnostic_operation {
            operation.record(DiagnosticStage::Queue, category, None);
        }
    }

    pub(super) fn record_compaction_stop(&self, thread_id: &str, turn_id: &str) {
        let exact = self.conversation_surface().is_some_and(|surface| {
            surface.context_compaction_thread_id() == Some(thread_id)
                && surface
                    .status_line
                    .context_compaction_cancellation_target(Some(thread_id))
                    .is_some_and(|target| target.turn_id == turn_id)
        });
        if exact && let Some(operation) = &self.compaction_diagnostic_operation {
            operation.record(
                DiagnosticStage::Stop,
                DiagnosticCategory::Requested,
                Some(turn_id),
            );
        }
    }

    pub(super) fn queue_context_compaction_turn_from_composer(
        &mut self,
        fragment: UserInputFragment,
        cx: &mut Context<Self>,
    ) -> bool {
        let (thread_id, execution_target, automatic_title_generation_allowed, turn_options) =
            match &self.state {
                ShellState::Ready(ready) => {
                    let Some(thread_id) = ready
                        .surface
                        .selected_thread_context_compaction_id()
                        .or_else(|| ready.surface.selected_held_compaction_thread_id())
                        .map(str::to_string)
                    else {
                        return false;
                    };
                    let automatic_title_generation_allowed = ready
                        .loaded_workspace
                        .workspace_state
                        .thread_automatic_title_generation_eligible(&ConversationThreadId::new(
                            thread_id.clone(),
                        ));
                    let turn_options = ready
                        .surface
                        .pending_turn_start_options(Some(thread_id.as_str()));
                    (
                        thread_id,
                        ready.execution_target.clone(),
                        automatic_title_generation_allowed,
                        turn_options,
                    )
                }
                ShellState::BackendUnavailable(unavailable) => {
                    let Some(thread_id) = unavailable
                        .surface
                        .selected_thread_context_compaction_id()
                        .or_else(|| unavailable.surface.selected_held_compaction_thread_id())
                        .map(str::to_string)
                    else {
                        return false;
                    };
                    let automatic_title_generation_allowed = unavailable
                        .loaded_workspace
                        .workspace_state
                        .thread_automatic_title_generation_eligible(&ConversationThreadId::new(
                            thread_id.clone(),
                        ));
                    let turn_options = unavailable
                        .surface
                        .pending_turn_start_options(Some(thread_id.as_str()));
                    let execution_target = Self::registered_thread_execution_target(
                        &unavailable.loaded_workspace,
                        &thread_id,
                    )
                    .unwrap_or_else(|| unavailable.execution_target.clone());
                    (
                        thread_id,
                        execution_target,
                        automatic_title_generation_allowed,
                        turn_options,
                    )
                }
                ShellState::WorkspaceIdle(_)
                | ShellState::WorkspaceLoaded(_)
                | ShellState::Blocked(_)
                | ShellState::Discovering(_)
                | ShellState::Picker(_)
                | ShellState::Opening(_) => return false,
            };

        let queued = match &mut self.state {
            ShellState::Ready(ready) => ready.surface.queue_pending_turn_fragment(
                thread_id,
                execution_target,
                automatic_title_generation_allowed,
                turn_options,
                fragment,
            ),
            ShellState::BackendUnavailable(unavailable) => {
                unavailable.surface.queue_pending_turn_fragment(
                    thread_id,
                    execution_target,
                    automatic_title_generation_allowed,
                    turn_options,
                    fragment,
                )
            }
            ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_)
            | ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_) => false,
        };

        if queued {
            let workspace_id = self
                .loaded_workspace()
                .map(|loaded| loaded.workspace.id().as_str().to_string());
            if let Some(workspace_id) = workspace_id
                && let Some(surface) = self.conversation_surface_mut()
                && let Some(queue) = surface.pending_turn_input_queue.as_mut()
            {
                queue.bind_compaction_workspace(&workspace_id);
            }
            self.clear_composer_draft(cx);
        }
        queued
    }

    pub(crate) fn compaction_unavailable_reason(&self) -> Option<String> {
        match &self.state {
            ShellState::Ready(ready) => ready
                .report
                .compatibility()
                .compaction_observation()
                .session_id()
                .err()
                .map(|error| format!("Context compaction unavailable: {error}.")),
            _ => Some("Context compaction requires an available backend.".into()),
        }
    }

    fn new_compaction_target(&mut self, thread_id: &str) -> Option<ObserverTarget> {
        let workspace_id = self.loaded_workspace()?.workspace.id().as_str().to_string();
        let execution_target = self.current_conversation_submission_target().ok()?;
        self.next_compaction_generation = self.next_compaction_generation.checked_add(1)?;
        Some(ObserverTarget {
            workspace_id,
            execution_target,
            generation: self.next_compaction_generation,
            thread_id: thread_id.into(),
        })
    }

    pub(super) fn compaction_target_matches(&self, target: &ObserverTarget) -> bool {
        self.next_compaction_generation == target.generation
            && self
                .loaded_workspace()
                .is_some_and(|loaded| loaded.workspace.id().as_str() == target.workspace_id)
            && self.current_conversation_submission_target().ok().as_ref()
                == Some(&target.execution_target)
            && self
                .conversation_surface()
                .and_then(ConversationSurfaceState::selected_thread_id)
                == Some(target.thread_id.as_str())
    }

    pub(super) fn begin_compaction_observation(
        &mut self,
        connector: ManagedBackendClientConnector,
        thread_id: String,
    ) -> bool {
        if let Some(reason) = self.compaction_unavailable_reason() {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new("Context compaction unavailable", reason));
            }
            return false;
        }
        let Some(target) = self.new_compaction_target(&thread_id) else {
            return false;
        };
        let request_timeout = self.bootstrap.probe_timeout();
        let warning_threshold = self.current_context_compaction_timeout();
        let runtime_alias = format!("compaction-runtime-{}", target.generation);
        let diagnostic_handle = self
            .compaction_diagnostics
            .begin(CompactionDiagnosticStart {
                local_generation: target.generation,
                workspace_identity: beryl_model::workspace::BerylWorkspaceId::new(
                    target.workspace_id.as_str(),
                )
                .is_ok()
                .then_some(target.workspace_id.as_str()),
                runtime_alias: Some(&runtime_alias),
                thread_identity: None,
                warning_threshold,
            });
        let diagnostic_started = Instant::now();
        self.compaction_diagnostic_operation = Some(CompactionDiagnosticOperation {
            handle: diagnostic_handle.clone(),
            started: diagnostic_started,
        });
        let task = compaction_observer::spawn_observer(
            connector,
            target.clone(),
            request_timeout,
            warning_threshold,
            Some((diagnostic_handle, diagnostic_started)),
        );
        if let Some(surface) = self.conversation_surface_mut() {
            if let Some(queue) = surface.pending_turn_input_queue.as_mut() {
                queue.bind_compaction_workspace(&target.workspace_id);
            }
            surface.begin_context_compaction(&thread_id);
        }
        self.status_operation_receiver = Some(StatusOperationTask::Compaction {
            control: CompactionControl::new(target),
            task: Some(task),
        });
        true
    }

    pub(super) fn apply_compaction_update(&mut self, update: ObserverUpdate) -> bool {
        if !self.compaction_target_matches(&update.target) {
            return false;
        }
        let thread_id = update.target.thread_id.clone();
        let Some(StatusOperationTask::Compaction { control, .. }) =
            self.status_operation_receiver.as_mut()
        else {
            return false;
        };
        let Some(kind) = control.accept(update) else {
            return false;
        };
        match kind {
            UpdateKind::Prepared | UpdateKind::Activity(_) => {}
            UpdateKind::TurnKnown { turn_id } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface
                        .status_line
                        .set_context_compaction_turn_id(&thread_id, turn_id);
                }
            }
            UpdateKind::TokenUsage { turn_id, usage } => {
                self.apply_token_usage_update(thread_id, turn_id, usage);
            }
            UpdateKind::Warning => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Compaction is taking longer than expected",
                        "Beryl is still observing this operation. Accepted input remains queued.",
                    ));
                }
            }
            UpdateKind::Unconfirmed(reason) => {
                let message = match reason {
                    UnconfirmedReason::Unavailable => {
                        "The compaction observation connection is unavailable. Beryl will retry observation; accepted input remains queued."
                    }
                    _ => {
                        "Compaction completion is not confirmed. Accepted input remains queued; Beryl will not repeat the compaction request."
                    }
                };
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Compaction outcome unconfirmed",
                        message,
                    ));
                }
                if reason == UnconfirmedReason::Unavailable {
                    self.block_if_backend_process_dead(
                        "Managed backend unavailable during compaction",
                        "Compaction completion is not confirmed.",
                        message,
                    );
                }
            }
            UpdateKind::Finished(outcome) => {
                self.status_operation_receiver = None;
                self.finish_observed_compaction(&thread_id, outcome);
            }
        }
        true
    }

    fn finish_observed_compaction(&mut self, thread_id: &str, outcome: Outcome) {
        let had_queue = self.conversation_surface().is_some_and(|surface| {
            surface
                .pending_turn_input_queue
                .as_ref()
                .is_some_and(|queue| queue.is_for_thread(thread_id))
        });
        if let Some(surface) = self.conversation_surface_mut() {
            surface.finish_context_compaction(thread_id);
            surface
                .status_line
                .set_compaction_outcome(thread_id, matches!(outcome, Outcome::Succeeded));
            match &outcome {
                Outcome::Succeeded => {
                    surface.selected_thread_status = Some(ThreadStatus::Idle);
                    surface.finish_running_tool_activity_for_thread_ok(thread_id);
                    surface.notices.clear_all();
                }
                Outcome::Failed { message } | Outcome::Rejected { message } => {
                    surface.finish_running_tool_activity_for_thread_error(thread_id);
                    surface.fail_pending_turn_input_queue_for_thread(
                        thread_id,
                        context_compaction_queue_failure_message(message),
                    );
                    // Delivery failure is not evidence of backend idle.
                    surface.selected_thread_status = None;
                    surface.set_notice(SurfaceNotice::new(
                        "Context compaction failed",
                        message.clone(),
                    ));
                }
                Outcome::Interrupted => {
                    surface.hold_compaction_input(thread_id);
                    surface.finish_running_tool_activity_for_thread_error(thread_id);
                    surface.set_notice(SurfaceNotice::new("Context compaction interrupted", "Accepted input remains queued. Your next submission will join it and resume only after the backend is idle."));
                }
            }
        }
        if matches!(outcome, Outcome::Succeeded)
            && !self.begin_pending_turn_input_queue_for_thread(thread_id)
            && let Some(surface) = self.conversation_surface_mut()
            && surface
                .pending_turn_input_queue
                .as_ref()
                .is_some_and(|queue| queue.is_for_thread(thread_id))
        {
            surface.hold_compaction_input(thread_id);
        }
        if had_queue {
            let held = self.conversation_surface().is_some_and(|surface| {
                surface
                    .pending_turn_input_queue
                    .as_ref()
                    .is_some_and(|queue| queue.is_for_thread(thread_id))
            });
            self.record_compaction_queue(match outcome {
                Outcome::Failed { .. } | Outcome::Rejected { .. } => DiagnosticCategory::Failed,
                _ if held => DiagnosticCategory::QueueHeld,
                _ => DiagnosticCategory::QueueReleased,
            });
        }
    }

    pub(super) fn handle_compaction_worker_stopped(&mut self) -> bool {
        let Some(StatusOperationTask::Compaction { task, .. }) =
            self.status_operation_receiver.as_mut()
        else {
            return false;
        };
        *task = None;
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new("Compaction outcome unconfirmed", "The observation worker stopped. Accepted input and the exact stop target are preserved."));
        }
        true
    }

    pub(super) fn begin_held_queue_eligibility(&mut self, thread_id: &str) {
        if self.status_operation_receiver.is_some() {
            return;
        }
        let Some(target) = self.new_compaction_target(thread_id) else {
            return;
        };
        let Some(connector) =
            self.backend_client_connector_for_execution_target(&target.execution_target)
        else {
            return;
        };
        let timeout = self.bootstrap.probe_timeout();
        let cancellation = Cancellation::default();
        let worker_cancel = cancellation.clone();
        let worker_target = target.clone();
        let diagnostic = self.compaction_diagnostic_operation.clone();
        self.record_compaction_queue(DiagnosticCategory::Requested);
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let result = (|| {
                if worker_cancel.is_cancelled() {
                    return None;
                }
                let mut session = connector.connect_client(timeout).ok()?;
                if worker_cancel.is_cancelled() {
                    return None;
                }
                let response = session
                    .read_thread(
                        &worker_target.thread_id,
                        ThreadReadOptions::metadata_only(),
                        timeout,
                    )
                    .ok()?;
                (response.thread.summary().id == worker_target.thread_id)
                    .then_some(response.thread.status)
            })();
            if worker_cancel.is_cancelled()
                && let Some(diagnostic) = &diagnostic
            {
                diagnostic.record(DiagnosticStage::Queue, DiagnosticCategory::Cancelled, None);
            }
            if !worker_cancel.is_cancelled() {
                let _ = sender.try_send(result);
            }
        });
        self.status_operation_receiver = Some(StatusOperationTask::QueueCheck(QueueCheckTask {
            target,
            receiver,
            cancellation,
        }));
    }

    pub(super) fn finish_held_queue_eligibility(
        &mut self,
        target: ObserverTarget,
        status: Option<ThreadStatus>,
    ) {
        if !self.compaction_target_matches(&target) {
            return;
        }
        self.record_compaction_queue(match &status {
            Some(ThreadStatus::Idle) => DiagnosticCategory::Idle,
            Some(_) => DiagnosticCategory::Active,
            None => DiagnosticCategory::Unavailable,
        });
        let authorized = self
            .conversation_surface_mut()
            .and_then(|surface| surface.pending_turn_input_queue.as_mut())
            .is_some_and(|queue| {
                queue.authorize_after_fresh_idle(
                    &target.workspace_id,
                    &target.thread_id,
                    &target.execution_target,
                    status.as_ref(),
                )
            });
        if authorized {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.selected_thread_status = Some(ThreadStatus::Idle);
                surface.held_compaction_thread_id = None;
            }
            if !self.begin_pending_turn_input_queue_for_thread(&target.thread_id)
                && let Some(surface) = self.conversation_surface_mut()
            {
                surface.hold_compaction_input(&target.thread_id);
            }
        } else if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new("Input remains queued", "The backend is busy or its idle state could not be confirmed. Submit again when it is available to resume the queued input."));
        }
        let held = self.conversation_surface().is_some_and(|surface| {
            surface
                .pending_turn_input_queue
                .as_ref()
                .is_some_and(|queue| queue.is_for_thread(&target.thread_id))
        });
        self.record_compaction_queue(if authorized && !held {
            DiagnosticCategory::QueueReleased
        } else {
            DiagnosticCategory::QueueHeld
        });
    }
}

impl ConversationSurfaceState {
    pub(super) fn snapshot_for_compaction_reopen(&self, workspace_id: &str) -> Self {
        let mut snapshot = self.snapshot_for_backend_reopen();
        if let Some(thread_id) = snapshot
            .context_compaction_thread_id
            .clone()
            .or_else(|| snapshot.held_compaction_thread_id.clone())
        {
            if let Some(queue) = snapshot
                .pending_turn_input_queue
                .as_mut()
                .filter(|queue| queue.is_for_thread(&thread_id))
            {
                queue.bind_compaction_workspace(workspace_id);
            }
            snapshot.finish_context_compaction(&thread_id);
            snapshot.hold_compaction_input(&thread_id);
        }
        snapshot
    }

    pub(super) fn selected_held_compaction_thread_id(&self) -> Option<&str> {
        self.held_compaction_thread_id
            .as_deref()
            .filter(|id| self.selected_thread_id() == Some(*id))
    }

    pub(super) fn hold_compaction_input(&mut self, thread_id: &str) {
        self.held_compaction_thread_id = Some(thread_id.to_string());
        let removal = self
            .pending_turn_input_queue
            .as_mut()
            .filter(|queue| queue.is_for_thread(thread_id))
            .and_then(|queue| {
                let index = queue.turn_index();
                queue
                    .hold_after_compaction()
                    .map(|fragment| (index, fragment))
            });
        if let Some((index, fragment)) = removal {
            self.execution_details.remove_user_input_fragments(&[(
                index,
                fragment.id,
                &fragment.text,
            )]);
            if let Some(turn) = self.execution_details.turns().get(index) {
                self.transcript_presentation
                    .replace_turn(index, turn.clone());
            }
            self.sync_live_transcript_rows(self.transcript_presentation.len());
        }
    }

    pub(super) fn restore_held_input_after_reopen(
        &mut self,
        mut queue: super::pending_turn_input::PendingTurnInputQueue,
        workspace_id: &str,
        workspace_state: &beryl_model::conversation::WorkspaceConversationState,
    ) {
        let Some(thread_id) = self.selected_thread_id().map(str::to_string) else {
            return;
        };
        let Some(registration) =
            workspace_state.thread_registration(&ConversationThreadId::new(thread_id.clone()))
        else {
            return;
        };
        if !queue.rebase_after_reopen(
            workspace_id,
            &thread_id,
            registration.execution_target(),
            self.execution_details.turns().len(),
        ) {
            return;
        }
        let index = self
            .execution_details
            .begin_pending_turn_with_fragments(queue.fragments().to_vec());
        debug_assert_eq!(queue.turn_index(), index);
        self.held_compaction_thread_id = Some(queue.thread_id().to_string());
        if let Some(turn) = self.execution_details.turns().get(index) {
            self.transcript_presentation
                .append_turn(index, turn.clone());
        }
        self.pending_turn_input_queue = Some(queue);
        self.sync_live_transcript_rows(self.transcript_presentation.len());
    }
}
