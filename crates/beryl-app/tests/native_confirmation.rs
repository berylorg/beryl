#![cfg(target_os = "windows")]

#[path = "support/desktop_placement_native.rs"]
mod native;

use gpui::{
    AppContext, Application, AsyncApp, WindowsNativeConfirmationCompleted,
    WindowsNativeConfirmationOutcome as Outcome, WindowsNativeConfirmationRequest as Request,
    WindowsNativeConfirmationTestFault as Fault, with_windows_window_destruction_observer_for_test,
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::{
        Controls::TDM_CLICK_BUTTON,
        Input::KeyboardAndMouse::GetFocus,
        WindowsAndMessaging::{
            GW_ENABLEDPOPUP, GetWindow, GetWindowTextW, IDCANCEL, IDOK, PostMessageW, WM_CLOSE,
            WM_KEYDOWN,
        },
    },
};

fn request() -> Request {
    Request::new(
        "Exit Beryl?",
        "Running threads, including those not open in a window, will stop.",
        "Cancel",
        "Exit Beryl",
    )
    .unwrap()
}

async fn dialog(owner: usize, cx: &AsyncApp) -> HWND {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(handle) = unsafe { GetWindow(native::hwnd(owner), GW_ENABLEDPOPUP) } {
            if handle != native::hwnd(owner) && native::visible(handle.0 as usize) {
                return handle;
            }
        }
        assert!(
            Instant::now() < deadline,
            "native confirmation did not appear"
        );
        native::pump(cx).await;
    }
}

fn choose(dialog: HWND, button: i32) {
    unsafe {
        PostMessageW(
            Some(dialog),
            TDM_CLICK_BUTTON.0 as u32,
            WPARAM(button as usize),
            LPARAM(0),
        )
    }
    .unwrap();
}

#[test]
fn native_confirmation_bounds_reject_invalid_text() {
    assert!(Request::new("", "message", "Cancel", "Confirm").is_err());
    assert!(Request::new("title\0suffix", "message", "Cancel", "Confirm").is_err());
    assert!(
        Request::new(
            &"t".repeat(256),
            &"m".repeat(4096),
            &"c".repeat(128),
            &"a".repeat(128)
        )
        .is_ok()
    );
    for (title, message, cancel, confirm) in [
        ("t".repeat(257), "m".into(), "c".into(), "a".into()),
        ("t".into(), "m".repeat(4097), "c".into(), "a".into()),
        ("t".into(), "m".into(), "c".repeat(129), "a".into()),
        ("t".into(), "m".into(), "c".into(), "a".repeat(129)),
    ] {
        assert!(Request::new(&title, &message, &cancel, &confirm).is_err());
    }
}

#[test]
fn native_confirmation_preserves_exact_dialog_and_cancel_authority() {
    let completed = Rc::new(Cell::new(false));
    let observed = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let (owner, raw) = native::open(app, "confirmation-owner");
            assert!(
                owner
                    .update(app, |_, window, _| window
                        .begin_windows_native_confirmation(request()))
                    .unwrap()
                    .is_err()
            );
            owner
                .update(app, |_, window, app| window.publish(app))
                .unwrap()
                .unwrap();
            app.spawn(async move |cx| {
                for action in 0..8 {
                    let (control, completion) = owner
                        .update(cx, |_, window, _| {
                            window.begin_windows_native_confirmation(request())
                        })
                        .unwrap()
                        .unwrap();
                    assert!(!control.cleanup_settled());
                    assert!(
                        owner
                            .update(cx, |_, window, _| window
                                .begin_windows_native_confirmation(request()))
                            .unwrap()
                            .is_err()
                    );
                    control.reveal().unwrap();
                    if action == 0 {
                        control.cancel().unwrap();
                        assert_eq!(completion.await.unwrap(), Outcome::Cancelled);
                        assert!(control.cleanup_settled());
                        continue;
                    }
                    let popup = dialog(raw, cx).await;
                    assert!(!control.cleanup_settled());
                    let mut title = [0u16; 256];
                    let len = unsafe { GetWindowTextW(popup, &mut title) };
                    assert_eq!(
                        String::from_utf16(&title[..len as usize]).unwrap(),
                        "Exit Beryl?"
                    );
                    let _activation = control.reveal();
                    assert_eq!(dialog(raw, cx).await, popup);
                    match action {
                        1 => choose(popup, IDOK.0),
                        2 => choose(popup, IDCANCEL.0),
                        3 => {
                            unsafe {
                                PostMessageW(
                                    Some(popup),
                                    WM_CLOSE,
                                    Default::default(),
                                    Default::default(),
                                )
                            }
                            .unwrap();
                        }
                        4 => {
                            control.cancel().unwrap();
                            choose(popup, IDOK.0);
                        }
                        5 => drop(control),
                        6 | 7 => {
                            let key = if action == 6 { 0x0D } else { 0x1B };
                            unsafe {
                                PostMessageW(Some(popup), WM_KEYDOWN, WPARAM(key), LPARAM(0))
                            }
                            .unwrap();
                        }
                        _ => unreachable!(),
                    }
                    assert_eq!(
                        completion.await.unwrap(),
                        if action == 1 {
                            Outcome::Confirmed
                        } else {
                            Outcome::Cancelled
                        }
                    );
                    assert!(!native::alive(popup.0 as usize));
                    assert!(native::alive(raw));
                    if action != 1 {
                        assert_eq!(unsafe { GetFocus() }, native::hwnd(raw));
                    }
                }
                let (control, completion) = owner
                    .update(cx, |_, window, _| {
                        window.begin_windows_native_confirmation(request())
                    })
                    .unwrap()
                    .unwrap();
                drop(completion);
                let popup = dialog(raw, cx).await;
                control.cancel().unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while native::alive(popup.0 as usize) {
                    assert!(Instant::now() < deadline);
                    native::pump(cx).await;
                }
                native::pump(cx).await;
                assert!(control.cleanup_settled());
                let (failed_control, completion) = owner
                    .update(cx, |_, window, _| {
                        window.begin_windows_native_confirmation(
                            request().with_fault_for_test(Fault::NativeOpen),
                        )
                    })
                    .unwrap()
                    .unwrap();
                assert!(completion.await.is_err());
                assert!(failed_control.cleanup_settled());
                let (confirmed_control, completion) = owner
                    .update(cx, |_, window, _| {
                        window.begin_windows_native_confirmation(request())
                    })
                    .unwrap()
                    .unwrap();
                assert!(!confirmed_control.cleanup_settled());
                let popup = dialog(raw, cx).await;
                choose(popup, IDOK.0);
                assert_eq!(completion.await.unwrap(), Outcome::Confirmed);
                assert!(confirmed_control.cleanup_settled());
                native::dispose(cx, owner, raw).await;
                completed.set(true);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
        });
    assert!(observed.get());
}

#[test]
fn active_owner_removal_cancels_before_parent_destruction() {
    let completed = Rc::new(Cell::new(false));
    let observed = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let (owner, raw) = native::open(app, "confirmation-removed-owner");
            owner
                .update(app, |_, window, app| window.publish(app))
                .unwrap()
                .unwrap();
            let (control, completion) = owner
                .update(app, |_, window, _| {
                    window.begin_windows_native_confirmation(request())
                })
                .unwrap()
                .unwrap();
            let receipt = owner
                .update(app, |_, window, _| {
                    window.observe_windows_native_destruction()
                })
                .unwrap()
                .unwrap();
            app.spawn(async move |cx| {
                let popup = dialog(raw, cx).await;
                owner
                    .update(cx, |_, window, _| window.remove_window())
                    .unwrap();
                assert_eq!(completion.await.unwrap(), Outcome::Cancelled);
                assert!(!native::alive(popup.0 as usize));
                receipt.await.unwrap();
                assert!(!native::alive(raw));
                assert!(control.reveal().is_err());
                completed.set(true);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
        });
    assert!(observed.get());
}

#[test]
fn missing_native_settlement_never_grants_confirmation_or_parent_disposal() {
    let completed = Rc::new(Cell::new(false));
    let observed = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let (owner, raw) = native::open(app, "confirmation-unsettled-owner");
            owner
                .update(app, |_, window, app| window.publish(app))
                .unwrap()
                .unwrap();
            let (control, completion) = owner
                .update(app, |_, window, _| {
                    window.begin_windows_native_confirmation(
                        request().with_fault_for_test(Fault::MissingDestruction),
                    )
                })
                .unwrap()
                .unwrap();
            let mut receipt = owner
                .update(app, |_, window, _| {
                    window.observe_windows_native_destruction()
                })
                .unwrap()
                .unwrap();
            app.spawn(async move |cx| {
                let popup = dialog(raw, cx).await;
                choose(popup, IDOK.0);
                assert!(completion.await.is_err());
                assert!(!control.cleanup_settled());
                assert!(control.reveal().is_err());
                owner
                    .update(cx, |_, window, _| window.remove_window())
                    .unwrap();
                native::pump(cx).await;
                assert!(native::alive(raw));
                assert!(matches!(
                    Pin::new(&mut receipt).poll(&mut Context::from_waker(Waker::noop())),
                    Poll::Pending
                ));
                completed.set(true);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
        });
    assert!(observed.get());
}

#[test]
fn owner_removal_before_creation_cancels_without_opening_native_dialog() {
    let completed = Rc::new(Cell::new(false));
    let observed = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let (owner, raw) = native::open(app, "confirmation-queued-removal");
            owner
                .update(app, |_, window, app| window.publish(app))
                .unwrap()
                .unwrap();
            let (control, completion, receipt) = owner
                .update(app, |_, window, _| {
                    let (control, completion) = window
                        .begin_windows_native_confirmation(
                            request().with_fault_for_test(Fault::NativeOpen),
                        )
                        .unwrap();
                    let receipt = window.observe_windows_native_destruction().unwrap();
                    window.remove_window();
                    (control, completion, receipt)
                })
                .unwrap();
            app.spawn(async move |cx| {
                assert_eq!(completion.await.unwrap(), Outcome::Cancelled);
                receipt.await.unwrap();
                assert!(!native::alive(raw));
                assert!(control.reveal().is_err());
                completed.set(true);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
        });
    assert!(observed.get());
}

#[test]
fn native_owner_close_obeys_veto_and_waits_for_confirmation_settlement() {
    type Completion = Rc<RefCell<Option<Pin<Box<WindowsNativeConfirmationCompleted>>>>>;
    let completion: Completion = Rc::new(RefCell::new(None));
    let target = Rc::new(Cell::new(0));
    let destruction = Rc::new(Cell::new(false));
    let observer_completion = completion.clone();
    let observer_target = target.clone();
    let observer_destruction = destruction.clone();
    with_windows_window_destruction_observer_for_test(
        move |raw| {
            if raw != observer_target.get() {
                return;
            }
            let mut completion = observer_completion.borrow_mut().take().unwrap();
            assert!(
                matches!(
                    completion
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop())),
                    Poll::Ready(Ok(Outcome::Cancelled))
                ),
                "native parent destruction preceded modal settlement"
            );
            observer_destruction.set(true);
        },
        || {
            Application::new()
                .with_quit_on_last_window_close(false)
                .run(move |app| {
                    let (owner, raw) = native::open(app, "confirmation-native-close");
                    target.set(raw);
                    let allow_close = Rc::new(Cell::new(false));
                    let callback_allow = allow_close.clone();
                    owner
                        .update(app, |_, window, app| {
                            window.on_window_should_close(app, move |_, _| callback_allow.get());
                            window.publish(app)
                        })
                        .unwrap()
                        .unwrap();
                    let (control, receipt) = owner
                        .update(app, |_, window, _| {
                            let (control, result) =
                                window.begin_windows_native_confirmation(request()).unwrap();
                            *completion.borrow_mut() = Some(Box::pin(result));
                            (
                                control,
                                window.observe_windows_native_destruction().unwrap(),
                            )
                        })
                        .unwrap();
                    app.spawn(async move |cx| {
                        let popup = dialog(raw, cx).await;
                        unsafe {
                            PostMessageW(
                                Some(native::hwnd(raw)),
                                WM_CLOSE,
                                Default::default(),
                                Default::default(),
                            )
                        }
                        .unwrap();
                        native::pump(cx).await;
                        assert!(native::alive(raw) && native::alive(popup.0 as usize));
                        assert!(matches!(
                            completion
                                .borrow_mut()
                                .as_mut()
                                .unwrap()
                                .as_mut()
                                .poll(&mut Context::from_waker(Waker::noop())),
                            Poll::Pending
                        ));
                        allow_close.set(true);
                        unsafe {
                            PostMessageW(
                                Some(native::hwnd(raw)),
                                WM_CLOSE,
                                Default::default(),
                                Default::default(),
                            )
                        }
                        .unwrap();
                        receipt.await.unwrap();
                        assert!(completion.borrow().is_none());
                        assert!(!native::alive(raw) && !native::alive(popup.0 as usize));
                        assert!(control.reveal().is_err());
                        cx.update(|app| app.quit()).unwrap();
                    })
                    .detach();
                });
        },
    );
    assert!(destruction.get());
}
