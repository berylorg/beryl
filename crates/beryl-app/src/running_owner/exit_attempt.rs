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

#[derive(Debug)]
pub(crate) struct ExitAttemptOutcome {
    pub(crate) result: Result<ExitAttemptCompletion, ExitAttemptError>,
    pub(crate) command_completed: bool,
}

impl RunningProcessOwner {
    pub(crate) fn run_exit_attempt(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitRequest, ExitAttemptOutcome, &mut App)
        + 'static,
    ) -> Result<(), (RunningExitRequest, ExitObservationError)> {
        Self::observe_and_drive_exit(
            owner,
            request,
            cancellation,
            app,
            move |owner, mut request, result, app| {
                let release_unadmitted = matches!(
                    &result,
                    Ok(ExitAttemptCompletion::ConfirmedObservationCancelled)
                        | Err(ExitAttemptError::Routing(
                            ExitRoutingError::ObservationScheduling(_)
                                | ExitRoutingError::ConfirmedObservation(_)
                        ))
                );
                if release_unadmitted {
                    let mut owner = owner.borrow_mut();
                    let invoking = owner
                        .process
                        .commands
                        .bind_invoking_window(&mut request, None);
                    if owner
                        .shutdown_status()
                        .is_some_and(|(window, intent, status)| {
                            invoking == Some(window)
                                && intent == super::ShutdownIntent::ApplicationExit
                                && status == super::RunningShutdownStatus::AwaitingObservation
                        })
                    {
                        let _ = owner.end_unadmitted_shutdown();
                    }
                }
                let can_complete = !matches!(
                    &result,
                    Ok(ExitAttemptCompletion::Progress(
                        AppServiceShutdownProgress::Ready | AppServiceShutdownProgress::Waiting
                    ))
                );
                let command_completed = can_complete && Self::finish_exit(owner, &request);
                completed(
                    owner,
                    request,
                    ExitAttemptOutcome {
                        result,
                        command_completed,
                    },
                    app,
                );
            },
        )
    }

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
