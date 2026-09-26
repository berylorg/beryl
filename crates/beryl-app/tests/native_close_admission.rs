#![cfg(target_os = "windows")]

#[path = "support/desktop_placement_native.rs"]
mod native;

use gpui::Application;
use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{SendMessageW, WM_CLOSE},
};

fn request_close(raw: usize) {
    unsafe {
        SendMessageW(
            native::hwnd(raw),
            WM_CLOSE,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
    };
}

#[test]
fn native_close_requires_successful_callback_and_recovers_after_reentrant_denial() {
    let completed = Rc::new(Cell::new(false));
    let result = completed.clone();
    Application::new().run(move |app| {
        let (control, _) = native::open(app, "close-admission-control");
        app.spawn(async move |cx| {
            for registered in [true, false] {
                let (window, raw) = cx
                    .update(|app| {
                        native::open(app, if registered { "callback" } else { "default" })
                    })
                    .unwrap();
                let calls = Rc::new(Cell::new(0));
                let allowed = Rc::new(Cell::new(false));
                let observed_calls = calls.clone();
                let observed_allowed = allowed.clone();
                let receipt = window
                    .update(cx, |_, window, app| {
                        if registered {
                            window.on_window_should_close(app, move |_, _| {
                                observed_calls.set(observed_calls.get() + 1);
                                observed_allowed.get()
                            });
                        }
                        window.publish(app).unwrap();
                        window.observe_windows_native_destruction().unwrap()
                    })
                    .unwrap();
                let mut receipt = Box::pin(receipt);
                assert!(native::visible(raw));
                if registered {
                    window.update(cx, |_, _, _| request_close(raw)).unwrap();
                    assert!(native::alive(raw));
                    assert_eq!(calls.get(), 0, "borrowed app cannot execute close callback");
                    assert!(matches!(
                        receipt
                            .as_mut()
                            .poll(&mut Context::from_waker(Waker::noop())),
                        Poll::Pending
                    ));
                    request_close(raw);
                    assert!(native::alive(raw));
                    assert_eq!(calls.get(), 1, "ordinary callback executes and vetoes");
                    allowed.set(true);
                }
                request_close(raw);
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if let Poll::Ready(outcome) = receipt
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                    {
                        outcome.unwrap();
                        break;
                    }
                    assert!(Instant::now() < deadline, "native close did not settle");
                    native::pump(cx).await;
                }
                assert!(!native::alive(raw));
                assert_eq!(calls.get(), if registered { 2 } else { 0 });
                native::pump(cx).await;
                assert!(window.update(cx, |_, _, _| ()).is_err());
            }
            result.set(true);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(completed.get());
}
