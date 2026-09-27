use crate::running_owner::{
    ExitConfirmationError, ExitConfirmationRoute, ExitObservationError, ExitWorkClassification,
    ExitWorkError, ExitWorkRoute, RunningProcessOwner, RunningShutdownStatus,
    ShutdownConfirmationResult, ShutdownIntent,
};

mod exit_observation {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_exit_observation.rs"
    ));
}

mod confirmation_route {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_exit_confirmation.rs"
    ));
}

mod initial_routing {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_exit_routing.rs"
    ));
}

mod progress_route {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_exit_progress.rs"
    ));
}

mod progress_driver {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_exit_driver.rs"
    ));
}

mod command_consumer {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_exit_consumer.rs"
    ));
}

#[test]
fn native_exit_idle_classification_admits_only_the_active_request() {
    run(false, None, None);
}

#[test]
fn native_exit_new_work_requires_confirmation_after_stale_idle_refusal() {
    run(true, None, None);
}

#[test]
fn native_exit_work_routes_confirmation_and_preserves_positive_context() {
    run(true, Some(true), None);
}

#[test]
fn native_exit_work_routes_cancel_without_admission_or_request_completion() {
    run(true, Some(false), None);
}

#[test]
fn native_exit_confirmation_establishes_original_intent_without_fencing() {
    run(true, Some(true), Some(false));
}

#[test]
fn native_exit_confirmation_cancel_consumes_once_without_completing_request() {
    run(true, Some(false), Some(false));
}

#[test]
fn native_exit_confirmation_prevents_completion_until_consumed() {
    run(true, Some(true), Some(true));
}

fn run(new_work: bool, confirm: Option<bool>, replace_request: Option<bool>) {
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
                        let (mut request, idle) = exit_observation::delivered(&owner, cx).await;
                        assert!(!idle.has_work());
                        assert!(RunningProcessOwner::finish_exit(&owner, &request));
                        command.request_exit();
                        let mut successor = next_request(&owner, cx).await;
                        request = cx
                            .update(|app| {
                                match RunningProcessOwner::observe_and_route_exit(
                                    &owner,
                                    request,
                                    ProjectionCancellationToken::new(),
                                    app,
                                    |_, _, _, _| {
                                        panic!("stale request must not receive observation")
                                    },
                                ) {
                                    Err((request, ExitObservationError::Request(_))) => request,
                                    _ => panic!("stale request must be returned on refusal"),
                                }
                            })
                            .unwrap();
                        assert!(matches!(
                            cx.update(|app| RunningProcessOwner::route_exit_work(
                                &owner,
                                &mut request,
                                Ok(idle.clone()),
                                app,
                                |_, _| panic!("stale request must not receive completion"),
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
                                cx.update(|app| RunningProcessOwner::route_exit_work(
                                    &owner,
                                    &mut successor,
                                    Err(failure),
                                    app,
                                    |_, _| panic!("failed observation must not receive completion"),
                                ))
                                .unwrap(),
                                Err(ExitWorkError::Observation(_))
                            ));
                            assert!(owner.borrow().shutdown_status().is_none());
                            permit.commit(|| ()).unwrap();
                        }
                        let pending = Rc::new(RefCell::new(None));
                        let delivered = pending.clone();
                        successor = cx
                            .update(|app| {
                                RunningProcessOwner::observe_shutdown_work(
                                    &owner,
                                    ProjectionCancellationToken::new(),
                                    app,
                                    move |_, result, _| *delivered.borrow_mut() = Some(result),
                                )
                                .unwrap();
                                assert!(!RunningProcessOwner::finish_exit(&owner, &successor));
                                assert!(owner.borrow().exit_requested());
                                let mut successor =
                                    match RunningProcessOwner::observe_and_route_exit(
                                        &owner,
                                        successor,
                                        ProjectionCancellationToken::new(),
                                        app,
                                        |_, _, _, _| {
                                            panic!("busy scheduling must not receive completion")
                                        },
                                    ) {
                                        Err((request, ExitObservationError::Scheduling(_))) => {
                                            request
                                        }
                                        _ => panic!(
                                            "busy scheduling must return the original request"
                                        ),
                                    };
                                assert!(matches!(
                                    RunningProcessOwner::route_exit_work(
                                        &owner,
                                        &mut successor,
                                        Ok(idle.clone()),
                                        app,
                                        |_, _| panic!("busy owner must not receive completion"),
                                    ),
                                    Err(ExitWorkError::IntentBusy)
                                ));
                                successor
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
                            if let Some(confirm) = confirm {
                                confirmation_route::exercise(
                                    &owner,
                                    &mut successor,
                                    fresh,
                                    confirm,
                                    replace_request,
                                    cx,
                                )
                                .await;
                            } else {
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
                            }
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
                                    RunningProcessOwner::route_exit_work(
                                        &owner,
                                        &mut successor,
                                        Ok(fresh),
                                        app,
                                        |_, _| panic!("idle routing must not receive completion"),
                                    )
                                })
                                .unwrap();
                            match result {
                                Ok(ExitWorkRoute::Admitted) => break,
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
