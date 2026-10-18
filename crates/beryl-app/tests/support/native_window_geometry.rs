use crate::main_window::MainWindowShellRoot;
use gpui::{AppContext, AsyncApp, Pixels, WindowHandle, px};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::Duration;
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow},
};

pub(crate) fn restore(window: WindowHandle<MainWindowShellRoot>, cx: &mut AsyncApp) {
    let handle = window
        .update(cx, |_, native, _| {
            if native.is_minimized() != Some(true) {
                return None;
            }
            let handle = native
                .window_handle()
                .expect("original native window is live");
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                panic!("expected the original Windows native handle");
            };
            Some(HWND(handle.hwnd.get() as *mut _))
        })
        .unwrap();
    if let Some(handle) = handle {
        unsafe {
            let _ = ShowWindow(handle, SW_SHOWNOACTIVATE);
        }
    }
}

pub(crate) async fn prepare_width_for_shrink(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut AsyncApp,
    shrink: Pixels,
) {
    restore(window, cx);
    let required = window
        .update(cx, |root, native, _| {
            let committed = root.controller().unwrap().minimum_size();
            let (protection, _) = root.test_minimum_update_status();
            let minimum = committed.width.max(protection.width);
            let scale = native.scale_factor();
            let width = px((f32::from(minimum + shrink) * scale).ceil() / scale);
            let viewport = native.viewport_size();
            if viewport.width < width {
                native.resize(gpui::size(width, viewport.height));
            }
            width
        })
        .unwrap();
    for _ in 0..100 {
        cx.update(|app| {
            app.update_window(window.into(), |_, native, app| {
                native.draw_and_present_for_test(app)
            })
            .unwrap();
        })
        .unwrap();
        let ready = window
            .update(cx, |root, native, _| {
                let (protection, pending) = root.test_minimum_update_status();
                let committed = root.controller().unwrap().minimum_size();
                native.is_minimized() == Some(false)
                    && native.viewport_size().width >= required
                    && native.viewport_size().width - shrink
                        >= committed.width.max(protection.width)
                    && !pending
            })
            .unwrap();
        if ready {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    panic!("original native window did not establish adequate resize headroom");
}
