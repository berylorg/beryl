use super::admission::ConfirmedShutdownError;
use super::{
    ConfirmedShutdownAdmission, ExitConfirmationError, ExitConfirmationRoute, ExitObservationError,
    ExitWorkError, ExitWorkRoute, RunningProcessOwner,
};
use crate::{cas_projection::ProjectionCancellationToken, startup_owner::RunningExitRequest};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ExitRoutingCompletion {
    Admitted,
    Cancelled,
    ConfirmedObservationCancelled,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitRoutingError {
    #[error(transparent)]
    Work(#[from] ExitWorkError),
    #[error(transparent)]
    Confirmation(#[from] ExitConfirmationError),
    #[error("settled Exit confirmation is unavailable")]
    ConfirmationUnavailable,
    #[error("confirmed Exit observation could not be scheduled: {0}")]
    ObservationScheduling(String),
    #[error(transparent)]
    ConfirmedObservation(#[from] ConfirmedShutdownError),
}

impl RunningProcessOwner {
    pub(crate) fn observe_and_route_exit(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<ExitRoutingCompletion, ExitRoutingError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), (RunningExitRequest, ExitObservationError)> {
        Self::observe_exit_work(
            owner,
            request,
            cancellation.clone(),
            app,
            move |owner, request, observation, app| {
                let delivery = Rc::new(RefCell::new(Some((request, completed))));
                let settled = delivery.clone();
                let routed = {
                    let mut delivery = delivery.borrow_mut();
                    let (request, _) = delivery.as_mut().unwrap();
                    Self::route_exit_work(owner, request, observation, app, move |owner, app| {
                        let result = {
                            let mut delivery = settled.borrow_mut();
                            let (request, _) = delivery.as_mut().unwrap();
                            owner.borrow_mut().consume_exit_confirmation(request, app)
                        };
                        let result = match result {
                            Ok(Some(ExitConfirmationRoute::Cancelled)) => {
                                Ok(ExitRoutingCompletion::Cancelled)
                            }
                            Ok(Some(ExitConfirmationRoute::AwaitingObservation)) => {
                                let observed = settled.clone();
                                match Self::observe_confirmed_shutdown(
                                    owner,
                                    cancellation,
                                    app,
                                    move |owner, result, app| {
                                        let (request, completed) =
                                            observed.borrow_mut().take().unwrap();
                                        let result = result
                                            .map(|admission| match admission {
                                                ConfirmedShutdownAdmission::Admitted => {
                                                    ExitRoutingCompletion::Admitted
                                                }
                                                ConfirmedShutdownAdmission::Cancelled => {
                                                    ExitRoutingCompletion::ConfirmedObservationCancelled
                                                }
                                            })
                                            .map_err(ExitRoutingError::ConfirmedObservation);
                                        completed(owner, request, result, app);
                                    },
                                ) {
                                    Ok(()) => return,
                                    Err(error) => {
                                        Err(ExitRoutingError::ObservationScheduling(error))
                                    }
                                }
                            }
                            Ok(None) => Err(ExitRoutingError::ConfirmationUnavailable),
                            Err(error) => Err(ExitRoutingError::Confirmation(error)),
                        };
                        let (request, completed) = settled.borrow_mut().take().unwrap();
                        completed(owner, request, result, app);
                    })
                };
                let result = match routed {
                    Ok(ExitWorkRoute::Confirming) => return,
                    Ok(ExitWorkRoute::Admitted) => Ok(ExitRoutingCompletion::Admitted),
                    Err(error) => Err(ExitRoutingError::Work(error)),
                };
                let (request, completed) = delivery.borrow_mut().take().unwrap();
                completed(owner, request, result, app);
            },
        )
    }
}
