use super::RunningProcessOwner;
use crate::{
    app_services::AppServiceCloseError,
    cas_projection::{ProjectionCancellationToken, ShutdownWorkObservation},
    startup_owner::RunningExitRequest,
};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitObservationError {
    #[error("the Exit request cannot observe work: {0}")]
    Request(String),
    #[error("Exit work observation could not be scheduled: {0}")]
    Scheduling(String),
}

impl RunningProcessOwner {
    pub(crate) fn observe_exit_work(
        owner: &Rc<RefCell<Self>>,
        mut request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<ShutdownWorkObservation, AppServiceCloseError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), (RunningExitRequest, ExitObservationError)> {
        let job = {
            let mut owner = owner.borrow_mut();
            if let Err(error) = owner.resolve_exit_window(&mut request, app) {
                return Err((request, ExitObservationError::Request(error)));
            }
            match owner.prepare_initial_observation() {
                Ok(job) => job,
                Err(error) => return Err((request, ExitObservationError::Scheduling(error))),
            }
        };
        Self::spawn_initial_observation(
            owner,
            job,
            cancellation,
            app,
            move |owner, result, app| completed(owner, request, result, app),
            || {},
            |_| {},
        );
        Ok(())
    }
}
