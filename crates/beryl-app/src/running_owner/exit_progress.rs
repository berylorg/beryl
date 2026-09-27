use super::{RunningProcessOwner, RunningShutdownStatus, ShutdownIntent};
use crate::{
    app_services::{AppServiceCloseError, AppServiceShutdownProgress},
    cas_projection::ProjectionCancellationToken,
    startup_owner::RunningExitRequest,
};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitProgressError {
    #[error("Exit progress request is unavailable: {0}")]
    Request(String),
    #[error("the Exit request has no matching admitted shutdown intent")]
    Intent,
    #[error("Exit progress could not be scheduled: {0}")]
    Scheduling(String),
    #[error("settled Exit progress is unavailable")]
    Unavailable,
    #[error(transparent)]
    Service(#[from] AppServiceCloseError),
}

impl RunningProcessOwner {
    pub(crate) fn advance_exit(
        owner: &Rc<RefCell<Self>>,
        mut request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<AppServiceShutdownProgress, ExitProgressError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), (RunningExitRequest, ExitProgressError)> {
        {
            let owner = owner.borrow();
            let invoking = match owner.resolve_exit_window(&mut request, app) {
                Ok(invoking) => invoking,
                Err(error) => return Err((request, ExitProgressError::Request(error))),
            };
            if owner.shutdown_status()
                != Some((
                    invoking,
                    ShutdownIntent::ApplicationExit,
                    RunningShutdownStatus::Admitted,
                ))
            {
                return Err((request, ExitProgressError::Intent));
            }
        }
        let delivery = Rc::new(RefCell::new(Some((request, completed))));
        let settled = delivery.clone();
        match Self::advance_shutdown(owner, cancellation, app, move |owner, app| {
            let result = owner
                .borrow_mut()
                .take_shutdown_progress()
                .ok_or(ExitProgressError::Unavailable)
                .and_then(|result| result.map_err(ExitProgressError::Service));
            let (request, completed) = settled.borrow_mut().take().unwrap();
            completed(owner, request, result, app);
        }) {
            Ok(()) => Ok(()),
            Err(error) => {
                let (request, _) = delivery.borrow_mut().take().unwrap();
                Err((request, ExitProgressError::Scheduling(error)))
            }
        }
    }
}
