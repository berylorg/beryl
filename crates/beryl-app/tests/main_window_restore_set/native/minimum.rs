use super::*;
use gpui::{AppContext as _, Bounds, Pixels, Size};
use windows::Win32::{
    Foundation::{LPARAM, RECT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{GetActiveWindow, GetFocus},
        WindowsAndMessaging::{
            GetClientRect, GetForegroundWindow, GetWindowRect, IsIconic, IsZoomed, MINMAXINFO,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SendMessageW, SetWindowPos, WM_GETMINMAXINFO,
        },
    },
};

fn rectangles(raw: usize) -> (RECT, RECT) {
    let mut client = RECT::default();
    let mut outer = RECT::default();
    unsafe {
        GetClientRect(windows_native::hwnd(raw), &mut client).unwrap();
        GetWindowRect(windows_native::hwnd(raw), &mut outer).unwrap();
    }
    (client, outer)
}

fn dimensions(rect: RECT) -> (i32, i32) {
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn checked_client_extent(minimum: Size<Pixels>, scale: f32) -> (i32, i32) {
    (
        (f64::from(minimum.width) * f64::from(scale)).ceil() as i32,
        (f64::from(minimum.height) * f64::from(scale)).ceil() as i32,
    )
}

fn assert_contained(selector: &str, bounds: Bounds<Pixels>, viewport: Size<Pixels>) {
    assert!(bounds.size.width > gpui::px(0.) && bounds.size.height > gpui::px(0.));
    assert!(
        bounds.left() >= gpui::px(0.) && bounds.right() <= viewport.width,
        "{selector} escapes native viewport {viewport:?}: {bounds:?}"
    );
    assert!(
        bounds.top() >= gpui::px(0.) && bounds.bottom() <= viewport.height,
        "{selector} escapes native viewport {viewport:?}: {bounds:?}"
    );
}

async fn assert_native_composition_at_minimum(
    index: usize,
    shell: &MainWindowShell,
    raw: usize,
    editor_line_height: f32,
    cx: &mut AsyncApp,
) {
    let window = shell.window();
    cx.update_window(window.into(), |_, window, app| {
        window.draw_and_present_for_test(app)
    })
    .unwrap();
    let original_root = window.entity(cx).unwrap().entity_id();
    let original_editor = window
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .and_then(|mount| mount.read(app).contribution())
                .map(|editor| {
                    let snapshot = editor.read(app).surface_snapshot(app).unwrap();
                    (
                        editor.entity_id(),
                        snapshot.selection,
                        snapshot.binding,
                        snapshot.source_selection,
                    )
                })
        })
        .unwrap();
    let (minimum, scale, original_focus) = window
        .update(cx, |root, window, app| {
            let minimum = root.controller().unwrap().minimum_size();
            assert_eq!(window.minimum_size_for_test(), Some(minimum));
            assert_eq!(window.is_minimized(), Some(false));
            (minimum, window.scale_factor(), window.focused(app))
        })
        .unwrap();
    let (client, outer) = rectangles(raw);
    let client_size = dimensions(client);
    let outer_size = dimensions(outer);
    let border = (outer_size.0 - client_size.0, outer_size.1 - client_size.1);
    let expected = checked_client_extent(minimum, scale);
    let foreground = unsafe { GetForegroundWindow() };
    let active = unsafe { GetActiveWindow() };
    let focus = unsafe { GetFocus() };
    assert!(!unsafe { IsIconic(windows_native::hwnd(raw)) }.as_bool());
    assert!(!unsafe { IsZoomed(windows_native::hwnd(raw)) }.as_bool());
    unsafe {
        SetWindowPos(
            windows_native::hwnd(raw),
            None,
            0,
            0,
            expected.0 + border.0,
            expected.1 + border.1,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
        )
        .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let viewport = window
            .update(cx, |_, window, _| window.viewport_size())
            .unwrap();
        if dimensions(rectangles(raw).0) == expected
            && (f64::from(viewport.width) * f64::from(scale) - f64::from(expected.0)).abs() < 0.01
            && (f64::from(viewport.height) * f64::from(scale) - f64::from(expected.1)).abs() < 0.01
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "original native client did not reach minimum {minimum:?}"
        );
        windows_native::pump(cx).await;
    }
    cx.update_window(window.into(), |_, window, app| {
        window.draw_and_present_for_test(app)
    })
    .unwrap();
    cx.update_window(window.into(), |_, window, _| {
        let viewport = window.viewport_size();
        assert!(viewport.width >= minimum.width && viewport.height >= minimum.height);
        assert_eq!(window.minimum_size_for_test(), Some(minimum));
        let bounds = |selector| {
            window
                .debug_bounds_for_test(selector)
                .unwrap_or_else(|| panic!("missing native rendered {selector}"))
        };
        let toolbar = bounds("main-window-toolbar");
        let transcript = bounds("main-window-transcript-region");
        let status = bounds("main-window-status-line");
        assert_eq!(toolbar.bottom(), transcript.top());
        assert_eq!(status.bottom(), viewport.height);
        match window.debug_bounds_for_test("main-window-user-input-panel") {
            Some(panel) => {
                let editor = bounds("conversation-composer-root");
                assert!(original_editor.is_some());
                assert_contained("editor", editor, viewport);
                assert!(editor.size.width >= gpui::px(30.));
                assert!(editor.size.height >= gpui::px(editor_line_height));
                assert!(editor.top() >= panel.top() && editor.bottom() <= panel.bottom());
                assert_eq!(panel.top(), transcript.bottom());
                assert_eq!(panel.bottom(), status.top());
                assert!(panel.size.height <= viewport.height * 0.5);
            }
            None => {
                assert!(original_editor.is_none());
                assert!(
                    window
                        .debug_bounds_for_test("conversation-composer-root")
                        .is_none()
                );
                assert_eq!(transcript.bottom(), status.top());
                assert_eq!(transcript.size.height, gpui::px(0.));
            }
        }
        for selector in [
            "main-window-toolbar",
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
            assert_contained(selector, bounds(selector), viewport);
        }
        if let Some(command) = window.debug_bounds_for_test("main-window-new-window") {
            assert_contained("main-window-new-window", command, viewport);
        }
    })
    .unwrap();
    let mut tracking = MINMAXINFO::default();
    unsafe {
        SendMessageW(
            windows_native::hwnd(raw),
            WM_GETMINMAXINFO,
            Some(WPARAM(0)),
            Some(LPARAM((&mut tracking as *mut MINMAXINFO) as isize)),
        );
    }
    assert_eq!(
        (tracking.ptMinTrackSize.x, tracking.ptMinTrackSize.y),
        (expected.0 + border.0, expected.1 + border.1)
    );
    let (client, after) = rectangles(raw);
    assert_eq!(dimensions(client), expected);
    assert_eq!((after.left, after.top), (outer.left, outer.top));
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    assert_eq!(unsafe { GetActiveWindow() }, active);
    assert_eq!(unsafe { GetFocus() }, focus);
    assert!(!unsafe { IsIconic(windows_native::hwnd(raw)) }.as_bool());
    assert!(!unsafe { IsZoomed(windows_native::hwnd(raw)) }.as_bool());
    assert!(windows_native::visible(raw));
    assert_eq!(cx.update(|app| raw_handle(index, shell, app)).unwrap(), raw);
    assert_eq!(window.entity(cx).unwrap().entity_id(), original_root);
    window
        .update(cx, |root, window, app| {
            assert_eq!(window.focused(app), original_focus);
            let after = root
                .controller()
                .unwrap()
                .composer_mount()
                .and_then(|mount| mount.read(app).contribution())
                .map(|editor| {
                    let snapshot = editor.read(app).surface_snapshot(app).unwrap();
                    (
                        editor.entity_id(),
                        snapshot.selection,
                        snapshot.binding,
                        snapshot.source_selection,
                    )
                });
            assert_eq!(after, original_editor);
        })
        .unwrap();
}

#[test]
fn native_original_restored_and_threadless_shells_fit_their_enforced_client_minimum() {
    let fixtures = home_support::worker(|| {
        vec![
            (fixture(Kind::Restored, 171), 16.),
            (fixture(Kind::Threadless, 181), 16.),
            (
                fixture_with_editor_metrics(Kind::Restored, 191, Some(80.)),
                80.,
            ),
        ]
    })
    .join()
    .unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let result = finished.clone();
    Application::new().run(move |app| {
        let (control, _) = windows_native::open(app, "restore-set-minimum");
        app.spawn(async move |cx| {
            for (
                NativeFixture {
                    fixture,
                    lifetime,
                    mut prepared,
                    appearance,
                    saved,
                    ids,
                    cancellation: _,
                },
                editor_line_height,
            ) in fixtures
            {
                let handles = Arc::new(std::sync::Mutex::new(Vec::new()));
                let observed = handles.clone();
                prepared.test_before_publication(move |index, shell, app| {
                    let raw = raw_handle(index, shell, app);
                    assert!(!windows_native::visible(raw));
                    observed.lock().unwrap().push(raw);
                    Ok(())
                });
                let complete = Rc::new(RefCell::new(None));
                let delivered = complete.clone();
                cx.update(|app| {
                    let appearance = GpuiAppearanceWindowSet::new(
                        appearance,
                        NonZeroUsize::new(256).unwrap(),
                        app,
                    );
                    drop(prepared.start(
                        appearance,
                        move |value, _| *delivered.borrow_mut() = Some(value),
                        app,
                    ));
                })
                .unwrap();
                let published = match wait_for(&complete, cx).await {
                    MainWindowNativeRestoreSetCompletion::Published(value) => value,
                    MainWindowNativeRestoreSetCompletion::Failed(value) => {
                        panic!("{}", value.error)
                    }
                };
                assert_eq!(published.window_ids(), ids);
                let originals = handles.lock().unwrap().clone();
                assert_eq!(originals.len(), published.shells().len());
                for (index, (shell, raw)) in published
                    .shells()
                    .iter()
                    .zip(originals.iter().copied())
                    .enumerate()
                {
                    assert_native_composition_at_minimum(index, shell, raw, editor_line_height, cx)
                        .await;
                }
                let disposed = Rc::new(RefCell::new(None));
                let delivered = disposed.clone();
                cx.update(|app| {
                    published
                        .test_dispose(move |value, _| *delivered.borrow_mut() = Some(value), app)
                })
                .unwrap();
                let disposal = wait_for(&disposed, cx).await;
                assert!(disposal.retained.is_none(), "{}", disposal.error);
                assert!(disposal.diagnostics.is_empty());
                assert!(originals.iter().all(|raw| !windows_native::alive(*raw)));
                assert_eq!(fixture.process.main_window_occupancy(), 0);
                drop((disposal, lifetime));
                cx.background_executor()
                    .spawn(async move {
                        assert_eq!(snapshot(&fixture), saved);
                        cleanup(fixture);
                    })
                    .await;
            }
            result.store(true, Ordering::SeqCst);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(finished.load(Ordering::SeqCst));
}
