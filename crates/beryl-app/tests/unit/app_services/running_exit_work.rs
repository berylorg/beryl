use crate::running_owner::{
    ExitWorkClassification, ExitWorkError, RunningProcessOwner, RunningShutdownStatus,
    ShutdownIntent,
};

#[test]
fn native_exit_idle_classification_admits_only_the_active_request() {
    run(false);
}

#[test]
fn native_exit_new_work_requires_confirmation_after_stale_idle_refusal() {
    run(true);
}

fn run(new_work: bool) {
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
                    let owner = RunningProcessOwner::start(running, app);
                    let command = owner.borrow().window_exit_command(invoking, app).unwrap();
                    command.request_exit();
                    app.spawn(async move |cx| {
                        let mut request = next_request(&owner, cx).await;
                        let idle = observe(&owner, ProjectionCancellationToken::new(), cx)
                            .await
                            .unwrap();
                        assert!(!idle.has_work());
                        assert!(RunningProcessOwner::finish_exit(&owner, &request));
                        command.request_exit();
                        let mut successor = next_request(&owner, cx).await;
                        assert!(matches!(
                            cx.update(|app| owner.borrow_mut().classify_exit_work(
                                &mut request,
                                Ok(idle.clone()),
                                app
                            ))
                            .unwrap(),
                            Err(ExitWorkError::Request(_))
                        ));
                        assert!(owner.borrow().exit_requested());
                        for failure in [
                            AppServiceCloseError::Unavailable,
                            cancelled_result(&owner, cx).await,
                        ] {
                            assert!(matches!(
                                cx.update(|app| owner.borrow_mut().classify_exit_work(
                                    &mut successor,
                                    Err(failure),
                                    app
                                ))
                                .unwrap(),
                                Err(ExitWorkError::Observation(_))
                            ));
                            assert!(owner.borrow().shutdown_status().is_none());
                            permit.commit(|| ()).unwrap();
                        }
                        let pending = Rc::new(RefCell::new(None));
                        let delivered = pending.clone();
                        cx.update(|app| {
                            RunningProcessOwner::observe_shutdown_work(
                                &owner,
                                ProjectionCancellationToken::new(),
                                app,
                                move |_, result, _| *delivered.borrow_mut() = Some(result),
                            )
                            .unwrap();
                            assert!(matches!(
                                owner.borrow_mut().classify_exit_work(
                                    &mut successor,
                                    Ok(idle.clone()),
                                    app
                                ),
                                Err(ExitWorkError::IntentBusy)
                            ));
                        })
                        .unwrap();
                        wait(&pending, cx).await;
                        pending.borrow_mut().take().unwrap().unwrap();
                        permit.commit(|| ()).unwrap();
                        if new_work {
                            let work = crate::cas_projection::test_faults::retain_projection_work(
                                owner.borrow().test_services().graph().unwrap().cas(),
                                beryl_model::SyndicThreadId::from_bytes([244; 16]),
                            );
                            assert!(
                                cx.update(|app| owner.borrow_mut().classify_exit_work(
                                    &mut successor,
                                    Ok(idle.clone()),
                                    app
                                ))
                                .unwrap()
                                .is_err()
                            );
                            assert!(owner.borrow().shutdown_status().is_none());
                            permit.commit(|| ()).unwrap();
                            let fresh = observe(&owner, ProjectionCancellationToken::new(), cx)
                                .await
                                .unwrap();
                            assert!(fresh.has_work());
                            let classified = cx
                                .update(|app| {
                                    owner.borrow_mut().classify_exit_work(
                                        &mut successor,
                                        Ok(fresh),
                                        app,
                                    )
                                })
                                .unwrap()
                                .unwrap();
                            let ExitWorkClassification::ConfirmationRequired {
                                invoking: original,
                                observation,
                            } = classified
                            else {
                                panic!("work must require confirmation")
                            };
                            assert_eq!(original, invoking);
                            assert_eq!(observation.running_threads(), 1);
                            assert!(observation.has_work());
                            assert!(owner.borrow().shutdown_status().is_none());
                            assert!(owner.borrow().exit_requested());
                            permit.commit(|| ()).unwrap();
                            drop(
                                owner
                                    .borrow()
                                    .test_services()
                                    .windows
                                    .reserve_main_window(beryl_model::WindowId::from_bytes(
                                        [245; 16],
                                    ))
                                    .unwrap(),
                            );
                            drop(work);
                        }
                        let deadline = Instant::now() + Duration::from_secs(5);
                        loop {
                            let fresh = observe(&owner, ProjectionCancellationToken::new(), cx)
                                .await
                                .unwrap();
                            let result = cx
                                .update(|app| {
                                    owner.borrow_mut().classify_exit_work(
                                        &mut successor,
                                        Ok(fresh),
                                        app,
                                    )
                                })
                                .unwrap();
                            match result {
                                Ok(ExitWorkClassification::Admitted) => break,
                                Ok(_) => panic!("idle work unexpectedly requires confirmation"),
                                Err(error) => {
                                    permit.commit(|| ()).unwrap();
                                    assert!(
                                        Instant::now() < deadline,
                                        "idle classification did not settle: {error}"
                                    );
                                    cx.background_executor()
                                        .timer(Duration::from_millis(10))
                                        .await;
                                }
                            }
                        }
                        assert_eq!(
                            owner.borrow().shutdown_status(),
                            Some((
                                invoking,
                                ShutdownIntent::ApplicationExit,
                                RunningShutdownStatus::Admitted
                            ))
                        );
                        assert!(owner.borrow().exit_requested());
                        assert!(permit.commit(|| ()).is_err());
                        assert!(matches!(
                            cx.update(|app| owner.borrow_mut().classify_exit_work(
                                &mut successor,
                                Ok(idle),
                                app
                            ))
                            .unwrap(),
                            Err(ExitWorkError::IntentBusy)
                        ));
                        let owner = super::running_shutdown_progress::exercise(
                            owner,
                            invoking,
                            ShutdownIntent::ApplicationExit,
                            false,
                            cx,
                        )
                        .await;
                        assert!(owner.borrow().shutdown_status().is_none());
                        assert!(RunningProcessOwner::finish_exit(&owner, &successor));
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

async fn wait<T>(slot: &Rc<RefCell<Option<T>>>, cx: &mut AsyncApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while slot.borrow().is_none() {
        assert!(Instant::now() < deadline, "GUI completion did not arrive");
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

async fn next_request(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    cx: &mut AsyncApp,
) -> startup_owner::RunningExitRequest {
    let slot = Rc::new(RefCell::new(None));
    let delivered = slot.clone();
    cx.update(|app| {
        RunningProcessOwner::wait_for_exit(owner, app, move |_, request, _| {
            *delivered.borrow_mut() = Some(request)
        })
    })
    .unwrap()
    .unwrap();
    wait(&slot, cx).await;
    slot.borrow_mut().take().unwrap()
}

async fn observe(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    cancellation: ProjectionCancellationToken,
    cx: &mut AsyncApp,
) -> Result<crate::cas_projection::ShutdownWorkObservation, AppServiceCloseError> {
    let slot = Rc::new(RefCell::new(None));
    let delivered = slot.clone();
    cx.update(|app| {
        RunningProcessOwner::observe_shutdown_work(owner, cancellation, app, move |_, result, _| {
            *delivered.borrow_mut() = Some(result)
        })
    })
    .unwrap()
    .unwrap();
    wait(&slot, cx).await;
    slot.borrow_mut().take().unwrap()
}

async fn cancelled_result(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    cx: &mut AsyncApp,
) -> AppServiceCloseError {
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    match observe(owner, cancellation, cx).await {
        Err(error) => error,
        Ok(_) => panic!("cancelled collection returned work evidence"),
    }
}
