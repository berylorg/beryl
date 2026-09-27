use super::{
    ExitObservationError, ExitProgressError, ExitRoutingCompletion, ExitRoutingError,
    RunningProcessOwner,
};
use crate::{
    app_services::AppServiceShutdownProgress, cas_projection::ProjectionCancellationToken,
    startup_owner::RunningExitRequest,
};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug)]
pub(crate) enum ExitAttemptCompletion {
    Cancelled,
    ConfirmedObservationCancelled,
    Progress(AppServiceShutdownProgress),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitAttemptError {
    #[error(transparent)]
    Routing(#[from] ExitRoutingError),
    #[error(transparent)]
    Progress(#[from] ExitProgressError),
}

impl RunningProcessOwner {
    pub(crate) fn observe_and_drive_exit(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<ExitAttemptCompletion, ExitAttemptError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), (RunningExitRequest, ExitObservationError)> {
        Self::observe_and_route_exit(
            owner,
            request,
            cancellation.clone(),
            app,
            move |owner, request, result, app| {
                let result = match result {
                    Ok(ExitRoutingCompletion::Admitted) => {
                        let delivery = Rc::new(RefCell::new(Some(completed)));
                        let settled = delivery.clone();
                        if let Err((request, error)) = Self::drive_exit(
                            owner,
                            request,
                            cancellation,
                            app,
                            move |owner, request, result, app| {
                                let completed = settled.borrow_mut().take().unwrap();
                                completed(
                                    owner,
                                    request,
                                    result
                                        .map(ExitAttemptCompletion::Progress)
                                        .map_err(ExitAttemptError::Progress),
                                    app,
                                );
                            },
                        ) {
                            let completed = delivery.borrow_mut().take().unwrap();
                            completed(owner, request, Err(ExitAttemptError::Progress(error)), app);
                        }
                        return;
                    }
                    Ok(ExitRoutingCompletion::Cancelled) => Ok(ExitAttemptCompletion::Cancelled),
                    Ok(ExitRoutingCompletion::ConfirmedObservationCancelled) => {
                        Ok(ExitAttemptCompletion::ConfirmedObservationCancelled)
                    }
                    Err(error) => Err(ExitAttemptError::Routing(error)),
                };
                completed(owner, request, result, app);
            },
        )
    }
}
