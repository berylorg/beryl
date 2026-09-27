use super::super::flight_registry::FlightRegistry;
use super::*;
use crate::cas_projection::{connection::registry, runtime_work::RuntimeWorkError};
use crate::process_admission::ProcessAdmissionFence;

impl ProjectionConnectionService {
    pub(crate) fn try_admit_observed_shutdown(
        &self,
        sessions: &ScheduledExecutionSessions,
        observation: &ShutdownWorkObservation,
    ) -> Result<ProcessAdmissionFence, ShutdownWorkError> {
        self.try_admit_observed_shutdown_with_window(sessions, observation, None)
    }

    pub(crate) fn try_admit_observed_shutdown_with_window(
        &self,
        sessions: &ScheduledExecutionSessions,
        observation: &ShutdownWorkObservation,
        window: Option<crate::window_acquisition::WindowShutdownAdmission<'_>>,
    ) -> Result<ProcessAdmissionFence, ShutdownWorkError> {
        let read = self.work_read();
        let revision = &observation.revision;
        read.validate_shutdown_source_identity(revision)?;
        let home = read.home.as_deref().ok_or(RuntimeWorkError::Closed)?;
        let compaction = read
            .context_compaction
            .as_ref()
            .ok_or(RuntimeWorkError::Closed)?;
        let closing = read.command_authorizer.prepare_process_closing()?;
        if let Some(window) = window {
            window.validate(&closing)?;
        }
        let _sessions = sessions.try_hold_work_revision(&revision.required.sessions)?;
        let _controls = compaction.try_hold_control_revisions(
            &read.stop_coordinator,
            revision.required.controls.stop.stamp,
            revision.required.controls.compaction.stamp,
        )?;
        let _flights = FlightRegistry::try_hold_work_revision(revision.flights)?;
        let _loaded = registry::try_hold_work_revision(revision.loaded)?;
        let _commands = read.command_authorizer.try_hold_work_open()?;
        Ok(self.connection_work_boundary().try_elect(
            &observation.connection_interval,
            || {
                home.try_elect_observed_coherent(
                    &observation.home_interval,
                    read.home_generation,
                    || closing.publish(),
                )
            },
        )??)
    }
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/shutdown_admission.rs"]
mod tests;
