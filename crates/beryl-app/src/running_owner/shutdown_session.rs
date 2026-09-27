use super::*;
use crate::exit_session::{
    ExitSessionExecution, ExitSessionPreparationError, execute_exit_session,
};
use std::panic::AssertUnwindSafe;

#[derive(Debug)]
pub(crate) enum RunningShutdownSession {
    Pending,
    Settled(Result<ExitSessionExecution, ExitSessionPreparationError>),
    Unwound,
}

impl RunningProcessOwner {
    pub(super) fn require_shutdown_session_released(
        owner: &Rc<RefCell<Self>>,
    ) -> Result<(), String> {
        if owner
            .borrow()
            .shutdown
            .as_ref()
            .is_some_and(|attempt| attempt.session.is_some())
        {
            return Err("Exit session outcome custody has not been released".into());
        }
        Ok(())
    }

    pub(crate) fn shutdown_session(&self) -> Option<&RunningShutdownSession> {
        self.shutdown.as_ref()?.session.as_ref()
    }

    pub(crate) fn publish_shutdown_session(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::publish_shutdown_session_with(owner, app, completed, || {})
    }

    fn publish_shutdown_session_with(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_execute: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        let (services, placements) = {
            let mut owner = owner.borrow_mut();
            let placements = owner.shutdown_placements()?;
            if owner.shutdown_session().is_some() {
                return Err("Exit session execution is already retained".into());
            }
            if owner
                .process
                .services
                .as_ref()
                .and_then(|services| services.graph())
                .is_none()
            {
                return Err("the complete service graph is unavailable".into());
            }
            owner.shutdown.as_mut().unwrap().session = Some(RunningShutdownSession::Pending);
            (owner.process.services.take().unwrap(), placements)
        };
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                before_execute();
                let graph = services.graph().expect("admitted original service graph");
                execute_exit_session(graph.home(), &graph.state().session(), placements)
            }));
            (services, result)
        });
        app.spawn(async move |cx| {
            let (services, result) = work.await;
            {
                let mut owner = retained.borrow_mut();
                owner.process.services = Some(services);
                owner
                    .shutdown
                    .as_mut()
                    .expect("retained session attempt")
                    .session = Some(match result {
                    Ok(result) => RunningShutdownSession::Settled(result),
                    Err(_) => RunningShutdownSession::Unwound,
                });
            }
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_publish_shutdown_session(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_execute: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::publish_shutdown_session_with(owner, app, completed, before_execute)
    }
}
