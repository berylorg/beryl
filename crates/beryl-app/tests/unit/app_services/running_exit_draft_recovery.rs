use crate::running_owner::{ExitProgressError, RunningShutdownDraftProgress};

#[test]
fn native_exit_draft_recovery_reopens_after_exact_release() {
    run(false, false);
}

#[test]
fn native_exit_draft_recovery_reopens_after_preparation_failure() {
    run(true, false);
}

#[test]
fn native_exit_draft_recovery_preserves_custody_on_release_failure() {
    run(false, true);
}

fn run(preparation_failure: bool, release_failure: bool) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(input, move |result, app| {
                let StartupCompletion::Running(running) = result else { panic!("startup failed") };
                let invoking = running.windows.window_ids()[0];
                let window = running.windows.shells()[0].window();
                let original_reason = window.read(app).unwrap().new_window_disabled_reason(app);
                    let owner = RunningProcessOwner::test_start_unmounted(running, app);
                owner.borrow().window_exit_command(invoking, app).unwrap().request_exit();
                app.spawn(async move |cx| {
                    let request = next_request(&owner, cx).await;
                    let identity = request.identity();
                    let (mut request, error) = cx.update(|app| {
                        RunningProcessOwner::recover_exit_drafts(&owner, request, app,
                            |_, _, _, _| panic!("unadmitted recovery"))
                    }).unwrap().err().unwrap();
                    assert!(matches!(error, ExitProgressError::Intent));
                    let observation = observe(&owner, ProjectionCancellationToken::new(), cx).await.unwrap();
                    cx.update(|app| owner.borrow_mut().try_begin_idle_shutdown(
                        invoking, ShutdownIntent::ApplicationExit, &observation, app)).unwrap().unwrap();
                    (request, _) = cx.update(|app| RunningProcessOwner::recover_exit_drafts(
                        &owner, request, app, |_, _, _, _| panic!("work not ready")))
                        .unwrap().err().unwrap();
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    assert!(cx.update(|app| RunningProcessOwner::drive_exit(
                        &owner, request, ProjectionCancellationToken::new(), app,
                        move |_, request, result, _| {
                            assert!(matches!(result.unwrap(), AppServiceShutdownProgress::Ready));
                            sender.send(request).ok().unwrap();
                        })).unwrap().is_ok());
                    request = receiver.await.unwrap();
                    let original_attempt = owner.borrow().test_services().graph().unwrap().shutdown;
                    let (returned, error) = cx.update(|app| RunningProcessOwner::recover_exit_drafts(
                        &owner, request, app, |_, _, _, _| panic!("no captured drafts")))
                        .unwrap().err().unwrap();
                    request = returned;
                    assert!(matches!(error, ExitProgressError::DraftRelease(_)));
                    cx.update(|app| {
                        if preparation_failure {
                            window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(false, cx))
                                .unwrap().unwrap();
                        }
                        let result = RunningProcessOwner::advance_shutdown_drafts(&owner, app);
                        if preparation_failure { assert!(result.is_err()); }
                        else { assert_eq!(result.unwrap(), RunningShutdownDraftProgress::Ready); }
                        window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(true, cx))
                            .unwrap().unwrap();
                    }).unwrap();
                    if release_failure {
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        assert!(cx.update(|app| {
                            window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(false, cx))
                                .unwrap().unwrap();
                            RunningProcessOwner::recover_exit_drafts(&owner, request, app,
                                move |owner, request, result, _| {
                                    assert!(matches!(result, Err(ExitProgressError::DraftRelease(_))));
                                    assert!(owner.try_borrow_mut().is_ok());
                                    assert!(!owner.borrow().test_services_on_worker());
                                    assert!(matches!(owner.borrow().shutdown_status(), Some((_, _, RunningShutdownStatus::WorkReady))));
                                    assert!(!RunningProcessOwner::finish_exit(owner, &request));
                                    sender.send(request).ok().unwrap();
                                })
                        }).unwrap().is_ok());
                        request = receiver.await.unwrap();
                        assert_eq!(owner.borrow().test_services().graph().unwrap().shutdown, original_attempt);
                        cx.update(|app| window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(true, cx)))
                            .unwrap().unwrap().unwrap();
                    }
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    let thread = std::thread::current().id();
                    let weak = Rc::downgrade(&owner);
                    assert!(cx.update(|app| {
                        let scheduled = RunningProcessOwner::recover_exit_drafts(&owner, request, app,
                            move |owner, request, result, app| {
                                assert_eq!(std::thread::current().id(), thread);
                                assert!(owner.try_borrow_mut().is_ok());
                                assert!(matches!(result.unwrap(), AppServiceShutdownProgress::Failed {
                                    reason: crate::cas_projection::ShutdownFailure::Cancelled, reopened: true
                                }));
                                assert!(owner.borrow().shutdown_status().is_none());
                                assert!(owner.borrow().exit_requested());
                                assert_eq!(window.read(app).unwrap().new_window_disabled_reason(app), original_reason);
                                assert!(RunningProcessOwner::finish_exit(owner, &request));
                                sender.send((owner.clone(), request)).ok().unwrap();
                            });
                        assert!(RunningProcessOwner::release_shutdown_drafts(&owner, app).is_err());
                        assert!(RunningProcessOwner::drive_shutdown_drafts(&owner,
                            crate::running_owner::RunningShutdownDraftAction::Release, app,
                            |_, _, _| panic!("overlapping release")).is_err());
                        scheduled
                    }).unwrap().is_ok());
                    drop(owner);
                    assert!(weak.upgrade().is_some());
                    let (owner, request) = receiver.await.unwrap();
                    assert!(Rc::ptr_eq(&identity, &request.identity()));
                    let (_, error) = cx.update(|app| RunningProcessOwner::recover_exit_drafts(
                        &owner, request, app, |_, _, _, _| panic!("stale request")))
                        .unwrap().err().unwrap();
                    assert!(matches!(error, ExitProgressError::Request(_)));
                    let running = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                    support::dispose_running(running, cx).await;
                    assert!(weak.upgrade().is_none());
                    observed.set(true);
                    cx.update(|app| app.quit()).unwrap();
                }).detach();
            }, app);
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_reopens(&directory);
}
