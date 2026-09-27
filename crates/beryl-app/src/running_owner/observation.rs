use super::*;
use crate::cas_projection::ProjectionCancellationToken;
use admission::{CompletedConfirmedShutdownObservation, ConfirmedShutdownError};

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
        Self::observe_confirmed_shutdown_with(owner, cancellation, app, completed, || {}, |_| {})
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
        before_collect: impl FnOnce() + Send + 'static,
        after_collect: impl FnOnce(&CompletedConfirmedShutdownObservation) + Send + 'static,
    ) -> Result<(), String> {
        let job = owner
            .borrow_mut()
            .prepare_confirmed_shutdown_observation()?;
        let retained = owner.clone();
        let worker_cancellation = cancellation.clone();
        let work = app.background_executor().spawn(async move {
            before_collect();
            let completion = job.collect(&worker_cancellation);
            after_collect(&completion);
            completion
        });
        app.spawn(async move |cx| {
            let completion = work.await;
            let result = {
                let mut owner = retained.borrow_mut();
                if cancellation.is_cancelled() {
                    owner
                        .discard_confirmed_shutdown_observation(completion)
                        .map(|()| ConfirmedShutdownAdmission::Cancelled)
                } else {
                    owner
                        .complete_confirmed_shutdown_observation(completion)
                        .map(|()| ConfirmedShutdownAdmission::Admitted)
                }
            };
            let _ = cx.update(|app| completed(&retained, result, app));
        })
        .detach();
        Ok(())
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
        before_collect: impl FnOnce() + Send + 'static,
        after_collect: impl FnOnce(&CompletedConfirmedShutdownObservation) + Send + 'static,
    ) -> Result<(), String> {
        Self::observe_confirmed_shutdown_with(
            owner,
            cancellation,
            app,
            completed,
            before_collect,
            after_collect,
        )
    }
}
