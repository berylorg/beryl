use crate::running_owner::RunningShutdownDraftAction;

#[test]
fn native_draft_driver_retains_owner_and_excludes_overlapping_progress() {
    run(false);
}

#[test]
fn native_draft_driver_delivers_failure_and_retains_recovery_custody() {
    run(true);
}

fn run(fail_preparation: bool) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                input,
                move |running, app| {
                    let StartupCompletion::Running(running) = running else {
                        panic!("startup failed")
                    };
                    let invoking = running.windows.window_ids()[0];
                    let window = running.windows.shells()[0].window();
                    let owner = RunningProcessOwner::test_start_unmounted(running, app);
                    assert!(
                        RunningProcessOwner::drive_shutdown_drafts(
                            &owner,
                            RunningShutdownDraftAction::Prepare,
                            app,
                            |_, _, _| panic!("unadmitted driver must not deliver")
                        )
                        .is_err()
                    );
                    app.spawn(async move |cx| {
                        let deadline = Instant::now() + Duration::from_secs(5);
                        loop {
                            let job = owner
                                .borrow()
                                .test_services()
                                .prepare_shutdown_observation()
                                .unwrap();
                            let observation = cx
                                .background_executor()
                                .spawn(async move {
                                    job.collect(&ProjectionCancellationToken::new()).unwrap()
                                })
                                .await;
                            if cx
                                .update(|app| {
                                    owner.borrow_mut().try_begin_idle_shutdown(
                                        invoking,
                                        ShutdownIntent::ApplicationExit,
                                        &observation,
                                        app,
                                    )
                                })
                                .unwrap()
                                .is_ok()
                            {
                                break;
                            }
                            assert!(Instant::now() < deadline);
                            cx.background_executor()
                                .timer(Duration::from_millis(10))
                                .await;
                        }
                        cx.update(|app| {
                            assert!(
                                RunningProcessOwner::drive_shutdown_drafts(
                                    &owner,
                                    RunningShutdownDraftAction::Prepare,
                                    app,
                                    |_, _, _| panic!("work not ready")
                                )
                                .is_err()
                            );
                            RunningProcessOwner::install_shutdown_interaction_gate(&owner, app)
                                .unwrap();
                        })
                        .unwrap();
                        prepare_work(&owner, cx).await;
                        let original_attempt =
                            owner.borrow().test_services().graph().unwrap().shutdown;
                        let cancelled = ProjectionCancellationToken::new();
                        cancelled.cancel();
                        let weak = Rc::downgrade(&owner);
                        let polls = Rc::new(Cell::new(0));
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        let thread = std::thread::current().id();
                        let start = Instant::now();
                        cx.update(|app| {
                            assert!(
                                RunningProcessOwner::drive_shutdown_drafts(
                                    &owner,
                                    RunningShutdownDraftAction::Release,
                                    app,
                                    |_, _, _| panic!("no preparation")
                                )
                                .is_err()
                            );
                            if fail_preparation {
                                window
                                    .update(app, |root, _, cx| {
                                        root.set_shutdown_interaction_gated(false, cx)
                                    })
                                    .unwrap()
                                    .unwrap();
                            }
                            RunningProcessOwner::test_drive_delayed_shutdown_drafts(
                                &owner,
                                app,
                                polls.clone(),
                                move |owner, result, app| {
                                    assert_eq!(std::thread::current().id(), thread);
                                    assert!(owner.try_borrow_mut().is_ok());
                                    assert_eq!(result.is_err(), fail_preparation);
                                    if !fail_preparation {
                                        assert_eq!(
                                            result.unwrap(),
                                            RunningShutdownDraftProgress::Ready
                                        );
                                    }
                                    assert_eq!(
                                        RunningProcessOwner::advance_shutdown_drafts(owner, app)
                                            .is_err(),
                                        fail_preparation
                                    );
                                    sender.send(owner.clone()).ok().unwrap();
                                },
                            )
                            .unwrap();
                            for action in [
                                RunningShutdownDraftAction::Prepare,
                                RunningShutdownDraftAction::Release,
                            ] {
                                assert!(
                                    RunningProcessOwner::drive_shutdown_drafts(
                                        &owner,
                                        action,
                                        app,
                                        |_, _, _| panic!("duplicate driver")
                                    )
                                    .is_err()
                                );
                            }
                            assert!(
                                RunningProcessOwner::advance_shutdown_drafts(&owner, app).is_err()
                            );
                            assert!(
                                RunningProcessOwner::release_shutdown_drafts(&owner, app).is_err()
                            );
                            assert!(
                                RunningProcessOwner::advance_shutdown(
                                    &owner,
                                    cancelled.clone(),
                                    app,
                                    |_, _| panic!("driver retains services")
                                )
                                .is_err()
                            );
                        })
                        .unwrap();
                        drop(owner);
                        assert!(weak.upgrade().is_some());
                        let owner = receiver.await.unwrap();
                        assert_eq!(polls.get(), 3);
                        assert!(start.elapsed() >= Duration::from_millis(100));
                        assert_eq!(
                            owner.borrow().test_services().graph().unwrap().shutdown,
                            original_attempt
                        );
                        if !fail_preparation {
                            let (sender, receiver) = futures_channel::oneshot::channel();
                            cx.update(|app| {
                                window
                                    .update(app, |root, _, cx| {
                                        root.set_shutdown_interaction_gated(false, cx)
                                    })
                                    .unwrap()
                                    .unwrap();
                                RunningProcessOwner::drive_shutdown_drafts(
                                    &owner,
                                    RunningShutdownDraftAction::Release,
                                    app,
                                    move |owner, result, _| {
                                        assert!(result.is_err());
                                        assert!(owner.try_borrow_mut().is_ok());
                                        sender.send(()).unwrap();
                                    },
                                )
                                .unwrap();
                            })
                            .unwrap();
                            receiver.await.unwrap();
                        }
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        cx.update(|app| {
                            window
                                .update(app, |root, _, cx| {
                                    root.set_shutdown_interaction_gated(true, cx)
                                })
                                .unwrap()
                                .unwrap();
                            assert!(
                                RunningProcessOwner::advance_shutdown(
                                    &owner,
                                    cancelled.clone(),
                                    app,
                                    |_, _| panic!("not released")
                                )
                                .is_err()
                            );
                            RunningProcessOwner::drive_shutdown_drafts(
                                &owner,
                                RunningShutdownDraftAction::Release,
                                app,
                                move |owner, result, app| {
                                    assert_eq!(
                                        result.unwrap(),
                                        RunningShutdownDraftProgress::Released
                                    );
                                    assert_eq!(
                                        RunningProcessOwner::release_shutdown_drafts(owner, app)
                                            .unwrap(),
                                        RunningShutdownDraftProgress::Released
                                    );
                                    assert!(
                                        RunningProcessOwner::release_shutdown_interaction_gate(
                                            owner, app
                                        )
                                        .is_err()
                                    );
                                    assert!(
                                        RunningProcessOwner::drive_shutdown_drafts(
                                            owner,
                                            RunningShutdownDraftAction::Prepare,
                                            app,
                                            |_, _, _| panic!("cannot prepare after release")
                                        )
                                        .is_err()
                                    );
                                    sender.send(()).unwrap();
                                },
                            )
                            .unwrap();
                        })
                        .unwrap();
                        receiver.await.unwrap();
                        let deadline = Instant::now() + Duration::from_secs(5);
                        loop {
                            cx.update(|app| {
                                RunningProcessOwner::advance_shutdown(
                                    &owner,
                                    cancelled.clone(),
                                    app,
                                    |_, _| {},
                                )
                            })
                            .unwrap()
                            .unwrap();
                            settled(&owner, cx).await;
                            match owner
                                .borrow_mut()
                                .take_shutdown_progress()
                                .unwrap()
                                .unwrap()
                            {
                                AppServiceShutdownProgress::Failed { reopened: true, .. } => break,
                                AppServiceShutdownProgress::Failed {
                                    reopened: false, ..
                                } => assert!(Instant::now() < deadline),
                                other => panic!("unexpected recovery: {other:?}"),
                            }
                        }
                        let process = Rc::try_unwrap(owner)
                            .ok()
                            .unwrap()
                            .into_inner()
                            .test_into_process();
                        support::dispose_running(process, cx).await;
                        assert!(weak.upgrade().is_none());
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
