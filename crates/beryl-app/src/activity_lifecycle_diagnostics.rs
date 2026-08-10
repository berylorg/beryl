use std::{collections::VecDeque, fmt, sync::Arc, time::Instant};

use serde::Serialize;

pub(crate) const ACTIVITY_LIFECYCLE_DIAGNOSTIC_CAPACITY: usize = 256;
pub(crate) const ACTIVITY_LIFECYCLE_IDENTITY_BYTE_CAPACITY: usize = 128 * 1024;
pub(crate) const ACTIVITY_LIFECYCLE_IDENTITY_FIELD_BYTE_LIMIT: usize = 512;
pub(crate) const ACTIVITY_LIFECYCLE_PROTOCOL_STRING_BYTE_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActivityLifecycleIdentityValidity {
    Valid,
    Missing,
    Blank,
    OverBound,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityLifecycleIdentity {
    pub(crate) validity: ActivityLifecycleIdentityValidity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
    pub(crate) original_byte_count: usize,
}

impl ActivityLifecycleIdentity {
    fn capture(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self {
                validity: ActivityLifecycleIdentityValidity::Missing,
                value: None,
                original_byte_count: 0,
            };
        };
        if value.trim().is_empty() {
            return Self {
                validity: ActivityLifecycleIdentityValidity::Blank,
                value: None,
                original_byte_count: value.len(),
            };
        }
        if value.len() > ACTIVITY_LIFECYCLE_IDENTITY_FIELD_BYTE_LIMIT {
            return Self {
                validity: ActivityLifecycleIdentityValidity::OverBound,
                value: None,
                original_byte_count: value.len(),
            };
        }
        Self {
            validity: ActivityLifecycleIdentityValidity::Valid,
            value: Some(value.to_string()),
            original_byte_count: value.len(),
        }
    }

    fn retained_bytes(&self) -> usize {
        self.value.as_ref().map_or(0, String::len)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityLifecycleProtocolString {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
    pub(crate) original_byte_count: usize,
    pub(crate) truncated: bool,
}

impl ActivityLifecycleProtocolString {
    fn capture(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self {
                value: None,
                original_byte_count: 0,
                truncated: false,
            };
        };
        let original_byte_count = value.len();
        let end = utf8_prefix_end(value, ACTIVITY_LIFECYCLE_PROTOCOL_STRING_BYTE_LIMIT);
        Self {
            value: Some(value[..end].to_string()),
            original_byte_count,
            truncated: end < original_byte_count,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityLifecycleDiagnosticEvent {
    pub(crate) sequence: u64,
    pub(crate) elapsed_micros: u64,
    pub(crate) stage: &'static str,
    pub(crate) category: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) thread: ActivityLifecycleIdentity,
    pub(crate) turn: ActivityLifecycleIdentity,
    pub(crate) item: ActivityLifecycleIdentity,
    pub(crate) item_type: ActivityLifecycleProtocolString,
    pub(crate) item_status: ActivityLifecycleProtocolString,
    pub(crate) projection_outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) before_row_status: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) after_row_status: Option<&'static str>,
    pub(crate) affected_row_count: usize,
}

impl ActivityLifecycleDiagnosticEvent {
    fn retained_identity_bytes(&self) -> usize {
        self.thread
            .retained_bytes()
            .saturating_add(self.turn.retained_bytes())
            .saturating_add(self.item.retained_bytes())
    }
}

#[derive(Clone)]
pub(crate) struct ActivityLifecycleDiagnosticObserver {
    callback: Arc<dyn Fn(&ActivityLifecycleDiagnosticEvent) + Send + Sync>,
}

impl ActivityLifecycleDiagnosticObserver {
    pub(crate) fn new(
        callback: impl Fn(&ActivityLifecycleDiagnosticEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            callback: Arc::new(callback),
        }
    }

    fn observe(&self, event: &ActivityLifecycleDiagnosticEvent) {
        (self.callback)(event);
    }
}

impl fmt::Debug for ActivityLifecycleDiagnosticObserver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActivityLifecycleDiagnosticObserver")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityLifecycleDiagnosticOmissions {
    pub(crate) evicted_event_count: u64,
    pub(crate) missing_identity_field_count: u64,
    pub(crate) blank_identity_field_count: u64,
    pub(crate) over_bound_identity_field_count: u64,
    pub(crate) truncated_protocol_string_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityLifecycleDiagnosticSnapshot {
    pub(crate) capacity: usize,
    pub(crate) identity_byte_capacity: usize,
    pub(crate) retained_count: usize,
    pub(crate) returned_count: usize,
    pub(crate) retained_identity_bytes: usize,
    pub(crate) oldest_sequence: Option<u64>,
    pub(crate) newest_sequence: Option<u64>,
    pub(crate) omissions: ActivityLifecycleDiagnosticOmissions,
    pub(crate) truncated: bool,
    pub(crate) events: Vec<ActivityLifecycleDiagnosticEvent>,
}

impl Default for ActivityLifecycleDiagnosticSnapshot {
    fn default() -> Self {
        Self {
            capacity: ACTIVITY_LIFECYCLE_DIAGNOSTIC_CAPACITY,
            identity_byte_capacity: ACTIVITY_LIFECYCLE_IDENTITY_BYTE_CAPACITY,
            retained_count: 0,
            returned_count: 0,
            retained_identity_bytes: 0,
            oldest_sequence: None,
            newest_sequence: None,
            omissions: ActivityLifecycleDiagnosticOmissions::default(),
            truncated: false,
            events: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActivityLifecycleDiagnosticInput<'a> {
    pub(crate) stage: &'static str,
    pub(crate) category: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) thread_id: Option<&'a str>,
    pub(crate) turn_id: Option<&'a str>,
    pub(crate) item_id: Option<&'a str>,
    pub(crate) item_type: Option<&'a str>,
    pub(crate) item_status: Option<&'a str>,
    pub(crate) projection_outcome: &'static str,
    pub(crate) before_row_status: Option<&'static str>,
    pub(crate) after_row_status: Option<&'static str>,
    pub(crate) affected_row_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ActivityLifecycleDiagnostics {
    events: VecDeque<ActivityLifecycleDiagnosticEvent>,
    started_at: Instant,
    next_sequence: u64,
    retained_identity_bytes: usize,
    omissions: ActivityLifecycleDiagnosticOmissions,
    observer: Option<ActivityLifecycleDiagnosticObserver>,
}

impl Default for ActivityLifecycleDiagnostics {
    fn default() -> Self {
        Self::with_observer(None)
    }
}

impl ActivityLifecycleDiagnostics {
    pub(crate) fn with_observer(observer: Option<ActivityLifecycleDiagnosticObserver>) -> Self {
        Self {
            events: VecDeque::new(),
            started_at: Instant::now(),
            next_sequence: 1,
            retained_identity_bytes: 0,
            omissions: ActivityLifecycleDiagnosticOmissions::default(),
            observer,
        }
    }

    pub(crate) fn record(&mut self, input: ActivityLifecycleDiagnosticInput<'_>) {
        let thread = ActivityLifecycleIdentity::capture(input.thread_id);
        let turn = ActivityLifecycleIdentity::capture(input.turn_id);
        let item = ActivityLifecycleIdentity::capture(input.item_id);
        for identity in [&thread, &turn, &item] {
            match identity.validity {
                ActivityLifecycleIdentityValidity::Valid => {}
                ActivityLifecycleIdentityValidity::Missing => {
                    self.omissions.missing_identity_field_count = self
                        .omissions
                        .missing_identity_field_count
                        .saturating_add(1);
                }
                ActivityLifecycleIdentityValidity::Blank => {
                    self.omissions.blank_identity_field_count =
                        self.omissions.blank_identity_field_count.saturating_add(1);
                }
                ActivityLifecycleIdentityValidity::OverBound => {
                    self.omissions.over_bound_identity_field_count = self
                        .omissions
                        .over_bound_identity_field_count
                        .saturating_add(1);
                }
            }
        }
        let item_type = ActivityLifecycleProtocolString::capture(input.item_type);
        let item_status = ActivityLifecycleProtocolString::capture(input.item_status);
        self.omissions.truncated_protocol_string_count = self
            .omissions
            .truncated_protocol_string_count
            .saturating_add(u64::from(item_type.truncated))
            .saturating_add(u64::from(item_status.truncated));

        let event = ActivityLifecycleDiagnosticEvent {
            sequence: self.next_sequence,
            elapsed_micros: self
                .started_at
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64,
            stage: input.stage,
            category: input.category,
            kind: input.kind,
            thread,
            turn,
            item,
            item_type,
            item_status,
            projection_outcome: input.projection_outcome,
            before_row_status: input.before_row_status,
            after_row_status: input.after_row_status,
            affected_row_count: input.affected_row_count,
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.retained_identity_bytes = self
            .retained_identity_bytes
            .saturating_add(event.retained_identity_bytes());
        self.events.push_back(event);
        self.enforce_bounds();
        if let (Some(observer), Some(event)) = (&self.observer, self.events.back()) {
            observer.observe(event);
        }
    }

    pub(crate) fn snapshot(&self) -> ActivityLifecycleDiagnosticSnapshot {
        ActivityLifecycleDiagnosticSnapshot {
            capacity: ACTIVITY_LIFECYCLE_DIAGNOSTIC_CAPACITY,
            identity_byte_capacity: ACTIVITY_LIFECYCLE_IDENTITY_BYTE_CAPACITY,
            retained_count: self.events.len(),
            returned_count: self.events.len(),
            retained_identity_bytes: self.retained_identity_bytes,
            oldest_sequence: self.events.front().map(|event| event.sequence),
            newest_sequence: self.events.back().map(|event| event.sequence),
            omissions: self.omissions.clone(),
            truncated: self.omissions.evicted_event_count > 0,
            events: self.events.iter().cloned().collect(),
        }
    }

    pub(crate) fn clear(&mut self) {
        let observer = self.observer.clone();
        *self = Self::with_observer(observer);
    }

    fn enforce_bounds(&mut self) {
        while self.events.len() > ACTIVITY_LIFECYCLE_DIAGNOSTIC_CAPACITY
            || self.retained_identity_bytes > ACTIVITY_LIFECYCLE_IDENTITY_BYTE_CAPACITY
        {
            let Some(evicted) = self.events.pop_front() else {
                break;
            };
            self.retained_identity_bytes = self
                .retained_identity_bytes
                .saturating_sub(evicted.retained_identity_bytes());
            self.omissions.evicted_event_count =
                self.omissions.evicted_event_count.saturating_add(1);
        }
    }
}

fn utf8_prefix_end(value: &str, max_bytes: usize) -> usize {
    if value.len() <= max_bytes {
        return value.len();
    }
    let mut end = 0;
    for (index, character) in value.char_indices() {
        let next = index.saturating_add(character.len_utf8());
        if next > max_bytes {
            break;
        }
        end = next;
    }
    end
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, TrySendError},
    };

    use super::*;

    fn input<'a>(item_type: Option<&'a str>) -> ActivityLifecycleDiagnosticInput<'a> {
        ActivityLifecycleDiagnosticInput {
            stage: "activity_ingress",
            category: "lifecycle",
            kind: "started",
            thread_id: Some("thread"),
            turn_id: Some("  "),
            item_id: None,
            item_type,
            item_status: Some("running"),
            projection_outcome: "inserted_running",
            before_row_status: None,
            after_row_status: Some("running"),
            affected_row_count: 1,
        }
    }

    fn without_elapsed(
        mut snapshot: ActivityLifecycleDiagnosticSnapshot,
    ) -> ActivityLifecycleDiagnosticSnapshot {
        for event in &mut snapshot.events {
            event.elapsed_micros = 0;
        }
        snapshot
    }

    #[test]
    fn observer_receives_the_exact_normalized_event_and_survives_clear() {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observer = ActivityLifecycleDiagnosticObserver::new({
            let observed = Arc::clone(&observed);
            move |event| observed.lock().unwrap().push(event.clone())
        });
        let mut diagnostics = ActivityLifecycleDiagnostics::with_observer(Some(observer));
        let long_item_type = "é".repeat(300);

        diagnostics.record(input(Some(&long_item_type)));
        let first_snapshot = diagnostics.snapshot();
        let first_observed = observed.lock().unwrap()[0].clone();

        assert_eq!(first_observed, first_snapshot.events[0]);
        assert_eq!(
            first_observed.thread,
            ActivityLifecycleIdentity::capture(Some("thread"))
        );
        assert_eq!(
            first_observed.turn,
            ActivityLifecycleIdentity::capture(Some("  "))
        );
        assert_eq!(
            first_observed.item,
            ActivityLifecycleIdentity::capture(None)
        );
        assert_eq!(
            first_observed.item_type,
            ActivityLifecycleProtocolString::capture(Some(&long_item_type))
        );
        assert_eq!(first_observed.affected_row_count, 1);

        diagnostics.clear();
        diagnostics.record(input(None));

        assert_eq!(observed.lock().unwrap().len(), 2);
        assert_eq!(diagnostics.snapshot().newest_sequence, Some(1));
    }

    #[test]
    fn queue_pressure_outcomes_do_not_change_ring_semantics() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let full_count = Arc::new(AtomicUsize::new(0));
        let observer = ActivityLifecycleDiagnosticObserver::new({
            let full_count = Arc::clone(&full_count);
            move |event| {
                if matches!(sender.try_send(event.sequence), Err(TrySendError::Full(_))) {
                    full_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        let mut baseline = ActivityLifecycleDiagnostics::default();
        let mut pressured = ActivityLifecycleDiagnostics::with_observer(Some(observer));

        for _ in 0..300 {
            baseline.record(input(None));
            pressured.record(input(None));
        }

        assert_eq!(full_count.load(Ordering::Relaxed), 299);
        assert_eq!(
            without_elapsed(pressured.snapshot()),
            without_elapsed(baseline.snapshot())
        );
    }

    #[test]
    fn disconnected_observer_handoff_does_not_change_ring_semantics() {
        let (sender, receiver) = mpsc::sync_channel::<u64>(1);
        drop(receiver);
        let disconnected_count = Arc::new(AtomicUsize::new(0));
        let observer = ActivityLifecycleDiagnosticObserver::new({
            let disconnected_count = Arc::clone(&disconnected_count);
            move |event| {
                if matches!(
                    sender.try_send(event.sequence),
                    Err(TrySendError::Disconnected(_))
                ) {
                    disconnected_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        let mut baseline = ActivityLifecycleDiagnostics::default();
        let mut disconnected = ActivityLifecycleDiagnostics::with_observer(Some(observer));

        for _ in 0..4 {
            baseline.record(input(None));
            disconnected.record(input(None));
        }

        assert_eq!(disconnected_count.load(Ordering::Relaxed), 4);
        assert_eq!(
            without_elapsed(disconnected.snapshot()),
            without_elapsed(baseline.snapshot())
        );
    }
}
