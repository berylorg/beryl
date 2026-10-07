use std::{
    io,
    process::{Child, ChildStderr, ChildStdin, ChildStdout},
    time::Duration,
};

use tracing::warn;
use wait_timeout::ChildExt;

use crate::ManagedBackendError;

const DROP_KILL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct SupervisedBackendProcess {
    launch_label: String,
    child: Option<Child>,
    host_process_tree: HostProcessTree,
}

impl SupervisedBackendProcess {
    pub(crate) fn new(child: Child, launch_label: impl Into<String>) -> Self {
        Self {
            launch_label: launch_label.into(),
            child: Some(child),
            host_process_tree: HostProcessTree::none(),
        }
    }

    pub(crate) fn configure_host_tree(&mut self) -> Result<(), ManagedBackendError> {
        let child = self
            .child
            .as_ref()
            .expect("process owns child during setup");
        self.host_process_tree = HostProcessTree::create_for_child(child, &self.launch_label())?;
        Ok(())
    }
    pub(crate) fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    pub(crate) fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.as_mut()?.stdin.take()
    }

    pub(crate) fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.as_mut()?.stdout.take()
    }

    pub(crate) fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.as_mut()?.stderr.take()
    }

    pub(crate) fn has_exited(&mut self) -> bool {
        match self.try_has_exited() {
            Ok(exited) => exited,
            Err(error) => {
                warn!(
                    %error,
                    launch = %self.launch_label(),
                    "failed to query managed backend process status"
                );
                false
            }
        }
    }

    pub(crate) fn shutdown(
        &mut self,
        grace_timeout: Duration,
        kill_timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        let Some(mut child) = self.child.take() else {
            self.host_process_tree.release();
            return Ok(());
        };

        let result = self.shutdown_child(&mut child, grace_timeout, kill_timeout);
        match result {
            Ok(()) => {
                self.host_process_tree.release();
                Ok(())
            }
            Err(error) => {
                self.child = Some(child);
                Err(error)
            }
        }
    }

    fn try_has_exited(&mut self) -> Result<bool, ManagedBackendError> {
        let launch = self.launch_label();
        let Some(child) = self.child.as_mut() else {
            return Ok(true);
        };

        child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|source| ManagedBackendError::QueryProcessStatus { launch, source })
    }

    fn shutdown_child(
        &self,
        child: &mut Child,
        grace_timeout: Duration,
        kill_timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        if self.child_already_exited(child)? {
            return Ok(());
        }

        if self.wait_for_exit(child, grace_timeout)? {
            return Ok(());
        }

        let direct_kill_result = match child.kill() {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == io::ErrorKind::InvalidInput => {
                if self.wait_for_exit(child, Duration::ZERO)? {
                    return Ok(());
                }
                Err(self.terminate_error(source))
            }
            Err(source) => Err(self.terminate_error(source)),
        };

        if direct_kill_result.is_ok() && self.wait_for_exit(child, kill_timeout)? {
            return Ok(());
        }

        if self.host_process_tree.terminate(&self.launch_label())?
            && self.wait_for_exit(child, kill_timeout)?
        {
            return Ok(());
        }

        match direct_kill_result {
            Ok(()) => Err(ManagedBackendError::ShutdownTimeout {
                launch: self.launch_label(),
                timeout: kill_timeout,
            }),
            Err(error) => Err(error),
        }
    }

    fn child_already_exited(&self, child: &mut Child) -> Result<bool, ManagedBackendError> {
        child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|source| self.status_error(source))
    }

    fn wait_for_exit(
        &self,
        child: &mut Child,
        timeout: Duration,
    ) -> Result<bool, ManagedBackendError> {
        child
            .wait_timeout(timeout)
            .map(|status| status.is_some())
            .map_err(|source| self.status_error(source))
    }

    fn launch_label(&self) -> String {
        self.launch_label.clone()
    }

    fn status_error(&self, source: io::Error) -> ManagedBackendError {
        ManagedBackendError::QueryProcessStatus {
            launch: self.launch_label(),
            source,
        }
    }

    fn terminate_error(&self, source: io::Error) -> ManagedBackendError {
        ManagedBackendError::TerminateProcess {
            launch: self.launch_label(),
            source,
        }
    }
}

impl Drop for SupervisedBackendProcess {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown(Duration::ZERO, DROP_KILL_TIMEOUT) {
            warn!(%error, "failed to drop supervised backend process");
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Default)]
struct HostProcessTree {
    job: Option<windows::core::Owned<windows::Win32::Foundation::HANDLE>>,
}

// Windows kernel handles are valid process-local values and may be closed from
// any thread in the owning process. The wrapper owns only the job handle.
#[cfg(target_os = "windows")]
unsafe impl Send for HostProcessTree {}

#[cfg(target_os = "windows")]
impl HostProcessTree {
    fn none() -> Self {
        Self { job: None }
    }

    fn create_for_child(child: &Child, launch: &str) -> Result<Self, ManagedBackendError> {
        use std::{mem::size_of, os::windows::io::AsRawHandle};

        use windows::{
            Win32::{
                Foundation::HANDLE,
                System::JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                    SetInformationJobObject,
                },
            },
            core::{Owned, PCWSTR},
        };

        let job = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(|source| {
            ManagedBackendError::CreateProcessJob {
                launch: launch.to_string(),
                source: windows_io_error(source),
            }
        })?;
        let job = unsafe { Owned::new(job) };

        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                *job,
                JobObjectExtendedLimitInformation,
                &limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(|source| ManagedBackendError::ConfigureProcessJob {
            launch: launch.to_string(),
            source: windows_io_error(source),
        })?;

        let process_handle = HANDLE(child.as_raw_handle());
        unsafe { AssignProcessToJobObject(*job, process_handle) }.map_err(|source| {
            ManagedBackendError::AssignProcessToJob {
                launch: launch.to_string(),
                source: windows_io_error(source),
            }
        })?;

        Ok(Self { job: Some(job) })
    }

    fn terminate(&self, launch: &str) -> Result<bool, ManagedBackendError> {
        use windows::Win32::System::JobObjects::TerminateJobObject;

        let Some(job) = &self.job else {
            return Ok(false);
        };

        unsafe { TerminateJobObject(**job, 1) }.map_err(|source| {
            ManagedBackendError::TerminateProcessJob {
                launch: launch.to_string(),
                source: windows_io_error(source),
            }
        })?;
        Ok(true)
    }

    fn release(&mut self) {
        drop(self.job.take());
    }
}

#[cfg(target_os = "windows")]
fn windows_io_error(source: windows::core::Error) -> io::Error {
    io::Error::other(source.to_string())
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug, Default)]
struct HostProcessTree;

#[cfg(not(target_os = "windows"))]
impl HostProcessTree {
    fn none() -> Self {
        Self
    }

    fn create_for_child(_child: &Child, _launch: &str) -> Result<Self, ManagedBackendError> {
        Ok(Self)
    }

    fn terminate(&self, _launch: &str) -> Result<bool, ManagedBackendError> {
        Ok(false)
    }

    fn release(&mut self) {}
}
