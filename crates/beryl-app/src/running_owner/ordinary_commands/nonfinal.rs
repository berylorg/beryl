use super::*;
mod restoration;

impl RunningProcessOwner {
    pub(in crate::running_owner) fn run_nonfinal_close(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            exit_attempt::ExitAttemptOutcome,
            &mut App,
        ) + 'static,
    ) {
        let window = owner.borrow().ordinary_close_window.unwrap();
        let gated = window
            .update(app, |root, _, cx| {
                root.set_ordinary_close_interaction_gated(true, cx);
                root.set_shutdown_interaction_gated(true, cx)
            })
            .map_err(|e| e.to_string())
            .and_then(|result| result);
        if let Err(error) = gated {
            Self::fail_nonfinal_close(owner, request, error, app, completed);
            return;
        }
        let delivery = Rc::new(RefCell::new(Some((request, completed))));
        let settled = delivery.clone();
        if let Err(error) = Self::drive_shutdown_drafts(
            owner,
            RunningShutdownDraftAction::Prepare,
            app,
            move |owner, result, app| {
                let (request, completed) = settled.borrow_mut().take().unwrap();
                match result {
                    Ok(RunningShutdownDraftProgress::Ready) => {
                        Self::prepare_detached_shutdown_sources(
                            owner,
                            ProjectionCancellationToken::new(),
                            app,
                            move |owner, result, app| {
                                if let Err(error) = result {
                                    Self::fail_nonfinal_close(
                                        owner, request, error, app, completed,
                                    );
                                    return;
                                }
                                Self::publish_ordinary_close_session(
                                    owner,
                                    request,
                                    app,
                                    move |owner, request, result, app| match result {
                                        Ok(ExitAttemptCompletion::SessionReady) => {
                                            Self::finish_nonfinal_close(
                                                owner, request, app, completed,
                                            )
                                        }
                                        Err(error) => {
                                            if owner.borrow().shutdown_session().is_none() {
                                                Self::fail_nonfinal_close(
                                                    owner,
                                                    request,
                                                    format!("{error:?}"),
                                                    app,
                                                    completed,
                                                );
                                                return;
                                            }
                                            let outcome = exit_attempt::ExitAttemptOutcome {
                                                result: Err(error),
                                                command_completed: false,
                                            };
                                            if Self::ordinary_close_home_unavailable(owner) {
                                                let _ = Self::prepare_ordinary_close_recovery(
                                                    owner, app,
                                                );
                                                Self::report_exit_failure(
                                                    owner, &request, &outcome, app,
                                                );
                                            } else {
                                                Self::report_ordinary_command_failure(
                                                    owner,
                                                    request.invoking_window(),
                                                    &format!("{:?}", outcome.result),
                                                    app,
                                                );
                                            }
                                            completed(owner, request, outcome, app);
                                        }
                                        _ => unreachable!(),
                                    },
                                );
                            },
                        )
                    }
                    Err(error) => Self::fail_nonfinal_close(owner, request, error, app, completed),
                    Ok(other) => Self::fail_nonfinal_close(
                        owner,
                        request,
                        format!("ordinary close drafts returned {other:?}"),
                        app,
                        completed,
                    ),
                }
            },
        ) {
            let (request, completed) = delivery.borrow_mut().take().unwrap();
            Self::fail_nonfinal_close(owner, request, error, app, completed);
        }
    }

    fn fail_nonfinal_close(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        error: String,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            exit_attempt::ExitAttemptOutcome,
            &mut App,
        ) + 'static,
    ) {
        if Self::ordinary_close_home_unavailable(owner) {
            let recovery = Self::prepare_ordinary_close_recovery(owner, app);
            let error = recovery
                .err()
                .map(|recovery| format!("{error}; recovery: {recovery}"))
                .unwrap_or(error);
            let outcome = exit_attempt::ExitAttemptOutcome {
                result: Err(ExitAttemptError::SessionPublication(error)),
                command_completed: false,
            };
            Self::report_exit_failure(owner, &request, &outcome, app);
            completed(owner, request, outcome, app);
            return;
        }
        Self::report_ordinary_command_failure(owner, request.invoking_window(), &error, app);
        let delivery = Rc::new(RefCell::new(Some((request, completed))));
        let settled = delivery.clone();
        if Self::drive_shutdown_drafts(
            owner,
            RunningShutdownDraftAction::Release,
            app,
            move |owner, result, app| {
                let (request, completed) = settled.borrow_mut().take().unwrap();
                let mut released = matches!(result, Ok(RunningShutdownDraftProgress::Released));
                if released {
                    released = owner
                        .borrow_mut()
                        .release_pre_native_close_if_present(&request.identity(), app)
                        .is_ok();
                }
                if released {
                    if let Some(window) = owner.borrow().ordinary_close_window {
                        released = matches!(
                            window.update(app, |root, _, cx| {
                                root.set_shutdown_interaction_gated(false, cx)?;
                                root.set_ordinary_close_interaction_gated(false, cx);
                                Ok::<_, String>(())
                            }),
                            Ok(Ok(()))
                        );
                    }
                }
                if released {
                    let mut retained = owner.borrow_mut();
                    retained.shutdown = None;
                    retained.ordinary_close_window = None;
                }
                let command_completed = released && Self::finish_exit(owner, &request);
                completed(
                    owner,
                    request,
                    exit_attempt::ExitAttemptOutcome {
                        result: Err(ExitAttemptError::SessionPublication(error)),
                        command_completed,
                    },
                    app,
                );
            },
        )
        .is_err()
        {
            let (request, completed) = delivery.borrow_mut().take().unwrap();
            completed(
                owner,
                request,
                exit_attempt::ExitAttemptOutcome {
                    result: Err(ExitAttemptError::SessionPublication(
                        "ordinary close draft release remains unproven".into(),
                    )),
                    command_completed: false,
                },
                app,
            );
        }
    }

    fn finish_nonfinal_close(
        owner: &Rc<RefCell<Self>>,
        request: RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            RunningExitRequest,
            exit_attempt::ExitAttemptOutcome,
            &mut App,
        ) + 'static,
    ) {
        let retained = owner.clone();
        let identity = request.identity();
        app.spawn(async move |cx| {
            let result = Self::complete_nonfinal_close(&retained, &identity, cx).await;
            cx.update(|app| {
                let command_completed = if result.is_ok() {
                    {
                        let mut owner = retained.borrow_mut();
                        owner.shutdown = None;
                        owner.ordinary_close_window = None;
                    }
                    Self::finish_exit(&retained, &request)
                } else {
                    false
                };
                let outcome = exit_attempt::ExitAttemptOutcome {
                    result: match result {
                        Ok(None) => Ok(ExitAttemptCompletion::WindowClosed),
                        Ok(Some(error)) | Err(error) => {
                            Err(ExitAttemptError::SessionPublication(error))
                        }
                    },
                    command_completed,
                };
                if let Err(error) = &outcome.result {
                    let native_recovery_allowed = retained
                        .borrow()
                        .process
                        .windows
                        .shells()
                        .iter()
                        .all(|shell| shell.nonfinal_native_recovery_allowed());
                    if !command_completed
                        && native_recovery_allowed
                        && Self::ordinary_close_home_unavailable(&retained)
                    {
                        let _ = Self::prepare_ordinary_close_recovery(&retained, app);
                        Self::report_exit_failure(&retained, &request, &outcome, app);
                    } else {
                        Self::report_ordinary_command_failure(
                            &retained,
                            request.invoking_window(),
                            &format!("{error:?}"),
                            app,
                        );
                    }
                }
                completed(&retained, request, outcome, app);
            })
            .expect("ordinary native close retains GUI delivery");
        })
        .detach();
    }

    async fn complete_nonfinal_close(
        owner: &Rc<RefCell<Self>>,
        identity: &Rc<()>,
        cx: &mut AsyncApp,
    ) -> Result<Option<String>, String> {
        let (drafts, window) = {
            let owner = owner.borrow();
            (
                owner
                    .shutdown
                    .as_ref()
                    .unwrap()
                    .drafts
                    .as_ref()
                    .unwrap()
                    .clone(),
                owner.ordinary_close_window.unwrap(),
            )
        };
        cx.update(|app| drafts.borrow_mut().install_detached_sources(app))
            .map_err(|e| e.to_string())??;
        loop {
            if window
                .update(cx, |root, _, cx| root.drain_detached_shutdown_reads(cx))
                .map_err(|e| e.to_string())?
            {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        let receipt = cx
            .update(|app| {
                owner
                    .borrow_mut()
                    .process
                    .windows
                    .shells_mut()
                    .iter_mut()
                    .find(|shell| shell.window() == window)
                    .ok_or("ordinary close native shell is unavailable")?
                    .begin_nonfinal_native_cleanup(identity, app)
            })
            .map_err(|e| e.to_string())??;
        let native = receipt.await.map_err(|e| e.to_string())?;
        match native {
            gpui::WindowsNativeWindowDestructionOutcome::Unresolved { error } => {
                return Err(format!(
                    "native close settlement remains unresolved: {error}"
                ));
            }
            gpui::WindowsNativeWindowDestructionOutcome::Survived { error } => {
                Self::restore_surviving_nonfinal_close(owner, identity, window, &drafts, cx)
                    .await?;
                return Ok(Some(format!("native window close failed: {error}")));
            }
            gpui::WindowsNativeWindowDestructionOutcome::Destroyed => {}
        }
        loop {
            if cx
                .update(|app| {
                    let mut retained = owner.borrow_mut();
                    let shell = retained
                        .process
                        .windows
                        .shells_mut()
                        .iter_mut()
                        .find(|shell| shell.window() == window)
                        .ok_or("destroyed nonfinal shell is unavailable")?;
                    drafts.borrow_mut().retire_destroyed_nonfinal(shell, app)
                })
                .map_err(|e| e.to_string())??
            {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        cx.update(|app| {
            owner
                .borrow_mut()
                .process
                .windows
                .release_closed_window(window, app)
        })
        .map_err(|e| e.to_string())??;
        Ok(None)
    }
}
