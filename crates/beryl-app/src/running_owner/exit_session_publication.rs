use super::{ExitProgressError, RunningProcessOwner, RunningShutdownStatus, ShutdownIntent};
use crate::startup_owner::RunningExitRequest;
use gpui::App;
use std::{cell::RefCell, rc::Rc};

impl RunningProcessOwner {
    pub(crate) fn publish_exit_session(
        owner: &Rc<RefCell<Self>>,
        mut request: RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitRequest, Result<(), String>, &mut App)
        + 'static,
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
                    RunningShutdownStatus::WorkReady,
                ))
            {
                return Err((request, ExitProgressError::Intent));
            }
        }
        let delivery = Rc::new(RefCell::new(Some((request, completed))));
        let settled = delivery.clone();
        Self::publish_shutdown_session(owner, app, move |owner, app| {
            let (request, completed) = settled.borrow_mut().take().unwrap();
            let result = owner.borrow().require_shutdown_session_ready();
            completed(owner, request, result, app);
        })
        .map_err(|error| {
            let (request, _) = delivery.borrow_mut().take().unwrap();
            (request, ExitProgressError::SessionPublication(error))
        })
    }
}
