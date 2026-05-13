#[path = "support/tempdir.rs"]
mod tempdir_support;

pub use beryl_app::{BerylHomeDir, BerylHomeDirError};

#[path = "../src/diagnostic_child_protocol.rs"]
mod diagnostic_child_protocol;

#[path = "../src/diagnostic_child_supervisor.rs"]
mod diagnostic_child_supervisor;

use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use diagnostic_child_protocol::{
    DIAGNOSTIC_CHILD_PROTOCOL_NAME, DIAGNOSTIC_CHILD_PROTOCOL_VERSION,
};
use diagnostic_child_supervisor::{
    DIAGNOSTIC_CHILD_STOP_BUDGET, DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT, DiagnosticChildLaunch,
    DiagnosticChildStartOutcome, DiagnosticChildStopOutcome, DiagnosticChildSupervisor,
    DiagnosticChildSupervisorError, SpawnedDiagnosticChildGuard, same_home_path,
};

#[test]
fn start_rejects_supervisor_home_as_child_home() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();

    let launch = DiagnosticChildLaunch::new(root.path(), PathBuf::from("not-needed"));
    let error = supervisor.start(&home, launch).unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::HomeCollidesWithSupervisor { .. }
    ));
}

#[test]
fn start_rejects_invalid_executable_paths_before_spawn() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let directory = tempdir_support::temp_dir("beryl-diagnostic-executable-dir-");
    let over_limit = root.path().join("x".repeat(1100));
    let cases = [
        (
            PathBuf::new(),
            "empty executable path should be rejected as invalid",
        ),
        (
            PathBuf::from("relative-beryl.exe"),
            "relative executable path should be rejected as invalid",
        ),
        (
            over_limit,
            "over-limit executable path should be rejected as invalid",
        ),
        (
            directory.path().to_path_buf(),
            "directory executable path should be rejected as invalid",
        ),
    ];

    for (path, message) in cases {
        let mut supervisor = DiagnosticChildSupervisor::default();
        let launch = DiagnosticChildLaunch::new(child.path(), path);
        let error = supervisor.start(&home, launch).unwrap_err();
        assert!(
            matches!(
                error,
                DiagnosticChildSupervisorError::InvalidExecutablePath { .. }
            ),
            "{message}: {error}"
        );
        assert!(!supervisor.has_child_for_test());
    }

    let mut supervisor = DiagnosticChildSupervisor::default();
    let missing = root.path().join("missing-beryl.exe");
    let launch = DiagnosticChildLaunch::new(child.path(), missing);
    let error = supervisor.start(&home, launch).unwrap_err();
    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::ExecutablePathAccess { .. }
    ));
    assert!(!supervisor.has_child_for_test());

    directory.close().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn start_verifies_startup_protocol_before_reporting_started() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(root.path(), FakeChildBehavior::HandshakeOk);
    let mut supervisor = DiagnosticChildSupervisor::default();
    let launch = DiagnosticChildLaunch::new(child.path(), executable.clone());

    let outcome = supervisor.start(&home, launch).unwrap();

    let DiagnosticChildStartOutcome::Started(identity) = outcome else {
        panic!("expected started diagnostic child");
    };
    assert_eq!(
        identity.executable_path,
        fs::canonicalize(&executable).unwrap()
    );
    assert!(supervisor.has_child_for_test());
    supervisor.stop().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn startup_protocol_failures_are_cleaned_up_without_retaining_child() {
    let cases = [
        (
            FakeChildBehavior::Eof,
            "EOF should be reported as startup protocol EOF",
        ),
        (
            FakeChildBehavior::Malformed,
            "malformed response should be reported as startup protocol malformed",
        ),
        (
            FakeChildBehavior::RemoteError,
            "remote error should be reported as startup protocol rejection",
        ),
        (
            FakeChildBehavior::Incompatible,
            "incompatible handshake should be reported as startup incompatibility",
        ),
    ];

    for (behavior, message) in cases {
        let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
        let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
        let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
        let executable = fake_child_executable(root.path(), behavior);
        let mut supervisor = DiagnosticChildSupervisor::default();
        let launch = DiagnosticChildLaunch::new(child.path(), executable);

        let error = supervisor.start(&home, launch).unwrap_err();

        match behavior {
            FakeChildBehavior::Eof => assert!(
                matches!(error, DiagnosticChildSupervisorError::StartupProtocolEof),
                "{message}: {error}"
            ),
            FakeChildBehavior::Malformed => assert!(
                matches!(
                    error,
                    DiagnosticChildSupervisorError::StartupProtocolMalformed { .. }
                ),
                "{message}: {error}"
            ),
            FakeChildBehavior::RemoteError => assert!(
                matches!(
                    error,
                    DiagnosticChildSupervisorError::StartupProtocolRejected { .. }
                ),
                "{message}: {error}"
            ),
            FakeChildBehavior::Incompatible => assert!(
                matches!(
                    error,
                    DiagnosticChildSupervisorError::StartupProtocolIncompatible { .. }
                ),
                "{message}: {error}"
            ),
            FakeChildBehavior::HandshakeOk | FakeChildBehavior::Timeout => unreachable!(),
        }
        assert!(!supervisor.has_child_for_test());
        child.close().unwrap();
        root.close().unwrap();
    }
}

#[test]
fn startup_protocol_timeout_is_cleaned_up_without_retaining_child() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(root.path(), FakeChildBehavior::Timeout);
    let mut supervisor = DiagnosticChildSupervisor::default();
    let launch = DiagnosticChildLaunch::new(child.path(), executable);

    let error = supervisor
        .start_for_test(&home, launch, Duration::from_millis(100))
        .unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::StartupProtocolTimeout { .. }
    ));
    assert!(!supervisor.has_child_for_test());
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn startup_cleanup_failure_retains_child_for_stop_retry() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();

    let error = supervisor
        .retain_startup_failure_child_for_test(
            spawn_sleep_child(),
            home,
            PathBuf::from("test-child"),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::RequestTimeout { .. }
    ));
    assert!(supervisor.has_child_for_test());
    supervisor.stop().unwrap();
    root.close().unwrap();
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

#[derive(Clone, Copy)]
enum FakeChildBehavior {
    HandshakeOk,
    Eof,
    Malformed,
    RemoteError,
    Incompatible,
    Timeout,
}

fn fake_child_executable(root: &std::path::Path, behavior: FakeChildBehavior) -> PathBuf {
    let path = root.join(fake_child_file_name(behavior));
    fs::write(&path, fake_child_script(behavior)).unwrap();
    make_executable_for_test(&path);
    path
}

#[cfg(target_os = "windows")]
fn fake_child_file_name(behavior: FakeChildBehavior) -> &'static str {
    match behavior {
        FakeChildBehavior::HandshakeOk => "fake child ok.cmd",
        FakeChildBehavior::Eof => "fake child eof.cmd",
        FakeChildBehavior::Malformed => "fake child malformed.cmd",
        FakeChildBehavior::RemoteError => "fake child error.cmd",
        FakeChildBehavior::Incompatible => "fake child incompatible.cmd",
        FakeChildBehavior::Timeout => "fake child timeout.cmd",
    }
}

#[cfg(not(target_os = "windows"))]
fn fake_child_file_name(behavior: FakeChildBehavior) -> &'static str {
    match behavior {
        FakeChildBehavior::HandshakeOk => "fake child ok.sh",
        FakeChildBehavior::Eof => "fake child eof.sh",
        FakeChildBehavior::Malformed => "fake child malformed.sh",
        FakeChildBehavior::RemoteError => "fake child error.sh",
        FakeChildBehavior::Incompatible => "fake child incompatible.sh",
        FakeChildBehavior::Timeout => "fake child timeout.sh",
    }
}

#[cfg(target_os = "windows")]
fn fake_child_script(behavior: FakeChildBehavior) -> String {
    let response = fake_child_response(behavior);
    match behavior {
        FakeChildBehavior::Eof => "@echo off\r\nexit /b 0\r\n".to_string(),
        FakeChildBehavior::Timeout => "@echo off\r\nping -n 60 127.0.0.1 >nul\r\n".to_string(),
        _ => {
            format!("@echo off\r\nset /p line=\r\necho {response}\r\nping -n 60 127.0.0.1 >nul\r\n")
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn fake_child_script(behavior: FakeChildBehavior) -> String {
    let response = fake_child_response(behavior);
    match behavior {
        FakeChildBehavior::Eof => "#!/bin/sh\nexit 0\n".to_string(),
        FakeChildBehavior::Timeout => "#!/bin/sh\nsleep 60\n".to_string(),
        _ => format!("#!/bin/sh\nIFS= read -r line\nprintf '%s\\n' '{response}'\nsleep 60\n"),
    }
}

fn fake_child_response(behavior: FakeChildBehavior) -> String {
    match behavior {
        FakeChildBehavior::HandshakeOk => format!(
            "{{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"{DIAGNOSTIC_CHILD_PROTOCOL_NAME}\",\"protocolVersion\":{DIAGNOSTIC_CHILD_PROTOCOL_VERSION}}}}}"
        ),
        FakeChildBehavior::Malformed => "not-json".to_string(),
        FakeChildBehavior::RemoteError => {
            "{\"id\":\"1\",\"ok\":false,\"error\":{\"kind\":\"unsupported_command\",\"message\":\"bad command\"}}"
                .to_string()
        }
        FakeChildBehavior::Incompatible => {
            "{\"id\":\"1\",\"ok\":true,\"result\":{\"protocol\":\"other\",\"protocolVersion\":999}}"
                .to_string()
        }
        FakeChildBehavior::Eof | FakeChildBehavior::Timeout => String::new(),
    }
}

#[cfg(unix)]
fn make_executable_for_test(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn make_executable_for_test(_path: &std::path::Path) {}

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
