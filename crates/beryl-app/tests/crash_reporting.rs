#![cfg(all(target_os = "windows", feature = "test-faults"))]

#[path = "support/crash_report_process.rs"]
mod support;

use support::ReportProcess;

#[test]
fn panic_report_outlives_the_application_without_inheriting_unrelated_handles() {
    let result = ReportProcess::start("panic", None).finish();
    assert!(!result.success);
    assert!(result.installation.contains("ready=true clean=true"));
    let report = result.report.unwrap();
    assert!(report.contains("storage writer invariant failed"));
    assert!(report.contains("crash_report_fixture"));
}

#[test]
fn normal_exit_silently_reaps_the_waiting_reporter() {
    let result = ReportProcess::start("normal", None).finish();
    assert!(result.success);
    assert!(result.installation.contains("ready=true"));
    assert!(result.report.is_none());
}

#[test]
fn an_ordinary_catch_cannot_resume_after_a_fatal_panic() {
    let result = ReportProcess::start("caught", None).finish();
    assert!(!result.success);
    assert!(!result.continued);
    assert!(
        result
            .report
            .unwrap()
            .contains("caught panic must be fatal")
    );
}

#[test]
fn oversized_utf8_panic_is_bounded_and_explicitly_truncated() {
    let result = ReportProcess::start("large", None).finish();
    assert!(!result.success);
    let report = result.report.unwrap();
    assert!(report.len() <= 4096);
    assert!(report.ends_with("[Report truncated]"));
}

#[test]
fn non_string_panic_does_not_invoke_payload_formatting() {
    let result = ReportProcess::start("non-string", None).finish();
    assert!(!result.success);
    assert!(
        result
            .report
            .unwrap()
            .contains("[Non-string panic payload]")
    );
}

#[test]
fn repeated_installation_preserves_the_original_reporter() {
    let result = ReportProcess::start("install-twice", None).finish();
    assert!(!result.success);
    assert!(
        result
            .report
            .unwrap()
            .contains("original reporter remains installed")
    );
}

#[test]
fn concurrent_panics_terminate_with_at_most_one_complete_report() {
    let result = ReportProcess::start("concurrent", None).finish();
    assert!(!result.success);
    if let Some(report) = result.report {
        assert!(report.contains("concurrent fatal panic"));
        assert!(report.len() <= 4096);
    }
}

#[test]
fn reporter_death_does_not_prevent_application_abort() {
    let mut process = ReportProcess::start("panic", None);
    process.stop_reporter();
    let result = process.finish();
    assert!(!result.success);
    assert!(result.report.is_none());
}

#[test]
fn job_breakaway_refusal_keeps_abort_only_handling() {
    let result = ReportProcess::start("job-refusal", None).finish();
    assert!(!result.success);
    assert!(result.installation.contains("ready=false"));
    assert!(result.report.is_none());
}

#[test]
fn failed_reporter_startup_keeps_abort_only_handling() {
    let result = ReportProcess::start("panic", Some("exit")).finish();
    assert!(!result.success);
    assert!(result.installation.contains("ready=false"));
    assert!(result.report.is_none());
}

#[test]
fn reporter_readiness_timeout_reaps_the_provisional_child() {
    let result = ReportProcess::start("panic", Some("timeout")).finish();
    assert!(!result.success);
    assert!(result.installation.contains("ready=false"));
    assert!(result.installation.contains("TimedOut"));
    assert!(result.report.is_none());
}
