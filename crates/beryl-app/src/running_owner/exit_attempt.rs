use super::{
    ExitDraftPreparationCompletion, ExitObservationError, ExitPlacementPreparationCompletion,
    ExitProgressError, ExitRoutingCompletion, ExitRoutingError, RunningProcessOwner,
};
use crate::{
    app_services::AppServiceShutdownProgress, cas_projection::ProjectionCancellationToken,
    startup_owner::RunningExitRequest,
};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug)]
pub(crate) enum ExitAttemptCompletion {
    Cancelled,
    ConfirmedObservationCancelled,
    PlacementsReady,
    Progress(AppServiceShutdownProgress),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitAttemptError {
    #[error(transparent)]
    Observation(#[from] ExitObservationError),
    #[error(transparent)]
    Routing(#[from] ExitRoutingError),
    #[error(transparent)]
    Progress(#[from] ExitProgressError),
    #[error("Exit draft preparation failed: {preparation}; recovery: {recovery:?}")]
    DraftPreparation {
        preparation: String,
        recovery: Result<AppServiceShutdownProgress, ExitProgressError>,
    },
    #[error("Exit placement preparation failed: {preparation}; recovery: {recovery:?}")]
    PlacementPreparation {
        preparation: String,
        recovery: Result<AppServiceShutdownProgress, ExitProgressError>,
    },
}

#[derive(Debug)]
pub(crate) struct ExitAttemptOutcome {
    pub(crate) result: Result<ExitAttemptCompletion, ExitAttemptError>,
    pub(crate) command_completed: bool,
}

impl RunningProcessOwner {
    pub(crate) fn wait_for_exit_attempt(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitRequest, ExitAttemptOutcome, &mut App)
        + 'static,
    ) -> Result<(), String> {
        Self::wait_for_exit(owner, app, move |owner, request, app| {
            let delivery = Rc::new(RefCell::new(Some(completed)));
            let settled = delivery.clone();
            if let Err((request, error)) = Self::run_exit_attempt(
                owner,
                request,
                cancellation,
                app,
                move |owner, request, outcome, app| {
                    Self::report_exit_failure(owner, &request, &outcome, app);
                    let completed = settled.borrow_mut().take().unwrap();
                    completed(owner, request, outcome, app);
                },
            ) {
                let command_completed = Self::finish_exit(owner, &request);
                let outcome = ExitAttemptOutcome {
                    result: Err(ExitAttemptError::Observation(error)),
                    command_completed,
                };
                Self::report_exit_failure(owner, &request, &outcome, app);
                let completed = delivery.borrow_mut().take().unwrap();
                completed(owner, request, outcome, app);
            }
        })
    }

    pub(crate) fn run_exit_attempt(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitRequest, ExitAttemptOutcome, &mut App)
        + 'static,
    ) -> Result<(), (RunningExitRequest, ExitObservationError)> {
        Self::observe_and_drive_exit(
            owner,
            request,
            cancellation,
            app,
            move |owner, request, result, app| {
                Self::complete_exit_attempt(owner, request, result, app, completed);
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn test_complete_exit_work(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitRequest, ExitAttemptOutcome, &mut App)
        + 'static,
    ) {
        Self::complete_exit_attempt(
            owner,
            request,
            Ok(ExitAttemptCompletion::Progress(
                AppServiceShutdownProgress::Ready,
            )),
            app,
            move |owner, request, outcome, app| {
                Self::report_exit_failure(owner, &request, &outcome, app);
                completed(owner, request, outcome, app);
            },
        );
    }

    fn complete_exit_attempt(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        result: Result<ExitAttemptCompletion, ExitAttemptError>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitRequest, ExitAttemptOutcome, &mut App)
        + 'static,
    ) {
        let settle = move |owner: &Rc<RefCell<Self>>, mut request, result, app: &mut App| {
            let release_unadmitted = matches!(
                &result,
                Ok(ExitAttemptCompletion::ConfirmedObservationCancelled)
                    | Err(ExitAttemptError::Routing(
                        ExitRoutingError::ObservationScheduling(_)
                            | ExitRoutingError::ConfirmedObservation(_)
                    ))
            );
            if release_unadmitted {
                let mut owner = owner.borrow_mut();
                let invoking = owner
                    .process
                    .commands
                    .bind_invoking_window(&mut request, None);
                if owner
                    .shutdown_status()
                    .is_some_and(|(window, intent, status)| {
                        invoking == Some(window)
                            && intent == super::ShutdownIntent::ApplicationExit
                            && status == super::RunningShutdownStatus::AwaitingObservation
                    })
                {
                    let _ = owner.end_unadmitted_shutdown();
                }
            }
            let can_complete = !matches!(
                &result,
                Ok(ExitAttemptCompletion::PlacementsReady)
                    | Ok(ExitAttemptCompletion::Progress(
                        AppServiceShutdownProgress::Ready | AppServiceShutdownProgress::Waiting
                    ))
            );
            let command_completed = can_complete && Self::finish_exit(owner, &request);
            completed(
                owner,
                request,
                ExitAttemptOutcome {
                    result,
                    command_completed,
                },
                app,
            );
        };
        if matches!(
            &result,
            Ok(ExitAttemptCompletion::Progress(
                AppServiceShutdownProgress::Ready
            ))
        ) {
            let delivery = Rc::new(RefCell::new(Some(settle)));
            let delivered = delivery.clone();
            if let Err((request, error)) = Self::prepare_exit_drafts(
                owner,
                request,
                app,
                move |owner, request, result, app| {
                    let settle = delivered.borrow_mut().take().unwrap();
                    let result = match result {
                        ExitDraftPreparationCompletion::Ready => {
                            Self::prepare_exit_attempt_placements(owner, request, app, settle);
                            return;
                        }
                        ExitDraftPreparationCompletion::Failed {
                            preparation,
                            recovery,
                        } => Err(ExitAttemptError::DraftPreparation {
                            preparation,
                            recovery,
                        }),
                    };
                    settle(owner, request, result, app);
                },
            ) {
                let settle = delivery.borrow_mut().take().unwrap();
                settle(owner, request, Err(ExitAttemptError::Progress(error)), app);
            }
        } else {
            settle(owner, request, result, app);
        }
    }

    fn prepare_exit_attempt_placements(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<ExitAttemptCompletion, ExitAttemptError>,
            &mut App,
        ) + 'static,
    ) {
        let delivery = Rc::new(RefCell::new(Some(completed)));
        let delivered = delivery.clone();
        if let Err((request, error)) = Self::prepare_exit_placements(
            owner,
            request,
            app,
            move |owner, request, result, app| {
                let result = match result {
                    ExitPlacementPreparationCompletion::Ready => {
                        Ok(ExitAttemptCompletion::PlacementsReady)
                    }
                    ExitPlacementPreparationCompletion::Failed {
                        preparation,
                        recovery,
                    } => Err(ExitAttemptError::PlacementPreparation {
                        preparation,
                        recovery,
                    }),
                };
                let completed = delivered.borrow_mut().take().unwrap();
                completed(owner, request, result, app);
            },
        ) {
            let completed = delivery.borrow_mut().take().unwrap();
            completed(owner, request, Err(ExitAttemptError::Progress(error)), app);
        }
    }

    pub(crate) fn observe_and_drive_exit(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<ExitAttemptCompletion, ExitAttemptError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), (RunningExitRequest, ExitObservationError)> {
        Self::observe_and_route_exit(
            owner,
            request,
            cancellation.clone(),
            app,
            move |owner, request, result, app| {
                let result = match result {
                    Ok(ExitRoutingCompletion::Admitted) => {
                        let delivery = Rc::new(RefCell::new(Some(completed)));
                        let settled = delivery.clone();
                        if let Err((request, error)) = Self::drive_exit(
                            owner,
                            request,
                            cancellation,
                            app,
                            move |owner, request, result, app| {
                                let completed = settled.borrow_mut().take().unwrap();
                                completed(
                                    owner,
                                    request,
                                    result
                                        .map(ExitAttemptCompletion::Progress)
                                        .map_err(ExitAttemptError::Progress),
                                    app,
                                );
                            },
                        ) {
                            let completed = delivery.borrow_mut().take().unwrap();
                            completed(owner, request, Err(ExitAttemptError::Progress(error)), app);
                        }
                        return;
                    }
                    Ok(ExitRoutingCompletion::Cancelled) => Ok(ExitAttemptCompletion::Cancelled),
                    Ok(ExitRoutingCompletion::ConfirmedObservationCancelled) => {
                        Ok(ExitAttemptCompletion::ConfirmedObservationCancelled)
                    }
                    Err(error) => Err(ExitAttemptError::Routing(error)),
                };
                completed(owner, request, result, app);
            },
        )
    }
}
