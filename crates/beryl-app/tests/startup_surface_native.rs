#![cfg(target_os = "windows")]

use beryl_app::startup_surface::{StartupSurface, StartupSurfaceEvent};
use gpui::Application;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::{
            FindWindowW, GWL_STYLE, GetWindowLongW, IsWindow, SendMessageW, WM_CLOSE, WS_THICKFRAME,
        },
    },
    core::PCWSTR,
};

#[test]
fn native_close_requests_exit_and_retains_surface_until_owner_disposal() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            app.spawn(async move |cx| {
                for (busy, blocked) in [(false, false), (false, true), (true, false)] {
                    let events = Rc::new(RefCell::new(Vec::new()));
                    let observed = events.clone();
                    let window = cx
                        .update(|app| {
                            let handler =
                                move |event, _: &mut gpui::App| observed.borrow_mut().push(event);
                            if busy {
                                StartupSurface::open_busy(handler, app)
                            } else {
                                StartupSurface::open_failure("Native startup failure", handler, app)
                            }
                            .unwrap()
                        })
                        .unwrap();
                    let title = format!(
                        "Beryl startup surface native {} {busy} {blocked}",
                        std::process::id()
                    );
                    if blocked {
                        window
                            .update(cx, |surface, window, cx| {
                                assert!(surface.block_cleanup(
                                    surface.attempt(cx),
                                    "Cleanup blocked",
                                    window,
                                    cx
                                ));
                            })
                            .unwrap();
                    }
                    window
                        .update(cx, |_, window, _| window.set_window_title(&title))
                        .unwrap();
                    let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                    let raw = unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }.unwrap();
                    assert_eq!(
                        unsafe { GetWindowLongW(raw, GWL_STYLE) } as u32 & WS_THICKFRAME.0,
                        0
                    );
                    for _ in 0..2 {
                        unsafe { SendMessageW(raw, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0))) };
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await;
                        assert!(unsafe { IsWindow(Some(raw)) }.as_bool());
                    }
                    assert_eq!(&*events.borrow(), &[StartupSurfaceEvent::Exit]);
                    window
                        .update(cx, |_, window, _| window.remove_window())
                        .unwrap();
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while unsafe { IsWindow(Some(raw)) }.as_bool() {
                        assert!(Instant::now() < deadline);
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    cx.update(|app| assert!(app.windows().is_empty())).unwrap();
                }
                done.set(true);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
        });
    assert!(completed.get());
}
