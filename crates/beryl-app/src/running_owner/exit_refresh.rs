use super::{
    ExitObservationError, ExitWorkClassification, ExitWorkError, IdleShutdownError,
    RunningProcessOwner,
};
use crate::{
    app_services::{AppServiceCloseError, CloseConfirmationPreparationError},
    cas_projection::{
        ProcessWorkError, ProjectionCancellationToken, RuntimeWorkError, ShutdownWorkError,
        ShutdownWorkObservation,
    },
    startup_owner::RunningExitRequest,
};
use gpui::App;
use std::{cell::RefCell, rc::Rc, time::Duration};

fn work_changed(error: &ExitWorkError) -> bool {
    match error {
        ExitWorkError::Observation(error)
        | ExitWorkError::Admission(IdleShutdownError::Service(error)) => {
            super::observation::service_work_changed(error)
        }
        ExitWorkError::Admission(IdleShutdownError::Preparation(
            CloseConfirmationPreparationError::Runtime(RuntimeWorkError::Stale),
        )) => true,
        _ => false,
    }
}

impl RunningProcessOwner {
    pub(super) fn observe_and_classify_exit_work(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<ExitWorkClassification, ExitWorkError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), (RunningExitRequest, ExitObservationError)> {
        Self::observe_and_classify_exit_work_with(
            owner,
            request,
            cancellation,
            app,
            completed,
            |_| {},
            || {},
        )
    }

    fn observe_and_classify_exit_work_with(
        owner: &Rc<RefCell<Self>>,
        mut request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<ExitWorkClassification, ExitWorkError>,
            &mut App,
        ) + 'static,
        after_collect: impl FnOnce(&Result<ShutdownWorkObservation, AppServiceCloseError>)
        + Send
        + 'static,
        before_refresh: impl FnOnce() + Send + 'static,
    ) -> Result<(), (RunningExitRequest, ExitObservationError)> {
        let job = {
            let mut owner = owner.borrow_mut();
            if let Err(error) = owner.resolve_exit_window(&mut request, app) {
                return Err((request, ExitObservationError::Request(error)));
            }
            match owner.prepare_initial_observation() {
                Ok(job) => job,
                Err(error) => return Err((request, ExitObservationError::Scheduling(error))),
            }
        };
        let retained = owner.clone();
        let token = cancellation.clone();
        let mut work = app.background_executor().spawn(async move {
            let result = job.collect(&token);
            after_collect(&result);
            result
        });
        app.spawn(async move |cx| {
            let mut before_refresh = Some(before_refresh);
            loop {
                let mut observation = work.await;
                let settled = cx.update(|app| {
                    let mut owner = retained.borrow_mut();
                    owner.observing_initial_work = false;
                    if cancellation.is_cancelled() {
                        observation =
                            Err(ShutdownWorkError::Work(ProcessWorkError::Cancelled).into());
                    }
                    let mut result = owner.classify_exit_work(&mut request, observation, app);
                    let next = if result.as_ref().is_err_and(work_changed) {
                        match owner.prepare_initial_observation() {
                            Ok(job) => Some(job),
                            Err(error) => {
                                result = Err(ExitWorkError::RefreshScheduling(error));
                                None
                            }
                        }
                    } else {
                        None
                    };
                    (result, next)
                });
                let Ok((result, next)) = settled else { return };
                if let Some(job) = next {
                    let token = cancellation.clone();
                    let executor = cx.background_executor().clone();
                    let before_refresh = before_refresh.take();
                    work = cx.background_executor().spawn(async move {
                        if let Some(before_refresh) = before_refresh {
                            before_refresh();
                        }
                        executor.timer(Duration::from_millis(50)).await;
                        job.collect(&token)
                    });
                    continue;
                }
                let _ = cx.update(|app| completed(&retained, request, result, app));
                return;
            }
        })
        .detach();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_observe_and_classify_exit_work_with(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<ExitWorkClassification, ExitWorkError>,
            &mut App,
        ) + 'static,
        after_collect: impl FnOnce(&Result<ShutdownWorkObservation, AppServiceCloseError>)
        + Send
        + 'static,
        before_refresh: impl FnOnce() + Send + 'static,
    ) -> Result<(), (RunningExitRequest, ExitObservationError)> {
        Self::observe_and_classify_exit_work_with(
            owner,
            request,
            cancellation,
            app,
            completed,
            after_collect,
            before_refresh,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("../../tests/unit/app_services/initial_work_refresh.rs");
}
