#![cfg(feature = "lifecycle-test-support")]

use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use beryl_backend::{
    BackendLaunchSpec, BackendWebSocketEndpoint, ManagedBackendError,
    lifecycle_test_support::{
        spawn_blocked_stdio_session, spawn_host_powershell_script, spawn_sleeping_host_process,
        wsl_shutdown_command_line,
    },
};
use beryl_model::workspace::RuntimeMode;
use wait_timeout::ChildExt;

const PROCESS_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const STDIO_DEADLINE_CHILD_ENV: &str = "BERYL_STDIO_DEADLINE_WATCHDOG_CHILD";

#[cfg(target_os = "windows")]
#[test]
fn blocked_stdio_write_is_terminal_and_cleanup_is_joined_under_watchdog() {
    let executable = std::env::current_exe().expect("locate stdio deadline test executable");
    let mut child = Command::new(executable)
        .args([
            "--exact",
            "blocked_stdio_write_watchdog_child",
            "--nocapture",
        ])
        .env(STDIO_DEADLINE_CHILD_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn stdio deadline watchdog child");

    let status = match child
        .wait_timeout(Duration::from_secs(15))
        .expect("wait for stdio deadline watchdog child")
    {
        Some(status) => status,
        None => {
            child
                .kill()
                .expect("kill hung stdio deadline watchdog child");
            let _ = child.wait();
            panic!("blocked stdio deadline scenario exceeded its outer watchdog");
        }
    };
    assert!(
        status.success(),
        "stdio deadline watchdog child failed: {status}"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn blocked_stdio_write_watchdog_child() {
    if std::env::var_os(STDIO_DEADLINE_CHILD_ENV).is_none() {
        return;
    }

    const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);
    const PAYLOAD_BYTES: usize = 1024 * 1024;
    let mut session = spawn_blocked_stdio_session().expect("spawn blocked stdio backend fixture");
    session
        .inject_process_termination_failures(2)
        .expect("inject immediate termination and first cleanup-attempt refusals");
    let process_id = session
        .process_id()
        .expect("blocked stdio fixture should expose its child pid");

    let started = Instant::now();
    let error = session
        .request_payload(PAYLOAD_BYTES, REQUEST_TIMEOUT)
        .expect_err("payload larger than pipe capacity must time out");
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(matches!(
        error,
        ManagedBackendError::RequestTimeout { ref method, timeout }
            if method == "config/read" && timeout == REQUEST_TIMEOUT
    ));
    assert_eq!(session.next_request_id(), 2);

    let error = session
        .request_payload(1, Duration::from_secs(1))
        .expect_err("a session with an ambiguous partial write must stay terminal");
    match error {
        ManagedBackendError::SessionPoisoned { ref method } if method == "config/read" => {}
        other => panic!("expected terminal poisoned-session error, got {other:?}"),
    }
    assert_eq!(
        session.next_request_id(),
        2,
        "a poisoned request must be rejected before request-id allocation"
    );

    let first_shutdown_started = Instant::now();
    let first_shutdown_error = session
        .shutdown()
        .expect_err("the first bounded cleanup attempt must report injected termination refusal");
    assert!(
        first_shutdown_started.elapsed() < Duration::from_secs(3),
        "explicit shutdown must not join the still-blocked writer after termination refusal"
    );
    match first_shutdown_error {
        ManagedBackendError::StdioCleanupFailures { failures } => {
            assert_eq!(
                failures.len(),
                2,
                "cleanup must retain the immediate-kill and bounded-attempt refusals"
            );
            assert!(
                failures
                    .iter()
                    .all(|failure| matches!(failure, ManagedBackendError::TerminateProcess { .. }))
            );
        }
        other => panic!("cleanup did not preserve both injected failures: {other:?}"),
    }
    assert!(session.cleanup_retained());
    assert!(!session.writer_finished());
    assert_eq!(session.write_count(), 1);
    assert!(
        windows_process_is_running(process_id).expect("process query should succeed"),
        "the injected refusal must leave the exact child owned and alive for retry"
    );

    session
        .shutdown()
        .expect("a later explicit shutdown should terminate, reap, and join retained ownership");
    assert!(session.cleanup_finished());
    assert!(session.writer_finished());
    wait_for_windows_process_exit(process_id, PROCESS_EXIT_TIMEOUT)
        .expect("successful retry should reap the exact retained child");
}

#[cfg(target_os = "windows")]
#[test]
fn post_commit_stdio_io_error_is_preserved_terminal_and_joined() {
    let mut session = spawn_blocked_stdio_session().expect("spawn stdio I/O failure fixture");
    session
        .fail_next_write_after_bytes(1)
        .expect("inject a writer-owned failure after one committed byte");
    let process_id = session.process_id().expect("fixture must expose child pid");

    let first = session
        .request_payload(1, Duration::from_secs(1))
        .expect_err("post-commit stdio failure must be returned");
    match first {
        ManagedBackendError::WriteRequest { source, .. } => {
            assert_eq!(source.kind(), std::io::ErrorKind::BrokenPipe);
        }
        other => panic!("ordinary post-commit stdio I/O provenance was lost: {other:?}"),
    }
    assert_eq!(session.next_request_id(), 2);
    assert!(matches!(
        session.request_payload(1, Duration::from_secs(1)),
        Err(ManagedBackendError::SessionPoisoned { ref method }) if method == "config/read"
    ));
    assert_eq!(session.next_request_id(), 2);

    session
        .shutdown()
        .expect("terminal stdio cleanup must join after exact child termination");
    assert!(session.cleanup_finished());
    assert!(session.writer_finished());
    wait_for_windows_process_exit(process_id, PROCESS_EXIT_TIMEOUT)
        .expect("post-commit stdio failure cleanup should reap the exact child");
}

#[cfg(target_os = "windows")]
#[test]
fn supervised_process_shutdown_is_synchronous() {
    let mut process = spawn_sleeping_host_process().expect("sleeping process should spawn");
    let process_id = process
        .process_id()
        .expect("supervised process should expose child process id");

    assert!(windows_process_is_running(process_id).expect("process query should succeed"));
    process
        .shutdown(Duration::ZERO, PROCESS_EXIT_TIMEOUT)
        .expect("explicit shutdown should kill the supervised process");

    wait_for_windows_process_exit(process_id, PROCESS_EXIT_TIMEOUT)
        .expect("process query should verify explicit shutdown");
}

#[cfg(target_os = "windows")]
#[test]
fn supervised_process_shutdown_is_idempotent() {
    let mut process = spawn_sleeping_host_process().expect("sleeping process should spawn");
    let process_id = process
        .process_id()
        .expect("supervised process should expose child process id");

    process
        .shutdown(Duration::ZERO, PROCESS_EXIT_TIMEOUT)
        .expect("first shutdown should succeed");
    process
        .shutdown(Duration::ZERO, PROCESS_EXIT_TIMEOUT)
        .expect("second shutdown should be a no-op");

    wait_for_windows_process_exit(process_id, PROCESS_EXIT_TIMEOUT)
        .expect("process query should verify repeated shutdown");
}

#[cfg(target_os = "windows")]
#[test]
fn supervised_process_drop_is_shutdown_fallback() {
    let process = spawn_sleeping_host_process().expect("sleeping process should spawn");
    let process_id = process
        .process_id()
        .expect("supervised process should expose child process id");

    drop(process);

    wait_for_windows_process_exit(process_id, PROCESS_EXIT_TIMEOUT)
        .expect("process query should verify drop shutdown");
}

#[cfg(target_os = "windows")]
#[test]
fn windows_job_object_cleanup_kills_descendant_processes() {
    let temp_dir = tempfile::tempdir().expect("test temp dir should be creatable");
    let pid_file = temp_dir.path().join("descendant.pid");

    let script = format!(
        "$child = Start-Process -FilePath powershell.exe -WindowStyle Hidden -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 60' -PassThru; Set-Content -LiteralPath {} -Value $child.Id; Start-Sleep -Seconds 60",
        powershell_single_quoted_path(&pid_file)
    );
    let mut process =
        spawn_host_powershell_script(script).expect("parent process should spawn descendant");
    let parent_process_id = process
        .process_id()
        .expect("supervised process should expose child process id");
    let descendant_process_id = read_pid_file_until(&pid_file, PROCESS_EXIT_TIMEOUT)
        .expect("descendant pid file should be written");

    assert!(
        windows_process_is_running(parent_process_id).expect("parent process query should succeed")
    );
    assert!(
        windows_process_is_running(descendant_process_id)
            .expect("descendant process query should succeed")
    );

    process
        .shutdown(Duration::ZERO, PROCESS_EXIT_TIMEOUT)
        .expect("explicit shutdown should release the job object");

    wait_for_windows_process_exit(parent_process_id, PROCESS_EXIT_TIMEOUT)
        .expect("process query should verify parent shutdown");
    wait_for_windows_process_exit(descendant_process_id, PROCESS_EXIT_TIMEOUT)
        .expect("process query should verify descendant shutdown");

    temp_dir
        .close()
        .expect("test temp dir should be removable after process cleanup");
}

#[test]
fn wsl_process_group_shutdown_command_targets_pidfile_process_group() {
    let launch = BackendLaunchSpec::managed_websocket(
        RuntimeMode::WslLinux {
            distro_name: "Ubuntu".to_string(),
        },
        "/work/beryl",
        BackendWebSocketEndpoint::loopback(49155),
        "/tmp/beryl-token.txt",
    );
    let command = wsl_shutdown_command_line(&launch)
        .expect("WSL cleanup command line should build")
        .expect("WSL launch should have cleanup command");

    assert_eq!(command.program(), "wsl.exe");
    assert_eq!(command.cwd(), None);
    assert_eq!(command.args().len(), 6);
    assert_eq!(command.args()[0], "--distribution");
    assert_eq!(command.args()[1], "Ubuntu");
    assert_eq!(command.args()[2], "--exec");
    assert_eq!(command.args()[3], "/bin/bash");
    assert_eq!(command.args()[4], "-lc");

    let shell = &command.args()[5];
    assert!(shell.contains("pid_file="));
    assert!(shell.contains("/tmp/beryl-codex-app-server/process-"));
    assert!(shell.contains("cat \"$pid_file\""));
    assert!(shell.contains("kill -TERM -- -\"$pid\""));
    assert!(shell.contains("kill -KILL -- -\"$pid\""));
    assert!(shell.contains("rm -f \"$pid_file\""));
    assert!(shell.contains("exit 2"));
}

#[cfg(target_os = "windows")]
fn windows_process_is_running(process_id: u32) -> Result<bool, String> {
    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "try {{ Get-Process -Id {process_id} -ErrorAction Stop | Out-Null; exit 0 }} catch {{ if ($_.FullyQualifiedErrorId -eq 'NoProcessFoundForGivenId,Microsoft.PowerShell.Commands.GetProcessCommand') {{ exit 1 }} [Console]::Error.WriteLine($_.Exception.Message); exit 2 }}"
            ),
        ])
        .status()
        .map_err(|error| format!("failed to query process {process_id}: {error}"))?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        Some(code) => Err(format!(
            "process query for {process_id} exited with unexpected status {code}"
        )),
        None => Err(format!(
            "process query for {process_id} exited without a canonical status code"
        )),
    }
}

#[cfg(target_os = "windows")]
fn wait_for_windows_process_exit(process_id: u32, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !windows_process_is_running(process_id)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    if windows_process_is_running(process_id)? {
        Err(format!("process {process_id} survived explicit shutdown"))
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn read_pid_file_until(path: &Path, timeout: Duration) -> Option<u32> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(value) = fs::read_to_string(path)
            && let Ok(process_id) = value.trim().parse()
        {
            return Some(process_id);
        }
        thread::sleep(Duration::from_millis(50));
    }
    None
}

#[cfg(target_os = "windows")]
fn powershell_single_quoted_path(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}
