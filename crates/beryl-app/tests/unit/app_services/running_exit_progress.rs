use crate::running_owner::ExitProgressError;

#[test]
fn native_exit_progress_retains_request_through_readiness_and_reopening() {
    exercise(false);
}

#[test]
fn native_exit_progress_refuses_an_ordinary_close_intent() {
    exercise(true);
}

fn exercise(ordinary_close: bool) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                input,
                move |result, app| {
                    let StartupCompletion::Running(running) = result else {
                        panic!("startup failed")
                    };
                    let invoking = running.windows.window_ids()[0];
                    let window = running.windows.shells()[0].window();
                    let original_reason = window.read(app).unwrap().new_window_disabled_reason(app);
                    let permit = running.services.process.execution_permit();
                    let owner = RunningProcessOwner::test_start_unmounted(running, app);
                    let command = owner.borrow().window_exit_command(invoking, app).unwrap();
                    command.request_exit();
                    app.spawn(async move |cx| {
                        let request = next_request(&owner, cx).await;
                        let identity = request.identity();
                        let (mut request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::advance_exit(
                                    &owner,
                                    request,
                                    ProjectionCancellationToken::new(),
                                    app,
                                    |_, _, _, _| panic!("refused progress must not notify"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::Intent));
                        cx.update(|app| assert_eq!(window.read(app).unwrap().new_window_disabled_reason(app), original_reason)).unwrap();
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        assert!(!owner.borrow().test_services_on_worker());
                        permit.commit(|| ()).unwrap();
                        let observation = observe(&owner, ProjectionCancellationToken::new(), cx)
                            .await
                            .unwrap();
                        let intent = if ordinary_close {
                            ShutdownIntent::FinalWindowClose
                        } else {
                            ShutdownIntent::ApplicationExit
                        };
                        cx.update(|app| {
                            owner.borrow_mut().try_begin_idle_shutdown(
                                invoking,
                                intent,
                                &observation,
                                app,
                            )
                        })
                        .unwrap()
                        .unwrap();
                        assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                        if ordinary_close {
                            let (returned, error) = cx
                                .update(|app| {
                                    RunningProcessOwner::recover_exit_drafts(
                                        &owner,
                                        request,
                                        app,
                                        |_, _, _, _| panic!("ordinary close cannot recover as Exit"),
                                    )
                                })
                                .unwrap()
                                .err()
                                .unwrap();
                            request = returned;
                            assert!(matches!(error, ExitProgressError::Intent));
                            let (returned, error) = cx
                                .update(|app| {
                                    RunningProcessOwner::advance_exit(
                                        &owner,
                                        request,
                                        ProjectionCancellationToken::new(),
                                        app,
                                        |_, _, _, _| {
                                            panic!("ordinary close must not advance through Exit")
                                        },
                                    )
                                })
                                .unwrap()
                                .err()
                                .unwrap();
                            request = returned;
                            assert!(matches!(error, ExitProgressError::Intent));
                            assert!(!owner.borrow().test_services_on_worker());
                            super::super::running_shutdown_progress::exercise(
                                owner.clone(),
                                invoking,
                                intent,
                                false,
                                cx,
                            )
                            .await;
                        } else {
                            let pending = Rc::new(RefCell::new(None));
                            let settled = pending.clone();
                            let (returned, error) = cx
                                .update(|app| {
                                    RunningProcessOwner::advance_shutdown(
                                        &owner,
                                        ProjectionCancellationToken::new(),
                                        app,
                                        move |_, _| {
                                            *settled.borrow_mut() = Some(());
                                        },
                                    )
                                    .unwrap();
                                    assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                                    RunningProcessOwner::advance_exit(
                                        &owner,
                                        request,
                                        ProjectionCancellationToken::new(),
                                        app,
                                        |_, _, _, _| {
                                            panic!("pending progress must not notify again")
                                        },
                                    )
                                })
                                .unwrap()
                                .err()
                                .unwrap();
                            request = returned;
                            assert!(matches!(error, ExitProgressError::Scheduling(_)));
                            assert!(Rc::ptr_eq(&identity, &request.identity()));
                            assert!(owner.borrow().test_services_on_worker());
                            wait(&pending, cx).await;
                            assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                            let mut result = owner.borrow_mut().take_shutdown_progress().unwrap().unwrap();
                            assert!(matches!(result, AppServiceShutdownProgress::Waiting | AppServiceShutdownProgress::Ready));
                            let slot = Rc::new(RefCell::new(None));
                            let token = ProjectionCancellationToken::new();
                            let deadline = Instant::now() + Duration::from_secs(5);
                            loop {
                                assert!(Instant::now() < deadline, "Exit progress did not settle");
                                assert!(Rc::ptr_eq(&identity, &request.identity()));
                                assert!(owner.borrow().exit_requested());
                                cx.update(|app| {
                                    let reason = window.read(app).unwrap().new_window_disabled_reason(app);
                                    if matches!(result, AppServiceShutdownProgress::Failed { reopened: true, .. }) {
                                        assert_eq!(reason, original_reason);
                                    } else {
                                        assert_eq!(reason.as_deref(), Some("Application Exit is waiting for active work and durable state."));
                                    }
                                }).unwrap();
                                match result {
                                    AppServiceShutdownProgress::Ready => {
                                        assert!(!RunningProcessOwner::finish_exit(
                                            &owner, &request
                                        ));
                                        assert!(permit.commit(|| ()).is_err());
                                        token.cancel();
                                    }
                                    AppServiceShutdownProgress::Waiting => {
                                        assert!(!RunningProcessOwner::finish_exit(
                                            &owner, &request
                                        ));
                                    }
                                    AppServiceShutdownProgress::Failed {
                                        reopened: false, ..
                                    } => {
                                        assert!(!RunningProcessOwner::finish_exit(
                                            &owner, &request
                                        ));
                                        assert!(permit.commit(|| ()).is_err());
                                    }
                                    AppServiceShutdownProgress::Failed {
                                        reopened: true, ..
                                    } => break,
                                }
                                let delivered = slot.clone();
                                assert!(
                                    cx.update(|app| RunningProcessOwner::advance_exit(
                                        &owner,
                                        request,
                                        token.clone(),
                                        app,
                                        move |owner, request, result, _| {
                                            assert!(!owner.borrow().test_services_on_worker());
                                            assert!(
                                                owner
                                                    .borrow_mut()
                                                    .take_shutdown_progress()
                                                    .is_none()
                                            );
                                            assert!(
                                                delivered
                                                    .borrow_mut()
                                                    .replace((request, result))
                                                    .is_none()
                                            );
                                        },
                                    ))
                                    .unwrap()
                                    .is_ok()
                                );
                                wait(&slot, cx).await;
                                let (returned, progress) = slot.borrow_mut().take().unwrap();
                                request = returned;
                                result = progress.unwrap();
                            }
                        }
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        assert!(owner.borrow().shutdown_status().is_none());
                        if !ordinary_close {
                            let observation =
                                observe(&owner, ProjectionCancellationToken::new(), cx)
                                    .await
                                    .unwrap();
                            cx.update(|app| {
                                owner.borrow_mut().try_begin_idle_shutdown(
                                    invoking,
                                    ShutdownIntent::ApplicationExit,
                                    &observation,
                                    app,
                                )
                            })
                            .unwrap()
                            .unwrap();
                            let cancellation = ProjectionCancellationToken::new();
                            cancellation.cancel();
                            let deadline = Instant::now() + Duration::from_secs(5);
                            loop {
                                assert!(Instant::now() < deadline, "reopening did not settle");
                                let slot = Rc::new(RefCell::new(None));
                                let delivered = slot.clone();
                                cx.update(|app| {
                                    RunningProcessOwner::advance_shutdown(
                                        &owner,
                                        cancellation.clone(),
                                        app,
                                        move |_, _| *delivered.borrow_mut() = Some(()),
                                    )
                                })
                                .unwrap()
                                .unwrap();
                                wait(&slot, cx).await;
                                assert!(!owner.borrow().test_services_on_worker());
                                assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                                let reopened = owner.borrow().shutdown_status().is_none();
                                let result = owner
                                    .borrow_mut()
                                    .take_shutdown_progress()
                                    .unwrap()
                                    .unwrap();
                                if reopened {
                                    assert!(matches!(
                                        result,
                                        AppServiceShutdownProgress::Failed { reopened: true, .. }
                                    ));
                                    break;
                                }
                            }
                        }
                        assert!(permit.commit(|| ()).is_err());
                        owner
                            .borrow()
                            .test_services()
                            .process
                            .execution_permit()
                            .commit(|| ())
                            .unwrap();
                        assert!(RunningProcessOwner::finish_exit(&owner, &request));
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::advance_exit(
                                    &owner,
                                    request,
                                    ProjectionCancellationToken::new(),
                                    app,
                                    |_, _, _, _| panic!("stale request must not advance"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::Request(_)));
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        assert!(!owner.borrow().exit_requested());
                        let running = Rc::try_unwrap(owner)
                            .ok()
                            .unwrap()
                            .into_inner()
                            .test_into_process();
                        support::dispose_running(running, cx).await;
                        observed.set(true);
                        cx.update(|app| app.quit()).unwrap();
                    })
                    .detach();
                },
                app,
            );
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_reopens(&directory);
}
