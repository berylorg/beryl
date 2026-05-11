#![allow(dead_code)]

#[path = "../src/shell/surface_notice.rs"]
mod surface_notice;

use beryl_backend::{TurnError, TurnInfo, TurnStatus};
use surface_notice::{
    MAX_REPORTED_SURFACE_NOTICE_SOURCE_KEYS, SurfaceNotice, SurfaceNoticeQueue,
    SurfaceNoticeSourceKey, backend_turn_error_detail, local_turn_failure_notice,
    selected_backend_turn_error_notice,
};

#[test]
fn surface_notice_queue_advances_after_active_dismissal() {
    let mut queue = SurfaceNoticeQueue::default();
    assert!(queue.push(SurfaceNotice::new("First", "one")));
    assert!(queue.push(SurfaceNotice::new("Second", "two")));

    assert_eq!(queue.active().map(SurfaceNotice::title), Some("First"));
    assert!(queue.dismiss_active());
    assert_eq!(queue.active().map(SurfaceNotice::title), Some("Second"));
    assert!(queue.dismiss_active());
    assert!(queue.active().is_none());
}

#[test]
fn turn_error_source_key_deduplicates_repeated_failure_reports() {
    let mut queue = SurfaceNoticeQueue::default();
    let key = SurfaceNoticeSourceKey::TurnError {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
    };

    assert!(queue.push(SurfaceNotice::turn_error("first detail", key.clone())));
    assert!(!queue.push(SurfaceNotice::turn_error("duplicate detail", key)));

    assert_eq!(queue.len(), 1);
    assert_eq!(
        queue.active().map(SurfaceNotice::selectable_text),
        Some("Turn error\nfirst detail".to_string())
    );
}

#[test]
fn source_key_deduplication_state_is_bounded() {
    let mut queue = SurfaceNoticeQueue::default();

    for index in 0..MAX_REPORTED_SURFACE_NOTICE_SOURCE_KEYS {
        assert!(queue.push(SurfaceNotice::turn_error(
            format!("detail {index}"),
            turn_error_key(index),
        )));
    }
    assert_eq!(
        queue.reported_source_key_count(),
        MAX_REPORTED_SURFACE_NOTICE_SOURCE_KEYS
    );
    assert!(!queue.push(SurfaceNotice::turn_error("duplicate", turn_error_key(0))));

    assert!(queue.push(SurfaceNotice::turn_error(
        "new detail",
        turn_error_key(MAX_REPORTED_SURFACE_NOTICE_SOURCE_KEYS),
    )));
    assert_eq!(
        queue.reported_source_key_count(),
        MAX_REPORTED_SURFACE_NOTICE_SOURCE_KEYS
    );
    assert!(queue.push(SurfaceNotice::turn_error(
        "expired source key can be reported again",
        turn_error_key(0),
    )));
}

#[test]
fn queue_overflow_keeps_active_notice_and_bounds_queued_notices() {
    let mut queue = SurfaceNoticeQueue::default();

    for index in 0..12 {
        assert!(queue.push(SurfaceNotice::new(
            format!("Notice {index}"),
            format!("detail {index}"),
        )));
    }

    assert_eq!(queue.len(), 8);
    assert_eq!(queue.active().map(SurfaceNotice::title), Some("Notice 0"));
}

#[test]
fn selected_failed_backend_turn_builds_turn_error_notice() {
    let turn = turn_info(
        "turn_1",
        TurnStatus::Failed,
        Some(TurnError {
            message: "backend rejected turn".to_string(),
            additional_details: None,
        }),
    );

    let notice = selected_backend_turn_error_notice(Some("thread_1"), "thread_1", &turn)
        .expect("selected failed turn should build a notice");

    assert_eq!(notice.title(), "Turn error");
    assert_eq!(notice.detail(), "backend rejected turn");
}

#[test]
fn local_turn_failure_builds_turn_error_notice() {
    let notice = local_turn_failure_notice("backend unavailable");

    assert_eq!(notice.title(), "Turn error");
    assert_eq!(notice.detail(), "backend unavailable");
    assert_eq!(notice.selectable_text(), "Turn error\nbackend unavailable");
}

#[test]
fn local_turn_failures_are_not_suppressed_by_reused_transient_identity() {
    let mut queue = SurfaceNoticeQueue::default();

    assert!(queue.push(local_turn_failure_notice("first backend unavailable")));
    assert!(queue.push(local_turn_failure_notice("second backend unavailable")));

    assert_eq!(queue.len(), 2);
    assert_eq!(
        queue.active().map(SurfaceNotice::selectable_text),
        Some("Turn error\nfirst backend unavailable".to_string())
    );
}

#[test]
fn clear_with_title_removes_matching_active_and_queued_notices() {
    let mut queue = SurfaceNoticeQueue::default();

    assert!(queue.push(SurfaceNotice::new("Keep active", "one")));
    assert!(queue.push(SurfaceNotice::new("Image input unavailable", "queued one")));
    assert!(queue.push(SurfaceNotice::new("Image input unavailable", "queued two")));
    assert!(queue.clear_with_title("Image input unavailable"));

    assert_eq!(queue.len(), 1);
    assert_eq!(
        queue.active().map(SurfaceNotice::title),
        Some("Keep active")
    );
    assert!(!queue.clear_with_title("Image input unavailable"));

    assert!(queue.push(SurfaceNotice::new("Image input unavailable", "active soon")));
    assert!(queue.dismiss_active());
    assert_eq!(
        queue.active().map(SurfaceNotice::title),
        Some("Image input unavailable")
    );
    assert!(queue.clear_with_title("Image input unavailable"));
    assert!(queue.active().is_none());
}

#[test]
fn interrupted_backend_turn_without_error_payload_does_not_build_notice() {
    let turn = turn_info("turn_1", TurnStatus::Interrupted, None);

    assert!(selected_backend_turn_error_notice(Some("thread_1"), "thread_1", &turn).is_none());
}

#[test]
fn backend_error_detail_uses_non_empty_additional_details_only() {
    let detail = backend_turn_error_detail(Some(&TurnError {
        message: "primary message".to_string(),
        additional_details: Some("  extra context  ".to_string()),
    }));
    assert_eq!(detail, "primary message\n\nextra context");

    let detail = backend_turn_error_detail(Some(&TurnError {
        message: "primary message".to_string(),
        additional_details: Some("   ".to_string()),
    }));
    assert_eq!(detail, "primary message");

    assert_eq!(
        backend_turn_error_detail(None),
        "The turn failed without an error payload from the backend."
    );
}

#[test]
fn selectable_text_uses_single_title_detail_line_break() {
    let notice = SurfaceNotice::new("Turn error", "primary message\n\nextra context");

    assert_eq!(
        notice.selectable_text(),
        "Turn error\nprimary message\n\nextra context"
    );
}

fn turn_error_key(index: usize) -> SurfaceNoticeSourceKey {
    SurfaceNoticeSourceKey::TurnError {
        thread_id: "thread_1".to_string(),
        turn_id: format!("turn_{index}"),
    }
}

fn turn_info(id: &str, status: TurnStatus, error: Option<TurnError>) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status,
        items: Vec::new(),
        error,
    }
}
