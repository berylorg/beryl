use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Serialize;

pub const COMPACTION_DIAGNOSTIC_CAPACITY: usize = 256;
pub const COMPACTION_DIAGNOSTIC_IDENTITY_BYTE_CAPACITY: usize = 128 * 1024;
pub const COMPACTION_DIAGNOSTIC_IDENTITY_FIELD_BYTE_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionDiagnosticIdentityValidity {
    Valid,
    Missing,
    Blank,
    OverBound,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionDiagnosticIdentity {
    pub(crate) validity: CompactionDiagnosticIdentityValidity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
    pub(crate) original_byte_count: usize,
}

impl CompactionDiagnosticIdentity {
    pub(crate) fn capture(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self {
                validity: CompactionDiagnosticIdentityValidity::Missing,
                value: None,
                original_byte_count: 0,
            };
        };
        if value.trim().is_empty() {
            return Self {
                validity: CompactionDiagnosticIdentityValidity::Blank,
                value: None,
                original_byte_count: value.len(),
            };
        }
        if value.len() > COMPACTION_DIAGNOSTIC_IDENTITY_FIELD_BYTE_LIMIT {
            return Self {
                validity: CompactionDiagnosticIdentityValidity::OverBound,
                value: None,
                original_byte_count: value.len(),
            };
        }
        Self {
            validity: CompactionDiagnosticIdentityValidity::Valid,
            value: Some(value.to_string()),
            original_byte_count: value.len(),
        }
    }

    fn retained_bytes(&self) -> usize {
        self.value.as_ref().map_or(0, String::len)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionDiagnosticStage {
    Request,
    Subscription,
    Start,
    Receipt,
    Status,
    Warning,
    Lifecycle,
    Transport,
    Reconcile,
    Stop,
    Queue,
    Cleanup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionDiagnosticCategory {
    Requested,
    Succeeded,
    Failed,
    Indeterminate,
    Rejected,
    IdentityKnown,
    ThresholdReached,
    Started,
    ItemStarted,
    ItemCompleted,
    Retrying,
    CompletedAwaitingIdle,
    Disconnected,
    ReconnectRequested,
    Reconnected,
    RuntimeChanged,
    Accepted,
    Completed,
    Interrupted,
    Unknown,
    Idle,
    Active,
    Unavailable,
    Invalid,
    Cancelled,
    Finished,
    QueueHeld,
    QueueReleased,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionDiagnosticAcceptance {
    Pending,
    Accepted,
    Rejected,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionDiagnosticOutcome {
    Pending,
    Completed,
    Failed,
    Interrupted,
    Cancelled,
    Unconfirmed,
    Rejected,
}

#[derive(Clone, Debug)]
pub struct CompactionDiagnosticStart<'a> {
    pub local_generation: u64,
    pub workspace_identity: Option<&'a str>,
    pub runtime_alias: Option<&'a str>,
    pub thread_identity: Option<&'a str>,
    pub warning_threshold: Duration,
}

#[derive(Clone, Debug)]
pub struct CompactionDiagnosticEventData<'a> {
    pub turn_identity: Option<&'a str>,
    pub elapsed: Duration,
    pub last_event_age: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompactionDiagnosticEvent {
    pub(crate) sequence: u64,
    pub(crate) elapsed_micros: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_event_age_micros: Option<u64>,
    pub(crate) local_generation: u64,
    pub(crate) stage: CompactionDiagnosticStage,
    pub(crate) category: CompactionDiagnosticCategory,
    pub(crate) workspace: CompactionDiagnosticIdentity,
    pub(crate) runtime_alias: CompactionDiagnosticIdentity,
    pub(crate) backend_observation_session: CompactionDiagnosticIdentity,
    pub(crate) operation: CompactionDiagnosticIdentity,
    pub(crate) thread: CompactionDiagnosticIdentity,
    pub(crate) turn: CompactionDiagnosticIdentity,
}

impl CompactionDiagnosticEvent {
    fn retained_identity_bytes(&self) -> usize {
        self.workspace
            .retained_bytes()
            .saturating_add(self.runtime_alias.retained_bytes())
            .saturating_add(self.backend_observation_session.retained_bytes())
            .saturating_add(self.operation.retained_bytes())
            .saturating_add(self.thread.retained_bytes())
            .saturating_add(self.turn.retained_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompactionDiagnosticSummary {
    pub(crate) local_generation: u64,
    pub(crate) workspace: CompactionDiagnosticIdentity,
    pub(crate) runtime_alias: CompactionDiagnosticIdentity,
    pub(crate) backend_observation_session: CompactionDiagnosticIdentity,
    pub(crate) operation: CompactionDiagnosticIdentity,
    pub(crate) thread: CompactionDiagnosticIdentity,
    pub(crate) turn: CompactionDiagnosticIdentity,
    pub(crate) warning_threshold_micros: u64,
    pub(crate) acceptance: CompactionDiagnosticAcceptance,
    pub(crate) outcome: CompactionDiagnosticOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) elapsed_micros: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_event_age_micros: Option<u64>,
    #[serde(skip)]
    started_at: Instant,
    #[serde(skip)]
    last_backend_activity_at: Option<Instant>,
}

impl CompactionDiagnosticSummary {
    fn is_live(&self) -> bool {
        matches!(
            self.outcome,
            CompactionDiagnosticOutcome::Pending | CompactionDiagnosticOutcome::Unconfirmed
        )
    }

    fn retained_identity_bytes(&self) -> usize {
        self.workspace
            .retained_bytes()
            .saturating_add(self.runtime_alias.retained_bytes())
            .saturating_add(self.backend_observation_session.retained_bytes())
            .saturating_add(self.operation.retained_bytes())
            .saturating_add(self.thread.retained_bytes())
            .saturating_add(self.turn.retained_bytes())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompactionDiagnosticOmissions {
    pub(crate) evicted_event_count: u64,
    pub(crate) missing_identity_field_count: u64,
    pub(crate) blank_identity_field_count: u64,
    pub(crate) over_bound_identity_field_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionDiagnosticSnapshot {
    pub(crate) capacity: usize,
    pub(crate) identity_byte_capacity: usize,
    pub(crate) retained_count: usize,
    pub(crate) returned_count: usize,
    pub(crate) retained_identity_bytes: usize,
    pub(crate) oldest_sequence: Option<u64>,
    pub(crate) newest_sequence: Option<u64>,
    pub(crate) omissions: CompactionDiagnosticOmissions,
    pub(crate) truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<CompactionDiagnosticSummary>,
    pub(crate) events: Vec<CompactionDiagnosticEvent>,
}

impl Default for CompactionDiagnosticSnapshot {
    fn default() -> Self {
        Self {
            capacity: COMPACTION_DIAGNOSTIC_CAPACITY,
            identity_byte_capacity: COMPACTION_DIAGNOSTIC_IDENTITY_BYTE_CAPACITY,
            retained_count: 0,
            returned_count: 0,
            retained_identity_bytes: 0,
            oldest_sequence: None,
            newest_sequence: None,
            omissions: CompactionDiagnosticOmissions::default(),
            truncated: false,
            summary: None,
            events: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct CompactionDiagnostics {
    inner: Arc<Mutex<CompactionDiagnosticsInner>>,
}

#[derive(Clone)]
pub struct CompactionDiagnosticHandle {
    inner: Arc<Mutex<CompactionDiagnosticsInner>>,
    local_generation: u64,
}

#[derive(Debug)]
struct CompactionDiagnosticsInner {
    events: VecDeque<CompactionDiagnosticEvent>,
    next_sequence: u64,
    retained_identity_bytes: usize,
    omissions: CompactionDiagnosticOmissions,
    summary: Option<CompactionDiagnosticSummary>,
}

impl Default for CompactionDiagnostics {
    fn default() -> Self {
        Self::with_started_at(Instant::now())
    }
}

impl CompactionDiagnostics {
    pub fn with_started_at(_started_at: Instant) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CompactionDiagnosticsInner {
                events: VecDeque::new(),
                next_sequence: 1,
                retained_identity_bytes: 0,
                omissions: CompactionDiagnosticOmissions::default(),
                summary: None,
            })),
        }
    }

    pub fn begin(&self, start: CompactionDiagnosticStart<'_>) -> CompactionDiagnosticHandle {
        let mut inner = self
            .inner
            .lock()
            .expect("compaction diagnostics lock poisoned");
        let started_at = Instant::now();
        let summary = CompactionDiagnosticSummary {
            local_generation: start.local_generation,
            workspace: capture_identity(&mut inner.omissions, start.workspace_identity),
            runtime_alias: capture_identity(&mut inner.omissions, start.runtime_alias),
            backend_observation_session: capture_identity(&mut inner.omissions, None),
            operation: capture_identity(&mut inner.omissions, None),
            thread: capture_identity(&mut inner.omissions, start.thread_identity),
            turn: capture_identity(&mut inner.omissions, None),
            warning_threshold_micros: duration_micros(start.warning_threshold),
            acceptance: CompactionDiagnosticAcceptance::Pending,
            outcome: CompactionDiagnosticOutcome::Pending,
            elapsed_micros: Some(0),
            last_event_age_micros: None,
            started_at,
            last_backend_activity_at: None,
        };
        replace_summary(&mut inner, summary);
        record_locked(
            &mut inner,
            start.local_generation,
            CompactionDiagnosticStage::Request,
            CompactionDiagnosticCategory::Requested,
            CompactionDiagnosticEventData {
                turn_identity: None,
                elapsed: Duration::ZERO,
                last_event_age: None,
            },
        );
        CompactionDiagnosticHandle {
            inner: Arc::clone(&self.inner),
            local_generation: start.local_generation,
        }
    }

    pub fn snapshot(&self) -> CompactionDiagnosticSnapshot {
        self.snapshot_limited(None, COMPACTION_DIAGNOSTIC_CAPACITY)
    }

    pub fn snapshot_limited(
        &self,
        after_sequence: Option<u64>,
        limit: usize,
    ) -> CompactionDiagnosticSnapshot {
        let inner = self
            .inner
            .lock()
            .expect("compaction diagnostics lock poisoned");
        let events = inner
            .events
            .iter()
            .filter(|event| after_sequence.is_none_or(|after| event.sequence > after))
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let returned_count = events.len();
        let truncated = inner.omissions.evicted_event_count > 0
            || inner.events.len() > returned_count
            || after_sequence.is_some_and(|after| {
                inner
                    .events
                    .front()
                    .is_some_and(|event| after < event.sequence.saturating_sub(1))
            });
        let mut summary = inner.summary.clone();
        if let Some(summary) = summary.as_mut().filter(|summary| summary.is_live()) {
            let now = Instant::now();
            summary.elapsed_micros = Some(duration_micros(
                now.saturating_duration_since(summary.started_at),
            ));
            summary.last_event_age_micros = summary
                .last_backend_activity_at
                .map(|activity_at| duration_micros(now.saturating_duration_since(activity_at)));
        }
        CompactionDiagnosticSnapshot {
            capacity: COMPACTION_DIAGNOSTIC_CAPACITY,
            identity_byte_capacity: COMPACTION_DIAGNOSTIC_IDENTITY_BYTE_CAPACITY,
            retained_count: inner.events.len(),
            returned_count,
            retained_identity_bytes: inner.retained_identity_bytes,
            oldest_sequence: inner.events.front().map(|event| event.sequence),
            newest_sequence: inner.events.back().map(|event| event.sequence),
            omissions: inner.omissions.clone(),
            truncated,
            summary,
            events,
        }
    }
}

impl CompactionDiagnosticHandle {
    pub fn bind_operation(&self, operation_id: Option<&str>, observation_session_id: Option<&str>) {
        let mut inner = self
            .inner
            .lock()
            .expect("compaction diagnostics lock poisoned");
        let operation = capture_identity(&mut inner.omissions, operation_id);
        let observation_session = capture_identity(&mut inner.omissions, observation_session_id);
        if let Some(summary) = latest_summary_mut(&mut inner, self.local_generation) {
            let before = summary.retained_identity_bytes();
            summary.operation = operation;
            summary.backend_observation_session = observation_session;
            let after = summary.retained_identity_bytes();
            inner.retained_identity_bytes = inner
                .retained_identity_bytes
                .saturating_add(after)
                .saturating_sub(before);
        }
        enforce_bounds(&mut inner);
    }

    pub fn bind_turn(&self, turn_id: Option<&str>) {
        let mut inner = self
            .inner
            .lock()
            .expect("compaction diagnostics lock poisoned");
        let turn = capture_identity(&mut inner.omissions, turn_id);
        if let Some(summary) = latest_summary_mut(&mut inner, self.local_generation) {
            let before = summary.retained_identity_bytes();
            summary.turn = turn;
            let after = summary.retained_identity_bytes();
            inner.retained_identity_bytes = inner
                .retained_identity_bytes
                .saturating_add(after)
                .saturating_sub(before);
        }
        enforce_bounds(&mut inner);
    }

    pub fn bind_thread(&self, thread_id: Option<&str>) {
        let mut inner = self
            .inner
            .lock()
            .expect("compaction diagnostics lock poisoned");
        let thread = capture_identity(&mut inner.omissions, thread_id);
        if let Some(summary) = latest_summary_mut(&mut inner, self.local_generation) {
            let before = summary.retained_identity_bytes();
            summary.thread = thread;
            let after = summary.retained_identity_bytes();
            inner.retained_identity_bytes = inner
                .retained_identity_bytes
                .saturating_add(after)
                .saturating_sub(before);
        }
        enforce_bounds(&mut inner);
    }

    pub fn set_acceptance(&self, acceptance: CompactionDiagnosticAcceptance) {
        let mut inner = self
            .inner
            .lock()
            .expect("compaction diagnostics lock poisoned");
        if let Some(summary) = latest_summary_mut(&mut inner, self.local_generation) {
            summary.acceptance = acceptance;
        }
    }

    pub fn set_outcome(&self, outcome: CompactionDiagnosticOutcome) {
        let mut inner = self
            .inner
            .lock()
            .expect("compaction diagnostics lock poisoned");
        if let Some(summary) = latest_summary_mut(&mut inner, self.local_generation) {
            summary.outcome = outcome;
        }
    }

    pub fn record(
        &self,
        stage: CompactionDiagnosticStage,
        category: CompactionDiagnosticCategory,
        data: CompactionDiagnosticEventData<'_>,
    ) {
        let mut inner = self
            .inner
            .lock()
            .expect("compaction diagnostics lock poisoned");
        record_locked(&mut inner, self.local_generation, stage, category, data);
    }

    pub fn finish(
        &self,
        outcome: CompactionDiagnosticOutcome,
        data: CompactionDiagnosticEventData<'_>,
    ) {
        let mut inner = self
            .inner
            .lock()
            .expect("compaction diagnostics lock poisoned");
        if let Some(summary) = latest_summary_mut(&mut inner, self.local_generation) {
            summary.outcome = outcome;
            summary.elapsed_micros = Some(duration_micros(data.elapsed));
            if let Some(last_event_age) = data.last_event_age {
                summary.last_event_age_micros = Some(duration_micros(last_event_age));
            }
        }
        record_locked(
            &mut inner,
            self.local_generation,
            CompactionDiagnosticStage::Cleanup,
            if outcome == CompactionDiagnosticOutcome::Cancelled {
                CompactionDiagnosticCategory::Cancelled
            } else {
                CompactionDiagnosticCategory::Finished
            },
            data,
        );
    }
}

fn record_locked(
    inner: &mut CompactionDiagnosticsInner,
    local_generation: u64,
    stage: CompactionDiagnosticStage,
    category: CompactionDiagnosticCategory,
    data: CompactionDiagnosticEventData<'_>,
) {
    let summary = inner
        .summary
        .as_ref()
        .filter(|summary| summary.local_generation == local_generation);
    let (workspace, runtime_alias, backend_observation_session, operation, thread, retained_turn) =
        match summary {
            Some(summary) => (
                summary.workspace.clone(),
                summary.runtime_alias.clone(),
                summary.backend_observation_session.clone(),
                summary.operation.clone(),
                summary.thread.clone(),
                summary.turn.clone(),
            ),
            None => (
                CompactionDiagnosticIdentity::capture(None),
                CompactionDiagnosticIdentity::capture(None),
                CompactionDiagnosticIdentity::capture(None),
                CompactionDiagnosticIdentity::capture(None),
                CompactionDiagnosticIdentity::capture(None),
                CompactionDiagnosticIdentity::capture(None),
            ),
        };
    let turn = data
        .turn_identity
        .map(|value| capture_identity(&mut inner.omissions, Some(value)))
        .unwrap_or(retained_turn);
    let event = CompactionDiagnosticEvent {
        sequence: inner.next_sequence,
        elapsed_micros: duration_micros(data.elapsed),
        last_event_age_micros: data.last_event_age.map(duration_micros),
        local_generation,
        stage,
        category,
        workspace,
        runtime_alias,
        backend_observation_session,
        operation,
        thread,
        turn,
    };
    inner.next_sequence = inner.next_sequence.saturating_add(1);
    inner.retained_identity_bytes = inner
        .retained_identity_bytes
        .saturating_add(event.retained_identity_bytes());
    inner.events.push_back(event);
    if let Some(summary) = latest_summary_mut(inner, local_generation) {
        summary.elapsed_micros = Some(duration_micros(data.elapsed));
        if let Some(last_event_age) = data.last_event_age {
            summary.last_event_age_micros = Some(duration_micros(last_event_age));
            summary.last_backend_activity_at = Instant::now().checked_sub(last_event_age);
        }
    }
    enforce_bounds(inner);
}

fn replace_summary(inner: &mut CompactionDiagnosticsInner, summary: CompactionDiagnosticSummary) {
    inner.retained_identity_bytes = inner.retained_identity_bytes.saturating_sub(
        inner
            .summary
            .as_ref()
            .map_or(0, CompactionDiagnosticSummary::retained_identity_bytes),
    );
    inner.retained_identity_bytes = inner
        .retained_identity_bytes
        .saturating_add(summary.retained_identity_bytes());
    inner.summary = Some(summary);
    enforce_bounds(inner);
}

fn latest_summary_mut(
    inner: &mut CompactionDiagnosticsInner,
    generation: u64,
) -> Option<&mut CompactionDiagnosticSummary> {
    inner
        .summary
        .as_mut()
        .filter(|summary| summary.local_generation == generation)
}

fn capture_identity(
    omissions: &mut CompactionDiagnosticOmissions,
    value: Option<&str>,
) -> CompactionDiagnosticIdentity {
    let identity = CompactionDiagnosticIdentity::capture(value);
    match identity.validity {
        CompactionDiagnosticIdentityValidity::Valid => {}
        CompactionDiagnosticIdentityValidity::Missing => {
            omissions.missing_identity_field_count =
                omissions.missing_identity_field_count.saturating_add(1)
        }
        CompactionDiagnosticIdentityValidity::Blank => {
            omissions.blank_identity_field_count =
                omissions.blank_identity_field_count.saturating_add(1)
        }
        CompactionDiagnosticIdentityValidity::OverBound => {
            omissions.over_bound_identity_field_count =
                omissions.over_bound_identity_field_count.saturating_add(1)
        }
    }
    identity
}

fn enforce_bounds(inner: &mut CompactionDiagnosticsInner) {
    while inner.events.len() > COMPACTION_DIAGNOSTIC_CAPACITY
        || inner.retained_identity_bytes > COMPACTION_DIAGNOSTIC_IDENTITY_BYTE_CAPACITY
    {
        let Some(event) = inner.events.pop_front() else {
            break;
        };
        inner.retained_identity_bytes = inner
            .retained_identity_bytes
            .saturating_sub(event.retained_identity_bytes());
        inner.omissions.evicted_event_count = inner.omissions.evicted_event_count.saturating_add(1);
    }
}

fn duration_micros(value: Duration) -> u64 {
    value.as_micros().min(u128::from(u64::MAX)) as u64
}
