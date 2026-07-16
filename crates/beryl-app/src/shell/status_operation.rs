use std::{
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use beryl_backend::{
    ApprovalRequest, ApprovalResponseDisposition, ManagedBackendClientConnector,
    ManagedBackendSession, ModelInfo, ThreadStatus, TurnStreamEvent,
};
use gpui::{
    Bounds, ClickEvent, Context, KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseUpEvent, Pixels,
    Window,
};
use tracing::warn;

use super::{
    ConversationSurfaceState, ShellView, SurfaceNotice,
    context_compaction::ContextCompactionStreamState,
    hard_stop::{HardStopOutcome, HardStopUpdate, spawn_hard_stop_worker},
    status_line::{CancellableActiveTurn, SelectedTurnHardStopTargets, ThreadTurnDefaults},
    status_operation_state::{
        HardStopHoldSource, HardStopRequestSummary, StatusLineOperationKind,
        StatusLineOperationState, StatusModelListCache, reasoning_effort_for_model_selection,
    },
    turn_stop::{TurnStopOutcome, TurnStopUpdate, spawn_turn_stop_worker},
};

const CONTEXT_COMPACTION_IDLE_POLL_INTERVAL: Duration = Duration::from_secs(10);
const STATUS_OPERATION_POLL_MAX_EVENTS_PER_FRAME: usize = 64;
const STATUS_OPERATION_POLL_MAX_FRAME_TIME: Duration = Duration::from_millis(4);

pub(super) enum StatusOperationUpdate {
    ModelListFinished(StatusModelListOutcome),
    ContextCompactionEvent(TurnStreamEvent),
    ContextCompactionFinished(ContextCompactionOutcome),
}

pub(super) enum StatusModelListOutcome {
    Loaded { models: Vec<ModelInfo> },
    Failed { message: String },
}

pub(super) enum ContextCompactionOutcome {
    Finished { thread_id: String },
    Failed { thread_id: String, message: String },
}

pub(super) fn spawn_status_model_list_worker(
    connector: ManagedBackendClientConnector,
    timeout: Duration,
) -> Receiver<StatusOperationUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || run_status_model_list_worker(connector, timeout, sender));
    receiver
}

pub(super) fn spawn_context_compaction_worker(
    connector: ManagedBackendClientConnector,
    thread_id: String,
    request_timeout: Duration,
    stream_timeout: Duration,
) -> Receiver<StatusOperationUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        run_context_compaction_worker(
            connector,
            thread_id,
            request_timeout,
            stream_timeout,
            sender,
        )
    });
    receiver
}

fn run_status_model_list_worker(
    connector: ManagedBackendClientConnector,
    timeout: Duration,
    sender: Sender<StatusOperationUpdate>,
) {
    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            let _ = sender.send(StatusOperationUpdate::ModelListFinished(
                StatusModelListOutcome::Failed {
                    message: format!("Beryl could not connect to the managed backend: {error}"),
                },
            ));
            return;
        }
    };

    let outcome = match session.list_models(timeout) {
        Ok(models) => StatusModelListOutcome::Loaded { models },
        Err(error) => StatusModelListOutcome::Failed {
            message: format!("Beryl could not load the backend model list: {error}"),
        },
    };

    let _ = sender.send(StatusOperationUpdate::ModelListFinished(outcome));
}

fn run_context_compaction_worker(
    connector: ManagedBackendClientConnector,
    thread_id: String,
    request_timeout: Duration,
    stream_timeout: Duration,
    sender: Sender<StatusOperationUpdate>,
) {
    let mut session = match connector.connect_client(request_timeout) {
        Ok(session) => session,
        Err(error) => {
            let _ = sender.send(StatusOperationUpdate::ContextCompactionFinished(
                ContextCompactionOutcome::Failed {
                    thread_id,
                    message: format!("Beryl could not connect to the managed backend: {error}"),
                },
            ));
            return;
        }
    };

    if let Err(error) = session.resume_thread_metadata(&thread_id, request_timeout) {
        let _ = sender.send(StatusOperationUpdate::ContextCompactionFinished(
            ContextCompactionOutcome::Failed {
                thread_id,
                message: format!(
                    "Beryl could not subscribe to the thread before context compaction: {error}"
                ),
            },
        ));
        return;
    }

    if let Err(error) = session.compact_thread(&thread_id, request_timeout) {
        let _ = sender.send(StatusOperationUpdate::ContextCompactionFinished(
            ContextCompactionOutcome::Failed {
                thread_id,
                message: format!("Beryl could not start context compaction: {error}"),
            },
        ));
        return;
    }

    let started_at = Instant::now();
    let mut stream_state = ContextCompactionStreamState::default();
    loop {
        let elapsed = started_at.elapsed();
        if elapsed >= stream_timeout {
            let _ = sender.send(StatusOperationUpdate::ContextCompactionFinished(
                ContextCompactionOutcome::Failed {
                    thread_id,
                    message: "Beryl timed out waiting for context compaction to finish."
                        .to_string(),
                },
            ));
            return;
        }

        let remaining = stream_timeout - elapsed;
        let event_timeout = remaining.min(CONTEXT_COMPACTION_IDLE_POLL_INTERVAL);
        let event = match session.next_turn_stream_event(event_timeout) {
            Ok(Some(TurnStreamEvent::ApprovalRequested(request))) => {
                if let Err(message) =
                    deny_status_operation_approval(&mut session, &request, request_timeout)
                {
                    let _ = sender.send(StatusOperationUpdate::ContextCompactionFinished(
                        ContextCompactionOutcome::Failed { thread_id, message },
                    ));
                    return;
                }
                continue;
            }
            Ok(Some(TurnStreamEvent::ProtocolError { error })) => {
                let _ = sender.send(StatusOperationUpdate::ContextCompactionFinished(
                    ContextCompactionOutcome::Failed {
                        thread_id,
                        message: format!(
                            "Beryl received a protocol error during context compaction: {}",
                            error.message
                        ),
                    },
                ));
                return;
            }
            Ok(Some(event)) => event,
            Ok(None) => continue,
            Err(error) => {
                let _ = sender.send(StatusOperationUpdate::ContextCompactionFinished(
                    ContextCompactionOutcome::Failed {
                        thread_id,
                        message: format!(
                            "Beryl lost the execution stream during context compaction: {error}"
                        ),
                    },
                ));
                return;
            }
        };

        let finished = stream_state.observe(&thread_id, &event);
        let _ = sender.send(StatusOperationUpdate::ContextCompactionEvent(event));

        if finished {
            let _ = sender.send(StatusOperationUpdate::ContextCompactionFinished(
                ContextCompactionOutcome::Finished { thread_id },
            ));
            return;
        }
    }
}

impl ConversationSurfaceState {
    pub(crate) fn status_line_operations(&self) -> &StatusLineOperationState {
        &self.status_line_operations
    }

    pub(crate) fn status_line_operations_mut(&mut self) -> &mut StatusLineOperationState {
        &mut self.status_line_operations
    }

    pub(super) fn begin_context_compaction(&mut self, thread_id: &str) {
        self.context_compaction_thread_id = Some(thread_id.to_string());
        self.status_line.begin_context_compaction(thread_id);
        self.close_transcript_branch_menu();
        self.cancel_transcript_edit_mode();
        if self.selected_thread_id() == Some(thread_id) {
            self.selected_thread_status = Some(ThreadStatus::Active {
                active_flags: Vec::new(),
            });
            self.notices.clear_all();
        }
    }

    pub(crate) fn current_status_model_reasoning(&self) -> (Option<String>, Option<String>) {
        let projection = self.status_line_projection();
        (
            known_status_value(&projection.model),
            known_status_value(&projection.reasoning_effort),
        )
    }

    pub(crate) fn set_pending_status_model_reasoning(
        &mut self,
        thread_id: &str,
        model: Option<String>,
        reasoning_effort: Option<String>,
    ) -> bool {
        let defaults = ThreadTurnDefaults::new(model, reasoning_effort);
        self.status_line
            .set_pending_turn_defaults(thread_id, defaults)
    }

    pub(super) fn finish_context_compaction(&mut self, thread_id: &str) {
        if let Some(target) = self
            .status_line
            .context_compaction_cancellation_target(Some(thread_id))
        {
            self.status_line_operations
                .finish_turn_stop_request_for_target(&target.thread_id, &target.turn_id);
            self.hard_stop_targets
                .finish_turn(&target.thread_id, &target.turn_id);
        }
        self.status_line.finish_context_compaction(thread_id);
        if self.context_compaction_thread_id.as_deref() == Some(thread_id) {
            self.context_compaction_thread_id = None;
        }
        if self.selected_thread_id() == Some(thread_id) {
            self.selected_thread_status = Some(ThreadStatus::Idle);
        }
    }
}

impl ShellView {
    pub(crate) fn status_model_cache(&self) -> &StatusModelListCache {
        &self.status_model_cache
    }

    pub(crate) fn status_line_backend_operation_available(&self) -> bool {
        self.backend_client_connector().is_some()
            && self.status_operation_receiver.is_none()
            && self.hard_stop_receiver.is_none()
    }

    pub(crate) fn status_line_model_reasoning_interactive(&self, available: bool) -> bool {
        available && self.status_line_backend_operation_available()
    }

    pub(crate) fn status_line_context_interactive(&self, available: bool) -> bool {
        available && self.status_line_backend_operation_available()
    }

    pub(crate) fn status_line_turn_operations_interactive(&self, available: bool) -> bool {
        available && self.backend_client_connector().is_some()
    }

    pub(crate) fn open_status_model_reasoning_popup(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let available = self
            .conversation_surface()
            .map(|surface| surface.status_line_projection().model_reasoning_available)
            .unwrap_or(false);
        if !self.status_line_model_reasoning_interactive(available) {
            return;
        }

        if let Some(surface) = self.conversation_surface_mut() {
            surface.close_transcript_branch_menu();
            surface
                .status_line_operations_mut()
                .open(StatusLineOperationKind::ModelReasoning, event.position);
            surface.reset_status_operation_scroll();
        }
        self.begin_status_model_list_load_if_needed(window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn open_status_context_popup(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let available = self
            .conversation_surface()
            .map(|surface| surface.status_line_projection().context_operation_available)
            .unwrap_or(false);
        if !self.status_line_context_interactive(available) {
            return;
        }

        if let Some(surface) = self.conversation_surface_mut() {
            surface.close_transcript_branch_menu();
            surface
                .status_line_operations_mut()
                .open(StatusLineOperationKind::Context, event.position);
            surface.reset_status_operation_scroll();
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn open_status_turn_operations_popup(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let available = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::status_line_turn_operation_target)
            .is_some();
        if !self.status_line_turn_operations_interactive(available) {
            return;
        }

        if let Some(surface) = self.conversation_surface_mut() {
            surface.close_transcript_branch_menu();
            surface
                .status_line_operations_mut()
                .open(StatusLineOperationKind::TurnOperations, event.position);
            surface.reset_status_operation_scroll();
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn handle_status_operation_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_dismiss = self.conversation_surface().is_some_and(|surface| {
            surface
                .status_line_operations()
                .should_dismiss_for_mouse_down(event.position)
        });
        if should_dismiss && let Some(surface) = self.conversation_surface_mut() {
            surface.status_line_operations_mut().close();
            cx.notify();
        }
    }

    pub(crate) fn handle_status_operation_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key.as_str() != "escape" {
            return false;
        }
        if let Some(surface) = self.conversation_surface_mut()
            && surface.status_line_operations().is_open()
        {
            surface.status_line_operations_mut().close();
            cx.notify();
            return true;
        }
        false
    }

    pub(crate) fn handle_status_operation_key_up(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !hard_stop_hold_key(event.keystroke.key.as_str()) {
            return false;
        }

        let cancelled = self.conversation_surface_mut().is_some_and(|surface| {
            surface
                .status_line_operations_mut()
                .cancel_hard_stop_hold_source(HardStopHoldSource::Keyboard)
        });
        if cancelled {
            cx.notify();
        }
        cancelled
    }

    pub(crate) fn record_status_operation_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.status_line_operations_mut().set_bounds(bounds);
        }
    }

    pub(crate) fn retry_status_model_list(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.status_line_backend_operation_available() {
            return;
        }
        self.status_model_cache = StatusModelListCache::default();
        self.begin_status_model_list_load_if_needed(window, cx);
        cx.notify();
    }

    pub(crate) fn select_status_model(
        &mut self,
        model: ModelInfo,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let available = self
            .conversation_surface()
            .map(|surface| surface.status_line_projection().model_reasoning_available)
            .unwrap_or(false);
        if !self.status_line_model_reasoning_interactive(available) {
            return;
        }
        let Some(thread_id) = self
            .conversation_surface()
            .and_then(|surface| surface.selected_thread_id().map(str::to_string))
        else {
            return;
        };
        let current_reasoning = self
            .conversation_surface()
            .and_then(|surface| surface.current_status_model_reasoning().1);
        let reasoning_effort =
            reasoning_effort_for_model_selection(&model, current_reasoning.as_deref());
        if let Some(surface) = self.conversation_surface_mut()
            && surface.set_pending_status_model_reasoning(
                &thread_id,
                Some(model.model),
                reasoning_effort,
            )
        {
            cx.notify();
        }
    }

    pub(crate) fn select_status_reasoning_effort(
        &mut self,
        model: String,
        reasoning_effort: String,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let available = self
            .conversation_surface()
            .map(|surface| surface.status_line_projection().model_reasoning_available)
            .unwrap_or(false);
        if !self.status_line_model_reasoning_interactive(available) {
            return;
        }
        let Some(thread_id) = self
            .conversation_surface()
            .and_then(|surface| surface.selected_thread_id().map(str::to_string))
        else {
            return;
        };
        if let Some(surface) = self.conversation_surface_mut()
            && surface.set_pending_status_model_reasoning(
                &thread_id,
                Some(model),
                Some(reasoning_effort),
            )
        {
            cx.notify();
        }
    }

    pub(crate) fn compact_selected_thread_from_status_popup(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (thread_id, available) = self
            .conversation_surface()
            .map(|surface| {
                (
                    surface.selected_thread_id().map(str::to_string),
                    surface.status_line_projection().context_operation_available,
                )
            })
            .unwrap_or((None, false));
        if !self.status_line_context_interactive(available) {
            return;
        }
        let Some(thread_id) = thread_id else {
            return;
        };
        let Some(connector) = self.backend_client_connector() else {
            if let Some(block) = self.current_conversation_submission_block() {
                self.report_backend_operation_block("Context unavailable", block, cx);
            }
            return;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.status_line_operations_mut().close();
            surface.begin_context_compaction(&thread_id);
        }
        self.status_operation_receiver = Some(spawn_context_compaction_worker(
            connector,
            thread_id,
            self.bootstrap.probe_timeout(),
            self.current_context_compaction_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
    }

    pub(crate) fn stop_selected_turn_from_status_popup(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .begin_soft_stop_selected_turn_from_control(window, cx)
            .is_ok()
        {
            cx.stop_propagation();
        }
    }

    pub(crate) fn begin_soft_stop_selected_turn_from_control(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<CancellableActiveTurn, (&'static str, String)> {
        if self.turn_stop_receiver.is_some() || self.hard_stop_receiver.is_some() {
            return Err((
                "turn_stop_pending",
                "Beryl already has selected-turn stop work in progress.".to_string(),
            ));
        }

        let target = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::status_line_turn_operation_target);
        if !self.status_line_turn_operations_interactive(target.is_some()) {
            return Err((
                "turn_stop_unavailable",
                "The selected child thread has no interruptible active turn.".to_string(),
            ));
        }
        let Some(target) = target else {
            return Err((
                "turn_stop_unavailable",
                "The selected child thread has no interruptible active turn.".to_string(),
            ));
        };
        let Some(connector) = self.backend_client_connector() else {
            return Err((
                "backend_unavailable",
                "Beryl does not have an active managed backend for turn stop.".to_string(),
            ));
        };

        let started = self.conversation_surface_mut().is_some_and(|surface| {
            surface
                .status_line_operations_mut()
                .begin_turn_stop_request(target.clone())
        });
        if !started {
            return Err((
                "turn_stop_pending",
                "Beryl could not start a duplicate selected-turn stop request.".to_string(),
            ));
        }

        self.turn_stop_receiver = Some(spawn_turn_stop_worker(
            connector,
            target.clone(),
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
        Ok(target)
    }

    pub(crate) fn begin_hard_stop_hold_from_status_popup(
        &mut self,
        _: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_hard_stop_hold_from_status_popup_source(HardStopHoldSource::Pointer, window, cx);
    }

    pub(crate) fn cancel_hard_stop_hold_from_status_popup(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut()
            && surface
                .status_line_operations_mut()
                .cancel_hard_stop_hold_source(HardStopHoldSource::Pointer)
        {
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(crate) fn cancel_hard_stop_hold_on_hover_change(
        &mut self,
        hovered: &bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *hovered {
            return;
        }

        if let Some(surface) = self.conversation_surface_mut()
            && surface.status_line_operations_mut().cancel_hard_stop_hold()
        {
            cx.notify();
        }
    }

    pub(crate) fn begin_hard_stop_keyboard_hold_from_status_popup(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.is_held || !hard_stop_hold_key(event.keystroke.key.as_str()) {
            return;
        }

        self.begin_hard_stop_hold_from_status_popup_source(
            HardStopHoldSource::Keyboard,
            window,
            cx,
        );
    }

    pub(crate) fn cancel_hard_stop_keyboard_hold_from_status_popup(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !hard_stop_hold_key(event.keystroke.key.as_str()) {
            return;
        }

        if let Some(surface) = self.conversation_surface_mut()
            && surface
                .status_line_operations_mut()
                .cancel_hard_stop_hold_source(HardStopHoldSource::Keyboard)
        {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn begin_hard_stop_hold_from_status_popup_source(
        &mut self,
        source: HardStopHoldSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.turn_stop_receiver.is_some() || self.hard_stop_receiver.is_some() {
            return;
        }

        let selected_targets = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::status_line_turn_hard_stop_targets);
        let hard_stop_available = selected_targets
            .as_ref()
            .is_some_and(|targets| !targets.targets.is_empty());
        if !self.status_line_turn_operations_interactive(hard_stop_available) {
            return;
        }
        let Some(selected_targets) = selected_targets else {
            return;
        };

        let started = self.conversation_surface_mut().is_some_and(|surface| {
            surface.status_line_operations_mut().begin_hard_stop_hold(
                selected_targets.selected_turn,
                source,
                Instant::now(),
            )
        });
        if !started {
            return;
        }

        self.schedule_poll_if_needed(window, cx);
        cx.stop_propagation();
        cx.notify();
    }
}

impl ShellView {
    pub(super) fn poll_status_operation_updates(&mut self) -> bool {
        let mut updated = false;
        let poll_started_at = Instant::now();
        let mut processed_updates = 0usize;
        loop {
            if processed_updates >= STATUS_OPERATION_POLL_MAX_EVENTS_PER_FRAME
                || poll_started_at.elapsed() >= STATUS_OPERATION_POLL_MAX_FRAME_TIME
            {
                return updated;
            }

            let next_update = match self.status_operation_receiver.as_ref() {
                Some(receiver) => receiver.try_recv(),
                None => return updated,
            };

            let update = match next_update {
                Ok(update) => {
                    processed_updates = processed_updates.saturating_add(1);
                    update
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.status_operation_receiver = None;
                    self.handle_status_operation_worker_stopped();
                    updated = true;
                    break;
                }
            };

            match update {
                StatusOperationUpdate::ModelListFinished(outcome) => {
                    self.status_operation_receiver = None;
                    self.finish_status_model_list(outcome);
                    updated = true;
                    break;
                }
                StatusOperationUpdate::ContextCompactionEvent(event) => {
                    updated |= self.apply_status_operation_event(event);
                }
                StatusOperationUpdate::ContextCompactionFinished(outcome) => {
                    self.status_operation_receiver = None;
                    self.finish_context_compaction(outcome);
                    updated = true;
                    break;
                }
            }
        }

        updated
    }

    pub(super) fn poll_turn_stop_updates(&mut self) -> bool {
        let Some(receiver) = self.turn_stop_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(TurnStopUpdate::Finished(outcome)) => {
                self.turn_stop_receiver = None;
                self.finish_turn_stop_request(outcome);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.turn_stop_receiver = None;
                self.handle_turn_stop_worker_stopped();
                true
            }
        }
    }

    pub(super) fn poll_hard_stop_updates(&mut self) -> bool {
        let Some(receiver) = self.hard_stop_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(HardStopUpdate::Finished(outcome)) => {
                self.hard_stop_receiver = None;
                self.finish_hard_stop_request(outcome);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.hard_stop_receiver = None;
                self.handle_hard_stop_worker_stopped();
                true
            }
        }
    }

    pub(super) fn poll_status_operation_hold(
        &mut self,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        let current_target = self.conversation_surface().and_then(|surface| {
            surface
                .status_line_turn_hard_stop_targets()
                .map(|targets| targets.selected_turn)
        });
        let now = Instant::now();
        let mut completed_target = None;
        let mut updated = false;

        if let Some(surface) = self.conversation_surface_mut() {
            let operations = surface.status_line_operations_mut();
            if !window.is_window_active() {
                updated |= operations.cancel_hard_stop_hold();
            } else {
                updated |=
                    operations.cancel_hard_stop_hold_for_target_change(current_target.as_ref());
                if operations.hard_stop_hold_active() {
                    completed_target = operations.complete_hard_stop_hold_if_ready(now);
                    updated = true;
                }
            }
        }

        if let Some(target) = completed_target {
            updated |= self.complete_hard_stop_hold_from_status_popup(target);
        }
        updated
    }

    fn begin_status_model_list_load_if_needed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.status_operation_receiver.is_some() {
            return;
        }

        let Some(connector) = self.backend_client_connector() else {
            self.status_model_cache
                .finish_failed("Beryl does not have an active managed backend.".to_string());
            return;
        };

        if !self.status_model_cache.should_load() {
            return;
        }

        self.status_model_cache.begin_loading();
        self.status_operation_receiver = Some(spawn_status_model_list_worker(
            connector,
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
    }

    fn finish_status_model_list(&mut self, outcome: StatusModelListOutcome) {
        match outcome {
            StatusModelListOutcome::Loaded { models } => {
                self.status_model_cache.finish_loaded(models);
            }
            StatusModelListOutcome::Failed { message } => {
                self.status_model_cache.finish_failed(message.clone());
                self.block_if_backend_process_dead(
                    "Managed backend disconnected while loading models",
                    "The backend process exited before Beryl could load the available model list.",
                    &message,
                );
            }
        }
    }

    fn finish_context_compaction(&mut self, outcome: ContextCompactionOutcome) {
        match outcome {
            ContextCompactionOutcome::Finished { thread_id } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.finish_context_compaction(&thread_id);
                    surface.finish_running_tool_activity_for_thread_ok(&thread_id);
                }
            }
            ContextCompactionOutcome::Failed { thread_id, message } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.finish_context_compaction(&thread_id);
                    surface.finish_running_tool_activity_for_thread_error(&thread_id);
                    surface.set_notice(SurfaceNotice::new(
                        "Context compaction failed",
                        message.clone(),
                    ));
                }

                self.block_if_backend_process_dead(
                    "Managed backend disconnected during context compaction",
                    "The backend process exited before context compaction finished.",
                    &message,
                );
            }
        }
    }

    fn finish_turn_stop_request(&mut self, outcome: TurnStopOutcome) {
        match outcome {
            TurnStopOutcome::Accepted { target } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    let target_matches =
                        surface.status_line_operations().turn_stop_request_target()
                            == Some(&target);
                    if target_matches {
                        surface.status_line_operations_mut().close();
                    }
                }
            }
            TurnStopOutcome::Failed { target, message } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface
                        .status_line_operations_mut()
                        .fail_turn_stop_request(target, message.clone());
                    surface.set_notice(SurfaceNotice::new("Turn stop failed", message.clone()));
                }

                self.block_if_backend_process_dead(
                    "Managed backend disconnected while stopping a turn",
                    "The backend process exited before Beryl could request turn cancellation.",
                    &message,
                );
            }
        }
    }

    fn finish_hard_stop_request(&mut self, outcome: HardStopOutcome) {
        match outcome {
            HardStopOutcome::Finished {
                selected_turn,
                outcomes,
            } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    let target_matches = surface
                        .status_line_operations()
                        .hard_stop_request_target()
                        .is_some_and(|target| target.selected_turn == selected_turn);
                    if !target_matches {
                        return;
                    }

                    let summary = surface
                        .status_line_operations_mut()
                        .finish_hard_stop_request(outcomes);
                    if let Some(summary) = summary {
                        if summary.failures.is_empty() && summary.request_error.is_none() {
                            surface.status_line_operations_mut().close();
                        } else {
                            surface.set_notice(SurfaceNotice::new(
                                "Hard stop partially failed",
                                hard_stop_summary_notice(&summary),
                            ));
                        }
                    }
                }
            }
            HardStopOutcome::Failed {
                selected_turn,
                message,
            } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface
                        .status_line_operations_mut()
                        .fail_hard_stop_request(selected_turn, message.clone());
                    surface.set_notice(SurfaceNotice::new("Hard stop failed", message.clone()));
                }

                self.block_if_backend_process_dead(
                    "Managed backend disconnected while hard-stopping a turn",
                    "The backend process exited before Beryl could request hard stop.",
                    &message,
                );
            }
        }
    }

    fn handle_turn_stop_worker_stopped(&mut self) {
        let message = "Beryl lost the background task that was stopping the active turn.";
        if let Some(surface) = self.conversation_surface_mut() {
            let target = surface
                .status_line_operations()
                .turn_stop_request_target()
                .cloned();
            if let Some(target) = target {
                surface
                    .status_line_operations_mut()
                    .fail_turn_stop_request(target, message.to_string());
            }
            surface.set_notice(SurfaceNotice::new("Turn stop failed", message));
        }

        self.block_if_backend_process_dead(
            "Turn stop stopped unexpectedly",
            message,
            "Beryl preserved the current window, but it cannot continue until its managed runtime is relaunched.",
        );
    }

    fn handle_hard_stop_worker_stopped(&mut self) {
        let message = "Beryl lost the background task that was hard-stopping the active turn.";
        if let Some(surface) = self.conversation_surface_mut() {
            let target = surface
                .status_line_operations()
                .hard_stop_request_target()
                .map(|target| target.selected_turn.clone());
            if let Some(target) = target {
                surface
                    .status_line_operations_mut()
                    .fail_hard_stop_request(target, message.to_string());
            }
            surface.set_notice(SurfaceNotice::new("Hard stop failed", message));
        }

        self.block_if_backend_process_dead(
            "Hard stop stopped unexpectedly",
            message,
            "Beryl preserved the current window, but it cannot continue until its managed runtime is relaunched.",
        );
    }

    fn apply_status_operation_event(&mut self, event: TurnStreamEvent) -> bool {
        let mut updated = self
            .conversation_surface_mut()
            .is_some_and(|surface| surface.observe_context_compaction_event(&event));
        match event {
            TurnStreamEvent::TokenUsageUpdated {
                thread_id,
                turn_id,
                token_usage,
            } => updated | self.apply_token_usage_update(thread_id, turn_id, token_usage),
            TurnStreamEvent::AccountRateLimitsUpdated { rate_limits } => {
                updated | self.apply_account_rate_limits_update(rate_limits)
            }
            _ => updated,
        }
    }

    fn handle_status_operation_worker_stopped(&mut self) {
        let message =
            "Beryl lost the background task that was running a status-line backend operation.";
        self.status_model_cache.finish_failed(message.to_string());
        if let Some(surface) = self.conversation_surface_mut() {
            let selected_thread_id = surface.selected_thread_id().map(str::to_string);
            if let Some(thread_id) = selected_thread_id.as_deref() {
                surface.finish_context_compaction(thread_id);
                surface.finish_running_tool_activity_for_thread_error(thread_id);
            }
            surface.status_line_operations_mut().close();
            surface.set_notice(SurfaceNotice::new("Status operation failed", message));
        }
        self.block_if_backend_process_dead(
            "Status operation stopped unexpectedly",
            message,
            "Beryl preserved the current window, but it cannot continue until its managed runtime is relaunched.",
        );
    }

    fn complete_hard_stop_hold_from_status_popup(
        &mut self,
        target: super::status_line::CancellableActiveTurn,
    ) -> bool {
        if self.turn_stop_receiver.is_some() || self.hard_stop_receiver.is_some() {
            return false;
        }

        let selected_targets = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::status_line_turn_hard_stop_targets)
            .filter(|targets| targets.selected_turn == target && !targets.targets.is_empty());
        let Some(selected_targets) = selected_targets else {
            return false;
        };

        let Some(connector) = self.backend_client_connector() else {
            let message = "Beryl does not have an active managed backend for hard stop.";
            if let Some(surface) = self.conversation_surface_mut() {
                surface
                    .status_line_operations_mut()
                    .fail_hard_stop_request(target, message.to_string());
                surface.set_notice(SurfaceNotice::new("Hard stop failed", message));
            }
            return true;
        };

        let started = self.conversation_surface_mut().is_some_and(|surface| {
            surface
                .status_line_operations_mut()
                .begin_hard_stop_request(selected_targets.clone())
        });
        if !started {
            return false;
        }

        self.hard_stop_receiver = Some(spawn_hard_stop_worker(
            connector,
            selected_targets,
            self.bootstrap.probe_timeout(),
        ));
        true
    }

    pub(crate) fn begin_hard_stop_selected_turn_from_control(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<SelectedTurnHardStopTargets, (&'static str, String)> {
        if self.turn_stop_receiver.is_some() || self.hard_stop_receiver.is_some() {
            return Err((
                "turn_stop_pending",
                "Beryl already has selected-turn stop work in progress.".to_string(),
            ));
        }

        let selected_targets = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::status_line_turn_hard_stop_targets)
            .filter(|targets| !targets.targets.is_empty());
        let Some(selected_targets) = selected_targets else {
            return Err((
                "hard_stop_unavailable",
                "The selected child thread has no probed hard-stop targets for its active turn."
                    .to_string(),
            ));
        };

        let Some(connector) = self.backend_client_connector() else {
            return Err((
                "backend_unavailable",
                "Beryl does not have an active managed backend for hard stop.".to_string(),
            ));
        };

        let started = self.conversation_surface_mut().is_some_and(|surface| {
            surface
                .status_line_operations_mut()
                .begin_hard_stop_request(selected_targets.clone())
        });
        if !started {
            return Err((
                "turn_stop_pending",
                "Beryl could not start a duplicate selected-turn hard-stop request.".to_string(),
            ));
        }

        self.hard_stop_receiver = Some(spawn_hard_stop_worker(
            connector,
            selected_targets.clone(),
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
        Ok(selected_targets)
    }
}

fn known_status_value(value: &str) -> Option<String> {
    (value != "Unknown").then(|| value.to_string())
}

fn hard_stop_summary_notice(summary: &HardStopRequestSummary) -> String {
    if let Some(error) = summary.request_error.as_ref() {
        return error.clone();
    }

    format!(
        "{} of {} hard-stop target{} failed.",
        summary.failures.len(),
        summary.target_count,
        if summary.target_count == 1 { "" } else { "s" }
    )
}

fn hard_stop_hold_key(key: &str) -> bool {
    matches!(key, "enter" | "space" | " ")
}

fn deny_status_operation_approval(
    session: &mut ManagedBackendSession,
    request: &ApprovalRequest,
    request_timeout: Duration,
) -> Result<(), String> {
    if request.response_disposition() == ApprovalResponseDisposition::ResponseRequired {
        warn!(
            approval = %request.summary(),
            approval_payload = %request.pretty_params(),
            "auto-denying unsupported backend approval request during status operation"
        );
        session.deny_approval_request(request).map_err(|error| {
            format!("Beryl could not deny the backend approval request: {error}")
        })?;
    } else {
        warn!(
            approval = %request.summary(),
            "observed an approval request already denied by the backend session"
        );
    }

    if request.kind().denial_response_interrupts_turn() {
        return Ok(());
    }

    let Some(thread_id) = request.thread_id() else {
        return Err(
            "Beryl denied a backend approval request but could not interrupt the turn because the request did not include a thread id."
                .to_string(),
        );
    };
    let Some(turn_id) = request.turn_id() else {
        return Err(
            "Beryl denied a backend approval request but could not interrupt the turn because the request did not include a turn id."
                .to_string(),
        );
    };

    session
        .interrupt_turn(thread_id, turn_id, request_timeout)
        .map_err(|error| {
            format!("Beryl denied the backend approval request but could not interrupt the turn: {error}")
        })
}
