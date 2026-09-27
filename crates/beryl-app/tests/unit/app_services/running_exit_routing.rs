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
    exercise(None);
}

#[test]
fn native_initial_exit_retains_request_until_confirmation_cancel() {
    exercise(Some(false));
}

#[test]
fn native_initial_exit_retains_request_until_positive_confirmation() {
    exercise(Some(true));
}

fn exercise(confirm: Option<bool>) {
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
                    let owner = RunningProcessOwner::start(running, app);
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
                            cx.update(|app| RunningProcessOwner::observe_and_route_exit(
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
                                        RunningProcessOwner::observe_and_route_exit(
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
                        let work = confirm.map(|_| {
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
                        assert!(
                            cx.update(|app| RunningProcessOwner::observe_and_route_exit(
                                &owner,
                                request,
                                ProjectionCancellationToken::new(),
                                app,
                                move |owner, mut request, result, app| {
                                    assert_eq!(std::thread::current().id(), gui_thread);
                                    assert!(Rc::ptr_eq(&callback_identity, &request.identity()));
                                    assert_eq!(
                                        owner
                                            .borrow()
                                            .resolve_exit_window(&mut request, app)
                                            .unwrap(),
                                        invoking
                                    );
                                    assert!(owner.borrow().exit_requested());
                                    if let Some(confirm) = confirm {
                                        assert_eq!(
                                            result.unwrap(),
                                            if confirm {
                                                ExitRoutingCompletion::AwaitingObservation
                                            } else {
                                                ExitRoutingCompletion::Cancelled
                                            }
                                        );
                                        assert_eq!(
                                            owner
                                                .borrow_mut()
                                                .consume_exit_confirmation(&mut request, app)
                                                .unwrap(),
                                            None
                                        );
                                        if confirm {
                                            assert_eq!(
                                                owner.borrow().shutdown_status(),
                                                Some((
                                                    invoking,
                                                    ShutdownIntent::ApplicationExit,
                                                    RunningShutdownStatus::AwaitingObservation,
                                                ))
                                            );
                                            owner.borrow_mut().end_unadmitted_shutdown().unwrap();
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
                            if confirm {
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
                        let owner = if confirm.is_none() {
                            assert!(permit.commit(|| ()).is_err());
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
                        assert!(RunningProcessOwner::finish_exit(&owner, &request));
                        assert!(!owner.borrow().exit_requested());
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
