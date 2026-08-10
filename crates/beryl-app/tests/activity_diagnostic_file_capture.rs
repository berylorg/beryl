use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use beryl_app::{
    ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY, ACTIVITY_CAPTURE_TOTAL_DATA_BYTE_CAPACITY,
    ActivityDiagnosticCaptureErrorCategory, ActivityDiagnosticCaptureEventV1,
    ActivityDiagnosticCaptureRuntimeState, ActivityDiagnosticCaptureSubmitOutcome,
    ActivityDiagnosticColorSourceV1, ActivityDiagnosticFileCaptureController,
    ActivityDiagnosticIdentityV1, ActivityDiagnosticIndicatorRoleV1,
    ActivityDiagnosticLifecycleCategoryV1, ActivityDiagnosticLifecycleEventV1,
    ActivityDiagnosticLifecycleKindV1, ActivityDiagnosticLifecycleStageV1,
    ActivityDiagnosticProjectionChangedV1, ActivityDiagnosticProjectionOutcomeV1,
    ActivityDiagnosticProtocolStringV1, ActivityDiagnosticRenderRowV1,
    ActivityDiagnosticRenderSampleV1, ActivityDiagnosticRowStatusV1,
    ActivityDiagnosticShellNotifiedV1, BerylHomeDir,
};

#[path = "support/tempdir.rs"]
mod tempdir;

const WAIT_LIMIT: Duration = Duration::from_secs(5);

struct CaptureFixture {
    temp: tempdir::TestTempDir,
    home: BerylHomeDir,
}

impl CaptureFixture {
    fn new(name: &str) -> Self {
        let temp = tempdir::temp_dir(name);
        let home = BerylHomeDir::from_explicit_path(temp.path()).unwrap();
        Self { temp, home }
    }

    fn capture_dir(&self) -> PathBuf {
        self.temp.join("diagnostics/activity-capture")
    }

    fn current(&self) -> PathBuf {
        self.capture_dir().join("activity.jsonl")
    }

    fn previous(&self) -> PathBuf {
        self.capture_dir().join("activity.previous.jsonl")
    }

    fn lock(&self) -> PathBuf {
        self.capture_dir().join("activity.lock")
    }
}

fn wait_for_state(
    controller: &ActivityDiagnosticFileCaptureController,
    expected: ActivityDiagnosticCaptureRuntimeState,
) {
    let deadline = Instant::now() + WAIT_LIMIT;
    while Instant::now() < deadline {
        if controller.status().runtime_state == expected {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!(
        "capture did not reach {expected:?}: {:?}",
        controller.status()
    );
}

fn disable_and_wait(controller: &ActivityDiagnosticFileCaptureController) {
    controller.disable().unwrap();
    wait_for_state(controller, ActivityDiagnosticCaptureRuntimeState::Disabled);
}

fn shell_event(source_sequence: u64) -> ActivityDiagnosticCaptureEventV1 {
    ActivityDiagnosticCaptureEventV1::ShellNotified(ActivityDiagnosticShellNotifiedV1 {
        source_sequence,
        elapsed_micros: source_sequence,
        projection_revision: source_sequence,
    })
}

fn maximum_render_event(source_sequence: u64) -> ActivityDiagnosticCaptureEventV1 {
    let identity = "x".repeat(512);
    let sampled_rows = (0..32)
        .map(|rendered_index| ActivityDiagnosticRenderRowV1 {
            rendered_index,
            thread_identity: ActivityDiagnosticIdentityV1::capture(Some(&identity)),
            turn_identity: ActivityDiagnosticIdentityV1::capture(Some(&identity)),
            item_identity: ActivityDiagnosticIdentityV1::capture(Some(&identity)),
            row_status: ActivityDiagnosticRowStatusV1::Running,
            status_indicator_theme_role: ActivityDiagnosticIndicatorRoleV1::Running,
            color_source: ActivityDiagnosticColorSourceV1::RendererFallback,
            resolved_rgba: [0, 0, 0, 0],
        })
        .collect();
    ActivityDiagnosticCaptureEventV1::RenderSample(ActivityDiagnosticRenderSampleV1 {
        source_sequence,
        elapsed_micros: source_sequence,
        render_revision: source_sequence,
        projection_revision: source_sequence,
        newest_notified_projection_revision: source_sequence,
        panel_visible: true,
        selected_thread_identity: Some(ActivityDiagnosticIdentityV1::capture(Some(&identity))),
        selected_thread_row_count: 32,
        rendered_range_start: 0,
        rendered_range_end: 32,
        overscan_row_count: 0,
        sampled_rows,
        row_sample_truncated: false,
        event_bytes: 0,
        event_bytes_truncated: false,
    })
}

fn wait_for_written(controller: &ActivityDiagnosticFileCaptureController, minimum: u64) {
    let deadline = Instant::now() + WAIT_LIMIT;
    while Instant::now() < deadline {
        if controller.status().written_record_count >= minimum {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!(
        "capture did not write {minimum} records: {:?}",
        controller.status()
    );
}

fn json_lines(path: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn all_json_lines(fixture: &CaptureFixture) -> Vec<serde_json::Value> {
    let mut records = if fixture.previous().is_file() {
        json_lines(&fixture.previous())
    } else {
        Vec::new()
    };
    if fixture.current().is_file() {
        records.extend(json_lines(&fixture.current()));
    }
    records
}

fn observable_complete_json_lines(path: &Path) -> Vec<serde_json::Value> {
    fs::read(path)
        .unwrap_or_default()
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| line.last() == Some(&b'\n') && line.len() > 1)
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect()
}

fn all_observable_complete_json_lines(fixture: &CaptureFixture) -> Vec<serde_json::Value> {
    let mut records = observable_complete_json_lines(&fixture.previous());
    records.extend(observable_complete_json_lines(&fixture.current()));
    records
}

fn sorted_keys(value: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

fn wait_for_record_kind(fixture: &CaptureFixture, record_kind: &str) {
    let deadline = Instant::now() + WAIT_LIMIT;
    while Instant::now() < deadline {
        if all_observable_complete_json_lines(fixture)
            .iter()
            .any(|record| record["recordKind"] == record_kind)
        {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("capture did not write record kind {record_kind}");
}

fn make_valid_segment(name: &str) -> Vec<u8> {
    let fixture = CaptureFixture::new(name);
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    fs::read(fixture.current()).unwrap()
}

#[test]
#[cfg(target_os = "windows")]
fn fixed_paths_are_used_and_capture_is_default_off() {
    let fixture = CaptureFixture::new("activity-capture-paths");
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    assert_eq!(
        controller.status().runtime_state,
        ActivityDiagnosticCaptureRuntimeState::Disabled
    );
    assert!(!fixture.capture_dir().exists());

    controller.enable(Some("test-build")).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    assert!(fixture.current().is_file());
    assert!(fixture.lock().is_file());
    assert!(!fixture.previous().exists());
    let mut names: Vec<_> = fs::read_dir(fixture.capture_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(names, ["activity.jsonl", "activity.lock"]);
    disable_and_wait(&controller);
}

#[test]
#[cfg(target_os = "windows")]
fn lifecycle_serialization_is_closed_and_content_free() {
    let fixture = CaptureFixture::new("activity-capture-privacy");
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    let event = ActivityDiagnosticLifecycleEventV1 {
        source_sequence: 7,
        elapsed_micros: 11,
        stage: ActivityDiagnosticLifecycleStageV1::ActivityIngress,
        category: ActivityDiagnosticLifecycleCategoryV1::Lifecycle,
        kind: ActivityDiagnosticLifecycleKindV1::Started,
        thread_identity: ActivityDiagnosticIdentityV1::capture(Some("thread-exact")),
        turn_identity: ActivityDiagnosticIdentityV1::capture(None),
        item_identity: ActivityDiagnosticIdentityV1::capture(Some("item-exact")),
        item_type: ActivityDiagnosticProtocolStringV1::capture(Some("tool")),
        item_status: ActivityDiagnosticProtocolStringV1::capture(None),
        projection_outcome: ActivityDiagnosticProjectionOutcomeV1::InsertedRunning,
        before_row_status: None,
        after_row_status: Some(ActivityDiagnosticRowStatusV1::Running),
        affected_row_count: 1,
    };
    assert_eq!(
        controller
            .sink()
            .try_record(ActivityDiagnosticCaptureEventV1::Lifecycle(event)),
        ActivityDiagnosticCaptureSubmitOutcome::Enqueued
    );
    wait_for_written(&controller, 1);
    disable_and_wait(&controller);

    let record = json_lines(&fixture.current())
        .into_iter()
        .find(|value| value["recordKind"] == "lifecycle_event")
        .unwrap();
    let mut keys: Vec<_> = record
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "affectedRowCount",
            "afterRowStatus",
            "captureGeneration",
            "captureSequence",
            "category",
            "elapsedMicros",
            "itemIdentity",
            "itemStatus",
            "itemType",
            "kind",
            "projectionOutcome",
            "recordKind",
            "schemaVersion",
            "sourceSequence",
            "stage",
            "threadIdentity",
            "turnIdentity",
        ]
    );
    let encoded = serde_json::to_string(&record).unwrap();
    for forbidden in [
        "command",
        "workingDirectory",
        "toolArguments",
        "toolOutput",
        "prompt",
        "message",
        "reasoning",
        "modelMetadata",
        "themeName",
        "backendResponse",
        "path",
        "errorText",
    ] {
        assert!(!encoded.contains(forbidden));
    }
}

#[test]
#[cfg(target_os = "windows")]
fn every_other_v1_record_shape_has_only_allowlisted_fields() {
    let fixture = CaptureFixture::new("activity-capture-closed-shapes");
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(Some("closed-shape-build")).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    let events = [
        ActivityDiagnosticCaptureEventV1::ProjectionChanged(
            ActivityDiagnosticProjectionChangedV1 {
                source_sequence: 1,
                elapsed_micros: 2,
                projection_revision: 3,
                newest_lifecycle_sequence: Some(4),
                total_row_count: 3,
                running_row_count: 1,
                finished_ok_row_count: 1,
                finished_error_row_count: 1,
            },
        ),
        shell_event(5),
        ActivityDiagnosticCaptureEventV1::RenderSample(ActivityDiagnosticRenderSampleV1 {
            source_sequence: 6,
            elapsed_micros: 7,
            render_revision: 8,
            projection_revision: 9,
            newest_notified_projection_revision: 9,
            panel_visible: true,
            selected_thread_identity: Some(ActivityDiagnosticIdentityV1::capture(Some("thread"))),
            selected_thread_row_count: 1,
            rendered_range_start: 0,
            rendered_range_end: 1,
            overscan_row_count: 2,
            sampled_rows: vec![ActivityDiagnosticRenderRowV1 {
                rendered_index: 0,
                thread_identity: ActivityDiagnosticIdentityV1::capture(Some("thread")),
                turn_identity: ActivityDiagnosticIdentityV1::capture(Some("turn")),
                item_identity: ActivityDiagnosticIdentityV1::capture(Some("item")),
                row_status: ActivityDiagnosticRowStatusV1::FinishedOk,
                status_indicator_theme_role: ActivityDiagnosticIndicatorRoleV1::Ok,
                color_source: ActivityDiagnosticColorSourceV1::ThemeRole,
                resolved_rgba: [1, 2, 3, 4],
            }],
            row_sample_truncated: false,
            event_bytes: 10,
            event_bytes_truncated: false,
        }),
    ];
    for event in events {
        assert_eq!(
            controller.sink().try_record(event),
            ActivityDiagnosticCaptureSubmitOutcome::Enqueued
        );
    }
    wait_for_written(&controller, 3);
    disable_and_wait(&controller);

    let records = all_json_lines(&fixture);
    let header = records
        .iter()
        .find(|record| record["recordKind"] == "segment_header")
        .unwrap();
    assert_eq!(
        sorted_keys(header),
        [
            "buildIdentity",
            "captureGeneration",
            "processId",
            "recordKind",
            "schemaVersion",
            "segmentSequence",
            "sessionId",
            "startedUnixMillis"
        ]
    );
    let projection = records
        .iter()
        .find(|record| record["recordKind"] == "projection_changed")
        .unwrap();
    assert_eq!(
        sorted_keys(projection),
        [
            "captureGeneration",
            "captureSequence",
            "elapsedMicros",
            "finishedErrorRowCount",
            "finishedOkRowCount",
            "newestLifecycleSequence",
            "projectionRevision",
            "recordKind",
            "runningRowCount",
            "schemaVersion",
            "sourceSequence",
            "totalRowCount"
        ]
    );
    let shell = records
        .iter()
        .find(|record| record["recordKind"] == "shell_notified")
        .unwrap();
    assert_eq!(
        sorted_keys(shell),
        [
            "captureGeneration",
            "captureSequence",
            "elapsedMicros",
            "projectionRevision",
            "recordKind",
            "schemaVersion",
            "sourceSequence"
        ]
    );
    let render = records
        .iter()
        .find(|record| record["recordKind"] == "render_sample")
        .unwrap();
    assert_eq!(
        sorted_keys(render),
        [
            "captureGeneration",
            "captureSequence",
            "elapsedMicros",
            "eventBytes",
            "eventBytesTruncated",
            "newestNotifiedProjectionRevision",
            "overscanRowCount",
            "panelVisible",
            "projectionRevision",
            "recordKind",
            "renderRevision",
            "renderedRangeEnd",
            "renderedRangeStart",
            "rowSampleTruncated",
            "sampledRows",
            "schemaVersion",
            "selectedThreadIdentity",
            "selectedThreadRowCount",
            "sourceSequence"
        ]
    );
    assert_eq!(
        sorted_keys(&render["sampledRows"][0]),
        [
            "colorSource",
            "itemIdentity",
            "renderedIndex",
            "resolvedRgba",
            "rowStatus",
            "statusIndicatorThemeRole",
            "threadIdentity",
            "turnIdentity"
        ]
    );
    assert_eq!(
        sorted_keys(&render["sampledRows"][0]["threadIdentity"]),
        ["originalByteCount", "validity", "value"]
    );

    for forbidden in [
        "command",
        "workingDirectory",
        "toolArguments",
        "toolOutput",
        "displayValue",
        "agentPath",
        "prompt",
        "message",
        "reasoning",
        "modelMetadata",
        "themeName",
        "themeDocument",
        "backendResponse",
        "pixelData",
        "imageData",
        "errorText",
    ] {
        assert!(!serde_json::to_string(&records).unwrap().contains(forbidden));
    }
}

#[test]
#[cfg(target_os = "windows")]
fn contention_is_unavailable_and_does_not_mutate_segments() {
    let fixture = CaptureFixture::new("activity-capture-contention");
    let valid = make_valid_segment("activity-capture-contention-seed");
    fs::create_dir_all(fixture.capture_dir()).unwrap();
    fs::write(fixture.previous(), &valid).unwrap();
    fs::write(fixture.current(), &valid).unwrap();
    let first = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    first.enable(None).unwrap();
    wait_for_state(&first, ActivityDiagnosticCaptureRuntimeState::Active);
    let current_before = fs::read(fixture.current()).unwrap();
    let previous_before = fs::read(fixture.previous()).unwrap();

    let second = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    second.enable(None).unwrap();
    wait_for_state(&second, ActivityDiagnosticCaptureRuntimeState::Unavailable);
    assert_eq!(fs::read(fixture.current()).unwrap(), current_before);
    assert_eq!(fs::read(fixture.previous()).unwrap(), previous_before);

    disable_and_wait(&second);
    disable_and_wait(&first);
}

#[test]
#[cfg(target_os = "windows")]
fn rapid_controls_publish_only_the_latest_generation() {
    let fixture = CaptureFixture::new("activity-capture-generation");
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    let first = controller.enable(None).unwrap();
    let second = controller.enable(None).unwrap();
    assert!(second > first);
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    assert_eq!(controller.status().capture_generation, second);
    assert_eq!(
        controller.sink().try_record(shell_event(1)),
        ActivityDiagnosticCaptureSubmitOutcome::Enqueued
    );
    wait_for_written(&controller, 1);
    disable_and_wait(&controller);
    for record in json_lines(&fixture.current()) {
        if record["recordKind"] != "segment_header" {
            assert_eq!(record["captureGeneration"], second);
        }
    }
}

#[test]
#[cfg(target_os = "windows")]
fn immediate_disable_cannot_be_overwritten_by_startup() {
    let fixture = CaptureFixture::new("activity-capture-disable-race");
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    controller.disable().unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Disabled);
    thread::sleep(Duration::from_millis(25));
    assert_eq!(
        controller.status().runtime_state,
        ActivityDiagnosticCaptureRuntimeState::Disabled
    );
    assert!(!controller.status().configured);
}

#[test]
#[cfg(target_os = "windows")]
fn invalid_v1_shape_is_rejected_before_enqueue() {
    let fixture = CaptureFixture::new("activity-capture-schema-rejection");
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    let invalid = ActivityDiagnosticCaptureEventV1::ProjectionChanged(
        ActivityDiagnosticProjectionChangedV1 {
            source_sequence: 1,
            elapsed_micros: 2,
            projection_revision: 3,
            newest_lifecycle_sequence: None,
            total_row_count: 1,
            running_row_count: 1,
            finished_ok_row_count: 1,
            finished_error_row_count: 0,
        },
    );
    assert_eq!(
        controller.sink().try_record(invalid),
        ActivityDiagnosticCaptureSubmitOutcome::SchemaRejected
    );
    assert_eq!(controller.status().schema_rejection_drop_count, 1);
    assert_eq!(controller.status().dropped_record_count, 1);
    disable_and_wait(&controller);
    assert!(
        all_json_lines(&fixture)
            .iter()
            .all(|record| record["recordKind"] == "segment_header")
    );
}

#[test]
#[cfg(target_os = "windows")]
fn saturated_queue_is_nonblocking_and_gap_eventually_accounts_for_drops() {
    let fixture = CaptureFixture::new("activity-capture-queue-gap");
    let controller =
        ActivityDiagnosticFileCaptureController::with_queue_capacity(fixture.home.clone(), 2)
            .unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);

    let mut observed_full = 0_u64;
    for source_sequence in 0..50_000 {
        if controller.sink().try_record(shell_event(source_sequence))
            == ActivityDiagnosticCaptureSubmitOutcome::QueueFull
        {
            observed_full += 1;
        }
    }
    assert!(observed_full > 0);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        controller.sink().try_record(shell_event(999_999)),
        ActivityDiagnosticCaptureSubmitOutcome::Enqueued
    );
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        controller.sink().try_record(shell_event(1_000_000)),
        ActivityDiagnosticCaptureSubmitOutcome::Enqueued
    );
    wait_for_record_kind(&fixture, "capture_gap");
    let status = controller.status();
    disable_and_wait(&controller);

    let gaps: Vec<_> = all_json_lines(&fixture)
        .into_iter()
        .filter(|record| record["recordKind"] == "capture_gap")
        .collect();
    let accounted_full: u64 = gaps
        .iter()
        .map(|record| record["queueFullDropCount"].as_u64().unwrap())
        .sum();
    assert!(accounted_full > 0);
    assert!(accounted_full <= status.queue_full_drop_count);
    assert_eq!(status.queue_full_drop_count, observed_full);
    assert_eq!(status.queue_disconnected_drop_count, 0);
    assert!(gaps.iter().all(|record| {
        record["firstDroppedCaptureSequence"].as_u64().unwrap()
            <= record["lastDroppedCaptureSequence"].as_u64().unwrap()
    }));
    assert!(gaps.iter().all(|record| sorted_keys(record)
        == [
            "captureGeneration",
            "captureSequence",
            "firstDroppedCaptureSequence",
            "lastDroppedCaptureSequence",
            "queueDisconnectedDropCount",
            "queueFullDropCount",
            "recordKind",
            "schemaVersion",
        ]));
}

#[test]
#[cfg(target_os = "windows")]
fn startup_filesystem_failure_fails_closed_without_replacing_the_target() {
    let fixture = CaptureFixture::new("activity-capture-writer-failure");
    fs::create_dir_all(fixture.current()).unwrap();
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Failed);
    let status = controller.status();
    assert!(status.configured);
    assert_eq!(
        status.error_category,
        Some(ActivityDiagnosticCaptureErrorCategory::Recovery)
    );
    assert_eq!(
        controller.sink().try_record(shell_event(1)),
        ActivityDiagnosticCaptureSubmitOutcome::Disabled
    );
    assert!(fixture.current().is_dir());
    assert!(!fixture.previous().exists());
    disable_and_wait(&controller);
}

#[test]
#[cfg(target_os = "windows")]
fn active_rotation_failure_fails_generation_closed_without_partial_record() {
    let fixture = CaptureFixture::new("activity-capture-active-rotation-failure");
    let valid = make_valid_segment("activity-capture-active-rotation-seed");
    fs::create_dir_all(fixture.capture_dir()).unwrap();
    fs::write(fixture.current(), valid).unwrap();
    extend_with_newlines(
        &fixture.current(),
        ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY - 4 * 1024,
    );
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    let before = fs::read(fixture.current()).unwrap();
    fs::create_dir(fixture.previous()).unwrap();

    assert_eq!(
        controller.sink().try_record(maximum_render_event(1)),
        ActivityDiagnosticCaptureSubmitOutcome::Enqueued
    );
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Failed);
    let status = controller.status();
    assert!(status.configured);
    assert_eq!(
        status.error_category,
        Some(ActivityDiagnosticCaptureErrorCategory::Rotation)
    );
    assert_eq!(
        controller.sink().try_record(shell_event(2)),
        ActivityDiagnosticCaptureSubmitOutcome::Disabled
    );
    assert_eq!(fs::read(fixture.current()).unwrap(), before);
    assert!(fixture.previous().is_dir());
    disable_and_wait(&controller);
}

#[test]
#[cfg(target_os = "windows")]
fn recovery_matrix_repairs_tails_and_resolves_partial_rotation_states() {
    let fixture = CaptureFixture::new("activity-capture-recovery");
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);

    OpenOptions::new()
        .append(true)
        .open(fixture.current())
        .unwrap()
        .write_all(b"torn-private-tail")
        .unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    let repaired = fs::read(fixture.current()).unwrap();
    assert_eq!(repaired.last(), Some(&b'\n'));
    assert!(!String::from_utf8_lossy(&repaired).contains("torn-private-tail"));

    fs::write(fixture.previous(), b"{}\n").unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    assert!(!fixture.previous().exists());

    fs::copy(fixture.current(), fixture.previous()).unwrap();
    fs::write(fixture.current(), b"not-json\n").unwrap();
    let previous_before = fs::read(fixture.previous()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    assert_eq!(fs::read(fixture.previous()).unwrap(), previous_before);
    assert_eq!(
        json_lines(&fixture.current())[0]["recordKind"],
        "segment_header"
    );

    fs::remove_file(fixture.current()).unwrap();
    let previous_before = fs::read(fixture.previous()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    assert_eq!(fs::read(fixture.previous()).unwrap(), previous_before);
    assert!(fixture.current().is_file());

    OpenOptions::new()
        .write(true)
        .open(fixture.previous())
        .unwrap()
        .set_len(ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY + 1)
        .unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    assert!(!fixture.previous().exists());
    assert!(controller.status().repair_count >= 4);
}

#[test]
#[cfg(target_os = "windows")]
fn recovery_accepts_every_valid_segment_presence_and_repairs_both_tails() {
    let valid = make_valid_segment("activity-capture-valid-segment-source");

    let current_only = CaptureFixture::new("activity-capture-current-only");
    fs::create_dir_all(current_only.capture_dir()).unwrap();
    fs::write(current_only.current(), &valid).unwrap();
    let controller =
        ActivityDiagnosticFileCaptureController::new(current_only.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    assert!(
        fs::read(current_only.current())
            .unwrap()
            .starts_with(&valid)
    );
    assert!(!current_only.previous().exists());

    let previous_only = CaptureFixture::new("activity-capture-previous-only");
    fs::create_dir_all(previous_only.capture_dir()).unwrap();
    fs::write(previous_only.previous(), &valid).unwrap();
    let controller =
        ActivityDiagnosticFileCaptureController::new(previous_only.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    assert_eq!(fs::read(previous_only.previous()).unwrap(), valid);
    assert_eq!(
        json_lines(&previous_only.current())[0]["recordKind"],
        "segment_header"
    );

    let two_valid = CaptureFixture::new("activity-capture-two-valid");
    fs::create_dir_all(two_valid.capture_dir()).unwrap();
    fs::write(two_valid.previous(), &valid).unwrap();
    fs::write(two_valid.current(), &valid).unwrap();
    let controller = ActivityDiagnosticFileCaptureController::new(two_valid.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    assert_eq!(fs::read(two_valid.previous()).unwrap(), valid);
    assert!(fs::read(two_valid.current()).unwrap().starts_with(&valid));

    let torn = CaptureFixture::new("activity-capture-both-torn");
    fs::create_dir_all(torn.capture_dir()).unwrap();
    let mut torn_bytes = valid.clone();
    torn_bytes.extend_from_slice(b"private-torn-tail");
    fs::write(torn.previous(), &torn_bytes).unwrap();
    fs::write(torn.current(), &torn_bytes).unwrap();
    let controller = ActivityDiagnosticFileCaptureController::new(torn.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    assert_eq!(fs::read(torn.previous()).unwrap(), valid);
    let repaired_current = fs::read(torn.current()).unwrap();
    assert!(repaired_current.starts_with(&valid));
    assert!(!String::from_utf8_lossy(&repaired_current).contains("private-torn-tail"));
    assert!(controller.status().repair_count >= 2);
}

#[test]
#[cfg(target_os = "windows")]
fn recovery_deletes_bad_previous_and_replaces_bad_current() {
    for (name, oversized) in [
        ("activity-capture-both-unusable", false),
        ("activity-capture-both-oversized", true),
    ] {
        let fixture = CaptureFixture::new(name);
        fs::create_dir_all(fixture.capture_dir()).unwrap();
        if oversized {
            for path in [fixture.previous(), fixture.current()] {
                let file = OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(path)
                    .unwrap();
                file.set_len(ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY + 1)
                    .unwrap();
            }
        } else {
            fs::write(fixture.previous(), b"{\"recordKind\":\"unknown\"}\n").unwrap();
            fs::write(fixture.current(), b"not-json\n").unwrap();
        }

        let controller =
            ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
        controller.enable(None).unwrap();
        wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
        disable_and_wait(&controller);
        assert!(!fixture.previous().exists());
        assert!(
            fs::metadata(fixture.current()).unwrap().len()
                <= ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY
        );
        assert_eq!(
            json_lines(&fixture.current())[0]["recordKind"],
            "segment_header"
        );
        assert!(controller.status().repair_count >= 2);
    }
}

#[test]
#[cfg(target_os = "windows")]
fn queued_records_from_an_old_generation_never_cross_the_new_header() {
    let fixture = CaptureFixture::new("activity-capture-generation-isolation");
    let controller =
        ActivityDiagnosticFileCaptureController::with_queue_capacity(fixture.home.clone(), 1)
            .unwrap();
    let first = controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    for source_sequence in 0..10_000 {
        let _ = controller.sink().try_record(shell_event(source_sequence));
    }

    let second = controller.enable(None).unwrap();
    assert!(second > first);
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    thread::sleep(Duration::from_millis(50));
    let enqueue_deadline = Instant::now() + WAIT_LIMIT;
    loop {
        if controller.sink().try_record(shell_event(999_999))
            == ActivityDiagnosticCaptureSubmitOutcome::Enqueued
        {
            break;
        }
        assert!(
            Instant::now() < enqueue_deadline,
            "new-generation marker was not admitted"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let deadline = Instant::now() + WAIT_LIMIT;
    while Instant::now() < deadline {
        if all_observable_complete_json_lines(&fixture)
            .iter()
            .any(|record| record["sourceSequence"] == 999_999)
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    disable_and_wait(&controller);

    let records = all_json_lines(&fixture);
    let new_header = records
        .iter()
        .rposition(|record| {
            record["recordKind"] == "segment_header"
                && record["captureGeneration"].as_u64() == Some(second)
        })
        .unwrap();
    assert!(records[new_header + 1..].iter().all(|record| {
        record["recordKind"] == "segment_header"
            || record["captureGeneration"].as_u64() == Some(second)
    }));
    assert!(
        records
            .iter()
            .any(|record| record["sourceSequence"] == 999_999)
    );
}

#[test]
#[cfg(target_os = "windows")]
fn capture_sequences_are_strictly_monotonic_for_admitted_records() {
    let fixture = CaptureFixture::new("activity-capture-sequence-order");
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    for source_sequence in 0..64 {
        assert_eq!(
            controller.sink().try_record(shell_event(source_sequence)),
            ActivityDiagnosticCaptureSubmitOutcome::Enqueued
        );
    }
    wait_for_written(&controller, 64);
    disable_and_wait(&controller);
    let sequences: Vec<_> = all_json_lines(&fixture)
        .iter()
        .filter_map(|record| record["captureSequence"].as_u64())
        .collect();
    assert_eq!(sequences.len(), 64);
    assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
}

fn extend_with_newlines(path: &Path, target_len: u64) {
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    let mut remaining = target_len - file.metadata().unwrap().len();
    let chunk = vec![b'\n'; 64 * 1024];
    while remaining > 0 {
        let count = remaining.min(chunk.len() as u64) as usize;
        file.write_all(&chunk[..count]).unwrap();
        remaining -= count as u64;
    }
}

#[test]
#[cfg(target_os = "windows")]
fn ordinary_event_rotation_writes_the_crossing_record_whole() {
    let fixture = CaptureFixture::new("activity-capture-ordinary-rotation");
    let valid = make_valid_segment("activity-capture-ordinary-rotation-seed");
    fs::create_dir_all(fixture.capture_dir()).unwrap();
    fs::write(fixture.current(), valid).unwrap();
    extend_with_newlines(
        &fixture.current(),
        ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY - 4 * 1024,
    );
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    let previous_bytes = fs::read(fixture.current()).unwrap();

    assert_eq!(
        controller.sink().try_record(maximum_render_event(77)),
        ActivityDiagnosticCaptureSubmitOutcome::Enqueued
    );
    wait_for_written(&controller, 1);
    disable_and_wait(&controller);

    assert_eq!(fs::read(fixture.previous()).unwrap(), previous_bytes);
    let current_bytes = fs::read(fixture.current()).unwrap();
    assert_eq!(current_bytes.last(), Some(&b'\n'));
    assert!(current_bytes.len() as u64 <= ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY);
    let records = json_lines(&fixture.current());
    assert_eq!(records[0]["recordKind"], "segment_header");
    let event = records
        .iter()
        .find(|record| record["recordKind"] == "render_sample")
        .unwrap();
    assert_eq!(event["sourceSequence"], 77);
    assert_eq!(event["sampledRows"].as_array().unwrap().len(), 32);
    assert_eq!(controller.status().rotation_count, 1);
}

#[test]
#[cfg(target_os = "windows")]
fn repeated_rotation_preserves_whole_records_and_exact_data_caps() {
    let fixture = CaptureFixture::new("activity-capture-rotation");
    let controller = ActivityDiagnosticFileCaptureController::new(fixture.home.clone()).unwrap();
    controller.enable(None).unwrap();
    wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
    disable_and_wait(&controller);
    extend_with_newlines(&fixture.current(), ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY);
    assert_eq!(
        fs::metadata(fixture.current()).unwrap().len(),
        ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY
    );

    for rotation in 1..=3 {
        controller.enable(None).unwrap();
        wait_for_state(&controller, ActivityDiagnosticCaptureRuntimeState::Active);
        disable_and_wait(&controller);
        let previous_len = fs::metadata(fixture.previous()).unwrap().len();
        let current_len = fs::metadata(fixture.current()).unwrap().len();
        assert_eq!(ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY, 10 * 1024 * 1024);
        assert_eq!(ACTIVITY_CAPTURE_TOTAL_DATA_BYTE_CAPACITY, 20 * 1024 * 1024);
        assert_eq!(previous_len, ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY);
        assert!(current_len <= ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY);
        assert!(previous_len + current_len <= ACTIVITY_CAPTURE_TOTAL_DATA_BYTE_CAPACITY);
        assert_eq!(fs::read(fixture.previous()).unwrap().last(), Some(&b'\n'));
        assert_eq!(fs::read(fixture.current()).unwrap().last(), Some(&b'\n'));
        assert_eq!(
            json_lines(&fixture.current())[0]["recordKind"],
            "segment_header"
        );
        if rotation < 3 {
            extend_with_newlines(&fixture.current(), ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY);
        }
    }
    assert!(controller.status().rotation_count >= 3);
}
