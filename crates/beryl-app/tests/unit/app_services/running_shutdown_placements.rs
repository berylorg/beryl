#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureCase {
    Success,
    Unwind,
    Hidden,
    FinalClose,
}

mod reconciliation {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_shutdown_session_reconciliation.rs"
    ));
}

async fn exercise_session(
    owner: Rc<RefCell<RunningProcessOwner>>,
    unwind: bool,
    cx: &mut AsyncApp,
) -> Rc<RefCell<RunningProcessOwner>> {
    use crate::exit_session::ExitSessionExecution;
    use crate::running_owner::RunningShutdownSession;
    let original_attempt = owner.borrow().test_services().graph().unwrap().shutdown;
    let placements = owner.borrow().shutdown_placements().unwrap();
    let weak = Rc::downgrade(&owner);
    let (release, parked) = std::sync::mpsc::sync_channel(1);
    let (sender, receiver) = futures_channel::oneshot::channel();
    let gui_thread = std::thread::current().id();
    assert!(owner.borrow().require_shutdown_session_ready().is_err());
    cx.update(|app| {
        RunningProcessOwner::test_publish_shutdown_session(
            &owner,
            app,
            move |owner, app| {
                assert_eq!(std::thread::current().id(), gui_thread);
                assert!(owner.try_borrow_mut().is_ok());
                assert!(!owner.borrow().test_services_on_worker());
                assert_eq!(
                    owner.borrow().test_services().graph().unwrap().shutdown,
                    original_attempt
                );
                assert_eq!(owner.borrow().shutdown_placements().unwrap(), placements);
                match owner.borrow().shutdown_session().unwrap() {
                    RunningShutdownSession::Unwound => assert!(unwind),
                    RunningShutdownSession::Settled(Ok(ExitSessionExecution::Committed {
                        receipt,
                        later_failure,
                        local_finalization,
                        ..
                    })) => {
                        assert!(!unwind);
                        assert!(later_failure.is_none());
                        assert!(local_finalization.is_none());
                        let owner = owner.borrow();
                        let graph = owner.test_services().graph().unwrap();
                        assert!(
                            graph
                                .state()
                                .session()
                                .committed_revision(graph.home(), receipt)
                                .unwrap()
                                .is_some()
                        );
                    }
                    other => panic!("unexpected session result: {other:?}"),
                }
                assert_session_fenced(owner, app);
                assert_eq!(
                    owner.borrow().require_shutdown_session_ready().is_ok(),
                    !unwind
                );
                assert!(
                    RunningProcessOwner::reconcile_shutdown_session(owner, app, |_, _| {
                        panic!("commit and unwind are not pending reconciliation")
                    })
                    .is_err()
                );
                sender.send(owner.clone()).ok().unwrap();
            },
            move || {
                assert_ne!(std::thread::current().id(), gui_thread);
                parked.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(!unwind, "injected session worker unwind");
            },
        )
        .unwrap();
        assert!(owner.borrow().test_services_on_worker());
        assert!(owner.borrow().require_shutdown_session_ready().is_err());
        assert!(matches!(
            owner.borrow().shutdown_session(),
            Some(RunningShutdownSession::Pending)
        ));
        assert_session_fenced(&owner, app);
    })
    .unwrap();
    drop(owner);
    assert!(weak.upgrade().is_some());
    release.send(()).unwrap();
    receiver.await.unwrap()
}

fn assert_session_fenced(owner: &Rc<RefCell<RunningProcessOwner>>, app: &mut gpui::App) {
    assert!(
        RunningProcessOwner::publish_shutdown_session(owner, app, |_, _| panic!(
            "duplicate session"
        ))
        .is_err()
    );
    assert!(RunningProcessOwner::release_shutdown_drafts(owner, app).is_err());
    assert!(
        RunningProcessOwner::drive_shutdown_drafts(
            owner,
            crate::running_owner::RunningShutdownDraftAction::Release,
            app,
            |_, _, _| panic!("session owns recovery custody")
        )
        .is_err()
    );
    let cancelled = ProjectionCancellationToken::new();
    cancelled.cancel();
    assert!(
        RunningProcessOwner::advance_shutdown(owner, cancelled, app, |_, _| panic!(
            "session owns recovery custody"
        ))
        .is_err()
    );
    assert!(RunningProcessOwner::release_shutdown_interaction_gate(owner, app).is_err());
}

#[test]
fn native_exit_placement_retains_owner_and_exact_complete_result() {
    run(CaptureCase::Success);
}

#[test]
fn native_exit_placement_unwind_drains_before_recovery() {
    run(CaptureCase::Unwind);
}

#[test]
fn native_exit_placement_admission_failure_preserves_attempt() {
    run(CaptureCase::Hidden);
}

#[test]
fn native_final_close_refuses_exit_placement_capture() {
    run(CaptureCase::FinalClose);
}

fn run(case: CaptureCase) {
    run_with_session(case, None);
}

#[test]
fn native_exit_session_retains_commit_and_fences_recovery() {
    run_with_session(CaptureCase::Success, Some(false));
}

#[test]
fn native_exit_session_unwind_retains_services_and_fences_recovery() {
    run_with_session(CaptureCase::Success, Some(true));
}

fn run_with_session(case: CaptureCase, session_unwind: Option<bool>) {
    run_with_reconciliation(case, session_unwind, None);
}

fn run_with_reconciliation(
    case: CaptureCase,
    session_unwind: Option<bool>,
    reconciliation_unwind: Option<bool>,
) {
    let directory = support::native_home();
    let faults = FaultController::new();
    let opening_faults = faults.clone();
    let input = input(directory.path(), move |path, _| {
        if reconciliation_unwind.is_none() {
            return support::open(path);
        }
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
            opening_faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let candidate = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        StartupHomeOpen::Ready {
            candidate,
            state,
            syndic,
        }
    });
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new().with_quit_on_last_window_close(false).run(move |app| {
        startup_owner::start(input, move |running, app| {
            let StartupCompletion::Running(running) = running else { panic!("startup failed") };
            let invoking = running.windows.window_ids()[0];
            let window = running.windows.shells()[0].window();
                    let owner = RunningProcessOwner::test_start_unmounted(running, app);
            assert!(owner.borrow().require_shutdown_session_ready().is_err());
            assert!(RunningProcessOwner::reconcile_shutdown_session(&owner, app,
                |_, _| panic!("unadmitted reconciliation")).is_err());
            assert!(RunningProcessOwner::publish_shutdown_session(&owner, app,
                |_, _| panic!("unadmitted session")).is_err());
            assert!(RunningProcessOwner::capture_shutdown_placements(&owner, app,
                |_, _, _| panic!("unadmitted capture")).is_err());
            app.spawn(async move |cx| {
                let intent = if case == CaptureCase::FinalClose {
                    ShutdownIntent::FinalWindowClose
                } else { ShutdownIntent::ApplicationExit };
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    let job = owner.borrow().test_services().prepare_shutdown_observation().unwrap();
                    let observation = cx.background_executor().spawn(async move {
                        job.collect(&ProjectionCancellationToken::new()).unwrap()
                    }).await;
                    if cx.update(|app| owner.borrow_mut().try_begin_idle_shutdown(
                        invoking, intent, &observation, app)).unwrap().is_ok() { break }
                    assert!(Instant::now() < deadline);
                    cx.background_executor().timer(Duration::from_millis(10)).await;
                }
                cx.update(|app| RunningProcessOwner::install_shutdown_interaction_gate(&owner, app)).unwrap().unwrap();
                prepare_work(&owner, cx).await;
                assert!(owner.borrow().require_shutdown_session_ready().is_err());
                let original_attempt = owner.borrow().test_services().graph().unwrap().shutdown;
                cx.update(|app| assert!(RunningProcessOwner::publish_shutdown_session(&owner, app,
                    |_, _| panic!("session requires drafts and placements")).is_err())).unwrap();
                cx.update(|app| {
                    assert!(RunningProcessOwner::capture_shutdown_placements(&owner, app,
                        |_, _, _| panic!("drafts not ready")).is_err());
                    assert_eq!(RunningProcessOwner::advance_shutdown_drafts(&owner, app).unwrap(),
                        RunningShutdownDraftProgress::Ready);
                }).unwrap();
                let cancelled = ProjectionCancellationToken::new();
                cancelled.cancel();
                let owner = if case == CaptureCase::FinalClose {
                    cx.update(|app| assert!(RunningProcessOwner::capture_shutdown_placements(&owner, app,
                        |_, _, _| panic!("ordinary close has no preserved Exit set")).is_err())).unwrap();
                    owner
                } else {
                    let (geometry, lease, released) = cx.update(|app| window.update(app, |_, window, _| {
                        let geometry = window.capture_windows_window_placement().unwrap();
                        let (lease, released) = window.lease_published_windows_window().unwrap();
                        (geometry, lease, released)
                    }).unwrap()).unwrap();
                    let raw = lease.raw_handle();
                    let desktop = cx.background_executor().spawn(async move {
                        crate::main_window::observe_windows_desktop(lease).unwrap()
                    }).await;
                    assert!(!released.await.unwrap().native_destroyed);
                    let expected = crate::main_window::windows_window_placement_from_capture(geometry, Some(desktop)).unwrap();
                    if case == CaptureCase::Hidden {
                        unsafe { windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                            windows::Win32::Foundation::HWND(raw as *mut _),
                            windows::Win32::UI::WindowsAndMessaging::SW_HIDE) };
                    }
                    let weak = Rc::downgrade(&owner);
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    let (release, parked) = std::sync::mpsc::sync_channel(1);
                    let parked = std::sync::Mutex::new(parked);
                    let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let began = started.clone();
                    let gui_thread = std::thread::current().id();
                    cx.update(|app| {
                        RunningProcessOwner::test_capture_shutdown_placements(&owner, app,
                            move |owner, result, app| {
                                assert_eq!(std::thread::current().id(), gui_thread);
                                assert!(owner.try_borrow_mut().is_ok());
                                assert_eq!(result.is_ok(), case == CaptureCase::Success);
                                assert_eq!(owner.borrow().test_services().graph().unwrap().shutdown, original_attempt);
                                assert!(RunningProcessOwner::capture_shutdown_placements(owner, app,
                                    |_, _, _| panic!("retained capture cannot repeat")).is_err());
                                if case == CaptureCase::Success {
                                    assert_eq!(owner.borrow().shutdown_placements().unwrap(), vec![(invoking, expected)]);
                                } else {
                                    let error = result.unwrap_err();
                                    assert!(error.contains(&invoking.to_string()));
                                    assert_eq!(owner.borrow().shutdown_placements().unwrap_err(), error);
                                    if case == CaptureCase::Unwind { assert!(error.contains("worker unwound")); }
                                }
                                sender.send(owner.clone()).ok().unwrap();
                            },
                            move |index| {
                                assert_eq!(index, 0);
                                assert_ne!(std::thread::current().id(), gui_thread);
                                began.store(true, Ordering::SeqCst);
                                parked.lock().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
                                assert!(case != CaptureCase::Unwind, "injected desktop worker unwind");
                            }).unwrap();
                        assert!(RunningProcessOwner::capture_shutdown_placements(&owner, app,
                            |_, _, _| panic!("duplicate capture")).is_err());
                        assert!(owner.borrow().shutdown_placements().is_err());
                        assert!(RunningProcessOwner::release_shutdown_drafts(&owner, app).is_err());
                        assert!(RunningProcessOwner::advance_shutdown(&owner, cancelled.clone(), app,
                            |_, _| panic!("native capture still owns custody")).is_err());
                    }).unwrap();
                    if case != CaptureCase::Hidden {
                        let deadline = Instant::now() + Duration::from_secs(5);
                        while !started.load(Ordering::SeqCst) {
                            assert!(Instant::now() < deadline);
                            cx.background_executor().timer(Duration::from_millis(10)).await;
                        }
                        cx.update(|app| {
                            assert!(window.update(app, |_, window, _| window.lease_published_windows_window()).unwrap().is_err());
                            assert!(RunningProcessOwner::release_shutdown_drafts(&owner, app).is_err());
                            assert!(RunningProcessOwner::drive_shutdown_drafts(&owner, crate::running_owner::RunningShutdownDraftAction::Release,
                                app, |_, _, _| panic!("pending native lease")).is_err());
                            assert!(!owner.borrow().test_services_on_worker());
                        }).unwrap();
                    }
                    drop(owner);
                    assert!(weak.upgrade().is_some());
                    if case != CaptureCase::Hidden { release.send(()).unwrap(); }
                    let owner = receiver.await.unwrap();
                    assert_eq!(started.load(Ordering::SeqCst), case != CaptureCase::Hidden);
                    if case == CaptureCase::Hidden {
                        unsafe { windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                            windows::Win32::Foundation::HWND(raw as *mut _),
                            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE) };
                    }
                    let (lease, released) = cx.update(|app| window.update(app, |_, window, _|
                        window.lease_published_windows_window().unwrap()).unwrap()).unwrap();
                    drop(lease);
                    assert!(!released.await.unwrap().native_destroyed);
                    owner
                };
                if let Some(unwind) = session_unwind {
                    let owner = exercise_session(owner, unwind, cx).await;
                    let process = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                    support::dispose_running(process, cx).await;
                    observed.set(true);
                    cx.update(|app| app.quit()).unwrap();
                    return;
                }
                if let Some(unwind) = reconciliation_unwind {
                    let owner = reconciliation::exercise(owner, faults, unwind, cx).await;
                    let process = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                    support::dispose_running(process, cx).await;
                    observed.set(true);
                    cx.update(|app| app.quit()).unwrap();
                    return;
                }
                cx.update(|app| {
                    assert_eq!(RunningProcessOwner::release_shutdown_drafts(&owner, app).unwrap(),
                        RunningShutdownDraftProgress::Released);
                    assert!(owner.borrow().shutdown_placements().is_err());
                    assert!(RunningProcessOwner::capture_shutdown_placements(&owner, app,
                        |_, _, _| panic!("released drafts invalidate capture")).is_err());
                }).unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    cx.update(|app| RunningProcessOwner::advance_shutdown(&owner, cancelled.clone(), app, |_, _| {})).unwrap().unwrap();
                    settled(&owner, cx).await;
                    match owner.borrow_mut().take_shutdown_progress().unwrap().unwrap() {
                        AppServiceShutdownProgress::Failed { reopened: true, .. } => break,
                        AppServiceShutdownProgress::Failed { reopened: false, .. } => assert!(Instant::now() < deadline),
                        other => panic!("unexpected recovery: {other:?}"),
                    }
                }
                assert!(owner.borrow().shutdown_status().is_none());
                let process = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                support::dispose_running(process, cx).await;
                observed.set(true);
                cx.update(|app| app.quit()).unwrap();
            }).detach();
        }, app);
        support::watchdog(app);
    });
    assert!(finished.get());
    assert_reopens(&directory);
}
