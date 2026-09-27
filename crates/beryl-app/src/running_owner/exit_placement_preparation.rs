use super::{ExitProgressError, RunningProcessOwner, RunningShutdownStatus, ShutdownIntent};
use crate::{app_services::AppServiceShutdownProgress, startup_owner::RunningExitRequest};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug)]
pub(crate) enum ExitPlacementPreparationCompletion {
    Ready,
    Failed {
        preparation: String,
        recovery: Result<AppServiceShutdownProgress, ExitProgressError>,
    },
}

impl RunningProcessOwner {
    pub(crate) fn prepare_exit_placements(
        owner: &Rc<RefCell<Self>>,
        mut request: RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            ExitPlacementPreparationCompletion,
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
        let scheduled = Self::capture_shutdown_placements(owner, app, move |owner, result, app| {
            let (request, completed) = settled.borrow_mut().take().unwrap();
            let preparation = match result {
                Ok(()) => {
                    completed(
                        owner,
                        request,
                        ExitPlacementPreparationCompletion::Ready,
                        app,
                    );
                    return;
                }
                Err(error) => error,
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
                        ExitPlacementPreparationCompletion::Failed {
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
                    ExitPlacementPreparationCompletion::Failed {
                        preparation,
                        recovery: Err(error),
                    },
                    app,
                );
            }
        });
        scheduled.map_err(|error| {
            let (request, _) = delivery.borrow_mut().take().unwrap();
            (request, ExitProgressError::PlacementPreparation(error))
        })
    }
}
