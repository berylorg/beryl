use super::{
    ExitConfirmationError, ExitConfirmationRoute, ExitObservationError, ExitWorkError,
    ExitWorkRoute, RunningProcessOwner,
};
use crate::{cas_projection::ProjectionCancellationToken, startup_owner::RunningExitRequest};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ExitRoutingCompletion {
    Admitted,
    Cancelled,
    AwaitingObservation,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitRoutingError {
    #[error(transparent)]
    Work(#[from] ExitWorkError),
    #[error(transparent)]
    Confirmation(#[from] ExitConfirmationError),
    #[error("settled Exit confirmation is unavailable")]
    ConfirmationUnavailable,
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
            cancellation,
            app,
            move |owner, request, observation, app| {
                let delivery = Rc::new(RefCell::new(Some((request, completed))));
                let settled = delivery.clone();
                let routed = {
                    let mut delivery = delivery.borrow_mut();
                    let (request, _) = delivery.as_mut().unwrap();
                    Self::route_exit_work(owner, request, observation, app, move |owner, app| {
                        let (mut request, completed) = settled.borrow_mut().take().unwrap();
                        let result = owner
                            .borrow_mut()
                            .consume_exit_confirmation(&mut request, app);
                        let result = match result {
                            Ok(Some(ExitConfirmationRoute::Cancelled)) => {
                                Ok(ExitRoutingCompletion::Cancelled)
                            }
                            Ok(Some(ExitConfirmationRoute::AwaitingObservation)) => {
                                Ok(ExitRoutingCompletion::AwaitingObservation)
                            }
                            Ok(None) => Err(ExitRoutingError::ConfirmationUnavailable),
                            Err(error) => Err(ExitRoutingError::Confirmation(error)),
                        };
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
