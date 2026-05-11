use beryl_backend::{
    AccountRateLimitsResponse, HardStopTarget, RateLimitSnapshot, RateLimitWindow,
    ThreadSessionMetadata, ThreadStatus, ThreadTokenUsage, TurnStartOptions,
};
use beryl_model::conversation::{
    ConversationThreadTokenUsageSnapshot, ConversationTokenUsageBreakdown,
    WorkspaceConversationState,
};
use std::collections::{BTreeMap, HashMap};

const UNKNOWN_LABEL: &str = "Unknown";
const DAILY_RATE_LIMIT_WINDOW_MINS: i64 = 24 * 60;
const WEEKLY_RATE_LIMIT_WINDOW_MINS: i64 = 7 * 24 * 60;
const GENERAL_CODEX_LIMIT_ID: &str = "codex";
const SPARK_LIMIT_TOKEN: &str = "spark";

#[derive(Clone, Debug, Default)]
pub(crate) struct StatusLineState {
    session_metadata: ThreadSessionMetadata,
    account_rate_limits: AccountRateLimitStatus,
    pending_new_thread_defaults: ThreadTurnDefaults,
    effective_new_thread_defaults: ThreadTurnDefaults,
    pending_turn_defaults_by_thread: HashMap<String, ThreadTurnDefaults>,
    effective_turn_defaults_by_thread: HashMap<String, ThreadTurnDefaults>,
    turn_state_overrides_by_thread: HashMap<String, StatusLineTurnStateOverride>,
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
    pub(crate) kind: CancellableActiveTurnKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CancellableActiveTurnKind {
    Ordinary,
    ContextCompaction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectedTurnHardStopTargets {
    pub(crate) selected_turn: CancellableActiveTurn,
    pub(crate) targets: Vec<HardStopTarget>,
    pub(crate) limitations: Vec<HardStopLimitation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HardStopLimitation {
    CommandExecutionTerminateUnsupported {
        process_id: String,
    },
    CommandExecutionProcessHandleUnavailable {
        thread_id: String,
        turn_id: String,
        item_id: String,
    },
    BackgroundTerminalCleanupUnsupported {
        thread_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusLineProjection {
    pub(crate) model: String,
    pub(crate) reasoning_effort: String,
    pub(crate) context_space_left: String,
    pub(crate) context_value_segments: Vec<StatusLineCellValueSegment>,
    pub(crate) last_turn_state: String,
    pub(crate) model_reasoning_available: bool,
    pub(crate) context_operation_available: bool,
    pub(crate) cancellable_active_turn: Option<CancellableActiveTurn>,
    pub(crate) hard_stop_targets: Option<SelectedTurnHardStopTargets>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLineCellValueSegmentKind {
    Label,
    Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct AccountRateLimitStatus {
    legacy: Option<RateLimitSnapshot>,
    by_limit_id: BTreeMap<String, RateLimitSnapshot>,
}

struct ContextStatus {
    plain_text: String,
    value_segments: Vec<StatusLineCellValueSegment>,
}

struct AccountRateLimitDisplayWindow {
    label: String,
    remaining_percent: u8,
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
            value_segments: vec![StatusLineCellValueSegment::value(last_turn_state_value)],
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum StatusLineTurnStateOverride {
    Compacting { turn_id: Option<String> },
}

impl CancellableActiveTurn {
    pub(crate) fn ordinary(thread_id: impl Into<String>, turn_id: impl Into<String>) -> Self {
        Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            kind: CancellableActiveTurnKind::Ordinary,
        }
    }

    pub(crate) fn context_compaction(
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            kind: CancellableActiveTurnKind::ContextCompaction,
        }
    }
}

impl SelectedTurnHardStopTargets {
    pub(crate) fn new(
        selected_turn: CancellableActiveTurn,
        targets: Vec<HardStopTarget>,
        limitations: Vec<HardStopLimitation>,
    ) -> Self {
        Self {
            selected_turn,
            targets,
            limitations,
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

    pub(crate) fn apply_account_rate_limits(&mut self, rate_limits: RateLimitSnapshot) -> bool {
        let mut next = self.account_rate_limits.clone();
        next.merge_snapshot(&rate_limits);
        if self.account_rate_limits == next {
            return false;
        }

        self.account_rate_limits = next;
        true
    }

    pub(crate) fn replace_account_rate_limits(
        &mut self,
        rate_limits: AccountRateLimitsResponse,
    ) -> bool {
        let next = AccountRateLimitStatus::from_response(rate_limits);
        if self.account_rate_limits == next {
            return false;
        }

        self.account_rate_limits = next;
        true
    }

    pub(crate) fn set_effective_new_thread_defaults(
        &mut self,
        defaults: Option<ThreadTurnDefaults>,
    ) -> bool {
        let defaults = defaults.unwrap_or_default();
        if self.effective_new_thread_defaults == defaults {
            return false;
        }
        self.effective_new_thread_defaults = defaults;
        true
    }

    pub(crate) fn clear_pending_new_thread_defaults(&mut self) -> bool {
        if self.pending_new_thread_defaults.is_empty() {
            return false;
        }
        self.pending_new_thread_defaults = ThreadTurnDefaults::default();
        true
    }

    pub(crate) fn set_pending_new_thread_defaults(&mut self, defaults: ThreadTurnDefaults) -> bool {
        let defaults = if defaults.is_empty() {
            ThreadTurnDefaults::default()
        } else {
            defaults
        };
        if self.pending_new_thread_defaults == defaults {
            return false;
        }
        self.pending_new_thread_defaults = defaults;
        true
    }

    pub(crate) fn bind_pending_new_thread_defaults_to_thread(&mut self, thread_id: &str) -> bool {
        if self.pending_new_thread_defaults.is_empty() {
            return false;
        }

        let defaults = std::mem::take(&mut self.pending_new_thread_defaults);
        self.set_pending_turn_defaults(thread_id, defaults)
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
        match selected_thread_id {
            Some(thread_id) => self
                .pending_turn_defaults_by_thread
                .get(thread_id)
                .map(ThreadTurnDefaults::to_turn_start_options)
                .unwrap_or_default(),
            None => self.pending_new_thread_defaults.to_turn_start_options(),
        }
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

    pub(crate) fn begin_context_compaction(&mut self, thread_id: impl Into<String>) -> bool {
        let thread_id = thread_id.into();
        let changed = !matches!(
            self.turn_state_overrides_by_thread.get(&thread_id),
            Some(StatusLineTurnStateOverride::Compacting { turn_id: None })
        );
        self.turn_state_overrides_by_thread.insert(
            thread_id,
            StatusLineTurnStateOverride::Compacting { turn_id: None },
        );
        changed
    }

    pub(crate) fn finish_context_compaction(&mut self, thread_id: &str) -> bool {
        self.turn_state_overrides_by_thread
            .remove(thread_id)
            .is_some()
    }

    pub(crate) fn set_context_compaction_turn_id(
        &mut self,
        thread_id: &str,
        turn_id: impl Into<String>,
    ) -> bool {
        let turn_id = turn_id.into();
        if turn_id.is_empty() {
            return false;
        }

        let Some(StatusLineTurnStateOverride::Compacting {
            turn_id: active_turn_id,
        }) = self.turn_state_overrides_by_thread.get_mut(thread_id)
        else {
            return false;
        };

        if active_turn_id.as_deref() == Some(turn_id.as_str()) {
            return false;
        }

        *active_turn_id = Some(turn_id);
        true
    }

    pub(crate) fn context_compaction_cancellation_target(
        &self,
        selected_thread_id: Option<&str>,
    ) -> Option<CancellableActiveTurn> {
        let selected_thread_id = selected_thread_id?;
        let Some(StatusLineTurnStateOverride::Compacting { turn_id }) =
            self.turn_state_overrides_by_thread.get(selected_thread_id)
        else {
            return None;
        };
        Some(CancellableActiveTurn::context_compaction(
            selected_thread_id,
            turn_id.clone()?,
        ))
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

    pub(crate) fn apply_token_usage_snapshot(
        &mut self,
        known_thread: bool,
        thread_id: String,
        snapshot: &ConversationThreadTokenUsageSnapshot,
    ) -> bool {
        if !known_thread || self.token_usage_by_thread.contains_key(&thread_id) {
            return false;
        }

        self.token_usage_by_thread
            .insert(thread_id, thread_token_usage_from_snapshot(snapshot));
        true
    }

    pub(crate) fn hydrate_token_usage_snapshots(
        &mut self,
        workspace_state: &WorkspaceConversationState,
        mut is_known_thread: impl FnMut(&str) -> bool,
    ) -> bool {
        let mut changed = false;
        for thread in workspace_state.threads() {
            let thread_id = thread.thread_id().as_str();
            let Some(snapshot) = thread.token_usage_snapshot() else {
                continue;
            };
            changed |= self.apply_token_usage_snapshot(
                is_known_thread(thread_id),
                thread_id.to_string(),
                snapshot,
            );
        }
        changed
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
            last_turn_state: self
                .turn_state_override_label(selected_thread_id)
                .unwrap_or(last_turn_state)
                .to_string(),
            model_reasoning_available,
            context_operation_available,
            cancellable_active_turn: None,
            hard_stop_targets: None,
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
            None,
        )
    }

    pub(crate) fn projection_with_turn_operations(
        &self,
        selected_thread_id: Option<&str>,
        model_reasoning_available: bool,
        context_operation_available: bool,
        last_turn_state: &'static str,
        cancellable_active_turn: Option<CancellableActiveTurn>,
        hard_stop_targets: Option<SelectedTurnHardStopTargets>,
    ) -> StatusLineProjection {
        let mut projection = self.projection_with_operation_availability(
            selected_thread_id,
            model_reasoning_available,
            context_operation_available,
            last_turn_state,
        );
        projection.cancellable_active_turn = cancellable_active_turn;
        projection.hard_stop_targets = hard_stop_targets;
        projection
    }

    fn turn_state_override_label(&self, selected_thread_id: Option<&str>) -> Option<&'static str> {
        let selected_thread_id = selected_thread_id?;
        self.turn_state_overrides_by_thread
            .get(selected_thread_id)
            .map(StatusLineTurnStateOverride::label)
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
        let mut plain_text = self
            .context_space_left_percent(selected_thread_id)
            .map(|percent| format!("{percent}%"))
            .unwrap_or_else(|| UNKNOWN_LABEL.to_string());
        let mut value_segments = vec![StatusLineCellValueSegment::value(plain_text.clone())];

        for window in self
            .account_rate_limits
            .display_windows(self.model_for_status(selected_thread_id))
        {
            plain_text.push_str(&format!(" {} {}%", window.label, window.remaining_percent));
            value_segments.push(StatusLineCellValueSegment::label(window.label));
            value_segments.push(StatusLineCellValueSegment::value(format!(
                "{}%",
                window.remaining_percent
            )));
        }

        ContextStatus {
            plain_text,
            value_segments,
        }
    }

    fn model_for_status(&self, selected_thread_id: Option<&str>) -> Option<&str> {
        match selected_thread_id {
            Some(thread_id) => self
                .pending_turn_defaults_by_thread
                .get(thread_id)
                .and_then(ThreadTurnDefaults::model)
                .or_else(|| {
                    self.effective_turn_defaults_by_thread
                        .get(thread_id)
                        .and_then(ThreadTurnDefaults::model)
                })
                .or(self.session_metadata.model.as_deref()),
            None => self
                .pending_new_thread_defaults
                .model()
                .or_else(|| self.effective_new_thread_defaults.model()),
        }
    }

    fn reasoning_effort_for_status(&self, selected_thread_id: Option<&str>) -> Option<&str> {
        match selected_thread_id {
            Some(thread_id) => self
                .pending_turn_defaults_by_thread
                .get(thread_id)
                .and_then(ThreadTurnDefaults::reasoning_effort)
                .or_else(|| {
                    self.effective_turn_defaults_by_thread
                        .get(thread_id)
                        .and_then(ThreadTurnDefaults::reasoning_effort)
                })
                .or(self.session_metadata.reasoning_effort.as_deref()),
            None => self
                .pending_new_thread_defaults
                .reasoning_effort()
                .or_else(|| self.effective_new_thread_defaults.reasoning_effort()),
        }
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
            model_reasoning_available: false,
            context_operation_available: false,
            cancellable_active_turn: None,
            hard_stop_targets: None,
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn turn_operation_available(&self) -> bool {
        self.cancellable_active_turn.is_some()
    }
}

impl StatusLineTurnStateOverride {
    fn label(&self) -> &'static str {
        match self {
            Self::Compacting { .. } => "compacting",
        }
    }
}

impl AccountRateLimitStatus {
    fn from_response(response: AccountRateLimitsResponse) -> Self {
        let mut status = Self::default();
        status.legacy = Some(response.rate_limits);
        if let Some(rate_limits_by_limit_id) = response.rate_limits_by_limit_id {
            for (limit_id, snapshot) in rate_limits_by_limit_id {
                status.replace_snapshot_for_key(limit_id, snapshot);
            }
        }
        status
    }

    fn merge_snapshot(&mut self, snapshot: &RateLimitSnapshot) {
        if let Some(limit_id) = snapshot_limit_id(snapshot) {
            self.merge_snapshot_for_key(&limit_id, snapshot);
            return;
        }

        self.merge_legacy_snapshot(snapshot);
        if let Some(limit_id) = self.general_limit_key() {
            self.merge_snapshot_for_key(&limit_id, snapshot);
        }
    }

    fn replace_snapshot_for_key(&mut self, limit_id: String, mut snapshot: RateLimitSnapshot) {
        if snapshot.limit_id.as_deref().is_none_or(str::is_empty) {
            snapshot.limit_id = Some(limit_id.clone());
        }
        self.by_limit_id.insert(limit_id, snapshot);
    }

    fn merge_snapshot_for_key(&mut self, limit_id: &str, snapshot: &RateLimitSnapshot) {
        let entry = self
            .by_limit_id
            .entry(limit_id.to_string())
            .or_insert_with(|| RateLimitSnapshot {
                limit_id: Some(limit_id.to_string()),
                limit_name: None,
                primary: None,
                secondary: None,
            });
        merge_rate_limit_snapshot(entry, snapshot);
    }

    fn merge_legacy_snapshot(&mut self, snapshot: &RateLimitSnapshot) {
        match self.legacy.as_mut() {
            Some(current) => merge_rate_limit_snapshot(current, snapshot),
            None => self.legacy = Some(snapshot.clone()),
        }
    }

    fn display_windows(&self, model: Option<&str>) -> Vec<AccountRateLimitDisplayWindow> {
        self.snapshot_for_model(model)
            .map(display_windows_from_snapshot)
            .unwrap_or_default()
    }

    fn snapshot_for_model(&self, model: Option<&str>) -> Option<&RateLimitSnapshot> {
        let model_key = model.and_then(normalized_identifier);
        if let Some(model_key) = model_key.as_deref() {
            if let Some(snapshot) = self.find_model_specific_snapshot(model_key) {
                return Some(snapshot);
            }
            if !model_key.contains(SPARK_LIMIT_TOKEN)
                && let Some(snapshot) = self.general_snapshot()
            {
                return Some(snapshot);
            }
        } else if let Some(snapshot) = self.general_snapshot() {
            return Some(snapshot);
        }

        self.legacy
            .as_ref()
            .or_else(|| self.single_bucket_snapshot())
    }

    fn find_model_specific_snapshot(&self, model_key: &str) -> Option<&RateLimitSnapshot> {
        let model_is_spark = model_key.contains(SPARK_LIMIT_TOKEN);
        self.by_limit_id
            .iter()
            .find(|(limit_id, snapshot)| {
                snapshot_identifier_keys(limit_id, snapshot).any(|candidate| {
                    candidate == model_key
                        || (model_is_spark && candidate.contains(SPARK_LIMIT_TOKEN))
                })
            })
            .map(|(_, snapshot)| snapshot)
    }

    fn general_snapshot(&self) -> Option<&RateLimitSnapshot> {
        self.by_limit_id
            .get(GENERAL_CODEX_LIMIT_ID)
            .or_else(|| {
                self.by_limit_id
                    .iter()
                    .find(|(limit_id, snapshot)| {
                        snapshot_identifier_keys(limit_id, snapshot)
                            .any(|candidate| candidate == GENERAL_CODEX_LIMIT_ID)
                    })
                    .map(|(_, snapshot)| snapshot)
            })
            .or(self.legacy.as_ref())
    }

    fn general_limit_key(&self) -> Option<String> {
        if self.by_limit_id.contains_key(GENERAL_CODEX_LIMIT_ID) {
            return Some(GENERAL_CODEX_LIMIT_ID.to_string());
        }

        self.by_limit_id.iter().find_map(|(limit_id, snapshot)| {
            snapshot_identifier_keys(limit_id, snapshot)
                .any(|candidate| candidate == GENERAL_CODEX_LIMIT_ID)
                .then(|| limit_id.clone())
        })
    }

    fn single_bucket_snapshot(&self) -> Option<&RateLimitSnapshot> {
        (self.by_limit_id.len() == 1)
            .then(|| self.by_limit_id.values().next())
            .flatten()
    }
}

fn merge_rate_limit_snapshot(target: &mut RateLimitSnapshot, update: &RateLimitSnapshot) {
    if update
        .limit_id
        .as_deref()
        .is_some_and(|value| !value.is_empty())
    {
        target.limit_id = update.limit_id.clone();
    }
    if update
        .limit_name
        .as_deref()
        .is_some_and(|value| !value.is_empty())
    {
        target.limit_name = update.limit_name.clone();
    }
    if update.primary.is_some() {
        target.primary = update.primary.clone();
    }
    if update.secondary.is_some() {
        target.secondary = update.secondary.clone();
    }
}

fn display_windows_from_snapshot(
    snapshot: &RateLimitSnapshot,
) -> Vec<AccountRateLimitDisplayWindow> {
    let mut windows = Vec::new();
    let mut seen_durations = Vec::new();
    for window in [snapshot.primary.as_ref(), snapshot.secondary.as_ref()]
        .into_iter()
        .flatten()
    {
        let Some(duration) = window.window_duration_mins else {
            continue;
        };
        if seen_durations.contains(&duration) {
            continue;
        }
        let Some(label) = rate_limit_window_label(duration) else {
            continue;
        };
        seen_durations.push(duration);
        windows.push(AccountRateLimitDisplayWindow {
            label,
            remaining_percent: remaining_percent_from_rate_limit_window(window),
        });
    }
    windows
}

fn rate_limit_window_label(window_duration_mins: i64) -> Option<String> {
    match window_duration_mins {
        WEEKLY_RATE_LIMIT_WINDOW_MINS => Some("Weekly".to_string()),
        DAILY_RATE_LIMIT_WINDOW_MINS => Some("Daily".to_string()),
        duration if duration > 0 && duration < 60 => Some(format!("{duration}m")),
        duration
            if duration > 0 && duration < DAILY_RATE_LIMIT_WINDOW_MINS && duration % 60 == 0 =>
        {
            Some(format!("{}h", duration / 60))
        }
        duration
            if duration > DAILY_RATE_LIMIT_WINDOW_MINS
                && duration % DAILY_RATE_LIMIT_WINDOW_MINS == 0 =>
        {
            Some(format!("{}d", duration / DAILY_RATE_LIMIT_WINDOW_MINS))
        }
        _ => None,
    }
}

fn snapshot_limit_id(snapshot: &RateLimitSnapshot) -> Option<String> {
    snapshot
        .limit_id
        .as_deref()
        .and_then(normalized_identifier)
        .or_else(|| {
            snapshot
                .limit_name
                .as_deref()
                .and_then(normalized_identifier)
        })
}

fn snapshot_identifier_keys<'a>(
    map_key: &'a str,
    snapshot: &'a RateLimitSnapshot,
) -> impl Iterator<Item = String> + 'a {
    [
        Some(map_key),
        snapshot.limit_id.as_deref(),
        snapshot.limit_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(normalized_identifier)
}

fn normalized_identifier(value: &str) -> Option<String> {
    let normalized = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    (!normalized.is_empty()).then_some(normalized)
}

fn label_or_unknown(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .unwrap_or(UNKNOWN_LABEL)
        .to_string()
}

fn thread_status_allows_user_operation(status: &ThreadStatus) -> bool {
    matches!(status, ThreadStatus::Idle) || status.waiting_on_user_input()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.is_empty()).then_some(value))
}

fn thread_token_usage_from_snapshot(
    snapshot: &ConversationThreadTokenUsageSnapshot,
) -> ThreadTokenUsage {
    ThreadTokenUsage {
        last: token_usage_breakdown_from_snapshot(snapshot.last()),
        total: token_usage_breakdown_from_snapshot(snapshot.total()),
        model_context_window: snapshot.model_context_window(),
    }
}

fn remaining_percent_from_rate_limit_window(window: &RateLimitWindow) -> u8 {
    let used = window.used_percent.clamp(0, 100);
    (100 - used) as u8
}

fn token_usage_breakdown_from_snapshot(
    value: &ConversationTokenUsageBreakdown,
) -> beryl_backend::TokenUsageBreakdown {
    beryl_backend::TokenUsageBreakdown {
        cached_input_tokens: value.cached_input_tokens(),
        input_tokens: value.input_tokens(),
        output_tokens: value.output_tokens(),
        reasoning_output_tokens: value.reasoning_output_tokens(),
        total_tokens: value.total_tokens(),
    }
}
