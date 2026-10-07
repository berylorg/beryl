use crate::{
    ManagedBackendError, ManagedBackendLaunchCleanup, WslSupervisorArtifact,
    launch_cleanup::ManagedProcessOwner,
    wsl_supervision::{ATTEMPT_TIMEOUT, WslSupervision},
};
use beryl_model::WslDistributionName;
use beryl_wsl_supervisor::{Frame, ObservationKind};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WslFilesystemOperation {
    Executable(String),
    Directory(String),
    UserHome,
}

impl WslFilesystemOperation {
    fn frame(&self, timeout: Duration) -> Result<Frame, ManagedBackendError> {
        if timeout.is_zero() || timeout > Duration::from_secs(30) || timeout.as_millis() == 0 {
            return Err(ManagedBackendError::WslObservationInvalid);
        }
        let (kind, path) = match self {
            Self::Executable(path) => (ObservationKind::Executable, Some(path)),
            Self::Directory(path) => (ObservationKind::Directory, Some(path)),
            Self::UserHome => (ObservationKind::Home, None),
        };
        if path
            .as_ref()
            .is_some_and(|path| !path.starts_with('/') || path.len() > 4096 || path.contains('\0'))
        {
            return Err(ManagedBackendError::WslObservationInvalid);
        }
        Ok(Frame::Observe {
            kind,
            path: path.cloned(),
            timeout_ms: timeout.as_millis() as u32,
        })
    }
}

pub struct WslFilesystemObservation;

impl WslFilesystemObservation {
    pub fn observe(
        artifact: Arc<WslSupervisorArtifact>,
        distribution: &WslDistributionName,
        operation: WslFilesystemOperation,
        timeout: Duration,
        cancellation: &AtomicBool,
    ) -> Result<String, WslFilesystemObservationFailure> {
        let frame = operation
            .frame(timeout)
            .map_err(WslFilesystemObservationFailure::without_owner)?;
        let deadline = Instant::now() + timeout;
        if cancellation.load(std::sync::atomic::Ordering::Acquire) {
            return Err(WslFilesystemObservationFailure::without_owner(
                ManagedBackendError::WslObservationCancelled,
            ));
        }
        let mut owner = WslSupervision::new(artifact, distribution.as_str().into(), "/".into())
            .map_err(WslFilesystemObservationFailure::without_owner)?;
        let observed = owner
            .launch(
                frame,
                deadline.min(Instant::now() + ATTEMPT_TIMEOUT),
                Some(cancellation),
            )
            .and_then(|()| owner.wait_observation(deadline, cancellation));
        let disposed = owner.shutdown(deadline.min(Instant::now() + ATTEMPT_TIMEOUT));
        match (observed, disposed) {
            (Ok(path), Ok(())) => Ok(path),
            (Err(error), Ok(())) => Err(WslFilesystemObservationFailure::without_owner(error)),
            (result, Err(cleanup_error)) => {
                let error = result.err().unwrap_or(cleanup_error);
                let cleanup =
                    ManagedBackendLaunchCleanup::new(Some(ManagedProcessOwner::Wsl(owner)), None);
                Err(WslFilesystemObservationFailure {
                    error,
                    cleanup: Some(cleanup),
                })
            }
        }
    }
}

#[derive(Debug)]
pub struct WslFilesystemObservationFailure {
    error: ManagedBackendError,
    cleanup: Option<ManagedBackendLaunchCleanup>,
}
impl WslFilesystemObservationFailure {
    fn without_owner(error: ManagedBackendError) -> Self {
        Self {
            error,
            cleanup: None,
        }
    }
    pub fn error(&self) -> &ManagedBackendError {
        &self.error
    }
    pub fn into_parts(self) -> (ManagedBackendError, Option<ManagedBackendLaunchCleanup>) {
        (self.error, self.cleanup)
    }
}
impl std::fmt::Display for WslFilesystemObservationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}
impl std::error::Error for WslFilesystemObservationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
