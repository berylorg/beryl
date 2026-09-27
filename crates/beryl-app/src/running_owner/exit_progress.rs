use super::{RunningProcessOwner, RunningShutdownStatus, ShutdownIntent};
use crate::{
    app_services::{AppServiceCloseError, AppServiceShutdownProgress},
    cas_projection::ProjectionCancellationToken,
    startup_owner::RunningExitRequest,
};
use gpui::App;
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitProgressError {
    #[error("Exit progress request is unavailable: {0}")]
    Request(String),
    #[error("the Exit request has no matching admitted shutdown intent")]
    Intent,
    #[error("Exit progress could not be scheduled: {0}")]
    Scheduling(String),
    #[error("Exit interaction could not transition: {0}")]
    Interaction(String),
    #[error("settled Exit progress is unavailable")]
    Unavailable,
    #[error(transparent)]
    Service(#[from] AppServiceCloseError),
}

impl RunningProcessOwner {
    pub(crate) fn drive_exit(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<AppServiceShutdownProgress, ExitProgressError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), (RunningExitRequest, ExitProgressError)> {
        let (sender, mut receiver) = futures_channel::oneshot::channel();
        Self::advance_exit(
            owner,
            request,
            cancellation.clone(),
            app,
            move |_, request, result, _| {
                let _ = sender.send((request, result));
            },
        )?;
        let retained = owner.clone();
        app.spawn(async move |cx| {
            loop {
                let Ok((request, result)) = receiver.await else {
                    return;
                };
                if !matches!(result, Ok(AppServiceShutdownProgress::Waiting)) {
                    let _ = cx.update(|app| completed(&retained, request, result, app));
                    return;
                }
                #[cfg(test)]
                {
                    retained.borrow_mut().exit_waiting_passes += 1;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let (sender, next) = futures_channel::oneshot::channel();
                receiver = next;
                match cx.update(|app| {
                    Self::advance_exit(
                        &retained,
                        request,
                        cancellation.clone(),
                        app,
                        move |_, request, result, _| {
                            let _ = sender.send((request, result));
                        },
                    )
                }) {
                    Ok(Ok(())) => {}
                    Ok(Err((request, error))) => {
                        let _ = cx.update(|app| completed(&retained, request, Err(error), app));
                        return;
                    }
                    Err(_) => return,
                }
            }
        })
        .detach();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_exit_waiting_passes(&self) -> usize {
        self.exit_waiting_passes
    }

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
        if let Err(error) = Self::install_shutdown_interaction_gate(owner, app) {
            return Err((request, ExitProgressError::Interaction(error)));
        }
        let delivery = Rc::new(RefCell::new(Some((request, completed))));
        let settled = delivery.clone();
        match Self::advance_shutdown(owner, cancellation, app, move |owner, app| {
            let result = Self::take_exit_progress(owner, app);
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

    pub(crate) fn take_exit_progress(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) -> Result<AppServiceShutdownProgress, ExitProgressError> {
        let reopened = matches!(
            owner.borrow().progress,
            Some(super::progress::RunningShutdownProgress::Settled(Ok(
                AppServiceShutdownProgress::Failed { reopened: true, .. }
            )))
        );
        if reopened {
            Self::release_shutdown_interaction_gate(owner, app)
                .map_err(ExitProgressError::Interaction)?;
        }
        owner
            .borrow_mut()
            .take_shutdown_progress()
            .ok_or(ExitProgressError::Unavailable)
            .and_then(|result| result.map_err(ExitProgressError::Service))
    }
}
