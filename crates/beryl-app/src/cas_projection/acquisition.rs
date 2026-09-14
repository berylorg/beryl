use std::sync::Arc;

use crate::process_admission::{ProcessAdmissionReservation, ProcessExecutionAdmissionError};

use super::{LiveCommandAuthorizer, ProjectionServiceGeneration};

#[cfg(test)]
#[path = "../../tests/unit/ordinary_execution_custody.rs"]
mod tests;

#[derive(Clone, Debug)]
pub(super) struct ProjectionAcquisition {
    service_generation: ProjectionServiceGeneration,
    _reservation: Arc<ProcessAdmissionReservation>,
}

pub(super) struct OrdinaryExecutionCustody {
    pub(super) acquisition: ProjectionAcquisition,
    _worker: super::service_config::ProjectionWorkerPermit,
}

impl OrdinaryExecutionCustody {
    pub(super) fn admit(
        worker: super::service_config::ProjectionWorkerPermit,
        commands: &LiveCommandAuthorizer,
        command: &super::LiveCommandPermit,
    ) -> Result<Self, ProcessExecutionAdmissionError> {
        Ok(Self {
            acquisition: ProjectionAcquisition::admit_from(commands, command)?,
            _worker: worker,
        })
    }
}

impl ProjectionAcquisition {
    #[cfg(feature = "test-faults")]
    pub(super) fn pause_for_test(&self, stage: super::test_faults::AcquisitionBarrierStage) {
        super::test_faults::pause_acquisition(self.service_generation, stage);
    }

    pub(super) fn admit(
        commands: &LiveCommandAuthorizer,
    ) -> Result<Self, ProcessExecutionAdmissionError> {
        let command = commands.authorize()?;
        Self::admit_from(commands, &command)
    }

    pub(super) fn admit_from(
        commands: &LiveCommandAuthorizer,
        command: &super::LiveCommandPermit,
    ) -> Result<Self, ProcessExecutionAdmissionError> {
        Ok(Self {
            service_generation: commands.service_generation(),
            _reservation: Arc::new(commands.execution_candidate_from(command)?.reserve()?),
        })
    }

    pub(super) fn belongs_to(&self, commands: &LiveCommandAuthorizer) -> bool {
        self.service_generation == commands.service_generation()
    }
}

impl From<ProcessExecutionAdmissionError> for super::ProjectionCoordinatorError {
    fn from(error: ProcessExecutionAdmissionError) -> Self {
        match error {
            ProcessExecutionAdmissionError::Process(error) => Self::AcquisitionFenced(error),
            ProcessExecutionAdmissionError::Service(error) => {
                Self::AcquisitionServiceUnavailable(error)
            }
        }
    }
}
