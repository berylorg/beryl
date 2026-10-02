#![cfg(target_os = "windows")]

#[path = "support/crash_mount/mod.rs"]
mod support;

use std::io::Write;
use std::time::{Duration, Instant};

use support::Application;

#[test]
fn malformed_reporter_invocation_cannot_enter_home_bootstrap() {
    let home = tempfile::tempdir().unwrap();
    let candidate = home.path().join("unopened-home");
    let mut application =
        Application::spawn(&candidate, &["--beryl-crash-reporter", "invalid"], false);
    let status = application.wait_for_exit();
    assert_eq!(
        status.code(),
        Some(1),
        "reserved receiver must reject malformed input"
    );
    assert!(!candidate.exists(), "reserved reporter mode opened home");
}

#[test]
fn ordinary_diagnostic_exit_releases_its_single_waiting_reporter() {
    let home = tempfile::tempdir().unwrap();
    let mut application = Application::spawn(home.path(), &["--diagnostic-target-stdio"], false);
    application.adopt_reporter();
    assert!(application.report_window().is_none());
    writeln!(
        application.input(),
        "{{\"id\":\"mount\",\"command\":\"handshake\"}}"
    )
    .unwrap();
    application.input().flush().unwrap();
    let response: serde_json::Value = serde_json::from_str(&application.read_line()).unwrap();
    assert_eq!(response["id"], "mount");
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["protocol"], "beryl_diagnostic_child");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        writeln!(
            application.input(),
            "{{\"id\":\"mount-stage\",\"command\":\"read_ui_state\"}}"
        )
        .unwrap();
        application.input().flush().unwrap();
        let response: serde_json::Value = serde_json::from_str(&application.read_line()).unwrap();
        assert_eq!(response["id"], "mount-stage");
        assert_eq!(response["ok"], true, "{response}");
        if response["result"]["stage"] == "running" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "application did not reach Running: {response}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    application.assert_single_waiting_reporter();
    application.close_input();
    let status = application.wait_for_exit();
    assert_eq!(
        status.code(),
        Some(0),
        "Running application must exit normally: {status}"
    );
    application.wait_for_reporter_exit();
    assert!(application.report_window().is_none());
}

#[cfg(feature = "test-faults")]
#[test]
fn opener_panic_aborts_application_before_independent_report_window() {
    let home = tempfile::tempdir().unwrap();
    let candidate = home.path().join("unopened-home");
    let mut application = Application::spawn(&candidate, &[], true);
    application.adopt_reporter();
    assert_eq!(application.read_line().trim_end(), "beryl-home-fault-ready");
    application.assert_single_waiting_reporter();
    assert!(
        !candidate.exists(),
        "fault must precede home candidate opening"
    );
    assert!(application.report_window().is_none());
    application.input().write_all(b"x").unwrap();
    application.input().flush().unwrap();
    let status = application.wait_for_exit();
    assert!(
        !status.success(),
        "application panic must be fatal: {status}"
    );
    assert!(!candidate.exists());
    application.close_report_window_after_parent_exit();
    application.wait_for_reporter_exit();
}
