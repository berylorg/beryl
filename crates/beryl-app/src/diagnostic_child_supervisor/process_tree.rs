use std::{io, process::Child};

use super::DiagnosticChildSupervisorError;

#[cfg(test)]
use super::{AcceptanceStartupFailureStage, test_support::AcceptanceTestControl};

#[cfg(target_os = "windows")]
pub(super) struct DiagnosticHostProcessTree {
    job: Option<windows::core::Owned<windows::Win32::Foundation::HANDLE>>,
}

#[cfg(target_os = "windows")]
unsafe impl Send for DiagnosticHostProcessTree {}

#[cfg(target_os = "windows")]
impl DiagnosticHostProcessTree {
    pub(super) fn empty() -> Self {
        Self { job: None }
    }

    pub(super) fn create_for_child(child: &Child) -> Result<Self, DiagnosticChildSupervisorError> {
        let (tree, error) = Self::create_for_child_retaining(
            child,
            #[cfg(test)]
            None,
        );
        match error {
            Some(error) => Err(error),
            None => Ok(tree),
        }
    }

    pub(super) fn create_for_child_retaining(
        child: &Child,
        #[cfg(test)] mut acceptance_test_control: Option<&mut AcceptanceTestControl>,
    ) -> (Self, Option<DiagnosticChildSupervisorError>) {
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

        let mut tree = Self { job: None };
        #[cfg(test)]
        if acceptance_test_control.as_mut().is_some_and(|control| {
            control.force_startup_failure(child.id(), AcceptanceStartupFailureStage::JobCreate)
        }) {
            return (
                tree,
                Some(DiagnosticChildSupervisorError::CreateProcessJob {
                    source: io::Error::other("forced Job creation failure for test"),
                }),
            );
        }
        let job = match unsafe { CreateJobObjectW(None, PCWSTR::null()) } {
            Ok(job) => job,
            Err(source) => {
                return (
                    tree,
                    Some(DiagnosticChildSupervisorError::CreateProcessJob {
                        source: windows_io_error(source),
                    }),
                );
            }
        };
        let job = unsafe { Owned::new(job) };
        tree.job = Some(job);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        #[cfg(test)]
        if acceptance_test_control.as_mut().is_some_and(|control| {
            control.force_startup_failure(child.id(), AcceptanceStartupFailureStage::JobConfigure)
        }) {
            return (
                tree,
                Some(DiagnosticChildSupervisorError::ConfigureProcessJob {
                    source: io::Error::other("forced Job configuration failure for test"),
                }),
            );
        }
        if let Err(source) = unsafe {
            SetInformationJobObject(
                **tree.job.as_ref().expect("created process Job is retained"),
                JobObjectExtendedLimitInformation,
                &limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } {
            return (
                tree,
                Some(DiagnosticChildSupervisorError::ConfigureProcessJob {
                    source: windows_io_error(source),
                }),
            );
        }
        #[cfg(test)]
        if let Some(control) = acceptance_test_control.as_ref() {
            control.observe_job_configured(child.id());
        }
        #[cfg(test)]
        if acceptance_test_control.as_mut().is_some_and(|control| {
            control.force_startup_failure(child.id(), AcceptanceStartupFailureStage::JobAssign)
        }) {
            return (
                tree,
                Some(DiagnosticChildSupervisorError::AssignProcessToJob {
                    source: io::Error::other("forced Job assignment failure for test"),
                }),
            );
        }
        if let Err(source) = unsafe {
            AssignProcessToJobObject(
                **tree
                    .job
                    .as_ref()
                    .expect("configured process Job is retained"),
                HANDLE(child.as_raw_handle()),
            )
        } {
            return (
                tree,
                Some(DiagnosticChildSupervisorError::AssignProcessToJob {
                    source: windows_io_error(source),
                }),
            );
        }
        #[cfg(test)]
        if let Some(control) = acceptance_test_control {
            if let Err(error) = control.run_job_assignment(child.id()) {
                return (tree, Some(error));
            }
        }
        (tree, None)
    }

    pub(super) fn terminate(&self) -> Result<bool, DiagnosticChildSupervisorError> {
        use windows::Win32::System::JobObjects::TerminateJobObject;

        let Some(job) = &self.job else {
            return Ok(false);
        };
        unsafe { TerminateJobObject(**job, 1) }.map_err(|source| {
            DiagnosticChildSupervisorError::TerminateProcessJob {
                source: windows_io_error(source),
            }
        })?;
        Ok(true)
    }

    pub(super) fn release(&mut self) {
        drop(self.job.take());
    }

    #[cfg(test)]
    pub(super) fn has_job_for_test(&self) -> bool {
        self.job.is_some()
    }

    #[cfg(test)]
    pub(super) fn empty_for_test() -> Self {
        Self::empty()
    }
}

#[cfg(target_os = "windows")]
fn windows_io_error(source: windows::core::Error) -> io::Error {
    io::Error::other(source.to_string())
}

#[cfg(not(target_os = "windows"))]
pub(super) struct DiagnosticHostProcessTree;

#[cfg(not(target_os = "windows"))]
impl DiagnosticHostProcessTree {
    pub(super) fn empty() -> Self {
        Self
    }

    pub(super) fn create_for_child(_child: &Child) -> Result<Self, DiagnosticChildSupervisorError> {
        Ok(Self)
    }

    pub(super) fn create_for_child_retaining(
        _child: &Child,
        #[cfg(test)] _acceptance_test_control: Option<&mut AcceptanceTestControl>,
    ) -> (Self, Option<DiagnosticChildSupervisorError>) {
        (Self, None)
    }

    pub(super) fn terminate(&self) -> Result<bool, DiagnosticChildSupervisorError> {
        Ok(false)
    }

    pub(super) fn release(&mut self) {}

    #[cfg(test)]
    pub(super) fn has_job_for_test(&self) -> bool {
        false
    }

    #[cfg(test)]
    pub(super) fn empty_for_test() -> Self {
        Self::empty()
    }
}
