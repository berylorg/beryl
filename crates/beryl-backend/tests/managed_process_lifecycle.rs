#![cfg(feature = "lifecycle-test-support")]

use std::{
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use beryl_backend::{
    BackendLaunchSpec, BackendWebSocketEndpoint,
    lifecycle_test_support::{
        spawn_host_powershell_script, spawn_sleeping_host_process, wsl_shutdown_command_line,
    },
};
use beryl_model::workspace::RuntimeMode;

const PROCESS_EXIT_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(target_os = "windows")]
#[test]
fn supervised_process_shutdown_is_synchronous() {
    let mut process = spawn_sleeping_host_process().expect("sleeping process should spawn");
    let process_id = process
        .process_id()
        .expect("supervised process should expose child process id");

    assert!(windows_process_exists(process_id));
    process
        .shutdown(Duration::ZERO, PROCESS_EXIT_TIMEOUT)
        .expect("explicit shutdown should kill the supervised process");

    assert!(
        wait_for_windows_process_exit(process_id, PROCESS_EXIT_TIMEOUT),
        "process {process_id} survived explicit shutdown"
    );
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

    assert!(
        wait_for_windows_process_exit(process_id, PROCESS_EXIT_TIMEOUT),
        "process {process_id} survived repeated shutdown"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn supervised_process_drop_is_shutdown_fallback() {
    let process = spawn_sleeping_host_process().expect("sleeping process should spawn");
    let process_id = process
        .process_id()
        .expect("supervised process should expose child process id");

    drop(process);

    assert!(
        wait_for_windows_process_exit(process_id, PROCESS_EXIT_TIMEOUT),
        "process {process_id} survived supervised process drop"
    );
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

    assert!(windows_process_exists(parent_process_id));
    assert!(windows_process_exists(descendant_process_id));

    process
        .shutdown(Duration::ZERO, PROCESS_EXIT_TIMEOUT)
        .expect("explicit shutdown should release the job object");

    assert!(
        wait_for_windows_process_exit(parent_process_id, PROCESS_EXIT_TIMEOUT),
        "parent process {parent_process_id} survived explicit shutdown"
    );
    assert!(
        wait_for_windows_process_exit(descendant_process_id, PROCESS_EXIT_TIMEOUT),
        "descendant process {descendant_process_id} survived job object cleanup"
    );

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
fn windows_process_exists(process_id: u32) -> bool {
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "if (Get-Process -Id {process_id} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
            ),
        ])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(target_os = "windows")]
fn wait_for_windows_process_exit(process_id: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !windows_process_exists(process_id) {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    !windows_process_exists(process_id)
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
