#![cfg(feature = "test-faults")]

use beryl_app::{activity_panel_native_test_support::Host, activity_panel_test_support::*};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::sync::Arc;

fn query() -> Query {
    Query {
        owner: 1,
        period: 1,
        revision: 1,
        total: 1_000_000,
    }
}
fn pages() -> Vec<Arc<Page>> {
    vec![Arc::new(Page {
        start: 0,
        rows: (0u64..64)
            .map(|value| {
                let mut bytes = [53; 16];
                bytes[..8].copy_from_slice(&value.to_le_bytes());
                Row {
                    id: beryl_model::SyndicItemId::from_bytes(bytes),
                    status: Status::Running,
                    agent: "Main".into(),
                    value: "bounded activity".into(),
                    tooltip: "bounded tooltip".into(),
                }
            })
            .collect(),
    })]
}

fn mount(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| Host::new(query(), pages(), window, cx))
}

#[gpui::test]
fn native_viewport_realizes_a_bounded_range_and_keeps_focus_across_pages(cx: &mut TestAppContext) {
    let (root, cx) = mount(cx);
    cx.run_until_parked();
    let initial = root.read_with(cx, |host, app| host.diagnostics(app));
    assert_eq!(initial.total, 1_000_000);
    assert!(initial.realized_rows > 0 && initial.realized_rows <= 14);
    root.update_in(cx, |host, window, cx| host.focus(false, window, cx));
    cx.simulate_keystrokes("end");
    let bottom = root.read_with(cx, |host, app| host.diagnostics(app));
    assert!(bottom.offset > 27_000_000.);
    assert_eq!(bottom.pending_pages, 1);
    assert_eq!(bottom.realized_rows, 0);
    root.update_in(cx, |host, window, cx| {
        let mut next = query();
        next.revision += 1;
        host.replace(next, pages(), window, cx);
        assert_eq!(host.focused(window, cx), (false, true, false));
    });
    cx.simulate_keystrokes("home");
    assert_eq!(
        root.read_with(cx, |host, app| host.diagnostics(app).offset),
        0.
    );
}

#[gpui::test]
fn native_handle_keys_resize_and_inert_settles_focus_without_another_commit(
    cx: &mut TestAppContext,
) {
    let (root, cx) = mount(cx);
    cx.run_until_parked();
    root.update_in(cx, |host, window, cx| host.focus(true, window, cx));
    cx.simulate_keystrokes("up");
    assert_eq!(
        root.read_with(cx, |host, _| host.heights()),
        vec![(152., true)]
    );
    root.update_in(cx, |host, window, cx| {
        host.inert(window, cx);
        assert_eq!(host.focused(window, cx), (false, false, true));
    });
    cx.simulate_keystrokes("up");
    assert_eq!(
        root.read_with(cx, |host, _| host.heights()),
        vec![(152., true)]
    );
    assert!(root.read_with(cx, |host, app| host.diagnostics(app).inert));
}

#[gpui::test]
fn captured_pointer_release_during_busy_commits_once_after_current_publication(
    cx: &mut TestAppContext,
) {
    let (root, cx) = mount(cx);
    cx.run_until_parked();
    let origin = cx
        .debug_bounds("activity-panel-resize-handle")
        .unwrap()
        .center();
    cx.simulate_mouse_down(origin, gpui::MouseButton::Left, Default::default());
    let outside = gpui::point(origin.x, origin.y - gpui::px(20.));
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Default::default());
    root.read_with(cx, |host, _| host.gate(true, false));
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Default::default());
    assert_eq!(
        root.read_with(cx, |host, _| host.heights()),
        vec![(164., false)]
    );
    root.update_in(cx, |host, window, cx| {
        host.gate(false, false);
        host.replace(query(), pages(), window, cx);
    });
    assert_eq!(
        root.read_with(cx, |host, _| host.heights()),
        vec![(164., false), (164., true)]
    );
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Default::default());
    assert_eq!(root.read_with(cx, |host, _| host.heights()).len(), 2);
}

#[gpui::test]
fn retiring_a_captured_resize_restores_allocation_without_saving(cx: &mut TestAppContext) {
    let (root, cx) = mount(cx);
    cx.run_until_parked();
    let origin = cx
        .debug_bounds("activity-panel-resize-handle")
        .unwrap()
        .center();
    cx.simulate_mouse_down(origin, gpui::MouseButton::Left, Default::default());
    let outside = gpui::point(origin.x, origin.y - gpui::px(20.));
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Default::default());
    root.update_in(cx, |host, window, cx| host.inert(window, cx));
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Default::default());
    assert_eq!(
        root.read_with(cx, |host, _| host.heights()),
        vec![(164., false)]
    );
    assert_eq!(
        root.read_with(cx, |host, app| host.diagnostics(app).height),
        144.
    );
}

#[gpui::test]
fn retry_is_single_flight_and_feedback_removal_returns_viewport_focus(cx: &mut TestAppContext) {
    let (root, cx) = mount(cx);
    cx.run_until_parked();
    root.update_in(cx, |host, window, app| {
        host.feedback(
            Some(Feedback {
                identity: 42,
                explanation: "Activity page unavailable".into(),
                pending: false,
                retry_available: true,
            }),
            window,
            app,
        );
        host.focus_retry(window, app);
    });
    cx.simulate_keystrokes("enter enter");
    assert_eq!(root.read_with(cx, |host, _| host.retry_count()), 1);
    root.update_in(cx, |host, window, app| {
        host.feedback(None, window, app);
        assert_eq!(host.focused(window, app), (false, true, false));
    });
}

#[cfg(target_os = "windows")]
#[test]
fn windows_handle_captures_outside_resize_and_releases_on_native_destruction() {
    use gpui::{AppContext, Application};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::{
            Input::KeyboardAndMouse::GetCapture,
            WindowsAndMessaging::{
                IsWindow, SendMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
            },
        },
    };
    let finished = Arc::new(AtomicBool::new(false));
    let completed = finished.clone();
    Application::new().run(move |app| {
        let control = app
            .open_window(
                gpui::WindowOptions {
                    show: false,
                    focus: false,
                    ..Default::default()
                },
                |_, app| app.new(|_| gpui::Empty),
            )
            .unwrap();
        let window = app
            .open_window(
                gpui::WindowOptions {
                    show: false,
                    focus: false,
                    ..Default::default()
                },
                |window, app| app.new(|cx| Host::new(query(), pages(), window, cx)),
            )
            .unwrap();
        app.spawn(async move |cx| {
            let raw = window
                .update(cx, |_, window, app| {
                    let _ = app;
                    let RawWindowHandle::Win32(raw) = window.window_handle().unwrap().as_raw()
                    else {
                        panic!("Windows window");
                    };
                    raw.hwnd.get()
                })
                .unwrap();
            cx.update(|app| {
                app.update_window(window.into(), |_, window, app| window.draw(app).clear())
                    .unwrap()
            })
            .unwrap();
            let (origin, scale) = window
                .update(cx, |_, window, _| {
                    (
                        window
                            .debug_bounds_for_test("activity-panel-resize-handle")
                            .unwrap()
                            .center(),
                        window.scale_factor(),
                    )
                })
                .unwrap();
            let coordinate = |x: f32, y: f32| {
                LPARAM(((u32::from(y as i16 as u16) << 16) | u32::from(x as i16 as u16)) as isize)
            };
            let x = f32::from(origin.x) * scale;
            let y = f32::from(origin.y) * scale;
            let handle = HWND(raw as *mut _);
            unsafe {
                SendMessageW(
                    handle,
                    WM_LBUTTONDOWN,
                    Some(WPARAM(1)),
                    Some(coordinate(x, y)),
                );
            }
            assert_eq!(unsafe { GetCapture() }, handle);
            unsafe {
                SendMessageW(
                    handle,
                    WM_MOUSEMOVE,
                    Some(WPARAM(1)),
                    Some(coordinate(x, y - 20. * scale)),
                );
                SendMessageW(
                    handle,
                    WM_LBUTTONUP,
                    Some(WPARAM(0)),
                    Some(coordinate(x, y - 20. * scale)),
                );
            }
            assert_ne!(unsafe { GetCapture() }, handle);
            let heights = window.read_with(cx, |host, _| host.heights()).unwrap();
            assert_eq!(
                heights.iter().filter(|(_, committed)| *committed).count(),
                1
            );
            assert!((heights.last().unwrap().0 - 164.).abs() < 1.);
            window
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while unsafe { IsWindow(Some(handle)) }.as_bool() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "Activity window destruction"
                );
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(15))
                    .await;
            }
            assert_ne!(unsafe { GetCapture() }, handle);
            completed.store(true, Ordering::Release);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(finished.load(Ordering::Acquire));
}
