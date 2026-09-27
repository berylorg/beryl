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
        mut request: RunningExitRequest,
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
                    ShutdownIntent::ApplicationExit,
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
                        completed(owner, request, ExitDraftPreparationCompletion::Ready, app);
                        return;
                    }
                    Err(error) => error,
                    Ok(other) => format!("draft preparation returned {other:?}"),
                };
                let delivery = Rc::new(RefCell::new(Some((preparation, completed))));
                let settled = delivery.clone();
                if let Err((request, error)) = Self::recover_exit_drafts(
                    owner,
                    request,
                    app,
                    move |owner, request, recovery, app| {
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
                    },
                ) {
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
            },
        );
        scheduled.map_err(|error| {
            let (request, _) = delivery.borrow_mut().take().unwrap();
            (request, ExitProgressError::DraftPreparation(error))
        })
    }
}
