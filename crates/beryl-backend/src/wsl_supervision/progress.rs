use crate::ManagedBackendError;
use beryl_wsl_supervisor::{Frame, Role, WorkloadResult};

#[derive(Debug, Default)]
pub(crate) struct ControlProgress {
    pub(crate) supervisor_ready: bool,
    pub(crate) workload_started: bool,
    pub(crate) namespace_closed: Option<WorkloadResult>,
    pub(crate) companions_closed: bool,
    pub(crate) pre_context_failure: bool,
    invalid: bool,
}

impl ControlProgress {
    pub(crate) fn retire(
        &mut self,
        request: Result<(), ManagedBackendError>,
        mut receive: impl FnMut() -> Result<Frame, ManagedBackendError>,
    ) -> Result<(), ManagedBackendError> {
        if self.invalid {
            return Err(ManagedBackendError::WslSupervisionUnavailable);
        }
        // A retired writer does not invalidate closure already held on the original reader.
        let _ = request;
        while !self.companions_closed && !self.pre_context_failure {
            match self.accept(receive()?) {
                Ok(()) => {}
                Err(ManagedBackendError::WslSupervisionFailure { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub(crate) fn accept(&mut self, frame: Frame) -> Result<(), ManagedBackendError> {
        if self.invalid {
            return Err(ManagedBackendError::WslSupervisionUnavailable);
        }
        match frame {
            Frame::Ready {
                role: Role::Supervisor,
            } if !self.supervisor_ready
                && !self.pre_context_failure
                && self.namespace_closed.is_none() =>
            {
                self.supervisor_ready = true
            }
            Frame::WorkloadStarted
                if self.supervisor_ready
                    && !self.workload_started
                    && self.namespace_closed.is_none() =>
            {
                self.workload_started = true
            }
            Frame::ShutdownPending if self.supervisor_ready && !self.companions_closed => {}
            Frame::OwnedNamespaceClosed { result }
                if self.supervisor_ready && self.namespace_closed.is_none() =>
            {
                self.namespace_closed = Some(result)
            }
            Frame::LinuxCompanionsClosed
                if self.namespace_closed.is_some() && !self.companions_closed =>
            {
                self.companions_closed = true
            }
            Frame::Failure { kind, errno } if !self.companions_closed => {
                if !self.supervisor_ready {
                    self.pre_context_failure = true;
                }
                return Err(ManagedBackendError::WslSupervisionFailure { kind, errno });
            }
            _ => {
                self.invalid = true;
                return Err(ManagedBackendError::WslSupervisionUnavailable);
            }
        }
        Ok(())
    }
}
