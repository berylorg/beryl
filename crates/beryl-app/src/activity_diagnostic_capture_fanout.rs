use crate::activity_diagnostic_file_capture::{
    ActivityDiagnosticCaptureEventV1, ActivityDiagnosticCaptureSink,
    ActivityDiagnosticColorSourceV1, ActivityDiagnosticIdentityV1,
    ActivityDiagnosticIdentityValidityV1, ActivityDiagnosticIndicatorRoleV1,
    ActivityDiagnosticLifecycleCategoryV1, ActivityDiagnosticLifecycleEventV1,
    ActivityDiagnosticLifecycleKindV1, ActivityDiagnosticLifecycleStageV1,
    ActivityDiagnosticProjectionChangedV1, ActivityDiagnosticProjectionOutcomeV1,
    ActivityDiagnosticProtocolStringV1, ActivityDiagnosticRenderRowV1,
    ActivityDiagnosticRenderSampleV1, ActivityDiagnosticRowStatusV1,
    ActivityDiagnosticShellNotifiedV1,
};
use crate::activity_lifecycle_diagnostics::{
    ActivityLifecycleDiagnosticEvent, ActivityLifecycleDiagnosticObserver,
    ActivityLifecycleIdentity, ActivityLifecycleIdentityValidity, ActivityLifecycleProtocolString,
};
use crate::activity_presentation_diagnostics::{
    ActivityPresentationDiagnosticEvent, ActivityPresentationDiagnosticObserver,
    ActivityPresentationIdentity, ActivityPresentationIdentityValidity,
    ActivityPresentationRowSample,
};

pub(crate) fn lifecycle_capture_observer(
    sink: ActivityDiagnosticCaptureSink,
) -> ActivityLifecycleDiagnosticObserver {
    ActivityLifecycleDiagnosticObserver::new(move |event| {
        if sink.is_active() {
            submit_active(&sink, lifecycle_capture_event(event));
        }
    })
}

pub(crate) fn presentation_capture_observer(
    sink: ActivityDiagnosticCaptureSink,
) -> ActivityPresentationDiagnosticObserver {
    ActivityPresentationDiagnosticObserver::new(move |event| {
        if sink.is_active() {
            submit_active(&sink, presentation_capture_event(event));
        }
    })
}

fn submit_active(
    sink: &ActivityDiagnosticCaptureSink,
    event: Option<ActivityDiagnosticCaptureEventV1>,
) {
    match event {
        Some(event) => {
            let _ = sink.try_record(event);
        }
        None => {
            let _ = sink.note_schema_rejection();
        }
    }
}

fn lifecycle_capture_event(
    event: &ActivityLifecycleDiagnosticEvent,
) -> Option<ActivityDiagnosticCaptureEventV1> {
    Some(ActivityDiagnosticCaptureEventV1::Lifecycle(
        ActivityDiagnosticLifecycleEventV1 {
            source_sequence: event.sequence,
            elapsed_micros: event.elapsed_micros,
            stage: lifecycle_stage(event.stage)?,
            category: lifecycle_category(event.category)?,
            kind: lifecycle_kind(event.kind)?,
            thread_identity: lifecycle_identity(&event.thread)?,
            turn_identity: lifecycle_identity(&event.turn)?,
            item_identity: lifecycle_identity(&event.item)?,
            item_type: lifecycle_protocol_string(&event.item_type)?,
            item_status: lifecycle_protocol_string(&event.item_status)?,
            projection_outcome: projection_outcome(event.projection_outcome)?,
            before_row_status: optional_row_status(event.before_row_status)?,
            after_row_status: optional_row_status(event.after_row_status)?,
            affected_row_count: u64::try_from(event.affected_row_count).ok()?,
        },
    ))
}

fn lifecycle_identity(
    identity: &ActivityLifecycleIdentity,
) -> Option<ActivityDiagnosticIdentityV1> {
    let validity = match identity.validity {
        ActivityLifecycleIdentityValidity::Valid => ActivityDiagnosticIdentityValidityV1::Valid,
        ActivityLifecycleIdentityValidity::Missing => ActivityDiagnosticIdentityValidityV1::Missing,
        ActivityLifecycleIdentityValidity::Blank => ActivityDiagnosticIdentityValidityV1::Blank,
        ActivityLifecycleIdentityValidity::OverBound => {
            ActivityDiagnosticIdentityValidityV1::OverBound
        }
    };
    ActivityDiagnosticIdentityV1::try_from_normalized(
        validity,
        identity.value.as_deref(),
        identity.original_byte_count,
    )
}

fn lifecycle_protocol_string(
    value: &ActivityLifecycleProtocolString,
) -> Option<ActivityDiagnosticProtocolStringV1> {
    ActivityDiagnosticProtocolStringV1::try_from_normalized(
        value.value.as_deref(),
        value.original_byte_count,
        value.truncated,
    )
}

fn lifecycle_stage(value: &str) -> Option<ActivityDiagnosticLifecycleStageV1> {
    match value {
        "activity_ingress" => Some(ActivityDiagnosticLifecycleStageV1::ActivityIngress),
        "fallback" => Some(ActivityDiagnosticLifecycleStageV1::Fallback),
        "stream_failure" => Some(ActivityDiagnosticLifecycleStageV1::StreamFailure),
        _ => None,
    }
}

fn lifecycle_category(value: &str) -> Option<ActivityDiagnosticLifecycleCategoryV1> {
    match value {
        "lifecycle" => Some(ActivityDiagnosticLifecycleCategoryV1::Lifecycle),
        "fallback" => Some(ActivityDiagnosticLifecycleCategoryV1::Fallback),
        "stream_failure" => Some(ActivityDiagnosticLifecycleCategoryV1::StreamFailure),
        _ => None,
    }
}

fn lifecycle_kind(value: &str) -> Option<ActivityDiagnosticLifecycleKindV1> {
    match value {
        "started" => Some(ActivityDiagnosticLifecycleKindV1::Started),
        "updated" => Some(ActivityDiagnosticLifecycleKindV1::Updated),
        "completed" => Some(ActivityDiagnosticLifecycleKindV1::Completed),
        "turn_completed" => Some(ActivityDiagnosticLifecycleKindV1::TurnCompleted),
        "thread_closed" => Some(ActivityDiagnosticLifecycleKindV1::ThreadClosed),
        "thread_archived" => Some(ActivityDiagnosticLifecycleKindV1::ThreadArchived),
        "thread_deleted" => Some(ActivityDiagnosticLifecycleKindV1::ThreadDeleted),
        "protocol_error" => Some(ActivityDiagnosticLifecycleKindV1::ProtocolError),
        "local_turn_failure" => Some(ActivityDiagnosticLifecycleKindV1::LocalTurnFailure),
        _ => None,
    }
}

fn projection_outcome(value: &str) -> Option<ActivityDiagnosticProjectionOutcomeV1> {
    match value {
        "inserted_running" => Some(ActivityDiagnosticProjectionOutcomeV1::InsertedRunning),
        "matched_running" => Some(ActivityDiagnosticProjectionOutcomeV1::MatchedRunning),
        "reactivated_existing" => Some(ActivityDiagnosticProjectionOutcomeV1::ReactivatedExisting),
        "matched_existing" => Some(ActivityDiagnosticProjectionOutcomeV1::MatchedExisting),
        "inserted_completed" => Some(ActivityDiagnosticProjectionOutcomeV1::InsertedCompleted),
        "no_running_match" => Some(ActivityDiagnosticProjectionOutcomeV1::NoRunningMatch),
        "finished_running_rows" => Some(ActivityDiagnosticProjectionOutcomeV1::FinishedRunningRows),
        _ => None,
    }
}

fn optional_row_status(value: Option<&str>) -> Option<Option<ActivityDiagnosticRowStatusV1>> {
    match value {
        Some(value) => Some(Some(row_status(value)?)),
        None => Some(None),
    }
}

fn row_status(value: &str) -> Option<ActivityDiagnosticRowStatusV1> {
    match value {
        "running" => Some(ActivityDiagnosticRowStatusV1::Running),
        "finished_ok" => Some(ActivityDiagnosticRowStatusV1::FinishedOk),
        "finished_error" => Some(ActivityDiagnosticRowStatusV1::FinishedError),
        _ => None,
    }
}

fn indicator_role(value: &str) -> Option<ActivityDiagnosticIndicatorRoleV1> {
    match value {
        "activity.indicator.running" => Some(ActivityDiagnosticIndicatorRoleV1::Running),
        "activity.indicator.ok" => Some(ActivityDiagnosticIndicatorRoleV1::Ok),
        "activity.indicator.error" => Some(ActivityDiagnosticIndicatorRoleV1::Error),
        _ => None,
    }
}

fn color_source(value: &str) -> Option<ActivityDiagnosticColorSourceV1> {
    match value {
        "theme_role" => Some(ActivityDiagnosticColorSourceV1::ThemeRole),
        "renderer_fallback" => Some(ActivityDiagnosticColorSourceV1::RendererFallback),
        _ => None,
    }
}

fn presentation_capture_event(
    event: &ActivityPresentationDiagnosticEvent,
) -> Option<ActivityDiagnosticCaptureEventV1> {
    match event.stage {
        "projection_changed" => Some(ActivityDiagnosticCaptureEventV1::ProjectionChanged(
            ActivityDiagnosticProjectionChangedV1 {
                source_sequence: event.sequence,
                elapsed_micros: event.elapsed_micros,
                projection_revision: event.projection_revision?,
                newest_lifecycle_sequence: event.newest_lifecycle_sequence,
                total_row_count: u64::try_from(event.total_row_count?).ok()?,
                running_row_count: u64::try_from(event.running_row_count?).ok()?,
                finished_ok_row_count: u64::try_from(event.finished_ok_row_count?).ok()?,
                finished_error_row_count: u64::try_from(event.finished_error_row_count?).ok()?,
            },
        )),
        "shell_notified" => Some(ActivityDiagnosticCaptureEventV1::ShellNotified(
            ActivityDiagnosticShellNotifiedV1 {
                source_sequence: event.sequence,
                elapsed_micros: event.elapsed_micros,
                projection_revision: event.projection_revision?,
            },
        )),
        "render_sample" => render_capture_event(event),
        _ => None,
    }
}

fn render_capture_event(
    event: &ActivityPresentationDiagnosticEvent,
) -> Option<ActivityDiagnosticCaptureEventV1> {
    let sampled_rows = event
        .sampled_rows
        .iter()
        .map(presentation_row)
        .collect::<Option<Vec<_>>>()?;
    let selected_thread_identity = match event.selected_thread.as_ref() {
        Some(identity) => Some(presentation_identity(identity)?),
        None => None,
    };
    Some(ActivityDiagnosticCaptureEventV1::RenderSample(
        ActivityDiagnosticRenderSampleV1 {
            source_sequence: event.sequence,
            elapsed_micros: event.elapsed_micros,
            render_revision: event.render_revision?,
            projection_revision: event.projection_revision?,
            newest_notified_projection_revision: event
                .newest_notified_projection_revision
                .unwrap_or(0),
            panel_visible: event.panel_visible?,
            selected_thread_identity,
            selected_thread_row_count: u64::try_from(event.selected_thread_row_count?).ok()?,
            rendered_range_start: u64::try_from(event.rendered_range_start?).ok()?,
            rendered_range_end: u64::try_from(event.rendered_range_end?).ok()?,
            overscan_row_count: u64::try_from(event.overscan_row_count?).ok()?,
            sampled_rows,
            row_sample_truncated: event.row_sample_truncated,
            event_bytes: u64::try_from(event.event_bytes).ok()?,
            event_bytes_truncated: event.event_bytes_truncated,
        },
    ))
}

fn presentation_row(row: &ActivityPresentationRowSample) -> Option<ActivityDiagnosticRenderRowV1> {
    Some(ActivityDiagnosticRenderRowV1 {
        rendered_index: u64::try_from(row.rendered_index).ok()?,
        thread_identity: presentation_identity(&row.thread)?,
        turn_identity: presentation_identity(&row.turn)?,
        item_identity: presentation_identity(&row.item)?,
        row_status: row_status(row.status)?,
        status_indicator_theme_role: indicator_role(row.status_indicator_theme_role)?,
        color_source: color_source(row.color_source)?,
        resolved_rgba: row.resolved_rgba,
    })
}

fn presentation_identity(
    identity: &ActivityPresentationIdentity,
) -> Option<ActivityDiagnosticIdentityV1> {
    let validity = match identity.validity {
        ActivityPresentationIdentityValidity::Valid => ActivityDiagnosticIdentityValidityV1::Valid,
        ActivityPresentationIdentityValidity::Missing => {
            ActivityDiagnosticIdentityValidityV1::Missing
        }
        ActivityPresentationIdentityValidity::Blank => ActivityDiagnosticIdentityValidityV1::Blank,
        ActivityPresentationIdentityValidity::OverBound => {
            ActivityDiagnosticIdentityValidityV1::OverBound
        }
    };
    ActivityDiagnosticIdentityV1::try_from_normalized(
        validity,
        identity.value.as_deref(),
        identity.original_byte_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity_presentation_diagnostics::{
        ActivityPresentationDiagnosticSnapshot, ActivityPresentationDiagnostics,
        ActivityPresentationRenderRow, ActivityProjectionDiagnosticState,
    };

    fn lifecycle_identity_source(
        validity: ActivityLifecycleIdentityValidity,
        value: Option<&str>,
        original_byte_count: usize,
    ) -> ActivityLifecycleIdentity {
        ActivityLifecycleIdentity {
            validity,
            value: value.map(str::to_string),
            original_byte_count,
        }
    }

    fn presentation_identity_source(
        validity: ActivityPresentationIdentityValidity,
        value: Option<&str>,
        original_byte_count: usize,
    ) -> ActivityPresentationIdentity {
        ActivityPresentationIdentity {
            validity,
            value: value.map(str::to_string),
            original_byte_count,
        }
    }

    fn empty_presentation_event(stage: &'static str) -> ActivityPresentationDiagnosticEvent {
        ActivityPresentationDiagnosticEvent {
            sequence: 1,
            elapsed_micros: 2,
            stage,
            projection_revision: None,
            newest_lifecycle_sequence: None,
            total_row_count: None,
            running_row_count: None,
            finished_ok_row_count: None,
            finished_error_row_count: None,
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
        }
    }

    fn projection_state(revision: u64) -> ActivityProjectionDiagnosticState {
        ActivityProjectionDiagnosticState {
            revision,
            newest_lifecycle_sequence: Some(revision),
            total_row_count: 1,
            running_row_count: 1,
            finished_ok_row_count: 0,
            finished_error_row_count: 0,
        }
    }

    fn observe_presentation_sequence(diagnostics: &ActivityPresentationDiagnostics, revision: u64) {
        let projection = projection_state(revision);
        diagnostics.observe_shell_notification(projection);
        diagnostics.observe_render(
            projection,
            true,
            Some("thread"),
            1,
            0..1,
            0,
            [ActivityPresentationRenderRow {
                rendered_index: 0,
                thread_id: "thread",
                turn_id: "turn",
                item_id: "item",
                status: "running",
                status_indicator_theme_role: "activity.indicator.running",
                used_theme_role: true,
                resolved_rgba: [1, 2, 3, 4],
            }],
        );
    }

    fn presentation_snapshot_without_timing(
        mut snapshot: ActivityPresentationDiagnosticSnapshot,
    ) -> ActivityPresentationDiagnosticSnapshot {
        snapshot.retained_event_bytes = 0;
        for event in &mut snapshot.events {
            event.elapsed_micros = 0;
            event.event_bytes = 0;
        }
        snapshot
    }

    #[test]
    fn lifecycle_conversion_preserves_normalized_validity_and_protocol_metadata() {
        let event = ActivityLifecycleDiagnosticEvent {
            sequence: 7,
            elapsed_micros: 11,
            stage: "activity_ingress",
            category: "lifecycle",
            kind: "completed",
            thread: lifecycle_identity_source(
                ActivityLifecycleIdentityValidity::Valid,
                Some("thread"),
                6,
            ),
            turn: lifecycle_identity_source(ActivityLifecycleIdentityValidity::Blank, None, 3),
            item: lifecycle_identity_source(
                ActivityLifecycleIdentityValidity::OverBound,
                None,
                513,
            ),
            item_type: ActivityLifecycleProtocolString {
                value: Some("commandExecution".to_string()),
                original_byte_count: 16,
                truncated: false,
            },
            item_status: ActivityLifecycleProtocolString {
                value: Some("x".repeat(512)),
                original_byte_count: 600,
                truncated: true,
            },
            projection_outcome: "matched_existing",
            before_row_status: Some("running"),
            after_row_status: Some("finished_ok"),
            affected_row_count: 1,
        };

        let ActivityDiagnosticCaptureEventV1::Lifecycle(converted) =
            lifecycle_capture_event(&event).unwrap()
        else {
            panic!("expected lifecycle DTO");
        };
        assert_eq!(converted.source_sequence, 7);
        assert_eq!(converted.elapsed_micros, 11);
        assert_eq!(
            converted.turn_identity,
            ActivityDiagnosticIdentityV1::try_from_normalized(
                ActivityDiagnosticIdentityValidityV1::Blank,
                None,
                3,
            )
            .unwrap()
        );
        assert_eq!(
            converted.item_identity,
            ActivityDiagnosticIdentityV1::try_from_normalized(
                ActivityDiagnosticIdentityValidityV1::OverBound,
                None,
                513,
            )
            .unwrap()
        );
        assert_eq!(
            converted.item_status,
            ActivityDiagnosticProtocolStringV1::try_from_normalized(
                Some(&"x".repeat(512)),
                600,
                true,
            )
            .unwrap()
        );
        assert_eq!(
            converted.before_row_status,
            Some(ActivityDiagnosticRowStatusV1::Running)
        );
        assert_eq!(
            converted.after_row_status,
            Some(ActivityDiagnosticRowStatusV1::FinishedOk)
        );
    }

    #[test]
    fn controlled_value_mappers_are_closed_over_the_v1_allowlist() {
        for value in ["activity_ingress", "fallback", "stream_failure"] {
            assert!(lifecycle_stage(value).is_some());
        }
        for value in ["lifecycle", "fallback", "stream_failure"] {
            assert!(lifecycle_category(value).is_some());
        }
        for value in [
            "started",
            "updated",
            "completed",
            "turn_completed",
            "thread_closed",
            "thread_archived",
            "thread_deleted",
            "protocol_error",
            "local_turn_failure",
        ] {
            assert!(lifecycle_kind(value).is_some());
        }
        for value in [
            "inserted_running",
            "matched_running",
            "reactivated_existing",
            "matched_existing",
            "inserted_completed",
            "no_running_match",
            "finished_running_rows",
        ] {
            assert!(projection_outcome(value).is_some());
        }
        assert!(lifecycle_stage("future_stage").is_none());
        assert!(lifecycle_category("future_category").is_none());
        assert!(lifecycle_kind("future_kind").is_none());
        assert!(projection_outcome("future_outcome").is_none());
        assert!(row_status("future_status").is_none());
        assert!(indicator_role("activity.indicator.future").is_none());
        assert!(color_source("future_source").is_none());
    }

    #[test]
    fn projection_and_notification_conversion_require_and_preserve_stage_fields() {
        let mut projection = empty_presentation_event("projection_changed");
        projection.projection_revision = Some(9);
        projection.newest_lifecycle_sequence = Some(8);
        projection.total_row_count = Some(4);
        projection.running_row_count = Some(1);
        projection.finished_ok_row_count = Some(2);
        projection.finished_error_row_count = Some(1);
        let ActivityDiagnosticCaptureEventV1::ProjectionChanged(converted) =
            presentation_capture_event(&projection).unwrap()
        else {
            panic!("expected projection DTO");
        };
        assert_eq!(converted.projection_revision, 9);
        assert_eq!(converted.newest_lifecycle_sequence, Some(8));
        assert_eq!(converted.total_row_count, 4);

        let mut notified = empty_presentation_event("shell_notified");
        notified.sequence = 3;
        notified.projection_revision = Some(9);
        let ActivityDiagnosticCaptureEventV1::ShellNotified(converted) =
            presentation_capture_event(&notified).unwrap()
        else {
            panic!("expected notification DTO");
        };
        assert_eq!(converted.source_sequence, 3);
        assert_eq!(converted.projection_revision, 9);

        assert!(presentation_capture_event(&empty_presentation_event("future_stage")).is_none());
        assert!(
            presentation_capture_event(&empty_presentation_event("projection_changed")).is_none()
        );
    }

    #[test]
    fn render_conversion_preserves_order_roles_colors_and_initial_notification_revision() {
        let valid = || {
            presentation_identity_source(
                ActivityPresentationIdentityValidity::Valid,
                Some("identity"),
                8,
            )
        };
        let event = ActivityPresentationDiagnosticEvent {
            sequence: 13,
            elapsed_micros: 17,
            stage: "render_sample",
            projection_revision: Some(2),
            newest_lifecycle_sequence: None,
            total_row_count: None,
            running_row_count: None,
            finished_ok_row_count: None,
            finished_error_row_count: None,
            render_revision: Some(3),
            newest_notified_projection_revision: None,
            panel_visible: Some(true),
            selected_thread: Some(presentation_identity_source(
                ActivityPresentationIdentityValidity::Missing,
                None,
                0,
            )),
            selected_thread_row_count: Some(2),
            rendered_range_start: Some(4),
            rendered_range_end: Some(6),
            overscan_row_count: Some(1),
            sampled_rows: vec![
                ActivityPresentationRowSample {
                    rendered_index: 4,
                    thread: valid(),
                    turn: valid(),
                    item: valid(),
                    status: "running",
                    status_indicator_theme_role: "activity.indicator.running",
                    color_source: "theme_role",
                    resolved_rgba: [1, 2, 3, 4],
                },
                ActivityPresentationRowSample {
                    rendered_index: 5,
                    thread: valid(),
                    turn: valid(),
                    item: valid(),
                    status: "finished_error",
                    status_indicator_theme_role: "activity.indicator.error",
                    color_source: "renderer_fallback",
                    resolved_rgba: [5, 6, 7, 8],
                },
            ],
            row_sample_truncated: false,
            event_bytes: 900,
            event_bytes_truncated: false,
        };

        let ActivityDiagnosticCaptureEventV1::RenderSample(converted) =
            presentation_capture_event(&event).unwrap()
        else {
            panic!("expected render DTO");
        };
        assert_eq!(converted.source_sequence, 13);
        assert_eq!(converted.newest_notified_projection_revision, 0);
        assert_eq!(converted.sampled_rows.len(), 2);
        assert_eq!(converted.sampled_rows[0].rendered_index, 4);
        assert_eq!(
            converted.sampled_rows[0].row_status,
            ActivityDiagnosticRowStatusV1::Running
        );
        assert_eq!(
            converted.sampled_rows[0].color_source,
            ActivityDiagnosticColorSourceV1::ThemeRole
        );
        assert_eq!(converted.sampled_rows[0].resolved_rgba, [1, 2, 3, 4]);
        assert_eq!(converted.sampled_rows[1].rendered_index, 5);
        assert_eq!(
            converted.sampled_rows[1].row_status,
            ActivityDiagnosticRowStatusV1::FinishedError
        );
        assert_eq!(
            converted.sampled_rows[1].status_indicator_theme_role,
            ActivityDiagnosticIndicatorRoleV1::Error
        );
        assert_eq!(
            converted.sampled_rows[1].color_source,
            ActivityDiagnosticColorSourceV1::RendererFallback
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn enabled_lifecycle_observer_persists_without_a_ring_read() {
        use std::{
            fs, thread,
            time::{Duration, Instant},
        };

        use crate::activity_diagnostic_file_capture::{
            ActivityDiagnosticCaptureRuntimeState, ActivityDiagnosticFileCaptureController,
        };
        use crate::activity_lifecycle_diagnostics::{
            ActivityLifecycleDiagnosticInput, ActivityLifecycleDiagnostics,
        };

        let temp = tempfile::Builder::new()
            .prefix("activity-capture-fanout-")
            .tempdir()
            .unwrap();
        let home = crate::BerylHomeDir::from_explicit_path(temp.path()).unwrap();
        let controller = ActivityDiagnosticFileCaptureController::new(home).unwrap();
        controller.enable(Some("phase3-test")).unwrap();
        wait_for_capture_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);

        let observer = lifecycle_capture_observer(controller.sink());
        let mut diagnostics = ActivityLifecycleDiagnostics::with_observer(Some(observer));
        diagnostics.record(ActivityLifecycleDiagnosticInput {
            stage: "activity_ingress",
            category: "lifecycle",
            kind: "started",
            thread_id: Some("thread"),
            turn_id: Some("turn"),
            item_id: Some("item"),
            item_type: Some("commandExecution"),
            item_status: Some("inProgress"),
            projection_outcome: "inserted_running",
            before_row_status: None,
            after_row_status: Some("running"),
            affected_row_count: 1,
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.status().written_record_count == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(controller.status().written_record_count, 1);
        let current = temp
            .path()
            .join("diagnostics/activity-capture/activity.jsonl");
        let records = fs::read_to_string(current).unwrap();
        assert!(records.lines().any(|line| {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            record["recordKind"] == "lifecycle_event"
                && record["threadIdentity"]["value"] == "thread"
                && record["projectionOutcome"] == "inserted_running"
        }));

        controller.disable().unwrap();
        wait_for_capture_state(&controller, ActivityDiagnosticCaptureRuntimeState::Disabled);
        drop(diagnostics);
        drop(controller);
        temp.close().unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn enabled_presentation_observer_preserves_ring_and_persists_all_record_kinds() {
        use std::{
            fs, thread,
            time::{Duration, Instant},
        };

        use crate::activity_diagnostic_file_capture::{
            ActivityDiagnosticCaptureRuntimeState, ActivityDiagnosticFileCaptureController,
        };

        let temp = tempfile::Builder::new()
            .prefix("activity-capture-fanout-presentation-")
            .tempdir()
            .unwrap();
        let home = crate::BerylHomeDir::from_explicit_path(temp.path()).unwrap();
        let controller = ActivityDiagnosticFileCaptureController::new(home).unwrap();
        controller.enable(Some("phase3-test")).unwrap();
        wait_for_capture_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);

        let baseline = ActivityPresentationDiagnostics::default();
        let captured = ActivityPresentationDiagnostics::with_observer(Some(
            presentation_capture_observer(controller.sink()),
        ));
        observe_presentation_sequence(&baseline, 1);
        observe_presentation_sequence(&captured, 1);

        assert_eq!(
            presentation_snapshot_without_timing(captured.snapshot()),
            presentation_snapshot_without_timing(baseline.snapshot())
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.status().written_record_count < 3 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(controller.status().written_record_count, 3);

        controller.disable().unwrap();
        wait_for_capture_state(&controller, ActivityDiagnosticCaptureRuntimeState::Disabled);
        let records = fs::read_to_string(
            temp.path()
                .join("diagnostics/activity-capture/activity.jsonl"),
        )
        .unwrap();
        let record_kinds: Vec<_> = records
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter_map(|record| record["recordKind"].as_str().map(str::to_string))
            .collect();
        assert!(record_kinds.iter().any(|kind| kind == "projection_changed"));
        assert!(record_kinds.iter().any(|kind| kind == "shell_notified"));
        assert!(record_kinds.iter().any(|kind| kind == "render_sample"));

        drop(captured);
        drop(controller);
        temp.close().unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn disabled_saturated_and_failed_real_sinks_preserve_presentation_ring() {
        use std::{fs, thread, time::Duration};

        use crate::activity_diagnostic_file_capture::{
            ActivityDiagnosticCaptureRuntimeState, ActivityDiagnosticFileCaptureController,
        };

        let disabled_temp = tempfile::Builder::new()
            .prefix("activity-capture-fanout-disabled-")
            .tempdir()
            .unwrap();
        let disabled_home = crate::BerylHomeDir::from_explicit_path(disabled_temp.path()).unwrap();
        let disabled_controller =
            ActivityDiagnosticFileCaptureController::new(disabled_home).unwrap();
        let disabled_baseline = ActivityPresentationDiagnostics::default();
        let disabled_capture = ActivityPresentationDiagnostics::with_observer(Some(
            presentation_capture_observer(disabled_controller.sink()),
        ));
        observe_presentation_sequence(&disabled_baseline, 1);
        observe_presentation_sequence(&disabled_capture, 1);
        assert_eq!(
            presentation_snapshot_without_timing(disabled_capture.snapshot()),
            presentation_snapshot_without_timing(disabled_baseline.snapshot())
        );
        assert_eq!(disabled_controller.status().written_record_count, 0);
        assert!(!disabled_temp.path().join("diagnostics").exists());

        let saturated_temp = tempfile::Builder::new()
            .prefix("activity-capture-fanout-saturated-")
            .tempdir()
            .unwrap();
        let saturated_home =
            crate::BerylHomeDir::from_explicit_path(saturated_temp.path()).unwrap();
        let saturated_controller =
            ActivityDiagnosticFileCaptureController::with_queue_capacity(saturated_home, 1)
                .unwrap();
        saturated_controller.enable(None).unwrap();
        wait_for_capture_state(
            &saturated_controller,
            ActivityDiagnosticCaptureRuntimeState::Active,
        );
        let saturated_baseline = ActivityPresentationDiagnostics::default();
        let saturated_capture = ActivityPresentationDiagnostics::with_observer(Some(
            presentation_capture_observer(saturated_controller.sink()),
        ));
        for revision in 1..=50_000 {
            saturated_baseline.observe_projection_change(projection_state(revision));
            saturated_capture.observe_projection_change(projection_state(revision));
            if saturated_controller.status().queue_full_drop_count > 0 {
                break;
            }
        }
        assert!(saturated_controller.status().queue_full_drop_count > 0);
        assert_eq!(
            presentation_snapshot_without_timing(saturated_capture.snapshot()),
            presentation_snapshot_without_timing(saturated_baseline.snapshot())
        );
        saturated_controller.disable().unwrap();
        wait_for_capture_state(
            &saturated_controller,
            ActivityDiagnosticCaptureRuntimeState::Disabled,
        );

        let failed_temp = tempfile::Builder::new()
            .prefix("activity-capture-fanout-failed-")
            .tempdir()
            .unwrap();
        let failed_current = failed_temp
            .path()
            .join("diagnostics/activity-capture/activity.jsonl");
        fs::create_dir_all(&failed_current).unwrap();
        let failed_home = crate::BerylHomeDir::from_explicit_path(failed_temp.path()).unwrap();
        let failed_controller = ActivityDiagnosticFileCaptureController::new(failed_home).unwrap();
        failed_controller.enable(None).unwrap();
        wait_for_capture_state(
            &failed_controller,
            ActivityDiagnosticCaptureRuntimeState::Failed,
        );
        let failed_baseline = ActivityPresentationDiagnostics::default();
        let failed_capture = ActivityPresentationDiagnostics::with_observer(Some(
            presentation_capture_observer(failed_controller.sink()),
        ));
        observe_presentation_sequence(&failed_baseline, 1);
        observe_presentation_sequence(&failed_capture, 1);
        assert_eq!(
            presentation_snapshot_without_timing(failed_capture.snapshot()),
            presentation_snapshot_without_timing(failed_baseline.snapshot())
        );
        assert_eq!(failed_controller.status().written_record_count, 0);
        assert!(failed_current.is_dir());
        failed_controller.disable().unwrap();
        wait_for_capture_state(
            &failed_controller,
            ActivityDiagnosticCaptureRuntimeState::Disabled,
        );

        drop(disabled_capture);
        drop(disabled_controller);
        drop(saturated_capture);
        drop(saturated_controller);
        drop(failed_capture);
        drop(failed_controller);
        thread::sleep(Duration::from_millis(10));
        disabled_temp.close().unwrap();
        saturated_temp.close().unwrap();
        failed_temp.close().unwrap();
    }

    #[cfg(target_os = "windows")]
    fn wait_for_capture_state(
        controller: &crate::ActivityDiagnosticFileCaptureController,
        expected: crate::ActivityDiagnosticCaptureRuntimeState,
    ) {
        use std::{
            thread,
            time::{Duration, Instant},
        };

        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.status().runtime_state != expected && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(controller.status().runtime_state, expected);
    }
}
