use std::{cell::RefCell, collections::VecDeque, rc::Rc, time::Instant};

use serde::Serialize;

pub(crate) const ACTIVITY_PRESENTATION_DIAGNOSTIC_CAPACITY: usize = 256;
pub(crate) const ACTIVITY_PRESENTATION_DIAGNOSTIC_BYTE_CAPACITY: usize = 512 * 1024;
pub(crate) const ACTIVITY_PRESENTATION_DIAGNOSTIC_EVENT_BYTE_LIMIT: usize = 64 * 1024;
pub(crate) const ACTIVITY_PRESENTATION_DIAGNOSTIC_ROW_LIMIT: usize = 32;
pub(crate) const ACTIVITY_PRESENTATION_IDENTITY_FIELD_BYTE_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActivityPresentationIdentityValidity {
    Valid,
    Missing,
    Blank,
    OverBound,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityPresentationIdentity {
    pub(crate) validity: ActivityPresentationIdentityValidity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
    pub(crate) original_byte_count: usize,
}

impl ActivityPresentationIdentity {
    fn capture(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self {
                validity: ActivityPresentationIdentityValidity::Missing,
                value: None,
                original_byte_count: 0,
            };
        };
        if value.trim().is_empty() {
            return Self {
                validity: ActivityPresentationIdentityValidity::Blank,
                value: None,
                original_byte_count: value.len(),
            };
        }
        if value.len() > ACTIVITY_PRESENTATION_IDENTITY_FIELD_BYTE_LIMIT {
            return Self {
                validity: ActivityPresentationIdentityValidity::OverBound,
                value: None,
                original_byte_count: value.len(),
            };
        }
        Self {
            validity: ActivityPresentationIdentityValidity::Valid,
            value: Some(value.to_string()),
            original_byte_count: value.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityPresentationRowSample {
    pub(crate) rendered_index: usize,
    pub(crate) thread: ActivityPresentationIdentity,
    pub(crate) turn: ActivityPresentationIdentity,
    pub(crate) item: ActivityPresentationIdentity,
    pub(crate) status: &'static str,
    pub(crate) status_indicator_theme_role: &'static str,
    pub(crate) color_source: &'static str,
    pub(crate) resolved_rgba: [u8; 4],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityPresentationDiagnosticEvent {
    pub(crate) sequence: u64,
    pub(crate) elapsed_micros: u64,
    pub(crate) stage: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) projection_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) newest_lifecycle_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) total_row_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) running_row_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) finished_ok_row_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) finished_error_row_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) render_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) newest_notified_projection_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) panel_visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selected_thread: Option<ActivityPresentationIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selected_thread_row_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rendered_range_start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rendered_range_end: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) overscan_row_count: Option<usize>,
    pub(crate) sampled_rows: Vec<ActivityPresentationRowSample>,
    pub(crate) row_sample_truncated: bool,
    pub(crate) event_bytes: usize,
    pub(crate) event_bytes_truncated: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityPresentationDiagnosticOmissions {
    pub(crate) evicted_event_count: u64,
    pub(crate) missing_identity_field_count: u64,
    pub(crate) blank_identity_field_count: u64,
    pub(crate) over_bound_identity_field_count: u64,
    pub(crate) row_sample_omission_count: u64,
    pub(crate) event_byte_truncation_count: u64,
    pub(crate) coalesced_render_sample_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityPresentationDiagnosticSnapshot {
    pub(crate) event_capacity: usize,
    pub(crate) event_byte_capacity: usize,
    pub(crate) event_byte_limit: usize,
    pub(crate) row_sample_capacity: usize,
    pub(crate) identity_field_byte_limit: usize,
    pub(crate) retained_count: usize,
    pub(crate) returned_count: usize,
    pub(crate) retained_event_bytes: usize,
    pub(crate) oldest_sequence: Option<u64>,
    pub(crate) newest_sequence: Option<u64>,
    pub(crate) omissions: ActivityPresentationDiagnosticOmissions,
    pub(crate) truncated: bool,
    pub(crate) events: Vec<ActivityPresentationDiagnosticEvent>,
}

impl Default for ActivityPresentationDiagnosticSnapshot {
    fn default() -> Self {
        Self {
            event_capacity: ACTIVITY_PRESENTATION_DIAGNOSTIC_CAPACITY,
            event_byte_capacity: ACTIVITY_PRESENTATION_DIAGNOSTIC_BYTE_CAPACITY,
            event_byte_limit: ACTIVITY_PRESENTATION_DIAGNOSTIC_EVENT_BYTE_LIMIT,
            row_sample_capacity: ACTIVITY_PRESENTATION_DIAGNOSTIC_ROW_LIMIT,
            identity_field_byte_limit: ACTIVITY_PRESENTATION_IDENTITY_FIELD_BYTE_LIMIT,
            retained_count: 0,
            returned_count: 0,
            retained_event_bytes: 0,
            oldest_sequence: None,
            newest_sequence: None,
            omissions: ActivityPresentationDiagnosticOmissions::default(),
            truncated: false,
            events: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActivityProjectionDiagnosticState {
    pub(crate) revision: u64,
    pub(crate) newest_lifecycle_sequence: Option<u64>,
    pub(crate) total_row_count: usize,
    pub(crate) running_row_count: usize,
    pub(crate) finished_ok_row_count: usize,
    pub(crate) finished_error_row_count: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActivityPresentationRenderRow<'a> {
    pub(crate) rendered_index: usize,
    pub(crate) thread_id: &'a str,
    pub(crate) turn_id: &'a str,
    pub(crate) item_id: &'a str,
    pub(crate) status: &'static str,
    pub(crate) status_indicator_theme_role: &'static str,
    pub(crate) used_theme_role: bool,
    pub(crate) resolved_rgba: [u8; 4],
}

#[derive(Clone, Debug)]
pub(crate) struct ActivityPresentationDiagnostics {
    state: Rc<RefCell<ActivityPresentationDiagnosticsState>>,
    observer: Option<ActivityPresentationDiagnosticObserver>,
}

#[derive(Clone)]
pub(crate) struct ActivityPresentationDiagnosticObserver {
    callback: Rc<dyn Fn(&ActivityPresentationDiagnosticEvent)>,
}

impl std::fmt::Debug for ActivityPresentationDiagnosticObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ActivityPresentationDiagnosticObserver")
            .finish_non_exhaustive()
    }
}

impl ActivityPresentationDiagnosticObserver {
    pub(crate) fn new(callback: impl Fn(&ActivityPresentationDiagnosticEvent) + 'static) -> Self {
        Self {
            callback: Rc::new(callback),
        }
    }

    fn observe(&self, event: &ActivityPresentationDiagnosticEvent) {
        (self.callback)(event);
    }
}

#[derive(Clone, Debug)]
struct StoredActivityPresentationEvent {
    event: ActivityPresentationDiagnosticEvent,
    encoded_bytes: usize,
}

#[derive(Clone, Debug)]
struct ActivityPresentationDiagnosticsState {
    events: VecDeque<StoredActivityPresentationEvent>,
    started_at: Instant,
    next_sequence: u64,
    next_render_revision: u64,
    retained_event_bytes: usize,
    last_projection_revision: u64,
    newest_notified_projection_revision: Option<u64>,
    omissions: ActivityPresentationDiagnosticOmissions,
}

impl Default for ActivityPresentationDiagnostics {
    fn default() -> Self {
        Self::with_observer(None)
    }
}

impl ActivityPresentationDiagnostics {
    pub(crate) fn with_observer(observer: Option<ActivityPresentationDiagnosticObserver>) -> Self {
        Self {
            state: Rc::new(RefCell::new(ActivityPresentationDiagnosticsState::default())),
            observer,
        }
    }

    pub(crate) fn observe_projection_change(&self, projection: ActivityProjectionDiagnosticState) {
        let mut state = self.state.borrow_mut();
        if projection.revision == 0 || projection.revision <= state.last_projection_revision {
            return;
        }
        state.last_projection_revision = projection.revision;
        let mut event = ActivityPresentationDiagnosticEvent {
            sequence: 0,
            elapsed_micros: 0,
            stage: "projection_changed",
            projection_revision: Some(projection.revision),
            newest_lifecycle_sequence: projection.newest_lifecycle_sequence,
            total_row_count: Some(projection.total_row_count),
            running_row_count: Some(projection.running_row_count),
            finished_ok_row_count: Some(projection.finished_ok_row_count),
            finished_error_row_count: Some(projection.finished_error_row_count),
            render_revision: None,
            newest_notified_projection_revision: None,
            panel_visible: None,
            selected_thread: None,
            selected_thread_row_count: None,
            rendered_range_start: None,
            rendered_range_end: None,
            overscan_row_count: None,
            sampled_rows: Vec::new(),
            row_sample_truncated: false,
            event_bytes: 0,
            event_bytes_truncated: false,
        };
        if self.observer.is_some() {
            state.prepare_event(&mut event);
            state.record_prepared(event.clone());
            drop(state);
            self.notify_observer(&event);
        } else {
            state.record(event);
        }
    }

    pub(crate) fn observe_shell_notification(&self, projection: ActivityProjectionDiagnosticState) {
        self.observe_projection_change(projection);
        let mut state = self.state.borrow_mut();
        if projection.revision == 0
            || state.newest_notified_projection_revision >= Some(projection.revision)
        {
            return;
        }
        state.newest_notified_projection_revision = Some(projection.revision);
        let mut event = ActivityPresentationDiagnosticEvent {
            sequence: 0,
            elapsed_micros: 0,
            stage: "shell_notified",
            projection_revision: Some(projection.revision),
            newest_lifecycle_sequence: None,
            total_row_count: None,
            running_row_count: None,
            finished_ok_row_count: None,
            finished_error_row_count: None,
            render_revision: None,
            newest_notified_projection_revision: Some(projection.revision),
            panel_visible: None,
            selected_thread: None,
            selected_thread_row_count: None,
            rendered_range_start: None,
            rendered_range_end: None,
            overscan_row_count: None,
            sampled_rows: Vec::new(),
            row_sample_truncated: false,
            event_bytes: 0,
            event_bytes_truncated: false,
        };
        if self.observer.is_some() {
            state.prepare_event(&mut event);
            state.record_prepared(event.clone());
            drop(state);
            self.notify_observer(&event);
        } else {
            state.record(event);
        }
    }

    pub(crate) fn observe_render<'a>(
        &self,
        projection: ActivityProjectionDiagnosticState,
        panel_visible: bool,
        selected_thread_id: Option<&str>,
        selected_thread_row_count: usize,
        rendered_range: std::ops::Range<usize>,
        overscan_row_count: usize,
        rows: impl IntoIterator<Item = ActivityPresentationRenderRow<'a>>,
    ) {
        self.observe_projection_change(projection);
        let mut state = self.state.borrow_mut();
        let render_revision = state.next_render_revision;
        state.next_render_revision = state.next_render_revision.saturating_add(1);
        let mut sampled_rows = Vec::new();
        let mut row_sample_truncated = false;
        for row in rows {
            if sampled_rows.len() >= ACTIVITY_PRESENTATION_DIAGNOSTIC_ROW_LIMIT {
                row_sample_truncated = true;
                state.omissions.row_sample_omission_count =
                    state.omissions.row_sample_omission_count.saturating_add(1);
                continue;
            }
            sampled_rows.push(state.capture_row(row));
        }
        let mut candidate = ActivityPresentationDiagnosticEvent {
            sequence: 0,
            elapsed_micros: 0,
            stage: "render_sample",
            projection_revision: Some(projection.revision),
            newest_lifecycle_sequence: None,
            total_row_count: None,
            running_row_count: None,
            finished_ok_row_count: None,
            finished_error_row_count: None,
            render_revision: Some(render_revision),
            newest_notified_projection_revision: state.newest_notified_projection_revision,
            panel_visible: Some(panel_visible),
            selected_thread: Some(state.capture_identity(selected_thread_id)),
            selected_thread_row_count: Some(selected_thread_row_count),
            rendered_range_start: Some(rendered_range.start),
            rendered_range_end: Some(rendered_range.end),
            overscan_row_count: Some(overscan_row_count),
            sampled_rows,
            row_sample_truncated,
            event_bytes: 0,
            event_bytes_truncated: false,
        };
        state.prepare_event(&mut candidate);
        if state
            .events
            .iter()
            .rev()
            .find(|stored| stored.event.stage == "render_sample")
            .is_some_and(|previous| render_state_equal(&previous.event, &candidate))
        {
            state.omissions.coalesced_render_sample_count = state
                .omissions
                .coalesced_render_sample_count
                .saturating_add(1);
            return;
        }
        if self.observer.is_some() {
            state.record_prepared(candidate.clone());
            drop(state);
            self.notify_observer(&candidate);
        } else {
            state.record_prepared(candidate);
        }
    }

    pub(crate) fn snapshot(&self) -> ActivityPresentationDiagnosticSnapshot {
        let state = self.state.borrow();
        ActivityPresentationDiagnosticSnapshot {
            event_capacity: ACTIVITY_PRESENTATION_DIAGNOSTIC_CAPACITY,
            event_byte_capacity: ACTIVITY_PRESENTATION_DIAGNOSTIC_BYTE_CAPACITY,
            event_byte_limit: ACTIVITY_PRESENTATION_DIAGNOSTIC_EVENT_BYTE_LIMIT,
            row_sample_capacity: ACTIVITY_PRESENTATION_DIAGNOSTIC_ROW_LIMIT,
            identity_field_byte_limit: ACTIVITY_PRESENTATION_IDENTITY_FIELD_BYTE_LIMIT,
            retained_count: state.events.len(),
            returned_count: state.events.len(),
            retained_event_bytes: state.retained_event_bytes,
            oldest_sequence: state.events.front().map(|stored| stored.event.sequence),
            newest_sequence: state.events.back().map(|stored| stored.event.sequence),
            omissions: state.omissions.clone(),
            truncated: state.omissions.evicted_event_count > 0
                || state.omissions.row_sample_omission_count > 0
                || state.omissions.event_byte_truncation_count > 0,
            events: state
                .events
                .iter()
                .map(|stored| stored.event.clone())
                .collect(),
        }
    }

    pub(crate) fn clear(&self) {
        *self.state.borrow_mut() = ActivityPresentationDiagnosticsState::default();
    }

    fn notify_observer(&self, event: &ActivityPresentationDiagnosticEvent) {
        if let Some(observer) = &self.observer {
            observer.observe(event);
        }
    }
}

impl Default for ActivityPresentationDiagnosticsState {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            started_at: Instant::now(),
            next_sequence: 1,
            next_render_revision: 1,
            retained_event_bytes: 0,
            last_projection_revision: 0,
            newest_notified_projection_revision: None,
            omissions: ActivityPresentationDiagnosticOmissions::default(),
        }
    }
}

impl ActivityPresentationDiagnosticsState {
    fn capture_identity(&mut self, value: Option<&str>) -> ActivityPresentationIdentity {
        let identity = ActivityPresentationIdentity::capture(value);
        match identity.validity {
            ActivityPresentationIdentityValidity::Valid => {}
            ActivityPresentationIdentityValidity::Missing => {
                self.omissions.missing_identity_field_count = self
                    .omissions
                    .missing_identity_field_count
                    .saturating_add(1);
            }
            ActivityPresentationIdentityValidity::Blank => {
                self.omissions.blank_identity_field_count =
                    self.omissions.blank_identity_field_count.saturating_add(1);
            }
            ActivityPresentationIdentityValidity::OverBound => {
                self.omissions.over_bound_identity_field_count = self
                    .omissions
                    .over_bound_identity_field_count
                    .saturating_add(1);
            }
        }
        identity
    }

    fn capture_row(
        &mut self,
        row: ActivityPresentationRenderRow<'_>,
    ) -> ActivityPresentationRowSample {
        ActivityPresentationRowSample {
            rendered_index: row.rendered_index,
            thread: self.capture_identity(Some(row.thread_id)),
            turn: self.capture_identity(Some(row.turn_id)),
            item: self.capture_identity(Some(row.item_id)),
            status: row.status,
            status_indicator_theme_role: row.status_indicator_theme_role,
            color_source: if row.used_theme_role {
                "theme_role"
            } else {
                "renderer_fallback"
            },
            resolved_rgba: row.resolved_rgba,
        }
    }

    fn prepare_event(&mut self, event: &mut ActivityPresentationDiagnosticEvent) {
        event.sequence = self.next_sequence;
        event.elapsed_micros = self
            .started_at
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        set_encoded_event_bytes(event);
        if event.event_bytes > ACTIVITY_PRESENTATION_DIAGNOSTIC_EVENT_BYTE_LIMIT {
            event.sampled_rows.clear();
            event.row_sample_truncated = true;
            event.event_bytes_truncated = true;
            self.omissions.event_byte_truncation_count =
                self.omissions.event_byte_truncation_count.saturating_add(1);
            set_encoded_event_bytes(event);
        }
    }

    fn record(&mut self, mut event: ActivityPresentationDiagnosticEvent) {
        self.prepare_event(&mut event);
        self.record_prepared(event);
    }

    fn record_prepared(&mut self, event: ActivityPresentationDiagnosticEvent) {
        self.next_sequence = self.next_sequence.saturating_add(1);
        let encoded_bytes = event.event_bytes;
        self.retained_event_bytes = self.retained_event_bytes.saturating_add(encoded_bytes);
        self.events.push_back(StoredActivityPresentationEvent {
            event,
            encoded_bytes,
        });
        while self.events.len() > ACTIVITY_PRESENTATION_DIAGNOSTIC_CAPACITY
            || self.retained_event_bytes > ACTIVITY_PRESENTATION_DIAGNOSTIC_BYTE_CAPACITY
        {
            let Some(evicted) = self.events.pop_front() else {
                break;
            };
            self.retained_event_bytes = self
                .retained_event_bytes
                .saturating_sub(evicted.encoded_bytes);
            self.omissions.evicted_event_count =
                self.omissions.evicted_event_count.saturating_add(1);
        }
    }
}

fn encoded_event_bytes(event: &ActivityPresentationDiagnosticEvent) -> usize {
    serde_json::to_vec(event).map_or(0, |encoded| encoded.len())
}

fn set_encoded_event_bytes(event: &mut ActivityPresentationDiagnosticEvent) {
    for _ in 0..4 {
        let encoded_bytes = encoded_event_bytes(event);
        if event.event_bytes == encoded_bytes {
            return;
        }
        event.event_bytes = encoded_bytes;
    }
}

fn render_state_equal(
    left: &ActivityPresentationDiagnosticEvent,
    right: &ActivityPresentationDiagnosticEvent,
) -> bool {
    left.projection_revision == right.projection_revision
        && left.newest_notified_projection_revision == right.newest_notified_projection_revision
        && left.panel_visible == right.panel_visible
        && left.selected_thread == right.selected_thread
        && left.selected_thread_row_count == right.selected_thread_row_count
        && left.rendered_range_start == right.rendered_range_start
        && left.rendered_range_end == right.rendered_range_end
        && left.overscan_row_count == right.overscan_row_count
        && left.sampled_rows == right.sampled_rows
        && left.row_sample_truncated == right.row_sample_truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection(revision: u64) -> ActivityProjectionDiagnosticState {
        ActivityProjectionDiagnosticState {
            revision,
            newest_lifecycle_sequence: Some(revision),
            total_row_count: 1,
            running_row_count: 1,
            finished_ok_row_count: 0,
            finished_error_row_count: 0,
        }
    }

    #[test]
    fn disabled_observer_preserves_event_order_and_render_coalescing() {
        let diagnostics = ActivityPresentationDiagnostics::with_observer(None);
        let rows = [ActivityPresentationRenderRow {
            rendered_index: 0,
            thread_id: "thread",
            turn_id: "turn",
            item_id: "item",
            status: "running",
            status_indicator_theme_role: "activity.indicator.running",
            used_theme_role: true,
            resolved_rgba: [1, 2, 3, 255],
        }];

        diagnostics.observe_shell_notification(projection(1));
        diagnostics.observe_render(projection(1), true, Some("thread"), 1, 0..1, 2, rows);
        diagnostics.observe_render(projection(1), true, Some("thread"), 1, 0..1, 2, rows);

        let snapshot = diagnostics.snapshot();
        assert_eq!(
            snapshot
                .events
                .iter()
                .map(|event| (event.sequence, event.stage))
                .collect::<Vec<_>>(),
            vec![
                (1, "projection_changed"),
                (2, "shell_notified"),
                (3, "render_sample"),
            ]
        );
        assert_eq!(snapshot.retained_count, 3);
        assert_eq!(snapshot.omissions.coalesced_render_sample_count, 1);
    }

    #[test]
    fn observer_runs_after_retention_skips_coalesced_samples_and_survives_clear() {
        let diagnostics_slot = Rc::new(RefCell::new(None::<ActivityPresentationDiagnostics>));
        let observations = Rc::new(RefCell::new(Vec::new()));
        let observer = ActivityPresentationDiagnosticObserver::new({
            let diagnostics_slot = Rc::downgrade(&diagnostics_slot);
            let observations = Rc::clone(&observations);
            move |event| {
                let diagnostics_slot = diagnostics_slot.upgrade().unwrap();
                let diagnostics_slot = diagnostics_slot.borrow();
                let snapshot = diagnostics_slot.as_ref().unwrap().snapshot();
                let retained = snapshot.events.last().unwrap();
                observations.borrow_mut().push((
                    event.sequence,
                    event.stage,
                    retained.sequence,
                    retained.stage,
                ));
            }
        });
        let diagnostics = ActivityPresentationDiagnostics::with_observer(Some(observer));
        *diagnostics_slot.borrow_mut() = Some(diagnostics.clone());
        let rows = [ActivityPresentationRenderRow {
            rendered_index: 0,
            thread_id: "thread",
            turn_id: "turn",
            item_id: "item",
            status: "finished_error",
            status_indicator_theme_role: "activity.indicator.error",
            used_theme_role: false,
            resolved_rgba: [4, 5, 6, 255],
        }];

        diagnostics.observe_shell_notification(projection(1));
        diagnostics.observe_render(projection(1), true, Some("thread"), 1, 0..1, 2, rows);
        diagnostics.observe_render(projection(1), true, Some("thread"), 1, 0..1, 2, rows);

        assert_eq!(
            *observations.borrow(),
            vec![
                (1, "projection_changed", 1, "projection_changed"),
                (2, "shell_notified", 2, "shell_notified"),
                (3, "render_sample", 3, "render_sample"),
            ]
        );
        assert_eq!(
            diagnostics
                .snapshot()
                .omissions
                .coalesced_render_sample_count,
            1
        );

        diagnostics.clear();
        diagnostics.observe_projection_change(projection(1));
        assert_eq!(observations.borrow().len(), 4);
        assert_eq!(observations.borrow()[3].0, 1);
        assert_eq!(observations.borrow()[3].1, "projection_changed");
    }

    #[test]
    fn coalesces_identical_render_state_and_keeps_exact_row_identity() {
        let diagnostics = ActivityPresentationDiagnostics::default();
        let rows = [ActivityPresentationRenderRow {
            rendered_index: 0,
            thread_id: "thread",
            turn_id: "turn",
            item_id: "item",
            status: "running",
            status_indicator_theme_role: "activity.indicator.running",
            used_theme_role: true,
            resolved_rgba: [1, 2, 3, 255],
        }];
        diagnostics.observe_render(projection(1), true, Some("thread"), 1, 0..1, 2, rows);
        diagnostics.observe_render(projection(1), true, Some("thread"), 1, 0..1, 2, rows);

        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(
            snapshot.events[1].sampled_rows[0].item.value.as_deref(),
            Some("item")
        );
        assert_eq!(snapshot.omissions.coalesced_render_sample_count, 1);
    }

    #[test]
    fn bounds_ring_and_marks_invalid_identity_without_retaining_it() {
        let diagnostics = ActivityPresentationDiagnostics::default();
        for revision in 1..=300 {
            diagnostics.observe_render(
                projection(revision),
                true,
                Some(&"x".repeat(ACTIVITY_PRESENTATION_IDENTITY_FIELD_BYTE_LIMIT + 1)),
                0,
                0..0,
                2,
                [],
            );
        }
        let snapshot = diagnostics.snapshot();
        assert!(snapshot.retained_count <= ACTIVITY_PRESENTATION_DIAGNOSTIC_CAPACITY);
        assert!(snapshot.retained_event_bytes <= ACTIVITY_PRESENTATION_DIAGNOSTIC_BYTE_CAPACITY);
        assert!(snapshot.omissions.evicted_event_count > 0);
        assert!(snapshot.omissions.over_bound_identity_field_count > 0);
        assert!(snapshot.events.iter().all(|event| {
            event
                .selected_thread
                .as_ref()
                .is_none_or(|identity| identity.value.is_none())
        }));
    }

    #[test]
    fn normalizes_escape_expansion_before_coalescing_and_retention() {
        let diagnostics = ActivityPresentationDiagnostics::default();
        let escaped_identity = "\u{0000}".repeat(ACTIVITY_PRESENTATION_IDENTITY_FIELD_BYTE_LIMIT);
        let rows = (0..ACTIVITY_PRESENTATION_DIAGNOSTIC_ROW_LIMIT)
            .map(|index| ActivityPresentationRenderRow {
                rendered_index: index,
                thread_id: &escaped_identity,
                turn_id: &escaped_identity,
                item_id: &escaped_identity,
                status: "running",
                status_indicator_theme_role: "activity.indicator.running",
                used_theme_role: true,
                resolved_rgba: [1, 2, 3, 255],
            })
            .collect::<Vec<_>>();

        diagnostics.observe_render(
            projection(1),
            true,
            Some("thread"),
            rows.len(),
            0..rows.len(),
            2,
            rows.iter().copied(),
        );
        diagnostics.observe_render(
            projection(1),
            true,
            Some("thread"),
            rows.len(),
            0..rows.len(),
            2,
            rows.iter().copied(),
        );

        let snapshot = diagnostics.snapshot();
        let render_samples = snapshot
            .events
            .iter()
            .filter(|event| event.stage == "render_sample")
            .collect::<Vec<_>>();
        assert_eq!(render_samples.len(), 1);
        assert!(render_samples[0].sampled_rows.is_empty());
        assert!(render_samples[0].row_sample_truncated);
        assert!(render_samples[0].event_bytes_truncated);
        assert!(render_samples[0].event_bytes <= ACTIVITY_PRESENTATION_DIAGNOSTIC_EVENT_BYTE_LIMIT);
        assert_eq!(snapshot.omissions.coalesced_render_sample_count, 1);
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains("\\u0000")
        );
    }

    #[test]
    fn coalescing_uses_the_bounded_event_retained_in_a_saturated_ring() {
        let diagnostics = ActivityPresentationDiagnostics::default();
        let row = [ActivityPresentationRenderRow {
            rendered_index: 0,
            thread_id: "thread",
            turn_id: "turn",
            item_id: "item",
            status: "running",
            status_indicator_theme_role: "activity.indicator.running",
            used_theme_role: true,
            resolved_rgba: [1, 2, 3, 255],
        }];
        for revision in 1..=300 {
            diagnostics.observe_render(projection(revision), true, Some("thread"), 1, 0..1, 2, row);
        }

        let before = diagnostics.snapshot();
        assert!(before.omissions.evicted_event_count > 0);
        diagnostics.observe_render(projection(300), true, Some("thread"), 1, 0..1, 2, row);
        let after = diagnostics.snapshot();

        assert_eq!(after.retained_count, before.retained_count);
        assert_eq!(after.retained_event_bytes, before.retained_event_bytes);
        assert_eq!(
            after.omissions.coalesced_render_sample_count,
            before
                .omissions
                .coalesced_render_sample_count
                .saturating_add(1)
        );
        let state = diagnostics.state.borrow();
        assert_eq!(
            state.retained_event_bytes,
            state
                .events
                .iter()
                .map(|stored| stored.encoded_bytes)
                .sum::<usize>()
        );
    }

    #[test]
    fn clear_resets_session_ring_and_cursor_sequence() {
        let diagnostics = ActivityPresentationDiagnostics::default();
        diagnostics.observe_projection_change(projection(1));
        diagnostics.clear();
        diagnostics.observe_projection_change(projection(1));
        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.events[0].sequence, 1);
    }

    #[test]
    fn samples_terminal_row_after_leaving_and_reentering_overscan() {
        let diagnostics = ActivityPresentationDiagnostics::default();
        let running = [ActivityPresentationRenderRow {
            rendered_index: 0,
            thread_id: "thread",
            turn_id: "turn",
            item_id: "item",
            status: "running",
            status_indicator_theme_role: "activity.indicator.running",
            used_theme_role: true,
            resolved_rgba: [1, 2, 3, 255],
        }];
        let offscreen = [ActivityPresentationRenderRow {
            rendered_index: 8,
            thread_id: "thread",
            turn_id: "turn",
            item_id: "other",
            status: "running",
            status_indicator_theme_role: "activity.indicator.running",
            used_theme_role: true,
            resolved_rgba: [1, 2, 3, 255],
        }];
        let terminal = [ActivityPresentationRenderRow {
            rendered_index: 0,
            thread_id: "thread",
            turn_id: "turn",
            item_id: "item",
            status: "finished_ok",
            status_indicator_theme_role: "activity.indicator.ok",
            used_theme_role: true,
            resolved_rgba: [4, 5, 6, 255],
        }];
        let terminal_projection = ActivityProjectionDiagnosticState {
            revision: 2,
            newest_lifecycle_sequence: Some(2),
            total_row_count: 1,
            running_row_count: 0,
            finished_ok_row_count: 1,
            finished_error_row_count: 0,
        };

        diagnostics.observe_render(projection(1), true, Some("thread"), 1, 0..1, 2, running);
        diagnostics.observe_render(projection(1), true, Some("thread"), 1, 8..9, 2, offscreen);
        diagnostics.observe_render(
            terminal_projection,
            true,
            Some("thread"),
            1,
            0..1,
            2,
            terminal,
        );

        let snapshot = diagnostics.snapshot();
        let samples = snapshot
            .events
            .iter()
            .filter(|event| event.stage == "render_sample")
            .collect::<Vec<_>>();
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].sampled_rows[0].status, "running");
        assert_eq!(
            samples[0].sampled_rows[0].thread.value.as_deref(),
            Some("thread")
        );
        assert_eq!(
            samples[0].sampled_rows[0].turn.value.as_deref(),
            Some("turn")
        );
        assert_eq!(
            samples[0].sampled_rows[0].item.value.as_deref(),
            Some("item")
        );
        assert_eq!(samples[1].rendered_range_start, Some(8));
        assert_eq!(
            samples[1].sampled_rows[0].item.value.as_deref(),
            Some("other")
        );
        assert!(
            samples[1]
                .sampled_rows
                .iter()
                .all(|row| row.item.value.as_deref() != Some("item"))
        );
        assert_eq!(samples[2].projection_revision, Some(2));
        assert_eq!(samples[2].sampled_rows[0].status, "finished_ok");
        assert_eq!(
            samples[2].sampled_rows[0].thread.value.as_deref(),
            Some("thread")
        );
        assert_eq!(
            samples[2].sampled_rows[0].turn.value.as_deref(),
            Some("turn")
        );
        assert_eq!(
            samples[2].sampled_rows[0].item.value.as_deref(),
            Some("item")
        );
        assert_eq!(
            samples[2].sampled_rows[0].status_indicator_theme_role,
            "activity.indicator.ok"
        );
        assert_eq!(samples[2].sampled_rows[0].color_source, "theme_role");
        assert_eq!(samples[2].sampled_rows[0].resolved_rgba, [4, 5, 6, 255]);
    }

    #[test]
    fn sample_row_count_is_bounded_and_marks_truncation() {
        let diagnostics = ActivityPresentationDiagnostics::default();
        let item_ids = (0..40)
            .map(|index| format!("item-{index}"))
            .collect::<Vec<_>>();
        let rows = |range: std::ops::Range<usize>| {
            item_ids[range.clone()]
                .iter()
                .enumerate()
                .map(|(offset, item_id)| ActivityPresentationRenderRow {
                    rendered_index: range.start + offset,
                    thread_id: "thread",
                    turn_id: "turn",
                    item_id,
                    status: "running",
                    status_indicator_theme_role: "activity.indicator.running",
                    used_theme_role: true,
                    resolved_rgba: [1, 2, 3, 255],
                })
                .collect::<Vec<_>>()
        };

        diagnostics.observe_render(
            projection(1),
            true,
            Some("thread"),
            40,
            0..40,
            2,
            rows(0..40),
        );

        let snapshot = diagnostics.snapshot();
        let samples = snapshot
            .events
            .iter()
            .filter(|event| event.stage == "render_sample")
            .collect::<Vec<_>>();
        assert_eq!(samples.len(), 1);
        assert_eq!(
            samples[0].sampled_rows.len(),
            ACTIVITY_PRESENTATION_DIAGNOSTIC_ROW_LIMIT
        );
        assert!(samples[0].row_sample_truncated);
        assert_eq!(
            samples[0].sampled_rows[0].item.value.as_deref(),
            Some("item-0")
        );
    }
}
