use super::*;
use crate::cas_projection::ProjectionCancellationToken;
use admission::{CompletedConfirmedShutdownObservation, ConfirmedShutdownError};
use std::time::Duration;

mod refresh;
pub(super) use refresh::service_work_changed;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ConfirmedShutdownAdmission {
    Admitted,
    Cancelled,
}

impl RunningProcessOwner {
    pub(crate) fn observe_confirmed_shutdown(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<ConfirmedShutdownAdmission, ConfirmedShutdownError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), String> {
        Self::observe_confirmed_shutdown_with(
            owner,
            cancellation,
            app,
            completed,
            false,
            || {},
            |_| {},
            || {},
        )
    }

    pub(super) fn observe_and_refresh_confirmed_shutdown(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<ConfirmedShutdownAdmission, ConfirmedShutdownError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), String> {
        Self::observe_confirmed_shutdown_with(
            owner,
            cancellation,
            app,
            completed,
            true,
            || {},
            |_| {},
            || {},
        )
    }

    fn observe_confirmed_shutdown_with(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<ConfirmedShutdownAdmission, ConfirmedShutdownError>,
            &mut App,
        ) + 'static,
        refresh: bool,
        before_collect: impl FnOnce() + Send + 'static,
        after_collect: impl FnOnce(&CompletedConfirmedShutdownObservation) + Send + 'static,
        before_refresh: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        let job = owner
            .borrow_mut()
            .prepare_confirmed_shutdown_observation()?;
        let retained = owner.clone();
        let worker_cancellation = cancellation.clone();
        let mut work = app.background_executor().spawn(async move {
            before_collect();
            let completion = job.collect(&worker_cancellation);
            after_collect(&completion);
            completion
        });
        let mut before_refresh = Some(before_refresh);
        app.spawn(async move |cx| {
            loop {
                let completion = work.await;
                let (result, next) = {
                    let mut owner = retained.borrow_mut();
                    let result = if cancellation.is_cancelled() {
                        owner
                            .discard_confirmed_shutdown_observation(completion)
                            .map(|()| ConfirmedShutdownAdmission::Cancelled)
                    } else {
                        owner
                            .complete_confirmed_shutdown_observation(completion)
                            .map(|()| ConfirmedShutdownAdmission::Admitted)
                    };
                    let next = if refresh
                        && result
                            .as_ref()
                            .is_err_and(|error| refresh::work_changed(error))
                    {
                        owner.prepare_confirmed_shutdown_observation().ok()
                    } else {
                        None
                    };
                    (result, next)
                };
                if let Some(job) = next {
                    #[cfg(test)]
                    {
                        retained.borrow_mut().confirmed_refreshes += 1;
                    }
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
                let _ = cx.update(|app| completed(&retained, result, app));
                return;
            }
        })
        .detach();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_confirmed_refreshes(&self) -> usize {
        self.confirmed_refreshes
    }

    #[cfg(test)]
    pub(crate) fn test_observe_confirmed_shutdown_with(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<ConfirmedShutdownAdmission, ConfirmedShutdownError>,
            &mut App,
        ) + 'static,
        refresh: bool,
        before_collect: impl FnOnce() + Send + 'static,
        after_collect: impl FnOnce(&CompletedConfirmedShutdownObservation) + Send + 'static,
        before_refresh: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::observe_confirmed_shutdown_with(
            owner,
            cancellation,
            app,
            completed,
            refresh,
            before_collect,
            after_collect,
            before_refresh,
        )
    }
}
