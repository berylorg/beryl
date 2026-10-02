use super::{
    ExitProgressError, RunningProcessOwner, RunningShutdownDraftAction,
    RunningShutdownDraftProgress, RunningShutdownStatus, ShutdownIntent,
};
use crate::{app_services::AppServiceShutdownProgress, startup_owner::RunningExitRequest};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug)]
pub(crate) enum ExitDraftPreparationCompletion {
    Ready,
    Failed {
        preparation: String,
        recovery: Result<AppServiceShutdownProgress, ExitProgressError>,
    },
}

impl RunningProcessOwner {
    pub(crate) fn prepare_exit_drafts(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            ExitDraftPreparationCompletion,
            &mut App,
        ) + 'static,
    ) -> Result<(), (RunningExitRequest, ExitProgressError)> {
        Self::prepare_exit_drafts_with_cancellation(
            owner,
            request,
            crate::cas_projection::ProjectionCancellationToken::new(),
            app,
            completed,
        )
    }

    pub(crate) fn prepare_exit_drafts_with_cancellation(
        owner: &Rc<RefCell<Self>>,
        mut request: RunningExitRequest,
        cancellation: crate::cas_projection::ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            ExitDraftPreparationCompletion,
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
            RunningShutdownDraftAction::Prepare,
            app,
            move |owner, result, app| {
                let (request, completed) = settled.borrow_mut().take().unwrap();
                let preparation = match result {
                    Ok(RunningShutdownDraftProgress::Ready) => {
                        Self::prepare_detached_shutdown_sources(
                            owner,
                            cancellation,
                            app,
                            move |owner, result, app| match result {
                                Ok(()) => completed(
                                    owner,
                                    request,
                                    ExitDraftPreparationCompletion::Ready,
                                    app,
                                ),
                                Err(preparation) => Self::fail_exit_draft_preparation(
                                    owner,
                                    request,
                                    preparation,
                                    app,
                                    completed,
                                ),
                            },
                        );
                        return;
                    }
                    Err(error) => error,
                    Ok(other) => format!("draft preparation returned {other:?}"),
                };
                Self::fail_exit_draft_preparation(owner, request, preparation, app, completed);
            },
        );
        scheduled.map_err(|error| {
            let (request, _) = delivery.borrow_mut().take().unwrap();
            (request, ExitProgressError::DraftPreparation(error))
        })
    }

    fn fail_exit_draft_preparation(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        preparation: String,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            ExitDraftPreparationCompletion,
            &mut App,
        ) + 'static,
    ) {
        if Self::ordinary_close_home_unavailable(owner) {
            let recovery = Self::prepare_ordinary_close_recovery(owner, app)
                .map(|()| crate::app_services::AppServiceShutdownProgress::Waiting)
                .map_err(ExitProgressError::DraftRelease);
            completed(
                owner,
                request,
                ExitDraftPreparationCompletion::Failed {
                    preparation,
                    recovery,
                },
                app,
            );
            return;
        }
        let delivery = Rc::new(RefCell::new(Some((preparation, completed))));
        let settled = delivery.clone();
        if let Err((request, error)) =
            Self::recover_exit_drafts(owner, request, app, move |owner, request, recovery, app| {
                let (preparation, completed) = settled.borrow_mut().take().unwrap();
                completed(
                    owner,
                    request,
                    ExitDraftPreparationCompletion::Failed {
                        preparation,
                        recovery,
                    },
                    app,
                );
            })
        {
            let (preparation, completed) = delivery.borrow_mut().take().unwrap();
            completed(
                owner,
                request,
                ExitDraftPreparationCompletion::Failed {
                    preparation,
                    recovery: Err(error),
                },
                app,
            );
        }
    }
}
