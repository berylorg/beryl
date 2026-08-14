use std::time::{Duration, Instant};

use beryl_backend::{BackendConfigDefaults, HardStopTarget, HardStopTargetOutcome, ModelInfo};
use beryl_model::workspace::WorkspaceId;
use gpui::{Bounds, Pixels, Point};

use super::status_line::{CancellableActiveTurn, SelectedTurnHardStopTargets, ThreadTurnDefaults};

pub(crate) const HARD_STOP_HOLD_DURATION: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Default)]
pub(crate) struct StatusLineOperationState {
    open: Option<StatusLineOperationOpen>,
    turn_stop_request: TurnStopRequestState,
    hard_stop_request: HardStopRequestState,
    hard_stop_hold: Option<HardStopHoldState>,
}

#[derive(Clone, Debug)]
pub(crate) struct StatusLineOperationOpen {
    kind: StatusLineOperationKind,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLineOperationKind {
    ModelReasoning,
    Context,
    TurnOperations,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TurnStopRequestState {
    target: Option<CancellableActiveTurn>,
    last_error: Option<TurnStopRequestError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TurnStopRequestError {
    target: CancellableActiveTurn,
    message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HardStopRequestState {
    target: Option<HardStopRequestTarget>,
    last_summary: Option<HardStopRequestSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HardStopRequestTarget {
    pub(crate) selected_turn: CancellableActiveTurn,
    pub(crate) targets: Vec<HardStopTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HardStopRequestSummary {
    pub(crate) selected_turn: CancellableActiveTurn,
    pub(crate) target_count: usize,
    pub(crate) succeeded_count: usize,
    pub(crate) failures: Vec<HardStopTargetFailure>,
    pub(crate) request_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HardStopTargetFailure {
    pub(crate) target: HardStopTarget,
    pub(crate) method: &'static str,
    pub(crate) message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HardStopHoldSource {
    Pointer,
    Keyboard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HardStopHoldState {
    target: CancellableActiveTurn,
    source: HardStopHoldSource,
    started_at: Instant,
    duration: Duration,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StatusModelListCache {
    target: Option<WorkspaceId>,
    loading_target: Option<WorkspaceId>,
    models: Option<Vec<ModelInfo>>,
    config_defaults: Option<BackendConfigDefaults>,
    loading: bool,
    last_error: Option<String>,
}

impl StatusLineOperationState {
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(crate) fn active(&self) -> Option<&StatusLineOperationOpen> {
        self.open.as_ref()
    }

    pub(crate) fn open(&mut self, kind: StatusLineOperationKind, position: Point<Pixels>) {
        self.hard_stop_hold = None;
        self.open = Some(StatusLineOperationOpen {
            kind,
            position,
            bounds: None,
        });
    }

    pub(crate) fn close(&mut self) {
        self.open = None;
        self.hard_stop_hold = None;
    }

    pub(crate) fn set_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        if let Some(open) = self.open.as_mut() {
            open.bounds = bounds;
        }
    }

    pub(crate) fn should_dismiss_for_mouse_down(&self, position: Point<Pixels>) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| !open.bounds.is_some_and(|bounds| bounds.contains(&position)))
    }

    pub(crate) fn turn_stop_request_in_flight(&self) -> bool {
        self.turn_stop_request.target.is_some()
    }

    pub(crate) fn hard_stop_request_in_flight(&self) -> bool {
        self.hard_stop_request.target.is_some()
    }

    pub(crate) fn stop_request_in_flight(&self) -> bool {
        self.turn_stop_request_in_flight() || self.hard_stop_request_in_flight()
    }

    pub(crate) fn hard_stop_hold_active(&self) -> bool {
        self.hard_stop_hold.is_some()
    }

    pub(crate) fn hard_stop_hold_progress_for_target(
        &self,
        target: &CancellableActiveTurn,
        now: Instant,
    ) -> Option<f32> {
        let hold = self.hard_stop_hold.as_ref()?;
        (hold.target == *target).then(|| hold.progress(now))
    }

    pub(crate) fn begin_hard_stop_hold(
        &mut self,
        target: CancellableActiveTurn,
        source: HardStopHoldSource,
        now: Instant,
    ) -> bool {
        if self.stop_request_in_flight()
            || !self
                .open
                .as_ref()
                .is_some_and(|open| open.kind == StatusLineOperationKind::TurnOperations)
        {
            return false;
        }

        if self
            .hard_stop_hold
            .as_ref()
            .is_some_and(|hold| hold.target == target && hold.source == source)
        {
            return false;
        }

        self.hard_stop_hold = Some(HardStopHoldState {
            target,
            source,
            started_at: now,
            duration: HARD_STOP_HOLD_DURATION,
        });
        true
    }

    pub(crate) fn cancel_hard_stop_hold(&mut self) -> bool {
        self.hard_stop_hold.take().is_some()
    }

    pub(crate) fn cancel_hard_stop_hold_source(&mut self, source: HardStopHoldSource) -> bool {
        let matches_source = self
            .hard_stop_hold
            .as_ref()
            .is_some_and(|hold| hold.source == source);
        if matches_source {
            self.hard_stop_hold = None;
        }
        matches_source
    }

    pub(crate) fn cancel_hard_stop_hold_for_target_change(
        &mut self,
        current_target: Option<&CancellableActiveTurn>,
    ) -> bool {
        let should_cancel = self
            .hard_stop_hold
            .as_ref()
            .is_some_and(|hold| match current_target {
                Some(current_target) => *current_target != hold.target,
                None => true,
            });
        if should_cancel {
            self.hard_stop_hold = None;
        }
        should_cancel
    }

    pub(crate) fn complete_hard_stop_hold_if_ready(
        &mut self,
        now: Instant,
    ) -> Option<CancellableActiveTurn> {
        let ready = self
            .hard_stop_hold
            .as_ref()
            .is_some_and(|hold| hold.is_complete(now));
        if ready {
            return self.hard_stop_hold.take().map(|hold| hold.target);
        }
        None
    }

    #[allow(dead_code)]
    pub(crate) fn turn_stop_request_target(&self) -> Option<&CancellableActiveTurn> {
        self.turn_stop_request.target.as_ref()
    }

    pub(crate) fn begin_turn_stop_request(&mut self, target: CancellableActiveTurn) -> bool {
        if self.stop_request_in_flight() {
            return false;
        }

        self.turn_stop_request.target = Some(target);
        self.turn_stop_request.last_error = None;
        true
    }

    #[allow(dead_code)]
    pub(crate) fn hard_stop_request_target(&self) -> Option<&HardStopRequestTarget> {
        self.hard_stop_request.target.as_ref()
    }

    #[allow(dead_code)]
    pub(crate) fn hard_stop_request_summary(&self) -> Option<&HardStopRequestSummary> {
        self.hard_stop_request.last_summary.as_ref()
    }

    #[allow(dead_code)]
    pub(crate) fn begin_hard_stop_request(
        &mut self,
        selected_targets: SelectedTurnHardStopTargets,
    ) -> bool {
        if self.stop_request_in_flight() || selected_targets.targets.is_empty() {
            return false;
        }

        self.hard_stop_request.target = Some(HardStopRequestTarget {
            selected_turn: selected_targets.selected_turn,
            targets: selected_targets.targets,
        });
        self.hard_stop_request.last_summary = None;
        true
    }

    #[allow(dead_code)]
    pub(crate) fn finish_hard_stop_request(
        &mut self,
        outcomes: Vec<HardStopTargetOutcome>,
    ) -> Option<HardStopRequestSummary> {
        let target = self.hard_stop_request.target.take()?;
        let summary = hard_stop_summary_from_outcomes(target, outcomes);
        self.hard_stop_request.last_summary = Some(summary.clone());
        Some(summary)
    }

    #[allow(dead_code)]
    pub(crate) fn fail_hard_stop_request(
        &mut self,
        selected_turn: CancellableActiveTurn,
        message: String,
    ) -> Option<HardStopRequestTarget> {
        let active = self.hard_stop_request.target.take();
        let target_count = active.as_ref().map_or(0, |target| target.targets.len());
        self.hard_stop_request.last_summary = Some(HardStopRequestSummary {
            selected_turn,
            target_count,
            succeeded_count: 0,
            failures: Vec::new(),
            request_error: Some(message),
        });
        active
    }

    pub(crate) fn clear_stop_requests_for_backend_exit(&mut self) -> bool {
        let changed = self.turn_stop_request.target.is_some()
            || self.hard_stop_request.target.is_some()
            || self.hard_stop_hold.is_some();
        self.turn_stop_request.target = None;
        self.hard_stop_request.target = None;
        self.hard_stop_hold = None;
        changed
    }

    #[allow(dead_code)]
    pub(crate) fn finish_turn_stop_request(&mut self) -> Option<CancellableActiveTurn> {
        self.turn_stop_request.target.take()
    }

    pub(crate) fn fail_turn_stop_request(
        &mut self,
        target: CancellableActiveTurn,
        message: String,
    ) -> Option<CancellableActiveTurn> {
        let active = self.turn_stop_request.target.take();
        self.turn_stop_request.last_error = Some(TurnStopRequestError { target, message });
        active
    }

    pub(crate) fn turn_stop_request_error(
        &self,
        target: Option<&CancellableActiveTurn>,
    ) -> Option<&str> {
        let error = self.turn_stop_request.last_error.as_ref()?;
        let target = target?;
        (&error.target == target).then_some(error.message.as_str())
    }

    pub(crate) fn finish_turn_stop_request_for_target(
        &mut self,
        thread_id: &str,
        turn_id: &str,
    ) -> bool {
        let matches_target = self
            .turn_stop_request
            .target
            .as_ref()
            .is_some_and(|target| target.thread_id == thread_id && target.turn_id == turn_id);
        let matches_error = self
            .turn_stop_request
            .last_error
            .as_ref()
            .is_some_and(|error| {
                error.target.thread_id == thread_id && error.target.turn_id == turn_id
            });
        let matches_hard_stop = self
            .hard_stop_request
            .target
            .as_ref()
            .is_some_and(|target| {
                target.selected_turn.thread_id == thread_id
                    && target.selected_turn.turn_id == turn_id
            });
        let matches_hard_summary =
            self.hard_stop_request
                .last_summary
                .as_ref()
                .is_some_and(|summary| {
                    summary.selected_turn.thread_id == thread_id
                        && summary.selected_turn.turn_id == turn_id
                });

        if !matches_target && !matches_error && !matches_hard_stop && !matches_hard_summary {
            return false;
        }

        if matches_target {
            self.turn_stop_request.target = None;
            if self
                .open
                .as_ref()
                .is_some_and(|open| open.kind == StatusLineOperationKind::TurnOperations)
            {
                self.open = None;
            }
        }
        if self.hard_stop_hold.as_ref().is_some_and(|hold| {
            hold.target.thread_id == thread_id && hold.target.turn_id == turn_id
        }) {
            self.hard_stop_hold = None;
        }
        if matches_error {
            self.turn_stop_request.last_error = None;
        }
        if matches_hard_stop {
            self.hard_stop_request.target = None;
            if self
                .open
                .as_ref()
                .is_some_and(|open| open.kind == StatusLineOperationKind::TurnOperations)
            {
                self.open = None;
            }
        }
        if matches_hard_summary {
            self.hard_stop_request.last_summary = None;
        }
        true
    }
}

fn hard_stop_summary_from_outcomes(
    target: HardStopRequestTarget,
    outcomes: Vec<HardStopTargetOutcome>,
) -> HardStopRequestSummary {
    let mut succeeded_count = 0;
    let mut failures = Vec::new();
    for outcome in outcomes {
        match outcome {
            HardStopTargetOutcome::Succeeded { .. } => {
                succeeded_count += 1;
            }
            HardStopTargetOutcome::Failed {
                target,
                method,
                message,
            } => failures.push(HardStopTargetFailure {
                target,
                method,
                message,
            }),
        }
    }

    HardStopRequestSummary {
        selected_turn: target.selected_turn,
        target_count: target.targets.len(),
        succeeded_count,
        failures,
        request_error: None,
    }
}

impl StatusLineOperationOpen {
    pub(crate) fn kind(&self) -> StatusLineOperationKind {
        self.kind
    }

    pub(crate) fn position(&self) -> Point<Pixels> {
        self.position
    }
}

impl HardStopHoldState {
    fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }

        let elapsed = now.saturating_duration_since(self.started_at);
        (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    fn is_complete(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }
}

impl StatusModelListCache {
    pub(crate) fn target(&self) -> Option<&WorkspaceId> {
        self.target.as_ref()
    }

    pub(crate) fn models(&self) -> Option<&[ModelInfo]> {
        self.models.as_deref()
    }

    pub(crate) fn loading(&self) -> bool {
        self.loading
    }

    pub(crate) fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub(crate) fn begin_loading_for(&mut self, target: WorkspaceId) {
        if self.target.as_ref() != Some(&target) {
            self.models = None;
            self.config_defaults = None;
        }
        self.target = Some(target.clone());
        self.loading_target = Some(target);
        self.loading = true;
        self.last_error = None;
    }

    #[cfg(test)]
    pub(crate) fn finish_loaded(&mut self, models: Vec<ModelInfo>) {
        self.finish_loaded_with_config(models, BackendConfigDefaults::default());
    }

    pub(crate) fn finish_loaded_for_target(
        &mut self,
        target: WorkspaceId,
        models: Vec<ModelInfo>,
        config_defaults: BackendConfigDefaults,
    ) {
        self.loading_target = None;
        self.target = Some(target);
        self.config_defaults = Some(config_defaults);
        self.models = Some(models);
        self.loading = false;
        self.last_error = None;
    }

    pub(crate) fn finish_loaded_with_config(
        &mut self,
        models: Vec<ModelInfo>,
        config_defaults: BackendConfigDefaults,
    ) {
        let Some(target) = self.loading_target.take() else {
            self.models = None;
            self.config_defaults = None;
            self.loading = false;
            self.last_error =
                Some("Beryl discarded a model list loaded without a runtime target.".to_string());
            return;
        };
        self.finish_loaded_for_target(target, models, config_defaults);
    }

    pub(crate) fn finish_failed(&mut self, message: String) {
        if let Some(target) = self.loading_target.take() {
            self.target = Some(target);
        }
        self.loading = false;
        self.last_error = Some(message);
    }

    pub(crate) fn should_load_for(&self, target: &WorkspaceId) -> bool {
        !self.loading && (self.models.is_none() || self.target.as_ref() != Some(target))
    }

    pub(crate) fn find_model(&self, value: &str) -> Option<&ModelInfo> {
        self.models()?
            .iter()
            .find(|model| model.model == value || model.id == value || model.display_name == value)
    }

    pub(crate) fn effective_default_turn_defaults(&self) -> Option<ThreadTurnDefaults> {
        let config_defaults = self.config_defaults.as_ref();
        let model = config_defaults
            .and_then(|defaults| defaults.model.clone())
            .or_else(|| self.default_model_for_new_thread());
        let reasoning_effort =
            config_defaults.and_then(|defaults| defaults.model_reasoning_effort.clone());

        if model.is_none() && reasoning_effort.is_none() {
            return None;
        }

        Some(ThreadTurnDefaults::new(model, reasoning_effort))
    }

    fn default_model_for_new_thread(&self) -> Option<String> {
        let models = self.models()?;
        models
            .iter()
            .find(|model| model.is_default)
            .or_else(|| models.iter().find(|model| !model.hidden))
            .map(|model| model.model.clone())
    }
}

pub(crate) fn reasoning_effort_for_model_selection(
    model: &ModelInfo,
    current_reasoning_effort: Option<&str>,
) -> Option<String> {
    if model.supported_reasoning_efforts.is_empty() {
        return None;
    }

    if let Some(current) = current_reasoning_effort
        && model
            .supported_reasoning_efforts
            .iter()
            .any(|effort| effort == current)
    {
        return Some(current.to_string());
    }

    model
        .default_reasoning_effort
        .as_deref()
        .filter(|default| {
            model
                .supported_reasoning_efforts
                .iter()
                .any(|effort| effort == default)
        })
        .map(str::to_string)
        .or_else(|| model.supported_reasoning_efforts.first().cloned())
}
