#[path = "support/tempdir.rs"]
mod tempdir_support;

pub use beryl_app::{BerylHomeDir, BerylHomeDirError};

#[path = "../src/diagnostic_child_protocol.rs"]
mod diagnostic_child_protocol;

#[path = "../src/diagnostic_child_supervisor.rs"]
mod diagnostic_child_supervisor;

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use diagnostic_child_supervisor::{
    DIAGNOSTIC_CHILD_STOP_BUDGET, DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT,
    DiagnosticChildStopOutcome, DiagnosticChildSupervisor, DiagnosticChildSupervisorError,
    SpawnedDiagnosticChildGuard, same_home_path,
};

#[test]
fn start_rejects_supervisor_home_as_child_home() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();

    let error = supervisor.start(&home, root.path()).unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::HomeCollidesWithSupervisor { .. }
    ));
}

#[test]
fn stop_without_running_child_is_idempotent() {
    let mut supervisor = DiagnosticChildSupervisor::default();

    let first = supervisor.stop().unwrap();
    let second = supervisor.stop().unwrap();

    assert_eq!(first, DiagnosticChildStopOutcome::NotRunning);
    assert_eq!(second, DiagnosticChildStopOutcome::NotRunning);
}

#[test]
fn same_home_path_uses_existing_directory_canonicalization() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-home-canonical-");
    let nested = root.path().join("child");
    std::fs::create_dir_all(&nested).unwrap();
    let equivalent = root.path().join(".").join("child");

    assert!(same_home_path(&nested, &equivalent));
}

#[test]
fn stop_response_timeout_exceeds_shutdown_budget() {
    assert!(DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT > DIAGNOSTIC_CHILD_STOP_BUDGET);
}

#[test]
fn spawned_child_guard_cleans_unclaimed_process() {
    let child = spawn_sleep_child();
    let mut guard = SpawnedDiagnosticChildGuard::new(child);

    assert!(guard.cleanup_for_test(Duration::from_secs(2)).unwrap());
}

#[test]
fn failed_stop_keeps_child_owned_for_retry() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-stop-ownership-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .adopt_child_for_test(spawn_sleep_child(), home, PathBuf::from("test-child"))
        .unwrap();

    let error = supervisor.force_stop_error_for_test().unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::RequestTimeout { .. }
    ));
    assert!(supervisor.has_child_for_test());
    supervisor.stop().unwrap();
}

#[cfg(target_os = "windows")]
fn spawn_sleep_child() -> Child {
    Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 60"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn powershell sleep child")
}

#[cfg(not(target_os = "windows"))]
fn spawn_sleep_child() -> Child {
    Command::new("sh")
        .args(["-c", "sleep 60"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn shell sleep child")
}
