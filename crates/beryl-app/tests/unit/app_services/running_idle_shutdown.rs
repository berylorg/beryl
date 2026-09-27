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
                    let permit = running.services.process.execution_permit();
                    let restore = running
                        .services
                        .graph()
                        .unwrap()
                        .restored_window_attempt()
                        .unwrap();
                    let owner = RunningProcessOwner::start(running, app);
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
