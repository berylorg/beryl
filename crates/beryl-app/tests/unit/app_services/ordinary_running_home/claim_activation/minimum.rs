use super::*;
use gpui::{Bounds, Pixels, Size};
use windows::Win32::{
    Foundation::RECT,
    UI::{
        Input::KeyboardAndMouse::{GetActiveWindow, GetFocus},
        WindowsAndMessaging::{
            GetClientRect, GetWindowRect, IsIconic, IsZoomed, MINMAXINFO, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOZORDER, SendMessageW, SetWindowPos, WM_GETMINMAXINFO,
        },
    },
};

fn rectangles(raw: HWND) -> (RECT, RECT) {
    let mut client = RECT::default();
    let mut outer = RECT::default();
    unsafe {
        GetClientRect(raw, &mut client).unwrap();
        GetWindowRect(raw, &mut outer).unwrap();
    }
    (client, outer)
}

fn dimensions(rect: RECT) -> (i32, i32) {
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn set_extent(raw: HWND, extent: (i32, i32)) {
    unsafe {
        SetWindowPos(
            raw,
            None,
            0,
            0,
            extent.0,
            extent.1,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
        )
        .unwrap();
    }
}

fn contained(selector: &str, bounds: Bounds<Pixels>, viewport: Size<Pixels>) {
    assert!(bounds.size.width > gpui::px(0.) && bounds.size.height > gpui::px(0.));
    assert!(
        bounds.left() >= gpui::px(0.) && bounds.right() <= viewport.width,
        "{selector} escapes native {viewport:?}: {bounds:?}"
    );
    assert!(
        bounds.top() >= gpui::px(0.) && bounds.bottom() <= viewport.height,
        "{selector} escapes native {viewport:?}: {bounds:?}"
    );
}

async fn draw(window: WindowHandle<MainWindowShellRoot>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, app| {
        window.draw_and_present_for_test(app)
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(10))
        .await;
}

#[test]
fn native_visible_lineage_and_original_editor_fit_the_enforced_client_minimum() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        lineage_fixture::prepare_home,
        final_selection,
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let raw = native(window, cx).await;
                let original_root = window.entity(cx).unwrap().entity_id();
                if window
                    .update(cx, |_, window, _| window.is_minimized())
                    .unwrap()
                    == Some(true)
                {
                    window
                        .update(cx, |_, window, _| window.activate_window())
                        .unwrap();
                }
                let child = lineage_fixture::child();
                let parent = SyndicThreadId::from_bytes([242; 16]);
                let (picker, key) = switcher_entry::open(window, child, cx).await;
                window
                    .update(cx, |_, _, app| {
                        picker.update(app, |picker, cx| picker.activate(&key, cx))
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(20);
                loop {
                    draw(window, cx).await;
                    let ready = window
                        .update(cx, |root, window, app| {
                            !root.test_running_thread_activation_pending()
                                && root
                                    .coherent_selected_thread_title(app)
                                    .is_some_and(|(selection, _)| selection.thread_id() == child)
                                && root.test_thread_lineage_view().is_some_and(|widget| {
                                    widget
                                        .read(app)
                                        .test_parent_input(parent, window)
                                        .is_some_and(|(_, _, available)| available)
                                })
                                && window.debug_bounds_for_test("thread-lineage").is_some()
                        })
                        .unwrap();
                    if ready {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "native child lineage did not become coherent"
                    );
                }
                let editor = composer(window, cx).await;
                let source = editor
                    .read_with(cx, |editor, app| editor.surface_snapshot(app).unwrap())
                    .unwrap();
                let (minimum, scale, focus_handle, id) = window
                    .update(cx, |root, window, app| {
                        let controller = root.controller().unwrap();
                        let minimum = controller.minimum_size();
                        assert_eq!(window.minimum_size_for_test(), Some(minimum));
                        assert_eq!(window.is_minimized(), Some(false));
                        (
                            minimum,
                            window.scale_factor(),
                            window.focused(app),
                            controller.window_id(),
                        )
                    })
                    .unwrap();
                *qualified.borrow_mut() = Some((id, child));
                let (client, original_outer) = rectangles(raw);
                let original_client = dimensions(client);
                let original_extent = dimensions(original_outer);
                let border = (
                    original_extent.0 - original_client.0,
                    original_extent.1 - original_client.1,
                );
                let expected = (
                    (f64::from(minimum.width) * f64::from(scale)).ceil() as i32,
                    (f64::from(minimum.height) * f64::from(scale)).ceil() as i32,
                );
                let foreground = unsafe { GetForegroundWindow() };
                let active = unsafe { GetActiveWindow() };
                let focus = unsafe { GetFocus() };
                assert!(!unsafe { IsIconic(raw) }.as_bool());
                assert!(!unsafe { IsZoomed(raw) }.as_bool());
                set_extent(raw, (expected.0 + border.0, expected.1 + border.1));
                let deadline = Instant::now() + Duration::from_secs(20);
                loop {
                    draw(window, cx).await;
                    let ready = window
                        .update(cx, |_, window, _| {
                            let size = window.viewport_size();
                            (f64::from(size.width) * f64::from(scale) - f64::from(expected.0)).abs()
                                < 0.01
                                && (f64::from(size.height) * f64::from(scale)
                                    - f64::from(expected.1))
                                .abs()
                                    < 0.01
                        })
                        .unwrap();
                    if ready && dimensions(rectangles(raw).0) == expected {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "native lineage client did not reach checked minimum"
                    );
                }
                window
                    .update(cx, |root, window, app| {
                        let viewport = window.viewport_size();
                        assert!(
                            viewport.width >= minimum.width && viewport.height >= minimum.height
                        );
                        let bounds = |selector| {
                            window
                                .debug_bounds_for_test(selector)
                                .unwrap_or_else(|| panic!("missing native {selector}"))
                        };
                        let toolbar = bounds("main-window-toolbar");
                        let lineage = bounds("thread-lineage");
                        let transcript = bounds("main-window-transcript-region");
                        let panel = bounds("main-window-user-input-panel");
                        let input = bounds("conversation-composer-root");
                        let status = bounds("main-window-status-line");
                        assert_eq!(toolbar.bottom(), lineage.top());
                        assert_eq!(lineage.bottom(), transcript.top());
                        assert_eq!(transcript.bottom(), panel.top());
                        assert_eq!(panel.bottom(), status.top());
                        assert_eq!(status.bottom(), viewport.height);
                        assert!(
                            input.size.width >= gpui::px(30.) && input.size.height >= gpui::px(16.)
                        );
                        assert!(input.top() >= panel.top() && input.bottom() <= panel.bottom());
                        assert!(panel.size.height <= viewport.height * 0.5);
                        for selector in [
                            "main-window-toolbar",
                            "thread-lineage",
                            "thread-lineage-current",
                            "main-window-user-input-panel",
                            "conversation-composer-root",
                            "main-window-status-line",
                            "main-window-new-thread-split-button",
                            "main-window-thread-back",
                            "main-window-thread-forward",
                            "main-window-thread-selector",
                            "main-window-running-threads",
                            "main-window-exit",
                            "main-window-status-model",
                            "main-window-status-context:unknown",
                            "main-window-status-turn",
                        ] {
                            contained(selector, bounds(selector), viewport);
                        }
                        if let Some(command) =
                            window.debug_bounds_for_test("main-window-new-window")
                        {
                            contained("main-window-new-window", command, viewport);
                        }
                        assert_eq!(window.focused(app), focus_handle);
                        assert_eq!(
                            root.coherent_selected_thread_title(app)
                                .unwrap()
                                .0
                                .thread_id(),
                            child
                        );
                        assert_eq!(
                            root.controller()
                                .unwrap()
                                .composer_mount()
                                .unwrap()
                                .read(app)
                                .contribution()
                                .unwrap()
                                .entity_id(),
                            editor.entity_id()
                        );
                        let after = editor.read(app).surface_snapshot(app).unwrap();
                        assert_eq!(after.selection, source.selection);
                        assert_eq!(after.binding, source.binding);
                        assert_eq!(after.source_selection, source.source_selection);
                    })
                    .unwrap();
                let mut tracking = MINMAXINFO::default();
                unsafe {
                    SendMessageW(
                        raw,
                        WM_GETMINMAXINFO,
                        Some(WPARAM(0)),
                        Some(LPARAM((&mut tracking as *mut MINMAXINFO) as isize)),
                    );
                }
                assert_eq!(
                    (tracking.ptMinTrackSize.x, tracking.ptMinTrackSize.y),
                    (expected.0 + border.0, expected.1 + border.1)
                );
                assert_eq!(window.entity(cx).unwrap().entity_id(), original_root);
                assert_eq!(unsafe { GetForegroundWindow() }, foreground);
                assert_eq!(unsafe { GetActiveWindow() }, active);
                assert_eq!(unsafe { GetFocus() }, focus);
                assert!(!unsafe { IsIconic(raw) }.as_bool());
                assert!(!unsafe { IsZoomed(raw) }.as_bool());
                let (_, resized) = rectangles(raw);
                assert_eq!(
                    (resized.left, resized.top),
                    (original_outer.left, original_outer.top)
                );
                assert_eq!(native(window, cx).await, raw);
                set_extent(raw, original_extent);
                let deadline = Instant::now() + Duration::from_secs(20);
                loop {
                    draw(window, cx).await;
                    if dimensions(rectangles(raw).0) == original_client {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "original adequate native bounds did not restore"
                    );
                }
                let deadline = Instant::now() + Duration::from_secs(20);
                loop {
                    let admitted = window
                        .update(cx, |root, _, _| {
                            if !root.test_exit_command_enabled() {
                                return false;
                            }
                            root.test_activate_exit_command();
                            assert!(
                                owner.borrow().exit_requested(),
                                "original eligible Exit did not synchronously admit its request"
                            );
                            true
                        })
                        .unwrap();
                    if admitted {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "original lineage window Exit did not become eligible"
                    );
                    draw(window, cx).await;
                }
            })
        },
        true,
        2,
    );
}
