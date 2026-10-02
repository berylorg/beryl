use super::*;

impl RunningProcessOwner {
    pub(in crate::running_owner) fn publish_ordinary_close_session(
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
        let ready = (|| {
            let retained = owner.borrow();
            if !retained.process.commands.is_active(&request)
                || !request.is_ordinary_close()
                || retained.shutdown_session().is_some()
            {
                return Err("ordinary close session identity is unavailable".into());
            }
            let attempt = retained
                .shutdown
                .as_ref()
                .ok_or("ordinary close attempt is unavailable")?;
            attempt.lease.validate().map_err(|e| e.to_string())?;
            if !attempt
                .drafts
                .as_ref()
                .is_some_and(|drafts| drafts.borrow().ready())
            {
                return Err("ordinary close drafts are not ready".into());
            }
            Ok::<_, String>(retained.shutdown_status().unwrap().0)
        })();
        let invoking = match ready {
            Ok(window) => window,
            Err(error) => {
                completed(
                    owner,
                    request,
                    Err(ExitAttemptError::SessionPublication(error)),
                    app,
                );
                return;
            }
        };
        let services = {
            let mut retained = owner.borrow_mut();
            if let Err(error) = retained.retain_pre_native_close(&request.identity(), app) {
                drop(retained);
                completed(
                    owner,
                    request,
                    Err(ExitAttemptError::SessionPublication(error)),
                    app,
                );
                return;
            }
            retained.shutdown.as_mut().unwrap().session = Some(RunningShutdownSession::Pending);
            retained.process.services.take().unwrap()
        };
        let retained = owner.clone();
        let identity = request.identity();
        #[cfg(test)]
        let before_removal = owner.borrow_mut().before_ordinary_session_removal.take();
        #[cfg(test)]
        let before_admission = owner.borrow_mut().before_ordinary_command_admission.take();
        let work = app.background_executor().spawn(async move {
            let graph = services.graph().unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                #[cfg(test)]
                if let Some(hook) = before_removal {
                    hook(graph.home());
                }
                #[cfg(test)]
                {
                    ordinary_close_session::OrdinaryCloseSession::execute_with_admission_hook(
                        graph.home(),
                        &graph.state().session(),
                        invoking,
                        |home| {
                            if let Some(hook) = before_admission {
                                hook(home);
                            }
                        },
                    )
                }
                #[cfg(not(test))]
                {
                    ordinary_close_session::OrdinaryCloseSession::execute(
                        graph.home(),
                        &graph.state().session(),
                        invoking,
                    )
                }
            }));
            (services, result)
        });
        app.spawn(async move |cx| {
            let (services, result) = work.await;
            let result = cx.update(|app| {
                {
                    let mut owner = retained.borrow_mut();
                    owner.process.services = Some(services);
                    if !owner.process.commands.is_active_identity(&identity) { return Err("ordinary close request changed during publication".to_owned()); }
                    owner.shutdown.as_mut().unwrap().session = match result {
                        Ok(Ok(close)) => Some(RunningShutdownSession::RemovedWindow(close)),
                        Ok(Err(error)) => { owner.shutdown.as_mut().unwrap().session = None; return Err(error); }
                        Err(_) => {
                            owner.shutdown.as_mut().unwrap().session = Some(RunningShutdownSession::Unwound);
                            return Err("ordinary close publication unwound without a proven command outcome".into());
                        }
                    };
                }
                let ready = retained.borrow().require_shutdown_session_ready();
                ready.map(|()| ExitAttemptCompletion::SessionReady)
            });
            let mut result = result.map_err(|e| e.to_string()).and_then(|r| r).map_err(ExitAttemptError::SessionPublication);
            if result.is_err() && !Self::ordinary_close_home_unavailable(&retained) && matches!(retained.borrow().shutdown_session(), Some(RunningShutdownSession::RemovedWindow(_))) {
                if let Err(restoration) = Self::restore_pre_native_close(&retained, &identity, cx).await {
                    result = Err(ExitAttemptError::SessionPublication(format!("{:?}; pre-native restoration: {restoration}", result.unwrap_err())));
                }
            }
            cx.update(|app| completed(&retained, request, result, app)).expect("ordinary close retains GUI delivery");
        }).detach();
    }

    pub(in crate::running_owner) fn fail_final_close_before_removal(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        error: ExitAttemptError,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            Result<ExitAttemptCompletion, ExitAttemptError>,
            &mut App,
        ) + 'static,
    ) {
        if Self::ordinary_close_home_unavailable(owner) {
            let _ = Self::prepare_ordinary_close_recovery(owner, app);
            completed(owner, request, Err(error), app);
            return;
        }
        if let Err(proof) = owner
            .borrow_mut()
            .release_pre_native_close_if_present(&request.identity(), app)
        {
            completed(
                owner,
                request,
                Err(ExitAttemptError::SessionPublication(format!(
                    "{error:?}; pre-native proof: {proof}"
                ))),
                app,
            );
            return;
        }
        let delivery = Rc::new(RefCell::new(Some((error, completed))));
        let settled = delivery.clone();
        if let Err((request, recovery)) =
            Self::recover_exit_drafts(owner, request, app, move |owner, request, recovery, app| {
                let (error, completed) = settled.borrow_mut().take().unwrap();
                let error = recovery
                    .err()
                    .map(|recovery| {
                        ExitAttemptError::SessionPublication(format!(
                            "{error:?}; recovery: {recovery:?}"
                        ))
                    })
                    .unwrap_or(error);
                completed(owner, request, Err(error), app);
            })
        {
            let (error, completed) = delivery.borrow_mut().take().unwrap();
            completed(
                owner,
                request,
                Err(ExitAttemptError::SessionPublication(format!(
                    "{error:?}; recovery: {recovery:?}"
                ))),
                app,
            );
        }
    }
}
