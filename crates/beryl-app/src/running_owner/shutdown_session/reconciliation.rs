use super::*;

impl RunningProcessOwner {
    pub(crate) fn reconcile_shutdown_session(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::reconcile_shutdown_session_with(owner, app, completed, || {})
    }

    fn reconcile_shutdown_session_with(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_reconcile: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        let (services, reconciliation) = {
            let mut owner = owner.borrow_mut();
            owner.shutdown_placements()?;
            if owner
                .process
                .services
                .as_ref()
                .and_then(|services| services.graph())
                .is_none()
            {
                return Err("the complete service graph is unavailable".into());
            }
            if !matches!(
                owner.shutdown_session(),
                Some(RunningShutdownSession::Settled(Ok(
                    ExitSessionExecution::Indeterminate(_)
                ))) | Some(RunningShutdownSession::Reconciled(
                    ExitSessionReconciled::Pending { .. }
                ))
            ) {
                return Err("no pending Exit session reconciliation is retained".into());
            }
            let slot = &mut owner.shutdown.as_mut().unwrap().session;
            let reconciliation = match slot.replace(RunningShutdownSession::Reconciling).unwrap() {
                RunningShutdownSession::Settled(Ok(ExitSessionExecution::Indeterminate(
                    pending,
                ))) => pending,
                RunningShutdownSession::Reconciled(ExitSessionReconciled::Pending {
                    reconciliation,
                    ..
                }) => reconciliation,
                _ => unreachable!("validated pending session reconciliation"),
            };
            (owner.process.services.take().unwrap(), reconciliation)
        };
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                before_reconcile();
                let graph = services.graph().expect("admitted original service graph");
                reconciliation.reconcile(graph.home())
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
                    Ok(result) => RunningShutdownSession::Reconciled(result),
                    Err(_) => RunningShutdownSession::Unwound,
                });
            }
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_reconcile_shutdown_session(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_reconcile: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::reconcile_shutdown_session_with(owner, app, completed, before_reconcile)
    }
}
