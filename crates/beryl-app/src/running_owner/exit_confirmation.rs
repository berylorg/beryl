use super::{RunningProcessOwner, ShutdownConfirmationResult, ShutdownIntent};
use crate::startup_owner::RunningExitRequest;
use gpui::App;
use std::rc::Rc;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ExitConfirmationRoute {
    Cancelled,
    AwaitingObservation,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitConfirmationError {
    #[error("Exit confirmation request is unavailable: {0}")]
    Request(String),
    #[error("the native confirmation belongs to another Exit request or shutdown intent")]
    Unrelated,
    #[error("Exit confirmation failed: {0}")]
    Native(String),
    #[error("the confirmed window set changed")]
    WindowSetChanged,
    #[error("confirmed Exit intent could not be retained: {0}")]
    Intent(String),
}

impl RunningProcessOwner {
    pub(crate) fn consume_exit_confirmation(
        &mut self,
        request: &mut RunningExitRequest,
        app: &App,
    ) -> Result<Option<ExitConfirmationRoute>, ExitConfirmationError> {
        let invoking = self
            .resolve_exit_window(request, app)
            .map_err(ExitConfirmationError::Request)?;
        let Some(operation) = &self.confirmation else {
            return Ok(None);
        };
        if operation.context.invoking != invoking
            || operation.context.intent != request.shutdown_intent()
            || !operation
                .exit_request
                .as_ref()
                .is_some_and(|identity| Rc::ptr_eq(identity, &request.identity()))
        {
            return Err(ExitConfirmationError::Unrelated);
        }
        match self
            .take_shutdown_confirmation()
            .map_err(ExitConfirmationError::Native)?
        {
            None => Ok(None),
            Some(ShutdownConfirmationResult::Cancelled) => {
                Ok(Some(ExitConfirmationRoute::Cancelled))
            }
            Some(ShutdownConfirmationResult::WindowSetChanged) => {
                Err(ExitConfirmationError::WindowSetChanged)
            }
            Some(ShutdownConfirmationResult::Confirmed(context)) => {
                self.begin_confirmed_shutdown(context)
                    .map_err(ExitConfirmationError::Intent)?;
                Ok(Some(ExitConfirmationRoute::AwaitingObservation))
            }
        }
    }
}
