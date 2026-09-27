use super::{IdleShutdownError, RunningProcessOwner, ShutdownIntent};
use crate::{
    app_services::AppServiceCloseError, cas_projection::ShutdownWorkObservation,
    startup_owner::RunningExitRequest,
};
use beryl_model::WindowId;
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ExitWorkRoute {
    Admitted,
    Confirming,
}

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
    #[error("Exit confirmation could not start: {0}")]
    Confirmation(String),
}

impl RunningProcessOwner {
    pub(crate) fn route_exit_work(
        owner: &Rc<RefCell<Self>>,
        request: &mut RunningExitRequest,
        result: Result<ShutdownWorkObservation, AppServiceCloseError>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<ExitWorkRoute, ExitWorkError> {
        let classified = owner
            .borrow_mut()
            .classify_exit_work(request, result, app)?;
        match classified {
            ExitWorkClassification::Admitted => Ok(ExitWorkRoute::Admitted),
            ExitWorkClassification::ConfirmationRequired {
                invoking,
                observation,
            } => {
                Self::begin_shutdown_confirmation(
                    owner,
                    invoking,
                    ShutdownIntent::ApplicationExit,
                    observation,
                    app,
                    completed,
                )
                .map_err(ExitWorkError::Confirmation)?;
                owner
                    .borrow_mut()
                    .confirmation
                    .as_mut()
                    .unwrap()
                    .exit_request = Some(request.identity());
                Ok(ExitWorkRoute::Confirming)
            }
        }
    }

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
