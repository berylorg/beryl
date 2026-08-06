//! Test-only helpers for managed backend lifecycle integration tests.

use std::{
    io,
    path::Path,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::Duration,
};

use beryl_model::workspace::RuntimeMode;

use crate::{
    BackendCommandLine, BackendCommandLineError, BackendLaunchSpec, ManagedBackendError,
    ManagedBackendSession, managed_process::SupervisedBackendProcess,
};

pub type LifecycleTestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug)]
pub struct TestSupervisedBackendProcess {
    process: SupervisedBackendProcess,
}

#[derive(Debug)]
pub struct TestBlockedStdioSession {
    session: ManagedBackendSession,
}

impl TestBlockedStdioSession {
    pub fn process_id(&self) -> Option<u32> {
        self.session.process_id()
    }

    pub fn request_payload(
        &mut self,
        payload_bytes: usize,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        let payload = "x".repeat(payload_bytes);
        self.session
            .read_config(Path::new(&payload), timeout)
            .map(|_| ())
    }

    pub fn shutdown(&mut self) -> Result<(), ManagedBackendError> {
        self.session.shutdown()
    }

    pub fn cleanup_finished(&self) -> bool {
        self.session.stdio_cleanup_finished_for_test()
    }

    pub fn cleanup_retained(&self) -> bool {
        self.session.stdio_cleanup_retained_for_test()
    }

    pub fn writer_finished(&self) -> bool {
        self.session.stdio_writer_finished_for_test()
    }

    pub fn next_request_id(&self) -> u64 {
        self.session.next_request_id_for_test()
    }

    pub fn write_count(&self) -> usize {
        self.session.stdio_write_count_for_test()
    }

    pub fn inject_process_termination_failures(&mut self, count: usize) -> LifecycleTestResult<()> {
        if !self
            .session
            .inject_process_termination_failures_for_test(count)
        {
            return Err("test session does not own a managed process".into());
        }
        Ok(())
    }

    pub fn fail_next_write_after_bytes(&mut self, bytes: usize) -> LifecycleTestResult<()> {
        if !self
            .session
            .fail_next_stdio_write_after_bytes_for_test(bytes)
        {
            return Err("test session does not own a stdio writer".into());
        }
        Ok(())
    }
}

pub fn spawn_blocked_stdio_session() -> LifecycleTestResult<TestBlockedStdioSession> {
    let launch_spec = host_test_launch_spec()?;
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 60"]);
    let session = ManagedBackendSession::launch_test_command(launch_spec, command)?;
    Ok(TestBlockedStdioSession { session })
}

pub fn pause_websocket_after_next_write_header(
    session: &mut ManagedBackendSession,
) -> LifecycleTestResult<(Receiver<()>, SyncSender<()>)> {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    if !session.pause_websocket_after_next_write_header_for_test(entered_tx, release_rx) {
        return Err("test session does not use WebSocket transport".into());
    }
    Ok((entered_rx, release_tx))
}

pub fn pause_websocket_after_next_read_frame_byte(
    session: &mut ManagedBackendSession,
) -> LifecycleTestResult<(Receiver<()>, SyncSender<()>)> {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    if !session.pause_websocket_after_next_read_frame_byte_for_test(entered_tx, release_rx) {
        return Err("test session does not use WebSocket transport".into());
    }
    Ok((entered_rx, release_tx))
}

pub fn pause_websocket_after_next_read_payload(
    session: &mut ManagedBackendSession,
) -> LifecycleTestResult<(Receiver<()>, SyncSender<()>)> {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    if !session.pause_websocket_after_next_read_payload_for_test(entered_tx, release_rx) {
        return Err("test session does not use WebSocket transport".into());
    }
    Ok((entered_rx, release_tx))
}

pub fn pause_websocket_before_next_write(
    session: &mut ManagedBackendSession,
) -> LifecycleTestResult<(Receiver<()>, SyncSender<()>)> {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    if !session.pause_websocket_before_next_write_for_test(entered_tx, release_rx) {
        return Err("test session does not use WebSocket transport".into());
    }
    Ok((entered_rx, release_tx))
}

pub fn pause_websocket_before_write_after(
    session: &mut ManagedBackendSession,
    skipped_writes: usize,
) -> LifecycleTestResult<(Receiver<()>, SyncSender<()>)> {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    if !session.pause_websocket_before_write_after_for_test(skipped_writes, entered_tx, release_rx)
    {
        return Err("test session does not use WebSocket transport".into());
    }
    Ok((entered_rx, release_tx))
}

pub fn pause_websocket_after_next_control_write_header(
    session: &mut ManagedBackendSession,
) -> LifecycleTestResult<(Receiver<()>, SyncSender<()>)> {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    if !session.pause_websocket_after_next_control_write_header_for_test(entered_tx, release_rx) {
        return Err("test session does not use WebSocket transport".into());
    }
    Ok((entered_rx, release_tx))
}

pub fn next_request_id(session: &ManagedBackendSession) -> u64 {
    session.next_request_id_for_test()
}

pub fn websocket_close_frame_attempts(
    session: &ManagedBackendSession,
) -> LifecycleTestResult<usize> {
    session
        .websocket_close_frame_attempts_for_test()
        .ok_or_else(|| "test session does not use WebSocket transport".into())
}

pub fn fail_next_websocket_write_after_header(
    session: &mut ManagedBackendSession,
    kind: io::ErrorKind,
) -> LifecycleTestResult<()> {
    if !session.fail_next_websocket_write_after_header_for_test(kind) {
        return Err("test session does not use WebSocket transport".into());
    }
    Ok(())
}

pub fn pause_before_next_transport_close_classification(
    session: &mut ManagedBackendSession,
) -> LifecycleTestResult<(Receiver<()>, SyncSender<()>)> {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    if !session.pause_before_next_transport_close_classification_for_test(entered_tx, release_rx) {
        return Err("test session does not use WebSocket transport".into());
    }
    Ok((entered_rx, release_tx))
}

impl TestSupervisedBackendProcess {
    pub fn process_id(&self) -> Option<u32> {
        self.process.process_id()
    }

    pub fn shutdown(
        &mut self,
        grace_timeout: Duration,
        kill_timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        self.process.shutdown(grace_timeout, kill_timeout)
    }
}

pub fn spawn_sleeping_host_process() -> LifecycleTestResult<TestSupervisedBackendProcess> {
    spawn_host_powershell_script("Start-Sleep -Seconds 60")
}

pub fn spawn_host_powershell_script(
    script: impl AsRef<str>,
) -> LifecycleTestResult<TestSupervisedBackendProcess> {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-Command", script.as_ref()]);
    spawn_host_command(command)
}

pub fn wsl_shutdown_command_line(
    launch_spec: &BackendLaunchSpec,
) -> Result<Option<BackendCommandLine>, BackendCommandLineError> {
    launch_spec
        .wsl_process_group_cleanup()
        .map(|cleanup| cleanup.shutdown_command_line())
        .transpose()
}

fn spawn_host_command(mut command: Command) -> LifecycleTestResult<TestSupervisedBackendProcess> {
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());

    let child = command.spawn()?;
    let process = SupervisedBackendProcess::new(host_test_launch_spec()?, child)?;
    Ok(TestSupervisedBackendProcess { process })
}

fn host_test_launch_spec() -> LifecycleTestResult<BackendLaunchSpec> {
    Ok(BackendLaunchSpec::managed_stdio(
        RuntimeMode::HostWindows,
        host_test_cwd()?,
    ))
}

fn host_test_cwd() -> LifecycleTestResult<PathBuf> {
    Ok(std::env::current_dir()?)
}
