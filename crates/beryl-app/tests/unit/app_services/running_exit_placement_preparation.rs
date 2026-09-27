use crate::running_owner::{
    ExitPlacementPreparationCompletion, ExitProgressError, RunningShutdownDraftProgress,
};

#[test]
fn native_exit_placement_preparation_retains_ready_attempt() {
    run(false, false);
}

#[test]
fn native_exit_placement_preparation_recovers_original_capture_failure() {
    run(true, false);
}

#[test]
fn native_exit_placement_preparation_preserves_both_failures_and_custody() {
    run(true, true);
}

fn run(capture_failure: bool, recovery_failure: bool) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new().with_quit_on_last_window_close(false).run(move |app| {
        startup_owner::start(input, move |result, app| {
            let StartupCompletion::Running(running) = result else { panic!("startup failed") };
            let invoking = running.windows.window_ids()[0];
            let window = running.windows.shells()[0].window();
            let original_reason = window.read(app).unwrap().new_window_disabled_reason(app);
            let owner = RunningProcessOwner::start(running, app);
            owner.borrow().window_exit_command(invoking, app).unwrap().request_exit();
            app.spawn(async move |cx| {
                let request = next_request(&owner, cx).await;
                let identity = request.identity();
                let (request, error) = cx.update(|app| RunningProcessOwner::prepare_exit_placements(
                    &owner, request, app, |_, _, _, _| panic!("unadmitted capture")))
                    .unwrap().err().unwrap();
                assert!(matches!(error, ExitProgressError::Intent));
                let observation = observe(&owner, ProjectionCancellationToken::new(), cx).await.unwrap();
                cx.update(|app| owner.borrow_mut().try_begin_idle_shutdown(
                    invoking, ShutdownIntent::ApplicationExit, &observation, app)).unwrap().unwrap();
                let (request, error) = cx.update(|app| RunningProcessOwner::prepare_exit_placements(
                    &owner, request, app, |_, _, _, _| panic!("work not ready")))
                    .unwrap().err().unwrap();
                assert!(matches!(error, ExitProgressError::Intent));
                let (sender, receiver) = futures_channel::oneshot::channel();
                assert!(cx.update(|app| RunningProcessOwner::drive_exit(
                    &owner, request, ProjectionCancellationToken::new(), app,
                    move |_, request, result, _| {
                        assert!(matches!(result.unwrap(), AppServiceShutdownProgress::Ready));
                        sender.send(request).ok().unwrap();
                    })).unwrap().is_ok());
                let request = receiver.await.unwrap();
                let attempt = owner.borrow().test_services().graph().unwrap().shutdown;
                let (request, error) = cx.update(|app| RunningProcessOwner::prepare_exit_placements(
                    &owner, request, app, |_, _, _, _| panic!("drafts not ready")))
                    .unwrap().err().unwrap();
                assert!(matches!(error, ExitProgressError::PlacementPreparation(_)));
                assert!(Rc::ptr_eq(&identity, &request.identity()));
                cx.update(|app| assert_eq!(RunningProcessOwner::advance_shutdown_drafts(&owner, app).unwrap(),
                    RunningShutdownDraftProgress::Ready)).unwrap();
                let (lease, released) = cx.update(|app| window.update(app, |_, window, _|
                    window.lease_published_windows_window().unwrap()).unwrap()).unwrap();
                let raw = lease.raw_handle();
                drop(lease);
                assert!(!released.await.unwrap().native_destroyed);
                if capture_failure {
                    unsafe { windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        windows::Win32::Foundation::HWND(raw as *mut _),
                        windows::Win32::UI::WindowsAndMessaging::SW_HIDE) };
                }
                if recovery_failure {
                    cx.update(|app| window.update(app, |root, _, cx|
                        root.set_shutdown_interaction_gated(false, cx)).unwrap().unwrap()).unwrap();
                }
                let (sender, receiver) = futures_channel::oneshot::channel();
                let weak = Rc::downgrade(&owner);
                let thread = std::thread::current().id();
                assert!(cx.update(|app| RunningProcessOwner::prepare_exit_placements(
                    &owner, request, app, move |owner, request, result, app| {
                        assert_eq!(thread, std::thread::current().id());
                        assert!(owner.try_borrow_mut().is_ok());
                        assert!(!owner.borrow().test_services_on_worker());
                        assert!(owner.borrow().exit_requested());
                        match result {
                            ExitPlacementPreparationCompletion::Ready => {
                                assert!(!capture_failure);
                                let placements = owner.borrow().shutdown_placements().unwrap();
                                assert_eq!(placements.len(), 1);
                                assert_eq!(placements[0].0, invoking);
                                assert_eq!(owner.borrow().test_services().graph().unwrap().shutdown, attempt);
                                assert!(!RunningProcessOwner::finish_exit(owner, &request));
                            }
                            ExitPlacementPreparationCompletion::Failed { preparation, recovery } => {
                                assert!(capture_failure);
                                assert!(preparation.contains(&invoking.to_string()));
                                if recovery_failure {
                                    assert!(matches!(recovery, Err(ExitProgressError::DraftRelease(_))));
                                    assert_eq!(owner.borrow().test_services().graph().unwrap().shutdown, attempt);
                                    assert!(preparation.contains("Exit geometry capture failed"));
                                    assert!(owner.borrow().shutdown_placements().is_err());
                                    assert!(!RunningProcessOwner::finish_exit(owner, &request));
                                } else {
                                    assert!(matches!(recovery.unwrap(), AppServiceShutdownProgress::Failed {
                                        reason: crate::cas_projection::ShutdownFailure::Cancelled, reopened: true
                                    }));
                                    assert!(owner.borrow().shutdown_status().is_none());
                                    assert_eq!(window.read(app).unwrap().new_window_disabled_reason(app), original_reason);
                                    assert!(RunningProcessOwner::finish_exit(owner, &request));
                                }
                            }
                        }
                        sender.send((owner.clone(), request)).ok().unwrap();
                    })).unwrap().is_ok());
                drop(owner);
                assert!(weak.upgrade().is_some());
                let (owner, mut request) = receiver.await.unwrap();
                assert!(Rc::ptr_eq(&identity, &request.identity()));
                if capture_failure {
                    unsafe { windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        windows::Win32::Foundation::HWND(raw as *mut _),
                        windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE) };
                }
                if !capture_failure || recovery_failure {
                    let (returned, error) = cx.update(|app| RunningProcessOwner::prepare_exit_placements(
                        &owner, request, app, |_, _, _, _| panic!("duplicate capture")))
                        .unwrap().err().unwrap();
                    assert!(matches!(error, ExitProgressError::PlacementPreparation(_)));
                    request = returned;
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    assert!(cx.update(|app| {
                        window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(true, cx)).unwrap().unwrap();
                        RunningProcessOwner::recover_exit_drafts(&owner, request, app,
                            move |owner, request, result, _| {
                                assert!(matches!(result.unwrap(), AppServiceShutdownProgress::Failed { reopened: true, .. }));
                                assert!(RunningProcessOwner::finish_exit(owner, &request));
                                sender.send(request).ok().unwrap();
                            })
                    }).unwrap().is_ok());
                    request = receiver.await.unwrap();
                }
                let (_, error) = cx.update(|app| RunningProcessOwner::prepare_exit_placements(
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
