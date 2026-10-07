use std::{
    thread,
    time::{Duration, Instant},
};

use crate::{
    ManagedBackendError,
    auth::ManagedBackendAuthMaterial,
    managed_process::SupervisedBackendProcess,
    wsl_supervision::{ATTEMPT_TIMEOUT, WslSupervision},
};

#[derive(Debug)]
pub(crate) enum ManagedProcessOwner {
    Host(SupervisedBackendProcess),
    Wsl(WslSupervision),
}

impl ManagedProcessOwner {
    pub(crate) fn shutdown(&mut self) -> Result<(), ManagedBackendError> {
        match self {
            Self::Host(process) => process.shutdown(Duration::ZERO, ATTEMPT_TIMEOUT),
            Self::Wsl(process) => process.shutdown(Instant::now() + ATTEMPT_TIMEOUT),
        }
    }
    pub(crate) fn process_id(&self) -> Option<u32> {
        match self {
            Self::Host(process) => process.process_id(),
            Self::Wsl(process) => process.process_id(),
        }
    }
    pub(crate) fn has_exited(&mut self) -> bool {
        match self {
            Self::Host(process) => process.has_exited(),
            Self::Wsl(process) => process.has_exited(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ManagedBackendLaunchCleanup {
    pub(crate) process: Option<ManagedProcessOwner>,
    pub(crate) auth: Option<ManagedBackendAuthMaterial>,
    pub(crate) stderr_reader: Option<thread::JoinHandle<()>>,
    pub(crate) process_closed: bool,
    pub(crate) complete: bool,
    pub(crate) stderr_join_failed: bool,
    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fail_stderr_join_for_lifecycle_test: bool,
}

impl ManagedBackendLaunchCleanup {
    pub(crate) fn new(
        process: Option<ManagedProcessOwner>,
        auth: Option<ManagedBackendAuthMaterial>,
    ) -> Self {
        Self {
            process,
            auth,
            stderr_reader: None,
            process_closed: false,
            complete: false,
            stderr_join_failed: false,
            #[cfg(feature = "lifecycle-test-support")]
            fail_stderr_join_for_lifecycle_test: false,
        }
    }

    pub fn shutdown(&mut self) -> Result<(), ManagedBackendError> {
        if self.complete {
            return Ok(());
        }
        if !self.process_closed {
            if let Some(process) = self.process.as_mut() {
                process.shutdown()?;
            }
            self.process_closed = true;
        }
        if let Some(auth) = self.auth.as_mut() {
            auth.cleanup()?;
        }
        if self.stderr_join_failed {
            return Err(ManagedBackendError::StderrReaderPanicked);
        }
        if let Some(reader) = self.stderr_reader.as_ref() {
            let deadline = Instant::now() + ATTEMPT_TIMEOUT;
            while !reader.is_finished() {
                if Instant::now() >= deadline {
                    return Err(ManagedBackendError::WslCompanionJoin);
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
        if let Some(reader) = self.stderr_reader.take() {
            if reader.join().is_err() {
                self.stderr_join_failed = true;
                return Err(ManagedBackendError::StderrReaderPanicked);
            }
        }
        #[cfg(feature = "lifecycle-test-support")]
        if std::mem::take(&mut self.fail_stderr_join_for_lifecycle_test) {
            return Err(ManagedBackendError::StderrReaderPanicked);
        }
        self.complete = true;
        Ok(())
    }
    pub fn is_complete(&self) -> bool {
        self.complete
    }
}

impl Drop for ManagedBackendLaunchCleanup {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            if !self.process_closed {
                if let Some(auth) = self.auth.as_mut() {
                    auth.preserve_file_on_drop();
                }
            }
            tracing::warn!(%error, "managed backend cleanup remains unproved");
        }
    }
}

#[derive(Debug)]
pub struct ManagedBackendLaunchFailure {
    error: ManagedBackendError,
    cleanup: Option<ManagedBackendLaunchCleanup>,
}

impl ManagedBackendLaunchFailure {
    pub(crate) fn new(error: ManagedBackendError, cleanup: ManagedBackendLaunchCleanup) -> Self {
        Self {
            error,
            cleanup: Some(cleanup),
        }
    }
    pub fn error(&self) -> &ManagedBackendError {
        &self.error
    }
    pub fn cleanup_pending(&self) -> bool {
        self.cleanup
            .as_ref()
            .is_some_and(|cleanup| !cleanup.is_complete())
    }
    pub fn shutdown(&mut self) -> Result<(), ManagedBackendError> {
        if let Some(cleanup) = self.cleanup.as_mut() {
            cleanup.shutdown()?;
        }
        Ok(())
    }
    pub fn into_parts(self) -> (ManagedBackendError, Option<ManagedBackendLaunchCleanup>) {
        (self.error, self.cleanup)
    }
}
impl From<ManagedBackendError> for ManagedBackendLaunchFailure {
    fn from(error: ManagedBackendError) -> Self {
        Self {
            error,
            cleanup: None,
        }
    }
}
impl std::fmt::Display for ManagedBackendLaunchFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}
impl std::error::Error for ManagedBackendLaunchFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
