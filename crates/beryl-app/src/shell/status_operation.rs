use std::{
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use beryl_backend::{ManagedBackendClientConnector, ModelInfo};
use gpui::{Bounds, ClickEvent, Context, KeyDownEvent, MouseDownEvent, Pixels, Window};

use super::{
    ConversationSurfaceState, ShellView, SurfaceNotice,
    status_line::ThreadTurnDefaults,
    status_operation_state::{
        StatusLineOperationKind, StatusLineOperationState, StatusModelListCache,
        reasoning_effort_for_model_selection,
    },
};

const STATUS_OPERATION_POLL_MAX_EVENTS_PER_FRAME: usize = 64;
const STATUS_OPERATION_POLL_MAX_FRAME_TIME: Duration = Duration::from_millis(4);

pub(super) enum StatusOperationUpdate {
    ModelListFinished(StatusModelListOutcome),
}

pub(super) enum StatusModelListOutcome {
    Loaded { models: Vec<ModelInfo> },
    Failed { message: String },
}

pub(super) fn spawn_status_model_list_worker(
    connector: ManagedBackendClientConnector,
    timeout: Duration,
) -> Receiver<StatusOperationUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || run_status_model_list_worker(connector, timeout, sender));
    receiver
}

fn run_status_model_list_worker(
    connector: ManagedBackendClientConnector,
    timeout: Duration,
    sender: Sender<StatusOperationUpdate>,
) {
    let mut session = match connector.connect_request_client(timeout) {
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

impl ConversationSurfaceState {
    pub(crate) fn status_line_operations(&self) -> &StatusLineOperationState {
        &self.status_line_operations
    }

    pub(crate) fn status_line_operations_mut(&mut self) -> &mut StatusLineOperationState {
        &mut self.status_line_operations
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

}

impl ShellView {
    pub(crate) fn status_model_cache(&self) -> &StatusModelListCache {
        &self.status_model_cache
    }

    pub(crate) fn status_line_backend_operation_available(&self) -> bool {
        self.backend_client_connector().is_some() && self.status_operation_receiver.is_none()
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
            }
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

    fn handle_status_operation_worker_stopped(&mut self) {
        let message =
            "Beryl lost the background task that was running a status-line backend operation.";
        self.status_model_cache.finish_failed(message.to_string());
        if let Some(surface) = self.conversation_surface_mut() {
            surface.status_line_operations_mut().close();
            surface.set_notice(SurfaceNotice::new("Status operation failed", message));
        }
        self.block_if_backend_process_dead(
            "Status operation stopped unexpectedly",
            message,
            "Beryl preserved the current window, but it cannot continue until its managed runtime is relaunched.",
        );
    }

}

fn known_status_value(value: &str) -> Option<String> {
    (value != "Unknown").then(|| value.to_string())
}
