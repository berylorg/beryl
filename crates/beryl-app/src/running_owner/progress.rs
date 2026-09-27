use super::*;
use crate::{
    app_services::{AppServiceCloseError, AppServiceShutdownProgress},
    cas_projection::ProjectionCancellationToken,
};

pub(super) enum RunningShutdownProgress {
    Polling,
    Settled(Result<AppServiceShutdownProgress, AppServiceCloseError>),
}

impl RunningProcessOwner {
    pub(crate) fn advance_shutdown(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::advance_shutdown_with(owner, cancellation, app, completed, || {})
    }

    fn advance_shutdown_with(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_poll: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::require_shutdown_session_released(owner)?;
        Self::require_shutdown_placements_settled(owner)?;
        let mut services = {
            let mut owner = owner.borrow_mut();
            if owner.progress.is_some() {
                return Err("shutdown progress is pending or has an unconsumed result".into());
            }
            if !owner
                .shutdown
                .as_ref()
                .is_some_and(|attempt| attempt.admitted)
            {
                return Err("no admitted running shutdown is retained".into());
            }
            if owner
                .shutdown
                .as_ref()
                .is_some_and(|attempt| attempt.work_ready)
                && !cancellation.is_cancelled()
            {
                return Err("shutdown work is already ready".into());
            }
            if owner.shutdown.as_ref().is_some_and(|attempt| {
                attempt.drafts.as_ref().is_some_and(|drafts| {
                    !drafts.try_borrow().is_ok_and(|drafts| drafts.released())
                })
            }) {
                return Err(
                    "shutdown draft obligations must release before service progress".into(),
                );
            }
            let services = owner
                .process
                .services
                .take()
                .ok_or("the complete service owner is unavailable")?;
            owner.shutdown.as_mut().unwrap().work_ready = false;
            owner.progress = Some(RunningShutdownProgress::Polling);
            services
        };
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            before_poll();
            let result = services.poll_shutdown(&cancellation);
            (services, result)
        });
        app.spawn(async move |cx| {
            let (services, result) = work.await;
            {
                let mut owner = retained.borrow_mut();
                owner.process.services = Some(services);
                if matches!(result, Ok(AppServiceShutdownProgress::Ready)) {
                    owner
                        .shutdown
                        .as_mut()
                        .expect("retained admitted shutdown")
                        .work_ready = true;
                }
                if matches!(
                    result,
                    Ok(AppServiceShutdownProgress::Failed { reopened: true, .. })
                ) {
                    owner.shutdown.take();
                }
                owner.progress = Some(RunningShutdownProgress::Settled(result));
            }
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn take_shutdown_progress(
        &mut self,
    ) -> Option<Result<AppServiceShutdownProgress, AppServiceCloseError>> {
        if !matches!(self.progress, Some(RunningShutdownProgress::Settled(_))) {
            return None;
        }
        let Some(RunningShutdownProgress::Settled(result)) = self.progress.take() else {
            unreachable!()
        };
        Some(result)
    }

    #[cfg(test)]
    pub(crate) fn test_advance_shutdown_with(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_poll: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::advance_shutdown_with(owner, cancellation, app, completed, before_poll)
    }

    #[cfg(test)]
    pub(crate) fn test_shutdown_progress_settled(&self) -> bool {
        matches!(self.progress, Some(RunningShutdownProgress::Settled(_)))
    }

    #[cfg(test)]
    pub(crate) fn test_services_on_worker(&self) -> bool {
        self.process.services.is_none()
    }
}
