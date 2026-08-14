//! Test-only helpers for managed backend lifecycle integration tests.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

use beryl_model::workspace::RuntimeMode;

use crate::{
    BackendCommandLine, BackendCommandLineError, BackendLaunchSpec, ManagedBackendError,
    managed_process::SupervisedBackendProcess,
};

pub type LifecycleTestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug)]
pub struct TestSupervisedBackendProcess {
    process: SupervisedBackendProcess,
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
