use beryl_backend::{ThreadSessionMetadata, ThreadStatus, ThreadTokenUsage, TurnStartOptions};
use std::collections::HashMap;

const UNKNOWN_LABEL: &str = "Unknown";

#[derive(Clone, Debug, Default)]
pub(crate) struct StatusLineState {
    session_metadata: ThreadSessionMetadata,
    pending_turn_defaults_by_thread: HashMap<String, ThreadTurnDefaults>,
    effective_turn_defaults_by_thread: HashMap<String, ThreadTurnDefaults>,
    token_usage_by_thread: HashMap<String, ThreadTokenUsage>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ThreadTurnDefaults {
    model: Option<String>,
    reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CancellableActiveTurn {
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusLineProjection {
    pub(crate) model: String,
    pub(crate) reasoning_effort: String,
    pub(crate) context_space_left: String,
    pub(crate) context_value_segments: Vec<StatusLineCellValueSegment>,
    pub(crate) last_turn_state: String,
    pub(crate) turn_view: StatusLineTurnView,
    pub(crate) model_reasoning_available: bool,
    pub(crate) context_operation_available: bool,
    pub(crate) cancellable_active_turn: Option<CancellableActiveTurn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusLineCellSpec {
    pub(crate) label: &'static str,
    pub(crate) value: String,
    pub(crate) value_segments: Vec<StatusLineCellValueSegment>,
    pub(crate) action: StatusLineCellAction,
    pub(crate) value_kind: StatusLineCellValueKind,
    pub(crate) enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusLineCellValueSegment {
    pub(crate) text: String,
    pub(crate) kind: StatusLineCellValueSegmentKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StatusLineTurnView {
    current: Option<usize>,
    total: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLineCellValueSegmentKind {
    Label,
    Value,
    SecondaryValue,
}

struct ContextStatus {
    plain_text: String,
    value_segments: Vec<StatusLineCellValueSegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLineCellAction {
    ModelReasoning,
    Context,
    TurnOperations,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLineCellValueKind {
    Default,
    TurnState,
}

pub(crate) fn status_line_model_reasoning_available(
    selected_thread_id: Option<&str>,
    selected_thread_status: Option<&ThreadStatus>,
) -> bool {
    match selected_thread_id {
        Some(_) => selected_thread_status.is_some_and(thread_status_allows_user_operation),
        None => true,
    }
}

pub(crate) fn status_line_context_operation_available(
    selected_thread_id: Option<&str>,
    selected_thread_status: Option<&ThreadStatus>,
) -> bool {
    selected_thread_id.is_some()
        && selected_thread_status.is_some_and(thread_status_allows_user_operation)
}

#[allow(dead_code)]
pub(crate) fn status_line_operations_available(
    selected_thread_id: Option<&str>,
    selected_thread_status: Option<&ThreadStatus>,
) -> bool {
    status_line_context_operation_available(selected_thread_id, selected_thread_status)
}

pub(crate) fn status_line_cell_specs(
    status: StatusLineProjection,
    model_reasoning_enabled: bool,
    context_enabled: bool,
    turn_operations_enabled: bool,
) -> [StatusLineCellSpec; 3] {
    let turn_operation_available = status.cancellable_active_turn.is_some();
    let model_reasoning_value = format!("{} / {}", status.model, status.reasoning_effort);
    let context_value_segments = if status.context_value_segments.is_empty() {
        vec![StatusLineCellValueSegment::value(
            status.context_space_left.clone(),
        )]
    } else {
        status.context_value_segments
    };
    let last_turn_state_value = status.last_turn_state;
    let turn_view_value = status.turn_view.display();
    [
        StatusLineCellSpec {
            label: "Model / Reasoning",
            value: model_reasoning_value.clone(),
            value_segments: vec![StatusLineCellValueSegment::value(model_reasoning_value)],
            action: StatusLineCellAction::ModelReasoning,
            value_kind: StatusLineCellValueKind::Default,
            enabled: model_reasoning_enabled,
        },
        StatusLineCellSpec {
            label: "Context",
            value: status.context_space_left,
            value_segments: context_value_segments,
            action: StatusLineCellAction::Context,
            value_kind: StatusLineCellValueKind::Default,
            enabled: context_enabled,
        },
        StatusLineCellSpec {
            label: "Turn",
            value: last_turn_state_value.clone(),
            value_segments: vec![
                StatusLineCellValueSegment::value(last_turn_state_value),
                StatusLineCellValueSegment::label("View"),
                StatusLineCellValueSegment::secondary_value(turn_view_value),
            ],
            action: if turn_operation_available {
                StatusLineCellAction::TurnOperations
            } else {
                StatusLineCellAction::None
            },
            value_kind: StatusLineCellValueKind::TurnState,
            enabled: turn_operation_available && turn_operations_enabled,
        },
    ]
}

pub(crate) fn turn_start_options_with_developer_instructions_context(
    options: TurnStartOptions,
    developer_instructions: Option<String>,
    defaults: ThreadTurnDefaults,
) -> TurnStartOptions {
    let Some(model) = defaults.model().map(str::to_string) else {
        return options.without_developer_instructions_context();
    };

    options.with_developer_instructions_context(
        developer_instructions,
        model,
        defaults.reasoning_effort().map(str::to_string),
    )
}

impl CancellableActiveTurn {
    pub(crate) fn ordinary(thread_id: impl Into<String>, turn_id: impl Into<String>) -> Self {
        Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
        }
    }
}

impl StatusLineCellValueSegment {
    fn label(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusLineCellValueSegmentKind::Label,
        }
    }

    fn value(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusLineCellValueSegmentKind::Value,
        }
    }

    fn secondary_value(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusLineCellValueSegmentKind::SecondaryValue,
        }
    }
}

impl ThreadTurnDefaults {
    #[allow(dead_code)]
    pub(crate) fn new(model: Option<String>, reasoning_effort: Option<String>) -> Self {
        Self {
            model: non_empty(model),
            reasoning_effort: non_empty(reasoning_effort),
        }
    }

    pub(crate) fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub(crate) fn reasoning_effort(&self) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }

    fn is_empty(&self) -> bool {
        self.model.is_none() && self.reasoning_effort.is_none()
    }

    fn to_turn_start_options(&self) -> TurnStartOptions {
        let mut options = TurnStartOptions::default();
        if let Some(model) = &self.model {
            options = options.with_model(model.clone());
        }
        if let Some(reasoning_effort) = &self.reasoning_effort {
            options = options.with_reasoning_effort(reasoning_effort.clone());
        }
        options
    }
}

impl StatusLineState {
    pub(crate) fn set_session_metadata_for_thread(
        &mut self,
        selected_thread_id: Option<&str>,
        metadata: ThreadSessionMetadata,
    ) {
        self.set_session_metadata(metadata);
        if let Some(thread_id) = selected_thread_id {
            self.effective_turn_defaults_by_thread.remove(thread_id);
        }
    }

    pub(crate) fn set_session_metadata(&mut self, metadata: ThreadSessionMetadata) {
        self.session_metadata = ThreadSessionMetadata {
            model: non_empty(metadata.model),
            model_provider: non_empty(metadata.model_provider),
            reasoning_effort: non_empty(metadata.reasoning_effort),
        };
    }

    pub(crate) fn clear_session_metadata(&mut self) {
        self.session_metadata = ThreadSessionMetadata::default();
    }

    #[allow(dead_code)]
    pub(crate) fn set_pending_turn_defaults(
        &mut self,
        thread_id: impl Into<String>,
        defaults: ThreadTurnDefaults,
    ) -> bool {
        let thread_id = thread_id.into();
        if defaults.is_empty() {
            return self
                .pending_turn_defaults_by_thread
                .remove(&thread_id)
                .is_some();
        }

        let changed = self.pending_turn_defaults_by_thread.get(&thread_id) != Some(&defaults);
        self.pending_turn_defaults_by_thread
            .insert(thread_id, defaults);
        changed
    }

    pub(crate) fn promote_pending_turn_defaults(&mut self, thread_id: &str) -> bool {
        let Some(defaults) = self.pending_turn_defaults_by_thread.remove(thread_id) else {
            return false;
        };

        if defaults.is_empty() {
            self.effective_turn_defaults_by_thread.remove(thread_id);
        } else {
            self.effective_turn_defaults_by_thread
                .insert(thread_id.to_string(), defaults);
        }
        true
    }

    pub(crate) fn pending_turn_start_options(
        &self,
        selected_thread_id: Option<&str>,
    ) -> TurnStartOptions {
        selected_thread_id
            .and_then(|thread_id| self.pending_turn_defaults_by_thread.get(thread_id))
            .map(ThreadTurnDefaults::to_turn_start_options)
            .unwrap_or_default()
    }

    pub(crate) fn effective_turn_context_defaults(
        &self,
        selected_thread_id: Option<&str>,
    ) -> ThreadTurnDefaults {
        ThreadTurnDefaults::new(
            self.model_for_status(selected_thread_id)
                .map(str::to_string),
            self.reasoning_effort_for_status(selected_thread_id)
                .map(str::to_string),
        )
    }

    pub(crate) fn apply_token_usage(
        &mut self,
        known_thread: bool,
        thread_id: String,
        _turn_id: String,
        token_usage: ThreadTokenUsage,
    ) -> bool {
        if !known_thread {
            return false;
        }

        self.token_usage_by_thread.insert(thread_id, token_usage);
        true
    }

    #[cfg(test)]
    pub(crate) fn cached_thread_count(&self) -> usize {
        self.token_usage_by_thread.len()
    }

    #[cfg(test)]
    pub(crate) fn projection(
        &self,
        selected_thread_id: Option<&str>,
        last_turn_state: &'static str,
    ) -> StatusLineProjection {
        self.projection_with_operation_availability(
            selected_thread_id,
            false,
            false,
            last_turn_state,
        )
    }

    pub(crate) fn projection_with_operation_availability(
        &self,
        selected_thread_id: Option<&str>,
        model_reasoning_available: bool,
        context_operation_available: bool,
        last_turn_state: &'static str,
    ) -> StatusLineProjection {
        let context_status = self.context_status(selected_thread_id);
        StatusLineProjection {
            model: label_or_unknown(self.model_for_status(selected_thread_id)),
            reasoning_effort: label_or_unknown(
                self.reasoning_effort_for_status(selected_thread_id),
            ),
            context_space_left: context_status.plain_text,
            context_value_segments: context_status.value_segments,
            last_turn_state: last_turn_state.to_string(),
            turn_view: StatusLineTurnView::unknown(),
            model_reasoning_available,
            context_operation_available,
            cancellable_active_turn: None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn projection_with_cancellable_active_turn(
        &self,
        selected_thread_id: Option<&str>,
        model_reasoning_available: bool,
        context_operation_available: bool,
        last_turn_state: &'static str,
        cancellable_active_turn: Option<CancellableActiveTurn>,
    ) -> StatusLineProjection {
        self.projection_with_turn_operations(
            selected_thread_id,
            model_reasoning_available,
            context_operation_available,
            last_turn_state,
            cancellable_active_turn,
        )
    }

    pub(crate) fn projection_with_turn_operations(
        &self,
        selected_thread_id: Option<&str>,
        model_reasoning_available: bool,
        context_operation_available: bool,
        last_turn_state: &'static str,
        cancellable_active_turn: Option<CancellableActiveTurn>,
    ) -> StatusLineProjection {
        let mut projection = self.projection_with_operation_availability(
            selected_thread_id,
            model_reasoning_available,
            context_operation_available,
            last_turn_state,
        );
        projection.cancellable_active_turn = cancellable_active_turn;
        projection
    }

    fn context_space_left_percent(&self, selected_thread_id: Option<&str>) -> Option<u8> {
        let selected_thread_id = selected_thread_id?;
        let usage = self.token_usage_by_thread.get(selected_thread_id)?;

        let model_context_window = usage.model_context_window?;
        if model_context_window <= 0 {
            return None;
        }

        let input_tokens = usage.last.input_tokens.max(0);
        let remaining = (model_context_window - input_tokens).clamp(0, model_context_window);
        let percent = ((remaining as f64 / model_context_window as f64) * 100.0).round();
        Some(percent.clamp(0.0, 100.0) as u8)
    }

    fn context_status(&self, selected_thread_id: Option<&str>) -> ContextStatus {
        let plain_text = self
            .context_space_left_percent(selected_thread_id)
            .map(|percent| format!("{percent}%"))
            .unwrap_or_else(|| UNKNOWN_LABEL.to_string());
        let value_segments = vec![StatusLineCellValueSegment::value(plain_text.clone())];

        ContextStatus {
            plain_text,
            value_segments,
        }
    }

    fn model_for_status(&self, selected_thread_id: Option<&str>) -> Option<&str> {
        selected_thread_id.and_then(|thread_id| {
            self.pending_turn_defaults_by_thread
                .get(thread_id)
                .and_then(ThreadTurnDefaults::model)
                .or_else(|| {
                    self.effective_turn_defaults_by_thread
                        .get(thread_id)
                        .and_then(ThreadTurnDefaults::model)
                })
                .or(self.session_metadata.model.as_deref())
        })
    }

    fn reasoning_effort_for_status(&self, selected_thread_id: Option<&str>) -> Option<&str> {
        selected_thread_id.and_then(|thread_id| {
            self.pending_turn_defaults_by_thread
                .get(thread_id)
                .and_then(ThreadTurnDefaults::reasoning_effort)
                .or_else(|| {
                    self.effective_turn_defaults_by_thread
                        .get(thread_id)
                        .and_then(ThreadTurnDefaults::reasoning_effort)
                })
                .or(self.session_metadata.reasoning_effort.as_deref())
        })
    }
}

impl StatusLineProjection {
    pub(crate) fn unknown() -> Self {
        Self {
            model: UNKNOWN_LABEL.to_string(),
            reasoning_effort: UNKNOWN_LABEL.to_string(),
            context_space_left: UNKNOWN_LABEL.to_string(),
            context_value_segments: vec![StatusLineCellValueSegment::value(UNKNOWN_LABEL)],
            last_turn_state: UNKNOWN_LABEL.to_string(),
            turn_view: StatusLineTurnView::unknown(),
            model_reasoning_available: false,
            context_operation_available: false,
            cancellable_active_turn: None,
        }
    }

    pub(crate) fn with_turn_view(mut self, turn_view: StatusLineTurnView) -> Self {
        self.turn_view = turn_view;
        self
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn turn_operation_available(&self) -> bool {
        self.cancellable_active_turn.is_some()
    }
}

impl StatusLineTurnView {
    pub(crate) fn new(current: Option<usize>, total: Option<usize>) -> Self {
        Self {
            current: positive_usize(current),
            total: positive_usize(total),
        }
    }

    pub(crate) fn unknown() -> Self {
        Self::new(None, None)
    }

    pub(crate) fn current(self) -> Option<usize> {
        self.current
    }

    pub(crate) fn total(self) -> Option<usize> {
        self.total
    }

    fn display(&self) -> String {
        format!(
            "{}/{}",
            turn_view_part(self.current),
            turn_view_part(self.total)
        )
    }
}

fn label_or_unknown(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .unwrap_or(UNKNOWN_LABEL)
        .to_string()
}

fn positive_usize(value: Option<usize>) -> Option<usize> {
    value.filter(|value| *value > 0)
}

fn turn_view_part(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn thread_status_allows_user_operation(status: &ThreadStatus) -> bool {
    matches!(status, ThreadStatus::Idle) || status.waiting_on_user_input()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.is_empty()).then_some(value))
}
