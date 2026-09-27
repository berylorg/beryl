use crate::running_owner::{RunningProcessOwner, ShutdownConfirmationResult, ShutdownIntent};
use gpui::WindowsNativeConfirmationTestFault as Fault;
use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::{
            Controls::TDM_CLICK_BUTTON,
            WindowsAndMessaging::{
                FindWindowW, GW_ENABLEDPOPUP, GetWindow, GetWindowTextW, IDOK, PostMessageW,
            },
        },
    },
    core::PCWSTR,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Confirm,
    AdmissionRefresh,
    AdmissionExit,
    AdmissionCancel,
    AdmissionAba,
    Cancel,
    MembershipAba,
    OpenFailure,
    SettlementFailure,
}

mod admission {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_confirmation_admission.rs"
    ));
}

#[test]
fn native_confirmed_intent_refreshes_work_and_admits_once() {
    run(Choice::AdmissionRefresh);
}
#[test]
fn native_confirmed_application_exit_preserves_its_shutdown_mode() {
    run(Choice::AdmissionExit);
}
#[test]
fn native_confirmed_intent_can_end_after_cancelled_worker_settles() {
    run(Choice::AdmissionCancel);
}
#[test]
fn native_confirmed_intent_rejects_membership_aba_before_lease_admission() {
    run(Choice::AdmissionAba);
}

#[test]
fn native_running_confirmation_preserves_intent_without_admission() {
    run(Choice::Confirm);
}
#[test]
fn native_running_confirmation_cancellation_preserves_execution() {
    run(Choice::Cancel);
}
#[test]
fn native_running_confirmation_rejects_membership_aba() {
    run(Choice::MembershipAba);
}
#[test]
fn native_running_confirmation_clean_failure_allows_fresh_attempt() {
    run(Choice::OpenFailure);
}
#[test]
fn native_running_confirmation_retains_uncertain_destruction() {
    run(Choice::SettlementFailure);
}

fn run(choice: Choice) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    let retained_failure = Rc::new(RefCell::new(None));
    let failed_owner = retained_failure.clone();
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
                    let intent = if choice == Choice::AdmissionExit {
                        ShutdownIntent::ApplicationExit
                    } else {
                        ShutdownIntent::FinalWindowClose
                    };
                    let main = running.windows.shells()[0].window();
                    let title = format!("running-confirmation-{}", std::process::id());
                    main.update(app, |_, window, _| window.set_window_title(&title))
                        .unwrap();
                    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
                    let native = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) }.unwrap();
                    let permit = running.services.process.execution_permit();
                    let restore = running
                        .services
                        .graph()
                        .unwrap()
                        .restored_window_attempt()
                        .unwrap();
                    let job = running.services.prepare_shutdown_observation().unwrap();
                    let owner = RunningProcessOwner::start(running, app);
                    app.spawn(async move |cx| {
                        let observation = cx
                            .background_executor()
                            .spawn(async move {
                                job.collect(&ProjectionCancellationToken::new()).unwrap()
                            })
                            .await;
                        let duplicate = observation.clone();
                        let fault = match choice {
                            Choice::OpenFailure => Some(Fault::NativeOpen),
                            Choice::SettlementFailure => Some(Fault::MissingDestruction),
                            _ => None,
                        };
                        let focus = main
                            .update(cx, |_, window, cx| {
                                let focus = cx.focus_handle();
                                window.focus(&focus);
                                focus
                            })
                            .unwrap();
                        cx.update(|app| {
                            RunningProcessOwner::test_begin_shutdown_confirmation(
                                &owner,
                                invoking,
                                intent,
                                observation,
                                app,
                                fault,
                            )
                        })
                        .unwrap()
                        .unwrap();
                        assert!(matches!(
                            cx.update(|app| owner
                                .borrow_mut()
                                .try_begin_idle_shutdown(invoking, intent, &duplicate, app,))
                                .unwrap(),
                            Err(crate::running_owner::IdleShutdownError::IntentBusy)
                        ));
                        if choice == Choice::OpenFailure {
                            main.update(cx, |_, window, cx| {
                                window.focus(&cx.focus_handle());
                                assert!(!focus.is_focused(window));
                            })
                            .unwrap();
                        }
                        assert!(
                            owner
                                .borrow_mut()
                                .take_shutdown_confirmation()
                                .unwrap()
                                .is_none()
                        );
                        let weak = Rc::downgrade(&owner);
                        drop(owner);
                        let owner = weak
                            .upgrade()
                            .expect("pending native task retains complete owner");
                        let deadline = Instant::now() + Duration::from_secs(5);
                        if choice != Choice::OpenFailure {
                            let dialog = loop {
                                if let Ok(dialog) = unsafe { GetWindow(native, GW_ENABLEDPOPUP) } {
                                    if dialog != native {
                                        break dialog;
                                    }
                                }
                                assert!(Instant::now() < deadline, "dialog did not appear");
                                cx.background_executor()
                                    .timer(Duration::from_millis(10))
                                    .await;
                            };
                            let mut title = [0u16; 128];
                            let len = unsafe { GetWindowTextW(dialog, &mut title) };
                            assert_eq!(
                                String::from_utf16_lossy(&title[..len as usize]),
                                "Exit Beryl?"
                            );
                            let reveal = cx
                                .update(|app| {
                                    RunningProcessOwner::begin_shutdown_confirmation(
                                        &owner,
                                        invoking,
                                        ShutdownIntent::ApplicationExit,
                                        duplicate,
                                        app,
                                    )
                                })
                                .unwrap();
                            if let Err(error) = reveal {
                                assert_eq!(error, "native confirmation activation was refused");
                            }
                            assert_eq!(
                                unsafe { GetWindow(native, GW_ENABLEDPOPUP) }.unwrap(),
                                dialog
                            );
                            permit.commit(|| ()).unwrap();
                            restore.validate_lifetime().unwrap();
                            main.update(cx, |root, _, _| {
                                assert!(!root.startup_interaction_gated())
                            })
                            .unwrap();
                            if choice == Choice::MembershipAba {
                                let reservation = owner
                                    .borrow()
                                    .test_process()
                                    .services
                                    .windows
                                    .reserve_main_window(beryl_model::WindowId::from_bytes(
                                        [231; 16],
                                    ))
                                    .unwrap();
                                drop(reservation);
                            }
                            if choice == Choice::Cancel {
                                main.update(cx, |_, window, cx| {
                                    window.focus(&cx.focus_handle());
                                    assert!(!focus.is_focused(window));
                                })
                                .unwrap();
                                RunningProcessOwner::cancel_shutdown_confirmation(&owner).unwrap();
                            } else {
                                unsafe {
                                    PostMessageW(
                                        Some(dialog),
                                        TDM_CLICK_BUTTON.0 as u32,
                                        WPARAM(IDOK.0 as usize),
                                        LPARAM(0),
                                    )
                                }
                                .unwrap();
                            }
                        }
                        let result = loop {
                            let result = owner.borrow_mut().take_shutdown_confirmation();
                            if !matches!(result, Ok(None)) {
                                break result;
                            }
                            assert!(Instant::now() < deadline, "confirmation did not settle");
                            cx.background_executor()
                                .timer(Duration::from_millis(10))
                                .await;
                        };
                        match choice {
                            Choice::Confirm
                            | Choice::AdmissionRefresh
                            | Choice::AdmissionExit
                            | Choice::AdmissionCancel
                            | Choice::AdmissionAba => {
                                let Ok(Some(ShutdownConfirmationResult::Confirmed(context))) =
                                    result
                                else {
                                    panic!("expected confirmation evidence")
                                };
                                assert_eq!(context.invoking(), invoking);
                                assert_eq!(context.intent(), intent);
                                assert_eq!(context.observation().running_threads(), 0);
                                owner
                                    .borrow()
                                    .test_process()
                                    .services
                                    .inspect_close_confirmation(context.snapshot(), invoking)
                                    .unwrap();
                                if choice != Choice::Confirm {
                                    admission::exercise(&owner, context, choice, cx).await;
                                }
                            }
                            Choice::Cancel => assert!(matches!(
                                result,
                                Ok(Some(ShutdownConfirmationResult::Cancelled))
                            )),
                            Choice::MembershipAba => assert!(matches!(
                                result,
                                Ok(Some(ShutdownConfirmationResult::WindowSetChanged))
                            )),
                            Choice::OpenFailure => {
                                assert!(result.is_err());
                                assert!(
                                    owner
                                        .borrow_mut()
                                        .take_shutdown_confirmation()
                                        .unwrap()
                                        .is_none()
                                );
                                main.update(cx, |_, window, _| assert!(focus.is_focused(window)))
                                    .unwrap();
                                permit.commit(|| ()).unwrap();
                                restore.validate_lifetime().unwrap();
                                let job = owner
                                    .borrow()
                                    .test_process()
                                    .services
                                    .prepare_shutdown_observation()
                                    .unwrap();
                                let observation = cx
                                    .background_executor()
                                    .spawn(async move {
                                        job.collect(&ProjectionCancellationToken::new()).unwrap()
                                    })
                                    .await;
                                cx.update(|app| {
                                    RunningProcessOwner::begin_shutdown_confirmation(
                                        &owner,
                                        invoking,
                                        ShutdownIntent::ApplicationExit,
                                        observation,
                                        app,
                                    )
                                })
                                .unwrap()
                                .unwrap();
                                let retry_deadline = Instant::now() + Duration::from_secs(5);
                                loop {
                                    if unsafe { GetWindow(native, GW_ENABLEDPOPUP) }
                                        .is_ok_and(|dialog| dialog != native)
                                    {
                                        break;
                                    }
                                    assert!(
                                        Instant::now() < retry_deadline,
                                        "fresh dialog did not appear"
                                    );
                                    cx.background_executor()
                                        .timer(Duration::from_millis(10))
                                        .await;
                                }
                                RunningProcessOwner::cancel_shutdown_confirmation(&owner).unwrap();
                                loop {
                                    let result =
                                        owner.borrow_mut().take_shutdown_confirmation().unwrap();
                                    if let Some(result) = result {
                                        assert!(matches!(
                                            result,
                                            ShutdownConfirmationResult::Cancelled
                                        ));
                                        break;
                                    }
                                    assert!(
                                        Instant::now() < retry_deadline,
                                        "fresh cancellation did not settle"
                                    );
                                    cx.background_executor()
                                        .timer(Duration::from_millis(10))
                                        .await;
                                }
                            }
                            Choice::SettlementFailure => {
                                assert!(result.is_err());
                                assert!(owner.borrow_mut().take_shutdown_confirmation().is_err());
                                assert!(
                                    RunningProcessOwner::reveal_shutdown_confirmation(&owner)
                                        .is_err()
                                );
                            }
                        }
                        if matches!(choice, Choice::AdmissionRefresh | Choice::AdmissionExit) {
                            assert_eq!(
                                permit.commit(|| ()),
                                Err(crate::process_admission::ProcessAdmissionError::Fenced)
                            );
                            assert!(restore.validate_lifetime().is_err());
                            assert!(
                                owner
                                    .borrow()
                                    .test_process()
                                    .services
                                    .graph()
                                    .unwrap()
                                    .shutdown
                                    .is_some()
                            );
                        } else {
                            permit.commit(|| ()).unwrap();
                            if choice == Choice::Cancel {
                                main.update(cx, |_, window, _| assert!(focus.is_focused(window)))
                                    .unwrap();
                            }
                            restore.validate_lifetime().unwrap();
                            assert!(
                                owner
                                    .borrow()
                                    .test_process()
                                    .services
                                    .graph()
                                    .unwrap()
                                    .shutdown
                                    .is_none()
                            );
                        }
                        main.update(cx, |root, _, _| assert!(!root.startup_interaction_gated()))
                            .unwrap();
                        if choice == Choice::SettlementFailure {
                            // The isolated test ends with native custody deliberately unresolved.
                            *failed_owner.borrow_mut() = Some(owner);
                        } else {
                            let mut running = Rc::try_unwrap(owner)
                                .ok()
                                .expect("settled task released owner")
                                .into_inner()
                                .test_into_process();
                            if matches!(choice, Choice::AdmissionRefresh | Choice::AdmissionExit) {
                                running.services = cx
                                    .background_executor()
                                    .spawn(async move {
                                        admission::reopen_for_disposal(running.services)
                                    })
                                    .await;
                            }
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
    if let Some(owner) = retained_failure.borrow_mut().take() {
        let running = Rc::try_unwrap(owner)
            .ok()
            .expect("settled task released owner")
            .into_inner()
            .test_into_process();
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
