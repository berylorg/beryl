use crate::running_owner::{ExitRoutingCompletion, ExitRoutingError};
use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::{
            Controls::TDM_CLICK_BUTTON,
            WindowsAndMessaging::{FindWindowW, GW_ENABLEDPOPUP, GetWindow, IDOK, PostMessageW},
        },
    },
    core::PCWSTR,
};

#[test]
fn native_initial_exit_routes_idle_after_cancelled_observation() {
    exercise(None, ObservationOutcome::Admitted, false, false);
}

#[test]
fn native_initial_exit_retains_request_until_confirmation_cancel() {
    exercise(Some(false), ObservationOutcome::Admitted, false, false);
}

#[test]
fn native_initial_exit_retains_request_until_confirmed_admission() {
    exercise(Some(true), ObservationOutcome::Admitted, false, false);
}

#[test]
fn native_initial_exit_retains_intent_after_confirmed_observation_cancel() {
    exercise(Some(true), ObservationOutcome::Cancelled, false, false);
}

#[test]
fn native_initial_exit_retains_intent_after_confirmed_admission_refusal() {
    exercise(Some(true), ObservationOutcome::Refused, false, false);
}

#[test]
fn native_exit_attempt_drives_idle_admission_to_ready() {
    exercise(None, ObservationOutcome::Admitted, true, false);
}

#[test]
fn native_exit_attempt_drives_confirmed_admission_to_ready() {
    exercise(Some(true), ObservationOutcome::Admitted, true, false);
}

#[test]
fn native_exit_attempt_does_not_drive_cancelled_confirmation() {
    exercise(Some(false), ObservationOutcome::Admitted, true, false);
}

#[test]
fn native_exit_attempt_does_not_drive_cancelled_confirmed_observation() {
    exercise(Some(true), ObservationOutcome::Cancelled, true, false);
}

#[test]
fn native_exit_attempt_does_not_drive_refused_confirmed_admission() {
    exercise(Some(true), ObservationOutcome::Refused, true, false);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ObservationOutcome {
    Admitted,
    Cancelled,
    Refused,
}

#[test]
fn native_exit_policy_retains_idle_readiness() {
    exercise(None, ObservationOutcome::Admitted, true, true);
}

#[test]
fn native_exit_policy_retains_confirmed_readiness() {
    exercise(Some(true), ObservationOutcome::Admitted, true, true);
}

#[test]
fn native_exit_policy_completes_cancelled_confirmation() {
    exercise(Some(false), ObservationOutcome::Admitted, true, true);
}

#[test]
fn native_exit_policy_completes_cancelled_confirmed_observation() {
    exercise(Some(true), ObservationOutcome::Cancelled, true, true);
}

#[test]
fn native_exit_policy_completes_refused_confirmed_admission() {
    exercise(Some(true), ObservationOutcome::Refused, true, true);
}

fn exercise(confirm: Option<bool>, outcome: ObservationOutcome, drive: bool, settle: bool) {
    let cancel_observation = outcome == ObservationOutcome::Cancelled;
    let admitted = outcome == ObservationOutcome::Admitted;
    let command_completed = settle && confirm.is_some() && (confirm == Some(false) || !admitted);
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
                    let main = running.windows.shells()[0].window();
                    let permit = running.services.process.execution_permit();
                    let owner = RunningProcessOwner::test_start_unmounted(running, app);
                    let command = owner.borrow().window_exit_command(invoking, app).unwrap();
                    command.request_exit();
                    app.spawn(async move |cx| {
                        let request = next_request(&owner, cx).await;
                        let identity = request.identity();
                        let cancelled = Rc::new(RefCell::new(None));
                        let delivered = cancelled.clone();
                        let token = ProjectionCancellationToken::new();
                        token.cancel();
                        assert!(
                            cx.update(|app| route(
                                drive,
                                false,
                                &owner,
                                request,
                                token,
                                app,
                                move |owner, request, result, app| {
                                    assert!(matches!(
                                        result,
                                        Err(ExitRoutingError::Work(ExitWorkError::Observation(_)))
                                    ));
                                    assert!(owner.borrow().shutdown_status().is_none());
                                    let token = ProjectionCancellationToken::new();
                                    token.cancel();
                                    assert!(
                                        route(
                                            drive,
                                            false,
                                            owner,
                                            request,
                                            token,
                                            app,
                                            move |owner, request, result, _| {
                                                assert!(matches!(
                                                    result,
                                                    Err(ExitRoutingError::Work(
                                                        ExitWorkError::Observation(_)
                                                    ))
                                                ));
                                                assert!(owner.borrow().shutdown_status().is_none());
                                                *delivered.borrow_mut() = Some(request);
                                            },
                                        )
                                        .is_ok()
                                    );
                                },
                            ))
                            .unwrap()
                            .is_ok()
                        );
                        wait(&cancelled, cx).await;
                        let request = cancelled.borrow_mut().take().unwrap();
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        permit.commit(|| ()).unwrap();
                        let mut work = confirm.map(|_| {
                            crate::cas_projection::test_faults::retain_projection_work(
                                owner.borrow().test_services().graph().unwrap().cas(),
                                beryl_model::SyndicThreadId::from_bytes([247; 16]),
                            )
                        });
                        let title = format!("initial-exit-routing-{}", std::process::id());
                        main.update(cx, |_, window, _| window.set_window_title(&title))
                            .unwrap();
                        let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
                        let native = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) }.unwrap();
                        let slot = Rc::new(RefCell::new(None));
                        let delivered = slot.clone();
                        let gui_thread = std::thread::current().id();
                        let callback_identity = identity.clone();
                        let observation_cancellation = ProjectionCancellationToken::new();
                        assert!(
                            cx.update(|app| route(
                                drive,
                                settle,
                                &owner,
                                request,
                                observation_cancellation.clone(),
                                app,
                                move |owner, mut request, result, app| {
                                    assert_eq!(std::thread::current().id(), gui_thread);
                                    assert!(Rc::ptr_eq(&callback_identity, &request.identity()));
                                    if !command_completed {
                                    assert_eq!(
                                        owner
                                            .borrow()
                                            .resolve_exit_window(&mut request, app)
                                            .unwrap(),
                                        invoking
                                    );
                                    }
                                    assert_eq!(owner.borrow().exit_requested(), !command_completed);
                                    if let Some(confirm) = confirm {
                                        if confirm && outcome == ObservationOutcome::Refused {
                                            assert!(matches!(result, Err(ExitRoutingError::ConfirmedObservation(_))));
                                        } else {
                                        assert_eq!(
                                            result.unwrap(),
                                            if confirm && cancel_observation {
                                                ExitRoutingCompletion::ConfirmedObservationCancelled
                                            } else if confirm {
                                                ExitRoutingCompletion::Admitted
                                            } else {
                                                ExitRoutingCompletion::Cancelled
                                            }
                                        );
                                        }
                                        if !command_completed {
                                        assert_eq!(
                                            owner
                                                .borrow_mut()
                                                .consume_exit_confirmation(&mut request, app)
                                                .unwrap(),
                                            None
                                        );
                                        }
                                        if confirm && !command_completed {
                                            assert_eq!(
                                                owner.borrow().shutdown_status(),
                                                Some((
                                                    invoking,
                                                    ShutdownIntent::ApplicationExit,
                                                    if admitted && drive {
                                                        RunningShutdownStatus::WorkReady
                                                    } else if admitted {
                                                        RunningShutdownStatus::Admitted
                                                    } else {
                                                        RunningShutdownStatus::AwaitingObservation
                                                    },
                                                ))
                                            );
                                            if !admitted {
                                                owner
                                                    .borrow_mut()
                                                    .end_unadmitted_shutdown()
                                                    .unwrap();
                                            }
                                        } else {
                                            assert!(owner.borrow().shutdown_status().is_none());
                                        }
                                    } else {
                                        assert_eq!(
                                            result.unwrap(),
                                            ExitRoutingCompletion::Admitted
                                        );
                                    }
                                    assert!(delivered.borrow_mut().replace(request).is_none());
                                },
                            ))
                            .unwrap()
                            .is_ok()
                        );
                        command.request_exit();
                        if let Some(confirm) = confirm {
                            let deadline = Instant::now() + Duration::from_secs(5);
                            let dialog = loop {
                                if let Ok(dialog) = unsafe { GetWindow(native, GW_ENABLEDPOPUP) } {
                                    if dialog != native {
                                        break dialog;
                                    }
                                }
                                assert!(
                                    Instant::now() < deadline,
                                    "Exit confirmation did not appear"
                                );
                                cx.background_executor()
                                    .timer(Duration::from_millis(10))
                                    .await;
                            };
                            assert!(slot.borrow().is_none());
                            permit.commit(|| ()).unwrap();
                            command.request_exit();
                            if cancel_observation {
                                observation_cancellation.cancel();
                            }
                            if confirm {
                                if admitted {
                                    drop(work.take());
                                }
                                unsafe {
                                    PostMessageW(
                                        Some(dialog),
                                        TDM_CLICK_BUTTON.0 as u32,
                                        WPARAM(IDOK.0 as usize),
                                        LPARAM(0),
                                    )
                                }
                                .unwrap();
                            } else {
                                RunningProcessOwner::cancel_shutdown_confirmation(&owner).unwrap();
                            }
                        }
                        wait(&slot, cx).await;
                        let request = slot.borrow_mut().take().unwrap();
                        drop(work);
                        if owner.borrow().shutdown_session().is_some() {
                            assert!(settle);
                            assert!(owner.borrow().require_shutdown_session_ready().is_ok());
                            assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                            assert!(cx.update(|app| RunningProcessOwner::release_shutdown_drafts(&owner, app)).unwrap().is_err());
                            let running = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                            support::dispose_running(running, cx).await;
                            observed.set(true);
                            cx.update(|app| app.quit()).unwrap();
                            return;
                        }
                        let owner = if confirm.is_none()
                            || (confirm == Some(true) && admitted)
                        {
                            assert!(permit.commit(|| ()).is_err());
                            if settle {
                                let (sender, receiver) = futures_channel::oneshot::channel();
                                cx.update(|app| RunningProcessOwner::drive_shutdown_drafts(
                                    &owner, crate::running_owner::RunningShutdownDraftAction::Release, app,
                                    move |_, result, _| {
                                        assert_eq!(result.unwrap(), crate::running_owner::RunningShutdownDraftProgress::Released);
                                        sender.send(()).ok().unwrap();
                                    })).unwrap().unwrap();
                                receiver.await.unwrap();
                            }
                            super::super::running_shutdown_progress::exercise(
                                owner,
                                invoking,
                                ShutdownIntent::ApplicationExit,
                                false,
                                cx,
                            )
                            .await
                        } else {
                            permit.commit(|| ()).unwrap();
                            owner
                        };
                        assert!(owner.borrow().shutdown_status().is_none());
                        assert_eq!(RunningProcessOwner::finish_exit(&owner, &request), !command_completed);
                        assert!(!owner.borrow().exit_requested());
                        if drive {
                            let (request, _) = cx.update(|app| {
                                RunningProcessOwner::observe_and_drive_exit(
                                    &owner,
                                    request,
                                    ProjectionCancellationToken::new(),
                                    app,
                                    |_, _, _, _| panic!("refused attempt called completion"),
                                )
                            }).unwrap().unwrap_err();
                            assert!(Rc::ptr_eq(&identity, &request.identity()));
                        }
                        if settle {
                            command.request_exit();
                            let request = next_request(&owner, cx).await;
                            assert!(!Rc::ptr_eq(&identity, &request.identity()));
                            let slot = Rc::new(RefCell::new(None));
                            let delivered = slot.clone();
                            let cancellation = ProjectionCancellationToken::new();
                            cancellation.cancel();
                            assert!(cx.update(|app| RunningProcessOwner::run_exit_attempt(
                                &owner, request, cancellation, app,
                                move |owner, request, outcome, _| {
                                    assert!(outcome.command_completed);
                                    assert!(matches!(outcome.result, Err(crate::running_owner::ExitAttemptError::Routing(ExitRoutingError::Work(ExitWorkError::Observation(_))))));
                                    assert!(!owner.borrow().exit_requested());
                                    *delivered.borrow_mut() = Some(request);
                                },
                            )).unwrap().is_ok());
                            wait(&slot, cx).await;
                            let request = slot.borrow_mut().take().unwrap();
                            let identity = request.identity();
                            let (request, _) = cx.update(|app| RunningProcessOwner::run_exit_attempt(
                                &owner, request, ProjectionCancellationToken::new(), app,
                                |_, _, _, _| panic!("stale policy attempt called completion"),
                            )).unwrap().unwrap_err();
                            assert!(Rc::ptr_eq(&identity, &request.identity()));
                        }
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

fn route(
    drive: bool,
    settle: bool,
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: startup_owner::RunningExitRequest,
    cancellation: ProjectionCancellationToken,
    app: &mut gpui::App,
    completed: impl FnOnce(
        &Rc<RefCell<RunningProcessOwner>>,
        startup_owner::RunningExitRequest,
        Result<ExitRoutingCompletion, ExitRoutingError>,
        &mut gpui::App,
    ) + 'static,
) -> Result<
    (),
    (
        startup_owner::RunningExitRequest,
        crate::running_owner::ExitObservationError,
    ),
> {
    use crate::running_owner::{ExitAttemptCompletion, ExitAttemptError};
    if !drive {
        return RunningProcessOwner::observe_and_route_exit(
            owner,
            request,
            cancellation,
            app,
            completed,
        );
    }
    let completed =
        move |owner: &Rc<RefCell<RunningProcessOwner>>, request, result, app: &mut gpui::App| {
            assert!(!owner.borrow().test_services_on_worker());
            assert!(owner.borrow_mut().take_shutdown_progress().is_none());
            let result = match result {
                Ok(ExitAttemptCompletion::WindowClosed) => {
                    panic!("Exit request cannot complete an ordinary close")
                }
                Ok(ExitAttemptCompletion::Progress(progress)) => {
                    assert!(matches!(
                        progress,
                        crate::app_services::AppServiceShutdownProgress::Ready
                    ));
                    assert!(!RunningProcessOwner::finish_exit(owner, &request));
                    Ok(ExitRoutingCompletion::Admitted)
                }
                Ok(ExitAttemptCompletion::Cancelled) => Ok(ExitRoutingCompletion::Cancelled),
                Ok(ExitAttemptCompletion::SessionReady) => {
                    assert!(settle);
                    assert!(!RunningProcessOwner::finish_exit(owner, &request));
                    Ok(ExitRoutingCompletion::Admitted)
                }
                Ok(ExitAttemptCompletion::ConfirmedObservationCancelled) => {
                    Ok(ExitRoutingCompletion::ConfirmedObservationCancelled)
                }
                Err(ExitAttemptError::Routing(error)) => Err(error),
                Err(error) => panic!("unexpected attempt progress failure: {error}"),
            };
            completed(owner, request, result, app);
        };
    if settle {
        RunningProcessOwner::run_exit_attempt(
            owner,
            request,
            cancellation,
            app,
            move |owner, request, outcome, app| {
                let expected = !matches!(
                    &outcome.result,
                    Ok(ExitAttemptCompletion::Progress(_) | ExitAttemptCompletion::SessionReady)
                );
                assert_eq!(outcome.command_completed, expected);
                completed(owner, request, outcome.result, app);
            },
        )
    } else {
        RunningProcessOwner::observe_and_drive_exit(owner, request, cancellation, app, completed)
    }
}
