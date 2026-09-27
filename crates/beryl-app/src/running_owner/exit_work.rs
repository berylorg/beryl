use super::{IdleShutdownError, RunningProcessOwner, ShutdownIntent};
use crate::{
    app_services::AppServiceCloseError, cas_projection::ShutdownWorkObservation,
    startup_owner::RunningExitRequest,
};
use beryl_model::WindowId;
use gpui::App;

pub(crate) enum ExitWorkClassification {
    Admitted,
    ConfirmationRequired {
        invoking: WindowId,
        observation: ShutdownWorkObservation,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitWorkError {
    #[error("Exit request cannot be classified: {0}")]
    Request(String),
    #[error("the running owner already retains shutdown observation or intent custody")]
    IntentBusy,
    #[error(transparent)]
    Observation(#[from] AppServiceCloseError),
    #[error(transparent)]
    Admission(#[from] IdleShutdownError),
}

impl RunningProcessOwner {
    pub(crate) fn classify_exit_work(
        &mut self,
        request: &mut RunningExitRequest,
        result: Result<ShutdownWorkObservation, AppServiceCloseError>,
        app: &App,
    ) -> Result<ExitWorkClassification, ExitWorkError> {
        let invoking = self
            .resolve_exit_window(request, app)
            .map_err(ExitWorkError::Request)?;
        if self.observing_initial_work
            || self.confirmation.is_some()
            || self.shutdown.is_some()
            || self.progress.is_some()
        {
            return Err(ExitWorkError::IntentBusy);
        }
        let observation = result?;
        if observation.has_work() {
            return Ok(ExitWorkClassification::ConfirmationRequired {
                invoking,
                observation,
            });
        }
        self.try_begin_idle_shutdown(invoking, ShutdownIntent::ApplicationExit, &observation, app)?;
        Ok(ExitWorkClassification::Admitted)
    }
}
