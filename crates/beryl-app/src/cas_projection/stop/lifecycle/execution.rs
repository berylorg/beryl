use super::AcceptedLifecycleYield;
use crate::process_admission::{
    ProcessAdmissionError, ProcessAdmissionReservation, ProcessExecutionAdmissionError,
};

impl AcceptedLifecycleYield {
    pub(super) fn execution_cancelled(&self) -> bool {
        self.execution.as_ref().is_some_and(|execution| {
            matches!(
                execution.commit(|| ()),
                Err(ProcessExecutionAdmissionError::Process(
                    ProcessAdmissionError::Fenced | ProcessAdmissionError::Stale
                ))
            )
        })
    }

    pub(in crate::cas_projection) fn reserve_execution(
        &mut self,
    ) -> Result<Option<ProcessAdmissionReservation>, ProcessExecutionAdmissionError> {
        if !self.continuation_pending {
            return Ok(None);
        }
        let Some(execution) = &self.execution else {
            return Ok(None);
        };
        match execution.reserve() {
            Ok(reservation) => Ok(Some(reservation)),
            Err(ProcessExecutionAdmissionError::Process(
                ProcessAdmissionError::Fenced | ProcessAdmissionError::Stale,
            )) => {
                self.cancel_continuation();
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}
