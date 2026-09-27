use super::{ExitObservationError, ExitWorkError, ExitWorkRoute, RunningProcessOwner};
use crate::{cas_projection::ProjectionCancellationToken, startup_owner::RunningExitRequest};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ExitRoutingCompletion {
    Admitted,
    ConfirmationSettled,
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
            Result<ExitRoutingCompletion, ExitWorkError>,
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
                        let (request, completed) = settled.borrow_mut().take().unwrap();
                        completed(
                            owner,
                            request,
                            Ok(ExitRoutingCompletion::ConfirmationSettled),
                            app,
                        );
                    })
                };
                let result = match routed {
                    Ok(ExitWorkRoute::Confirming) => return,
                    Ok(ExitWorkRoute::Admitted) => Ok(ExitRoutingCompletion::Admitted),
                    Err(error) => Err(error),
                };
                let (request, completed) = delivery.borrow_mut().take().unwrap();
                completed(owner, request, result, app);
            },
        )
    }
}
