use super::*;
use crate::{
    app_services::{AppServiceCloseError, PreparedShutdownObservation},
    cas_projection::{
        ProcessWorkError, ProjectionCancellationToken, ShutdownWorkError, ShutdownWorkObservation,
    },
};

impl RunningProcessOwner {
    pub(crate) fn observe_shutdown_work(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<ShutdownWorkObservation, AppServiceCloseError>,
            &mut App,
        ) + 'static,
    ) -> Result<(), String> {
        Self::observe_shutdown_work_with(owner, cancellation, app, completed, || {}, |_| {})
    }

    fn observe_shutdown_work_with(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<ShutdownWorkObservation, AppServiceCloseError>,
            &mut App,
        ) + 'static,
        before_collect: impl FnOnce() + Send + 'static,
        after_collect: impl FnOnce(&Result<ShutdownWorkObservation, AppServiceCloseError>)
        + Send
        + 'static,
    ) -> Result<(), String> {
        let job = owner.borrow_mut().prepare_initial_observation()?;
        Self::spawn_initial_observation(
            owner,
            job,
            cancellation,
            app,
            completed,
            before_collect,
            after_collect,
        );
        Ok(())
    }

    pub(super) fn prepare_initial_observation(
        &mut self,
    ) -> Result<PreparedShutdownObservation, String> {
        if self.observing_initial_work
            || self.confirmation.is_some()
            || self.shutdown.is_some()
            || self.progress.is_some()
        {
            return Err(
                "the running owner already retains shutdown observation or intent custody".into(),
            );
        }
        let job = self
            .process
            .services
            .as_ref()
            .ok_or("the complete service owner is on a worker")?
            .prepare_shutdown_observation()
            .map_err(|error| error.to_string())?;
        self.observing_initial_work = true;
        Ok(job)
    }

    pub(super) fn spawn_initial_observation(
        owner: &Rc<RefCell<Self>>,
        job: PreparedShutdownObservation,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<ShutdownWorkObservation, AppServiceCloseError>,
            &mut App,
        ) + 'static,
        before_collect: impl FnOnce() + Send + 'static,
        after_collect: impl FnOnce(&Result<ShutdownWorkObservation, AppServiceCloseError>)
        + Send
        + 'static,
    ) {
        let retained = owner.clone();
        let worker_cancellation = cancellation.clone();
        let work = app.background_executor().spawn(async move {
            before_collect();
            let result = job.collect(&worker_cancellation);
            after_collect(&result);
            result
        });
        app.spawn(async move |cx| {
            let mut result = work.await;
            if cancellation.is_cancelled() {
                result = Err(ShutdownWorkError::Work(ProcessWorkError::Cancelled).into());
            }
            retained.borrow_mut().observing_initial_work = false;
            let _ = cx.update(|app| completed(&retained, result, app));
        })
        .detach();
    }

    #[cfg(test)]
    pub(crate) fn test_observe_shutdown_work_with(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<ShutdownWorkObservation, AppServiceCloseError>,
            &mut App,
        ) + 'static,
        before_collect: impl FnOnce() + Send + 'static,
        after_collect: impl FnOnce(&Result<ShutdownWorkObservation, AppServiceCloseError>)
        + Send
        + 'static,
    ) -> Result<(), String> {
        Self::observe_shutdown_work_with(
            owner,
            cancellation,
            app,
            completed,
            before_collect,
            after_collect,
        )
    }
}
