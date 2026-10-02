use super::*;
use crate::running_owner::ExitDraftPreparationCompletion;

#[test]
fn native_detached_source_budget_refusal_recovers_complete_resident_set() {
    run(false);
}

#[test]
fn native_detached_source_cancellation_prevents_session_publication() {
    run(true);
}

fn run(cancelled: bool) {
    let directory = resident_recovery::resident_fixture::selected_home_with_windows(2);
    let mut configuration = input(directory.path(), |path, _| support::open(path));
    configuration.windows = resident_recovery::resident_fixture::selected_inputs();
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                configuration,
                move |result, app| {
                    let StartupCompletion::Running(running) = result else {
                        panic!("startup failed")
                    };
                    let invoking = running.windows.window_ids()[0];
                    let windows = running
                        .windows
                        .shells()
                        .iter()
                        .map(|shell| shell.window())
                        .collect::<Vec<_>>();
                    let limits = beryl_home_store::TemporaryReadPoolLimits::new(
                        1024 * 1024 * 1024,
                        if cancelled { 256 } else { 1 },
                        65_536,
                    )
                    .unwrap();
                    let owner = RunningProcessOwner::test_start_unmounted_with_detached_read_limits(
                        running, limits, app,
                    );
                    owner
                        .borrow()
                        .window_exit_command(invoking, app)
                        .unwrap()
                        .request_exit();
                    app.spawn(async move |cx| {
                        let request = next_request(&owner, cx).await;
                        let identity = request.identity();
                        let observation = observe(&owner, ProjectionCancellationToken::new(), cx)
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
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        assert!(
                            cx.update(|app| RunningProcessOwner::drive_exit(
                                &owner,
                                request,
                                ProjectionCancellationToken::new(),
                                app,
                                move |_, request, result, _| {
                                    assert!(matches!(
                                        result.unwrap(),
                                        AppServiceShutdownProgress::Ready
                                    ));
                                    sender.send(request).ok().unwrap();
                                }
                            ))
                            .unwrap()
                            .is_ok()
                        );
                        let request = receiver.await.unwrap();
                        let cancellation = ProjectionCancellationToken::new();
                        if cancelled {
                            cancellation.cancel();
                        }
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        assert!(
                            cx.update(|app| {
                                RunningProcessOwner::prepare_exit_drafts_with_cancellation(
                                    &owner,
                                    request,
                                    cancellation,
                                    app,
                                    move |_, request, result, _| {
                                        let ExitDraftPreparationCompletion::Failed {
                                            preparation,
                                            recovery,
                                        } = result
                                        else {
                                            panic!("incomplete detached set admitted")
                                        };
                                        if cancelled {
                                            assert!(
                                                preparation.contains("cancel"),
                                                "{preparation}"
                                            );
                                        }
                                        assert!(matches!(
                                            recovery.unwrap(),
                                            AppServiceShutdownProgress::Failed {
                                                reopened: true,
                                                ..
                                            }
                                        ));
                                        sender.send(request).ok().unwrap();
                                    },
                                )
                            })
                            .unwrap()
                            .is_ok()
                        );
                        let request = receiver.await.unwrap();
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        assert!(owner.borrow().shutdown_status().is_none());
                        let usage = owner.borrow().test_detached_read_usage();
                        assert_eq!(usage.sources, 0);
                        assert_eq!(usage.reserved_bytes, 0);
                        assert!(owner.borrow().test_services().graph().is_some());
                        cx.update(|app| {
                            for window in windows {
                                let root = window.read(app).unwrap();
                                let mount = root.controller().unwrap().composer_mount().unwrap();
                                let composer = mount.read(app).contribution().unwrap();
                                assert!(composer.read(app).gpui_input().read(app).is_enabled());
                                assert_eq!(root.test_exit_presentation().0, "Exit");
                            }
                            assert!(RunningProcessOwner::finish_exit(&owner, &request));
                            assert!(
                                RunningProcessOwner::finish_ready_exit(
                                    &owner,
                                    request,
                                    app,
                                    |_, _, _| panic!("stale cancelled request admitted")
                                )
                                .is_err()
                            );
                        })
                        .unwrap();
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
