#![cfg(target_os = "windows")]

use gpui::{
    App, AppContext, Application, AsyncApp, Empty, TitlebarOptions, WindowBounds, WindowHandle,
    WindowOptions, bounds, point, px, size, with_windows_window_destruction_observer_for_test,
};
use std::{
    cell::Cell,
    future::{Future, poll_fn},
    panic::{AssertUnwindSafe, catch_unwind},
    pin::pin,
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
    task::Poll,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{
            FindWindowW, GetForegroundWindow, IsWindow, IsWindowVisible, IsZoomed, SC_MAXIMIZE,
            SC_MINIMIZE, SC_RESTORE, SW_HIDE, SW_SHOWNOACTIVATE, SendMessageW, ShowWindow,
            WM_CLOSE, WM_SYSCOMMAND,
        },
    },
    core::PCWSTR,
};

fn hwnd(raw: usize) -> HWND {
    HWND(raw as *mut _)
}

fn alive(raw: usize) -> bool {
    unsafe { IsWindow(Some(hwnd(raw))) }.as_bool()
}

fn hidden(raw: usize) -> bool {
    alive(raw) && !unsafe { IsWindowVisible(hwnd(raw)) }.as_bool()
}

fn open(cx: &mut App, name: &str, maximized: bool) -> (WindowHandle<Empty>, usize) {
    let title = format!("Beryl native operation {} {name}", std::process::id());
    let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let rect = bounds(point(px(80.0), px(80.0)), size(px(640.0), px(480.0)));
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(if maximized {
                    WindowBounds::Maximized(rect)
                } else {
                    WindowBounds::Windowed(rect)
                }),
                show: false,
                focus: false,
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Empty),
        )
        .unwrap();
    let native = unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }.unwrap();
    assert!(hidden(native.0 as usize));
    (window, native.0 as usize)
}

fn hold_on_worker(
    token: impl Send + 'static,
    raw: usize,
    unwind: bool,
) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (release, wait) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        assert!(alive(raw));
        let _ = wait.recv();
        assert!(alive(raw));
        if unwind {
            panic!("injected native worker unwind");
        }
        drop(token);
    });
    (release, worker)
}

async fn pump(cx: &AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(15))
        .await;
}

async fn removed(cx: &AsyncApp, raw: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(raw) {
        assert!(Instant::now() < deadline, "native disposal did not settle");
        pump(cx).await;
    }
}

async fn removal_during_worker(cx: &mut AsyncApp, unwind: bool, drop_observer: bool) -> usize {
    let (window, raw) = cx
        .update(|cx| open(cx, &format!("removed-{unwind}-{drop_observer}"), false))
        .unwrap();
    let (token, released) = window
        .update(cx, |_, window, _| window.lease_hidden_windows_window())
        .unwrap()
        .unwrap();
    assert_eq!(token.raw_handle(), raw);
    let (release, worker) = hold_on_worker(token, raw, unwind);
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    assert!(window.update(cx, |_, _, _| ()).is_err());
    assert!(hidden(raw));
    if drop_observer {
        drop(released);
        pump(cx).await;
        assert!(hidden(raw));
        release.send(()).unwrap();
        removed(cx, raw).await;
    } else {
        pump(cx).await;
        assert!(hidden(raw));
        release.send(()).unwrap();
        let settled = released.await.unwrap();
        assert!(settled.native_destroyed);
        assert!(!settled.close_requested);
        assert!(!alive(raw));
    }
    assert_eq!(worker.join().is_err(), unwind);
    raw
}

async fn close_intent_retains_root(cx: &mut AsyncApp) -> usize {
    let (window, raw) = cx.update(|cx| open(cx, "close-intent", false)).unwrap();
    let foreground = unsafe { GetForegroundWindow() };
    let (token, released) = window
        .update(cx, |_, window, _| {
            window.activate_window();
            let lease = window.lease_hidden_windows_window().unwrap();
            assert!(window.lease_hidden_windows_window().is_err());
            window.minimize_window();
            window.zoom_window();
            lease
        })
        .unwrap();
    let (release, worker) = hold_on_worker(token, raw, false);
    for command in [SC_MINIMIZE, SC_MAXIMIZE, SC_RESTORE] {
        unsafe {
            SendMessageW(
                hwnd(raw),
                WM_SYSCOMMAND,
                Some(WPARAM(command as usize)),
                Some(LPARAM(0)),
            )
        };
    }
    pump(cx).await;
    assert!(hidden(raw));
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    for _ in 0..2 {
        unsafe { SendMessageW(hwnd(raw), WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0))) };
    }
    pump(cx).await;
    assert!(hidden(raw));
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    window
        .update(cx, |_, window, cx| {
            assert!(window.windows_native_close_requested());
            assert!(window.publish(cx).is_err());
        })
        .unwrap();
    release.send(()).unwrap();
    let settled = released.await.unwrap();
    worker.join().unwrap();
    assert!(settled.close_requested);
    assert!(!settled.native_destroyed);
    assert!(hidden(raw));
    window
        .update(cx, |_, window, cx| {
            assert!(window.windows_native_close_requested());
            assert!(window.lease_hidden_windows_window().is_err());
            assert!(window.publish(cx).is_err());
            window.activate_window();
            window.minimize_window();
            window.zoom_window();
        })
        .unwrap();
    pump(cx).await;
    assert!(hidden(raw));
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    removed(cx, raw).await;
    raw
}

async fn release_allows_publication(cx: &mut AsyncApp, maximized: bool) -> usize {
    let (window, raw) = cx
        .update(|cx| open(cx, &format!("released-{maximized}"), maximized))
        .unwrap();
    let foreground = unsafe { GetForegroundWindow() };
    let (token, released) = window
        .update(cx, |_, window, cx| {
            let lease = window.lease_hidden_windows_window().unwrap();
            assert!(window.publish(cx).is_err());
            lease
        })
        .unwrap();
    let (release, worker) = hold_on_worker(token, raw, false);
    release.send(()).unwrap();
    let settled = released.await.unwrap();
    worker.join().unwrap();
    assert!(!settled.close_requested);
    assert!(!settled.native_destroyed);
    assert!(hidden(raw));
    window
        .update(cx, |_, window, cx| {
            assert!(!window.windows_native_close_requested());
            assert!(window.lease_hidden_windows_window().is_err());
            window.publish(cx).unwrap();
            assert!(window.lease_hidden_windows_window().is_err());
        })
        .unwrap();
    assert!(unsafe { IsWindowVisible(hwnd(raw)) }.as_bool());
    assert_eq!(unsafe { IsZoomed(hwnd(raw)) }.as_bool(), maximized);
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    removed(cx, raw).await;
    raw
}

async fn published_observation(
    cx: &mut AsyncApp,
    unwind: bool,
    drop_observer: bool,
    ordinary_close: bool,
) -> usize {
    let (window, raw) = cx
        .update(|cx| open(cx, "published-observation", false))
        .unwrap();
    let allowed = Rc::new(Cell::new(false));
    let calls = Rc::new(Cell::new(0));
    let callback_allowed = allowed.clone();
    let callback_calls = calls.clone();
    let foreground = unsafe { GetForegroundWindow() };
    let (token, released) = window
        .update(cx, |_, window, _| {
            assert!(window.lease_published_windows_window().is_err());
            window.lease_hidden_windows_window().unwrap()
        })
        .unwrap();
    drop(token);
    assert!(!released.await.unwrap().native_destroyed);
    window
        .update(cx, |_, window, cx| {
            window.on_window_should_close(cx, move |_, _| {
                callback_calls.set(callback_calls.get() + 1);
                callback_allowed.get()
            });
            window.publish(cx).unwrap();
            unsafe { ShowWindow(hwnd(raw), SW_HIDE) };
            assert!(window.lease_published_windows_window().is_err());
            unsafe { ShowWindow(hwnd(raw), SW_SHOWNOACTIVATE) };
        })
        .unwrap();
    for _ in 0..2 {
        let (token, released) = window
            .update(cx, |_, window, _| {
                let lease = window.lease_published_windows_window().unwrap();
                assert!(window.lease_published_windows_window().is_err());
                assert!(window.lease_hidden_windows_window().is_err());
                lease
            })
            .unwrap();
        assert_eq!(token.raw_handle(), raw);
        drop(token);
        assert!(!released.await.unwrap().native_destroyed);
    }
    let (token, released) = window
        .update(cx, |_, window, _| window.lease_published_windows_window())
        .unwrap()
        .unwrap();
    let (release, worker) = hold_on_worker(token, raw, unwind);
    unsafe { SendMessageW(hwnd(raw), WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0))) };
    assert_eq!(calls.get(), 1);
    assert!(window.update(cx, |_, _, _| ()).is_ok());
    assert!(alive(raw));
    if ordinary_close {
        allowed.set(true);
        unsafe { SendMessageW(hwnd(raw), WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0))) };
        assert_eq!(calls.get(), 2);
    } else {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
                assert!(window.lease_published_windows_window().is_err());
            })
            .unwrap();
    }
    pump(cx).await;
    assert!(alive(raw));
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    if drop_observer {
        drop(released);
        pump(cx).await;
        assert!(alive(raw));
        release.send(()).unwrap();
        removed(cx, raw).await;
    } else {
        release.send(()).unwrap();
        assert!(released.await.unwrap().native_destroyed);
        assert!(!alive(raw));
    }
    assert_eq!(worker.join().is_err(), unwind);
    pump(cx).await;
    assert!(window.update(cx, |_, _, _| ()).is_err());
    raw
}

async fn bounded_catch<F: Future>(
    future: F,
    timeout: gpui::Task<()>,
) -> std::thread::Result<F::Output> {
    let mut future = pin!(future);
    let mut timeout = pin!(timeout);
    poll_fn(|cx| {
        match catch_unwind(AssertUnwindSafe(|| {
            if let Poll::Ready(value) = future.as_mut().poll(cx) {
                return Poll::Ready(value);
            }
            assert!(
                timeout.as_mut().poll(cx).is_pending(),
                "native test timed out"
            );
            Poll::Pending
        })) {
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(panic) => Poll::Ready(Err(panic)),
        }
    })
    .await
}

#[test]
fn native_operations_preserve_exact_lifetime_and_disposal() {
    let result = Arc::new(Mutex::new(None));
    let captured = result.clone();
    let destroyed = Arc::new(Mutex::new(Vec::new()));
    let observed = destroyed.clone();
    let gui_thread = std::thread::current().id();
    with_windows_window_destruction_observer_for_test(
        move |raw| {
            observed
                .lock()
                .unwrap()
                .push((raw, std::thread::current().id()));
        },
        || {
            Application::new().run(move |cx| {
                let (control_window, control) = open(cx, "control", false);
                let timeout = cx.background_executor().timer(Duration::from_secs(30));
                cx.spawn(async move |cx| {
                    let outcome = bounded_catch(
                        async {
                            let mut expected = Vec::new();
                            expected.push(removal_during_worker(cx, false, false).await);
                            expected.push(removal_during_worker(cx, true, false).await);
                            expected.push(removal_during_worker(cx, false, true).await);
                            expected.push(close_intent_retains_root(cx).await);
                            expected.push(release_allows_publication(cx, false).await);
                            expected.push(release_allows_publication(cx, true).await);
                            expected.push(published_observation(cx, false, false, false).await);
                            expected.push(published_observation(cx, true, false, false).await);
                            expected.push(published_observation(cx, false, true, false).await);
                            expected.push(published_observation(cx, false, false, true).await);
                            let (ordinary, raw) =
                                cx.update(|cx| open(cx, "ordinary", false)).unwrap();
                            ordinary
                                .update(cx, |_, window, cx| {
                                    window.publish(cx).unwrap();
                                    assert!(window.lease_hidden_windows_window().is_err());
                                    window.remove_window();
                                })
                                .unwrap();
                            removed(cx, raw).await;
                            expected.push(raw);
                            expected.push(control);
                            expected
                        },
                        timeout,
                    )
                    .await;
                    let failed = outcome.is_err();
                    *captured.lock().unwrap() = Some(outcome);
                    cx.update(|cx| {
                        if failed {
                            cx.quit();
                        } else {
                            control_window
                                .update(cx, |_, window, _| window.remove_window())
                                .unwrap();
                        }
                    })
                    .unwrap();
                })
                .detach();
            })
        },
    );
    match result
        .lock()
        .unwrap()
        .take()
        .expect("native test completed")
    {
        Err(panic) => std::panic::resume_unwind(panic),
        Ok(expected) => {
            assert_eq!(
                *destroyed.lock().unwrap(),
                expected
                    .iter()
                    .map(|raw| (*raw, gui_thread))
                    .collect::<Vec<_>>()
            );
            assert!(expected.into_iter().all(|raw| !alive(raw)));
        }
    }
}
