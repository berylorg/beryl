#![cfg(target_os = "windows")]

use gpui::Application;
#[path = "support/desktop_placement_native.rs"]
mod native;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, PostMessageW, TranslateMessage, WM_CLOSE,
};

#[test]
fn nested_native_dispatch_can_run_a_later_task_from_the_same_batch() {
    let completed = Rc::new(Cell::new(false));
    let observed = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let first = completed.clone();
            app.spawn(async move |cx| {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !first.get() {
                    assert!(
                        Instant::now() < deadline,
                        "nested native dispatch stranded a queued foreground task"
                    );
                    let mut message = MSG::default();
                    if unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                        unsafe {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    } else {
                        std::thread::yield_now();
                    }
                }
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
            app.spawn(async move |_| {
                completed.set(true);
            })
            .detach();
        });
    assert!(observed.get());
}

#[test]
fn self_waking_foreground_work_yields_to_native_close() {
    let polls = Rc::new(Cell::new(0));
    let observed = polls.clone();
    Application::new().run(move |app| {
        let (_, raw) = native::open(app, "foreground-dispatch-fairness");
        app.spawn(async move |_| {
            std::future::poll_fn(move |cx| {
                let count = polls.get() + 1;
                polls.set(count);
                if count == 1 {
                    unsafe {
                        PostMessageW(
                            Some(native::hwnd(raw)),
                            WM_CLOSE,
                            Default::default(),
                            Default::default(),
                        )
                    }
                    .unwrap();
                }
                    if !native::alive(raw) || count == 1000 {
                    return std::task::Poll::Ready(());
                }
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            })
            .await;
        })
        .detach();
    });
    assert!(
        observed.get() > 0 && observed.get() < 1000,
        "self-waking work must not strand native close"
    );
}
