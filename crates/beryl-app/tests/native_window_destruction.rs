#![cfg(target_os = "windows")]

#[path = "support/desktop_placement_native.rs"]
mod native;

use gpui::{
    AppContext, Application, AsyncApp, WindowsNativeWindowDestroyed,
    with_windows_window_destruction_observer_for_test,
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

type Receipt = Rc<RefCell<Option<Pin<Box<WindowsNativeWindowDestroyed>>>>>;

async fn wait_for_receipt(receipt: &Receipt, cx: &AsyncApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result = receipt
            .borrow_mut()
            .as_mut()
            .expect("registered receipt")
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        if let Poll::Ready(result) = result {
            result.expect("exact native destruction completion");
            receipt.borrow_mut().take();
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native destruction receipt timeout"
        );
        native::pump(cx).await;
    }
}

#[test]
fn exact_native_destruction_receipts_cover_hidden_published_and_leased_windows() {
    let receipt: Receipt = Rc::new(RefCell::new(None));
    let observed_receipt = receipt.clone();
    let target = Rc::new(Cell::new(0));
    let observed_target = target.clone();
    let attempts = Rc::new(Cell::new(0));
    let observed_attempts = attempts.clone();
    let completed = Rc::new(Cell::new(false));
    let result = completed.clone();
    with_windows_window_destruction_observer_for_test(
        move |raw| {
            if raw != observed_target.get() {
                return;
            }
            observed_attempts.set(observed_attempts.get() + 1);
            assert!(
                native::alive(raw),
                "observer runs before native destruction"
            );
            if let Some(receipt) = observed_receipt.borrow_mut().as_mut() {
                assert!(matches!(
                    receipt
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop())),
                    Poll::Pending
                ));
            }
        },
        || {
            Application::new().run(move |app| {
                let (control, _) = native::open(app, "destruction-receipt-control");
                app.spawn(async move |cx| {
                    for (name, publish, leased, abandon) in [
                        ("hidden", false, false, false),
                        ("published", true, false, false),
                        ("leased", false, true, false),
                        ("abandoned", false, false, true),
                    ] {
                        let (window, raw) = cx.update(|app| native::open(app, name)).unwrap();
                        target.set(raw);
                        attempts.set(0);
                        let registered = window
                            .update(cx, |_, window, _| {
                                window.observe_windows_native_destruction()
                            })
                            .unwrap()
                            .unwrap();
                        *receipt.borrow_mut() = Some(Box::pin(registered));
                        if abandon {
                            receipt.borrow_mut().take();
                        }
                        assert!(
                            window
                                .update(cx, |_, window, _| window
                                    .observe_windows_native_destruction())
                                .unwrap()
                                .is_err()
                        );
                        if publish {
                            window
                                .update(cx, |_, window, app| window.publish(app))
                                .unwrap()
                                .unwrap();
                            assert!(native::visible(raw));
                        }
                        let lease = leased.then(|| {
                            window
                                .update(cx, |_, window, _| window.lease_hidden_windows_window())
                                .unwrap()
                                .unwrap()
                        });
                        window
                            .update(cx, |_, window, _| window.remove_window())
                            .unwrap();
                        if let Some((lease, released)) = lease {
                            native::pump(cx).await;
                            assert!(native::alive(raw));
                            assert_eq!(attempts.get(), 0);
                            assert!(matches!(
                                receipt
                                    .borrow_mut()
                                    .as_mut()
                                    .unwrap()
                                    .as_mut()
                                    .poll(&mut Context::from_waker(Waker::noop())),
                                Poll::Pending
                            ));
                            cx.background_executor()
                                .spawn(async move {
                                    drop(lease);
                                })
                                .await;
                            assert!(released.await.unwrap().native_destroyed);
                        }
                        if abandon {
                            let deadline = Instant::now() + Duration::from_secs(5);
                            while native::alive(raw) {
                                assert!(
                                    Instant::now() < deadline,
                                    "abandoned observer pins window"
                                );
                                native::pump(cx).await;
                            }
                        } else {
                            wait_for_receipt(&receipt, cx).await;
                            assert!(!native::alive(raw));
                        }
                        assert_eq!(attempts.get(), 1);
                        assert!(window.update(cx, |_, _, _| ()).is_err());
                        target.set(0);
                    }
                    result.set(true);
                    control
                        .update(cx, |_, window, _| window.remove_window())
                        .unwrap();
                })
                .detach();
            });
        },
    );
    assert!(completed.get());
}
