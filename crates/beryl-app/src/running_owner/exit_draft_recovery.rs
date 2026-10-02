use super::{
    ExitProgressError, RunningProcessOwner, RunningShutdownDraftAction,
    RunningShutdownDraftProgress, RunningShutdownStatus, ShutdownIntent,
};
use crate::{
    app_services::AppServiceShutdownProgress, cas_projection::ProjectionCancellationToken,
    startup_owner::RunningExitRequest,
};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

impl RunningProcessOwner {
    pub(crate) fn recover_exit_drafts(
        owner: &Rc<RefCell<Self>>,
        mut request: RunningExitRequest,
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
                    request.shutdown_intent(),
                    RunningShutdownStatus::WorkReady,
                ))
            {
                return Err((request, ExitProgressError::Intent));
            }
        }
        let delivery = Rc::new(RefCell::new(Some((request, completed))));
        let settled = delivery.clone();
        let scheduled = Self::drive_shutdown_drafts(
            owner,
            RunningShutdownDraftAction::Release,
            app,
            move |owner, result, app| {
                if !matches!(result, Ok(RunningShutdownDraftProgress::Released)) {
                    let error = match result {
                        Err(error) => error,
                        Ok(other) => format!("draft release returned {other:?}"),
                    };
                    let (request, completed) = settled.borrow_mut().take().unwrap();
                    completed(
                        owner,
                        request,
                        Err(ExitProgressError::DraftRelease(error)),
                        app,
                    );
                    return;
                }
                let (request, completed) = settled.borrow_mut().take().unwrap();
                let delivery = Rc::new(RefCell::new(Some(completed)));
                let settled = delivery.clone();
                let cancelled = ProjectionCancellationToken::new();
                cancelled.cancel();
                if let Err((request, error)) = Self::drive_exit(
                    owner,
                    request,
                    cancelled,
                    app,
                    move |owner, request, result, app| {
                        let completed = settled.borrow_mut().take().unwrap();
                        completed(owner, request, result, app);
                    },
                ) {
                    let completed = delivery.borrow_mut().take().unwrap();
                    completed(owner, request, Err(error), app);
                }
            },
        );
        scheduled.map_err(|error| {
            let (request, _) = delivery.borrow_mut().take().unwrap();
            (request, ExitProgressError::DraftRelease(error))
        })
    }
}
