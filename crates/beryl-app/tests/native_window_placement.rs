#![cfg(target_os = "windows")]

use gpui::{
    App, AppContext, Application, Bounds, DevicePixels, Empty, Pixels, TitlebarOptions,
    WindowBounds, WindowOptions, WindowsOuterWindowPlacement, WindowsWindowPlacementMonitor, point,
    px, size, with_windows_window_creation_hook_for_test,
};
use std::{
    ops::ControlFlow,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
};
use windows::{
    Win32::{
        Foundation::RECT,
        UI::WindowsAndMessaging::{
            FindWindowW, GetForegroundWindow, GetWindowPlacement, GetWindowRect, IsWindow,
            IsWindowVisible, IsZoomed, SW_HIDE, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, ShowWindow,
            WINDOWPLACEMENT,
        },
    },
    core::PCWSTR,
};

fn physical(x: i32, y: i32, width: i32, height: i32) -> Bounds<DevicePixels> {
    Bounds::new(
        point(DevicePixels(x), DevicePixels(y)),
        size(DevicePixels(width), DevicePixels(height)),
    )
}

fn logical(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

#[test]
fn outer_conversion_preserves_negative_coordinates_and_work_area_offsets() {
    let monitor = physical(-1920, -1080, 1920, 1080);
    let work = physical(-1872, -1040, 1872, 1040);
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let bounds = logical(
            -1800.0 / scale,
            -900.0 / scale,
            900.0 / scale,
            600.0 / scale,
        );
        let outer = WindowsOuterWindowPlacement::new(bounds, scale, monitor, work, false).unwrap();
        assert_eq!(outer.screen_bounds(), physical(-1800, -900, 900, 600));
        assert_eq!(outer.workspace_bounds(), physical(-1848, -940, 900, 600));
        assert_eq!(
            WindowsOuterWindowPlacement::from_workspace_bounds(
                outer.workspace_bounds(),
                monitor,
                work,
                false,
            )
            .unwrap(),
            outer,
        );
        let tool = WindowsOuterWindowPlacement::new(bounds, scale, monitor, work, true).unwrap();
        assert_eq!(tool.workspace_bounds(), tool.screen_bounds());
        assert_eq!(
            WindowsOuterWindowPlacement::from_workspace_bounds(
                tool.workspace_bounds(),
                monitor,
                work,
                true,
            )
            .unwrap(),
            tool,
        );
    }
}

#[test]
fn outer_conversion_rejects_invalid_geometry_before_native_use() {
    let monitor = physical(0, 0, 1920, 1080);
    for bounds in [
        physical(0, 0, 0, 10),
        physical(0, 0, 10, -1),
        physical(i32::MAX - 10, 0, 20, 20),
        physical(i32::MAX - 20, 0, 20, 20),
    ] {
        assert!(
            WindowsOuterWindowPlacement::from_workspace_bounds(
                bounds,
                monitor,
                physical(48, 40, 1872, 1040),
                false,
            )
            .is_err()
        );
    }
    for bounds in [
        logical(f32::NAN, 0.0, 800.0, 600.0),
        logical(0.0, f32::INFINITY, 800.0, 600.0),
        logical(0.0, 0.0, 0.0, 600.0),
        logical(0.0, 0.0, 800.0, -1.0),
        logical(f32::MAX, 0.0, 800.0, 600.0),
        logical(0.0, 0.0, f32::INFINITY, 600.0),
        logical(2_147_483_520.0, 0.0, 1024.0, 600.0),
    ] {
        assert!(WindowsOuterWindowPlacement::new(bounds, 1.0, monitor, monitor, false).is_err());
    }
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(
            WindowsOuterWindowPlacement::new(
                logical(0.0, 0.0, 800.0, 600.0),
                scale,
                monitor,
                monitor,
                false
            )
            .is_err()
        );
    }
}

fn assert_rect(actual: RECT, expected: Bounds<DevicePixels>) {
    assert_eq!(
        (actual.left, actual.top, actual.right, actual.bottom),
        (
            expected.origin.x.0,
            expected.origin.y.0,
            expected.origin.x.0 + expected.size.width.0,
            expected.origin.y.0 + expected.size.height.0,
        )
    );
}

fn exercise_native(cx: &mut App, monitor: WindowsWindowPlacementMonitor, maximized: bool) -> usize {
    let work = monitor.work_area();
    let scale = monitor.scale_factor();
    let width = work.size.width.0.min(900);
    let height = work.size.height.0.min(650);
    let x = work.origin.x.0 + (work.size.width.0 - width) / 2;
    let y = work.origin.y.0 + (work.size.height.0 - height) / 2;
    let bounds = logical(
        x as f32 / scale,
        y as f32 / scale,
        width as f32 / scale,
        height as f32 / scale,
    );
    let expected = monitor.outer_placement(bounds, false).unwrap();
    let title = format!(
        "Beryl native placement evidence {} {maximized}",
        std::process::id()
    );
    let title_wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let foreground = unsafe { GetForegroundWindow() };
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(if maximized {
                    WindowBounds::Maximized(bounds)
                } else {
                    WindowBounds::Windowed(bounds)
                }),
                windows_outer_bounds_monitor: Some(monitor),
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
    let hwnd = unsafe { FindWindowW(None, PCWSTR(title_wide.as_ptr())) }.unwrap();
    assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    window
        .update(cx, |_, window, _| {
            assert_eq!(window.scale_factor(), scale);
            assert!(window.capture_windows_window_placement().is_err());
        })
        .unwrap();
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.unwrap();
    if !maximized {
        assert_rect(rect, expected.screen_bounds());
    }
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe { GetWindowPlacement(hwnd, &mut placement) }.unwrap();
    assert_rect(placement.rcNormalPosition, expected.workspace_bounds());
    window
        .update(cx, |_, window, cx| window.publish(cx))
        .unwrap()
        .unwrap();
    assert!(unsafe { IsWindowVisible(hwnd) }.as_bool());
    assert_eq!(unsafe { IsZoomed(hwnd) }.as_bool(), maximized);
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    for minimized in [false, true] {
        if minimized {
            let _ = unsafe { ShowWindow(hwnd, SW_SHOWMINNOACTIVE) };
        }
        window
            .update(cx, |_, window, _| {
                let captured = window.capture_windows_window_placement().unwrap();
                assert_eq!(captured.normal_outer_bounds(), expected.screen_bounds());
                assert_eq!(captured.maximized(), maximized);
                assert_eq!(captured.monitor(), monitor);
                for desktop in [
                    None,
                    Some(beryl_model::VirtualDesktopId::from_bytes([47; 16])),
                ] {
                    let saved = beryl_app::main_window::windows_window_placement_from_capture(
                        captured, desktop,
                    )
                    .unwrap();
                    let scale = f64::from(monitor.scale_factor());
                    let physical = expected.screen_bounds();
                    assert_eq!(
                        saved.bounds().x(),
                        (f64::from(physical.origin.x.0) / scale).round() as i32
                    );
                    assert_eq!(
                        saved.bounds().y(),
                        (f64::from(physical.origin.y.0) / scale).round() as i32
                    );
                    assert_eq!(
                        saved.bounds().width(),
                        (f64::from(physical.size.width.0) / scale).round() as u32
                    );
                    assert_eq!(
                        saved.bounds().height(),
                        (f64::from(physical.size.height.0) / scale).round() as u32
                    );
                    assert_eq!(
                        saved.display_state(),
                        if maximized {
                            beryl_model::WindowDisplayState::Maximized
                        } else {
                            beryl_model::WindowDisplayState::Normal
                        }
                    );
                    let hint = saved.monitor().unwrap();
                    assert_eq!(hint.id().as_str(), monitor.uuid().to_string());
                    let work = monitor.work_area();
                    assert_eq!(
                        hint.work_area().x(),
                        (f64::from(work.origin.x.0) / scale).round() as i32
                    );
                    assert_eq!(
                        hint.work_area().y(),
                        (f64::from(work.origin.y.0) / scale).round() as i32
                    );
                    assert_eq!(
                        hint.work_area().width(),
                        (f64::from(work.size.width.0) / scale).round() as u32
                    );
                    assert_eq!(
                        hint.work_area().height(),
                        (f64::from(work.size.height.0) / scale).round() as u32
                    );
                    assert_eq!(saved.virtual_desktop(), desktop);
                }
            })
            .unwrap();
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    }
    window
        .update(cx, |_, window, cx| window.publish(cx))
        .unwrap()
        .unwrap();
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
    window
        .update(cx, |_, window, _| {
            assert!(window.capture_windows_window_placement().is_err());
        })
        .unwrap();
    let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
    window
        .update(cx, |_, window, _| {
            assert!(window.capture_windows_window_placement().is_ok());
            window.remove_window();
            assert!(window.capture_windows_window_placement().is_err());
        })
        .unwrap();
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    hwnd.0 as usize
}

fn reject_stale_and_dispose_failed_native(cx: &mut App, monitor: WindowsWindowPlacementMonitor) {
    let work = monitor.work_area();
    let scale = monitor.scale_factor();
    let bounds = logical(
        work.origin.x.0 as f32 / scale,
        work.origin.y.0 as f32 / scale,
        640.0,
        480.0,
    );
    let mut stale_work = work;
    stale_work.origin.x.0 += 1;
    for stale in [
        monitor.with_test_work_area(stale_work),
        monitor.with_test_dpi(1),
        monitor.with_test_native_identity(usize::MAX),
    ] {
        assert!(
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    windows_outer_bounds_monitor: Some(stale),
                    show: false,
                    focus: false,
                    ..Default::default()
                },
                |_, cx| cx.new(|_| Empty)
            )
            .is_err()
        );
    }
    let captured = Arc::new(Mutex::new(None));
    let hook_capture = captured.clone();
    let foreground = unsafe { GetForegroundWindow() };
    let failed = with_windows_window_creation_hook_for_test(
        move |raw| {
            let hwnd = windows::Win32::Foundation::HWND(raw as *mut _);
            assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
            assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
            *hook_capture.lock().unwrap() = Some(raw);
            Err(std::io::Error::other("injected failure after native allocation").into())
        },
        || {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    windows_outer_bounds_monitor: Some(monitor),
                    show: false,
                    focus: false,
                    ..Default::default()
                },
                |_, cx| cx.new(|_| Empty),
            )
        },
    );
    assert!(failed.is_err());
    let raw = captured
        .lock()
        .unwrap()
        .take()
        .expect("native allocation hook ran");
    assert!(!unsafe { IsWindow(Some(windows::Win32::Foundation::HWND(raw as *mut _))) }.as_bool());
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
}

#[test]
fn native_outer_windows_stay_hidden_until_publication_without_activation() {
    let monitor = std::thread::spawn(|| {
        let mut selected = None;
        WindowsWindowPlacementMonitor::visit(|monitor| {
            selected = Some(monitor);
            Ok(ControlFlow::Break(()))
        })
        .unwrap();
        selected.expect("native placement qualification requires a Windows monitor")
    })
    .join()
    .unwrap();
    let result = Arc::new(Mutex::new(None));
    let captured = result.clone();
    Application::new().run(move |cx| {
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let normal = exercise_native(cx, monitor, false);
            let maximized = exercise_native(cx, monitor, true);
            reject_stale_and_dispose_failed_native(cx, monitor);
            for (bounds, display_id) in [
                (None, None),
                (
                    Some(WindowBounds::Fullscreen(logical(0.0, 0.0, 800.0, 600.0))),
                    None,
                ),
                (
                    Some(WindowBounds::Windowed(logical(0.0, 0.0, 800.0, 600.0))),
                    Some(cx.primary_display().unwrap().id()),
                ),
            ] {
                assert!(
                    cx.open_window(
                        WindowOptions {
                            window_bounds: bounds,
                            display_id,
                            windows_outer_bounds_monitor: Some(monitor),
                            show: false,
                            focus: false,
                            ..Default::default()
                        },
                        |_, cx| cx.new(|_| Empty)
                    )
                    .is_err()
                );
            }
            [normal, maximized]
        }));
        *captured.lock().unwrap() = Some(outcome);
        cx.quit();
    });
    match result
        .lock()
        .unwrap()
        .take()
        .expect("native application callback ran")
    {
        Err(panic) => std::panic::resume_unwind(panic),
        Ok(handles) => {
            for raw in handles {
                assert!(
                    !unsafe { IsWindow(Some(windows::Win32::Foundation::HWND(raw as *mut _))) }
                        .as_bool()
                );
            }
        }
    }
}
