use crate::running_owner::{
    IdleShutdownError, RunningProcessOwner, RunningShutdownStatus, ShutdownIntent,
};

#[test]
fn native_idle_final_close_retains_exact_admitted_intent() {
    run(ShutdownIntent::FinalWindowClose, false, None);
}

#[test]
fn native_idle_exit_retains_exact_admitted_intent() {
    run(ShutdownIntent::ApplicationExit, false, None);
}

#[test]
fn native_shutdown_gate_reports_a_missing_published_window_without_releasing_intent() {
    run_with_missing_window(ShutdownIntent::ApplicationExit, false, None, true, false);
}

#[test]
fn native_missing_prepared_draft_refuses_recovery_and_retains_the_attempt() {
    run_with_missing_window(ShutdownIntent::ApplicationExit, false, None, true, true);
}

#[test]
fn native_idle_admission_refuses_new_work_and_requires_confirmation() {
    run(ShutdownIntent::FinalWindowClose, true, None);
}

async fn observe(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    cx: &mut AsyncApp,
) -> crate::cas_projection::ShutdownWorkObservation {
    let job = owner
        .borrow()
        .test_services()
        .prepare_shutdown_observation()
        .unwrap();
    cx.background_executor()
        .spawn(async move { job.collect(&ProjectionCancellationToken::new()).unwrap() })
        .await
}

pub(super) fn run(intent: ShutdownIntent, new_work: bool, progress_ready_first: Option<bool>) {
    run_with_missing_window(intent, new_work, progress_ready_first, false, false);
}

fn run_with_missing_window(
    intent: ShutdownIntent,
    new_work: bool,
    progress_ready_first: Option<bool>,
    missing: bool,
    missing_draft: bool,
) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    let retained_failure = Rc::new(RefCell::new(None));
    let failed_process = retained_failure.clone();
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
                    let permit = running.services.process.execution_permit();
                    let restore = running
                        .services
                        .graph()
                        .unwrap()
                        .restored_window_attempt()
                        .unwrap();
                    let owner = RunningProcessOwner::test_start_unmounted(running, app);
                    let original_reason = window.read(app).unwrap().new_window_disabled_reason(app);
                    assert!(RunningProcessOwner::release_shutdown_interaction_gate(&owner, app).is_err());
                    assert!(RunningProcessOwner::install_shutdown_interaction_gate(&owner, app).is_err());
                    assert!(RunningProcessOwner::advance_shutdown_drafts(&owner, app).is_err());
                    assert!(RunningProcessOwner::release_shutdown_drafts(&owner, app).is_err());
                    assert_eq!(window.read(app).unwrap().new_window_disabled_reason(app), original_reason);
                    assert!(
                        RunningProcessOwner::advance_shutdown(
                            &owner,
                            ProjectionCancellationToken::new(),
                            app,
                            |_, _| panic!("unadmitted progress must not notify"),
                        )
                        .is_err()
                    );
                    app.spawn(async move |cx| {
                        let idle = observe(&owner, cx).await;
                        assert!(!idle.has_work());
                        let absent = beryl_model::WindowId::from_bytes([239; 16]);
                        assert!(
                            cx.update(|app| owner
                                .borrow_mut()
                                .try_begin_idle_shutdown(absent, intent, &idle, app))
                                .unwrap()
                                .is_err()
                        );
                        let reservation = permit.reserve().unwrap();
                        assert!(
                            cx.update(|app| owner
                                .borrow_mut()
                                .try_begin_idle_shutdown(invoking, intent, &idle, app))
                                .unwrap()
                                .is_err()
                        );
                        drop(reservation);
                        assert_refusal(&owner, &permit, &restore);
                        if new_work {
                            let work = crate::cas_projection::test_faults::retain_projection_work(
                                owner.borrow().test_services().graph().unwrap().cas(),
                                beryl_model::SyndicThreadId::from_bytes([237; 16]),
                            );
                            assert!(
                                cx.update(|app| owner
                                    .borrow_mut()
                                    .try_begin_idle_shutdown(invoking, intent, &idle, app))
                                    .unwrap()
                                    .is_err()
                            );
                            assert_refusal(&owner, &permit, &restore);
                            let busy = observe(&owner, cx).await;
                            assert!(busy.has_work());
                            assert!(matches!(
                                cx.update(|app| owner
                                    .borrow_mut()
                                    .try_begin_idle_shutdown(invoking, intent, &busy, app))
                                    .unwrap(),
                                Err(IdleShutdownError::ConfirmationRequired)
                            ));
                            assert_refusal(&owner, &permit, &restore);
                            drop(work);
                        }
                        let deadline = Instant::now() + Duration::from_secs(5);
                        let admitted = loop {
                            let fresh = observe(&owner, cx).await;
                            assert!(!fresh.has_work());
                            let result = cx
                                .update(|app| {
                                    owner
                                        .borrow_mut()
                                        .try_begin_idle_shutdown(invoking, intent, &fresh, app)
                                })
                                .unwrap();
                            if result.is_ok() {
                                break fresh;
                            }
                            assert_refusal(&owner, &permit, &restore);
                            assert!(
                                Instant::now() < deadline,
                                "idle admission did not settle: {result:?}"
                            );
                            cx.background_executor()
                                .timer(Duration::from_millis(10))
                                .await;
                        };
                        assert_eq!(
                            owner.borrow().shutdown_status(),
                            Some((invoking, intent, RunningShutdownStatus::Admitted))
                        );
                        assert_eq!(
                            permit.commit(|| ()),
                            Err(crate::process_admission::ProcessAdmissionError::Fenced)
                        );
                        assert!(restore.validate_lifetime().is_err());
                        let attempt = owner.borrow().test_services().graph().unwrap().shutdown;
                        assert!(matches!(
                            cx.update(|app| owner
                                .borrow_mut()
                                .try_begin_idle_shutdown(invoking, intent, &admitted, app))
                                .unwrap(),
                            Err(IdleShutdownError::IntentBusy)
                        ));
                        assert!(
                            cx.update(|app| RunningProcessOwner::begin_shutdown_confirmation(
                                &owner,
                                invoking,
                                intent,
                                admitted,
                                app,
                                |_, _| panic!("refused confirmation must not notify")
                            ))
                            .unwrap()
                            .is_err()
                        );
                        assert_eq!(
                            owner.borrow().test_services().graph().unwrap().shutdown,
                            attempt
                        );
                        assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
                        cx.update(|app| {
                            assert!(RunningProcessOwner::release_shutdown_interaction_gate(&owner, app).is_err());
                            for _ in 0..2 {
                                RunningProcessOwner::install_shutdown_interaction_gate(&owner, app)
                                    .unwrap();
                                assert_eq!(
                                    window.read(app).unwrap().new_window_disabled_reason(app).as_deref(),
                                    Some("Application Exit is waiting for active work and durable state.")
                                );
                            }
                        }).unwrap();
                        assert_eq!(owner.borrow().shutdown_status(),
                            Some((invoking, intent, RunningShutdownStatus::Admitted)));
                        assert_eq!(owner.borrow().test_services().graph().unwrap().shutdown, attempt);
                        if missing {
                            if missing_draft {
                                let deadline = Instant::now() + Duration::from_secs(5);
                                loop {
                                    cx.update(|app| RunningProcessOwner::advance_shutdown(&owner, ProjectionCancellationToken::new(), app, |_, _| {})).unwrap().unwrap();
                                    while !owner.borrow().test_shutdown_progress_settled() {
                                        assert!(Instant::now() < deadline);
                                        cx.background_executor().timer(Duration::from_millis(10)).await;
                                    }
                                    match owner.borrow_mut().take_shutdown_progress().unwrap().unwrap() {
                                        AppServiceShutdownProgress::Ready => break,
                                        AppServiceShutdownProgress::Waiting => assert!(Instant::now() < deadline),
                                        other => panic!("unexpected work readiness: {other:?}"),
                                    }
                                }
                                cx.update(|app| assert_eq!(RunningProcessOwner::advance_shutdown_drafts(&owner, app).unwrap(), crate::running_owner::RunningShutdownDraftProgress::Ready)).unwrap();
                            }
                            cx.update(|app| {
                                window.update(app, |_, window, _| window.remove_window()).unwrap();
                                assert!(RunningProcessOwner::install_shutdown_interaction_gate(&owner, app).is_err());
                            }).unwrap();
                            assert_eq!(owner.borrow().shutdown_status(),
                                Some((invoking, intent, if missing_draft { RunningShutdownStatus::WorkReady } else { RunningShutdownStatus::Admitted })));
                            assert_eq!(owner.borrow().test_services().graph().unwrap().shutdown, attempt);
                            let cancelled = ProjectionCancellationToken::new();
                            cancelled.cancel();
                            if missing_draft {
                                cx.update(|app| {
                                    assert!(RunningProcessOwner::advance_shutdown_drafts(&owner, app).is_err());
                                    assert!(RunningProcessOwner::release_shutdown_drafts(&owner, app).is_err());
                                    assert!(RunningProcessOwner::advance_shutdown(&owner, cancelled.clone(), app, |_, _| panic!("missing draft cannot authorize recovery")).is_err());
                                    assert!(!owner.borrow().test_services_on_worker());
                                    assert!(owner.borrow().shutdown_status().is_some());
                                    assert!(RunningProcessOwner::release_shutdown_interaction_gate(&owner, app).is_err());
                                }).unwrap();
                            } else {
                            let deadline = Instant::now() + Duration::from_secs(5);
                            loop {
                                cx.update(|app| RunningProcessOwner::advance_shutdown(
                                    &owner, cancelled.clone(), app, |_, _| {}
                                )).unwrap().unwrap();
                                while !owner.borrow().test_shutdown_progress_settled() {
                                    assert!(Instant::now() < deadline);
                                    cx.background_executor().timer(Duration::from_millis(10)).await;
                                }
                                cx.update(|app| {
                                    assert!(RunningProcessOwner::release_shutdown_interaction_gate(&owner, app).is_err());
                                    assert!(owner.borrow().test_shutdown_progress_settled());
                                    if owner.borrow().shutdown_status().is_none() {
                                        assert!(matches!(RunningProcessOwner::take_exit_progress(&owner, app), Err(crate::running_owner::ExitProgressError::Interaction(_))));
                                        assert!(owner.borrow().test_shutdown_progress_settled());
                                    }
                                }).unwrap();
                                let result = owner.borrow_mut().take_shutdown_progress().unwrap().unwrap();
                                if matches!(result, AppServiceShutdownProgress::Failed { reopened: true, .. }) {
                                    break;
                                }
                                assert!(matches!(result, AppServiceShutdownProgress::Failed { reopened: false, .. }));
                                assert!(Instant::now() < deadline);
                            }
                            }
                        }
                        assert!(
                            owner
                                .borrow_mut()
                                .prepare_confirmed_shutdown_observation()
                                .is_err()
                        );
                        let owner = if let Some(ready_first) = progress_ready_first {
                            super::running_shutdown_progress::exercise(
                                owner,
                                invoking,
                                intent,
                                ready_first,
                                cx,
                            )
                            .await
                        } else {
                            owner
                        };
                        assert!(restore.validate_lifetime().is_err());
                        assert!(permit.commit(|| ()).is_err());
                        let mut running = Rc::try_unwrap(owner)
                            .ok()
                            .unwrap()
                            .into_inner()
                            .test_into_process();
                        if running.services.graph().unwrap().shutdown.is_some() {
                            running.services = cx
                                .background_executor()
                                .spawn(async move {
                                    let mut services = running.services;
                                    let cancelled = ProjectionCancellationToken::new();
                                    cancelled.cancel();
                                    let deadline = Instant::now() + Duration::from_secs(5);
                                    loop {
                                        match services.poll_shutdown(&cancelled).unwrap() {
                                            AppServiceShutdownProgress::Failed {
                                                reopened: true,
                                                ..
                                            } => break services,
                                            AppServiceShutdownProgress::Failed {
                                                reopened: false,
                                                ..
                                            } => {
                                                assert!(Instant::now() < deadline);
                                                std::thread::yield_now();
                                            }
                                            other => {
                                                panic!(
                                                    "unexpected test cleanup progress: {other:?}"
                                                )
                                            }
                                        }
                                    }
                                })
                                .await;
                        }
                        if missing {
                            *failed_process.borrow_mut() = Some(running);
                        } else {
                            support::dispose_running(running, cx).await;
                        }
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
    if let Some(running) = retained_failure.borrow_mut().take() {
        let startup_owner::StartedProcess {
            mut services,
            windows,
            appearance,
            ..
        } = running;
        drop(windows);
        drop(appearance);
        std::thread::spawn(move || close(&mut services))
            .join()
            .unwrap();
    }
    assert_reopens(&directory);
}

fn assert_refusal(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    permit: &crate::process_admission::ProcessExecutionPermit,
    restore: &crate::main_window::RestoredWindowPreparationAttempt,
) {
    let owner = owner.borrow();
    assert!(owner.shutdown_status().is_none());
    assert!(owner.test_services().graph().unwrap().shutdown.is_none());
    permit.commit(|| ()).unwrap();
    restore.validate_lifetime().unwrap();
    drop(
        owner
            .test_services()
            .windows
            .reserve_main_window(beryl_model::WindowId::from_bytes([238; 16]))
            .unwrap(),
    );
}
