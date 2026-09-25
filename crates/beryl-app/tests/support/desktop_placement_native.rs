use gpui::{App, AppContext, AsyncApp, Empty, TitlebarOptions, WindowHandle, WindowOptions};
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        System::Com::{
            CLSCTX_ALL, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED, CoCreateInstance,
            CoInitializeEx, CoUninitialize,
        },
        UI::{
            Shell::{IVirtualDesktopManager, VirtualDesktopManager},
            WindowsAndMessaging::{EnumWindows, FindWindowW, IsWindow, IsWindowVisible},
        },
    },
    core::{BOOL, GUID, PCWSTR},
};

pub struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

pub fn with_manager<T>(f: impl FnOnce(&IVirtualDesktopManager) -> T) -> Result<T, i32> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(|error| error.code().0)?;
    let _apartment = Apartment;
    let manager: IVirtualDesktopManager =
        unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL) }
            .map_err(|error| error.code().0)?;
    Ok(f(&manager))
}

pub fn hwnd(raw: usize) -> HWND {
    HWND(raw as *mut _)
}

pub fn alive(raw: usize) -> bool {
    unsafe { IsWindow(Some(hwnd(raw))) }.as_bool()
}

pub fn visible(raw: usize) -> bool {
    unsafe { IsWindowVisible(hwnd(raw)) }.as_bool()
}

#[derive(Debug)]
pub struct DesktopFacts {
    pub desktop: Result<GUID, i32>,
    pub current: Result<bool, i32>,
}

pub fn facts(manager: &IVirtualDesktopManager, raw: usize) -> DesktopFacts {
    DesktopFacts {
        desktop: unsafe { manager.GetWindowDesktopId(hwnd(raw)) }.map_err(|error| error.code().0),
        current: unsafe { manager.IsWindowOnCurrentVirtualDesktop(hwnd(raw)) }
            .map(|value| value.as_bool())
            .map_err(|error| error.code().0),
    }
}

struct DesktopSearch<'a> {
    manager: &'a IVirtualDesktopManager,
    current: GUID,
    alternate: Option<GUID>,
    visited: usize,
}

unsafe extern "system" fn find_alternate(window: HWND, parameter: LPARAM) -> BOOL {
    let search = unsafe { &mut *(parameter.0 as *mut DesktopSearch<'_>) };
    search.visited += 1;
    if let Ok(desktop) = unsafe { search.manager.GetWindowDesktopId(window) } {
        if desktop != GUID::zeroed()
            && desktop != search.current
            && unsafe { search.manager.IsWindowOnCurrentVirtualDesktop(window) }
                .is_ok_and(|current| !current.as_bool())
        {
            search.alternate = Some(desktop);
            return BOOL(0);
        }
    }
    BOOL(i32::from(search.visited < 256))
}

pub fn desktops(control: usize) -> Result<(GUID, Option<GUID>), i32> {
    with_manager(|manager| {
        let current =
            unsafe { manager.GetWindowDesktopId(hwnd(control)) }.map_err(|error| error.code().0)?;
        let mut search = DesktopSearch {
            manager,
            current,
            alternate: None,
            visited: 0,
        };
        let _ = unsafe {
            EnumWindows(
                Some(find_alternate),
                LPARAM(&mut search as *mut DesktopSearch<'_> as isize),
            )
        };
        Ok((current, search.alternate))
    })?
}

pub fn open(cx: &mut App, name: &str) -> (WindowHandle<Empty>, usize) {
    let title = format!("Beryl desktop worker {} {name}", std::process::id());
    let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let window = cx
        .open_window(
            WindowOptions {
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
    let raw = unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }
        .unwrap()
        .0 as usize;
    (window, raw)
}

pub async fn pump(cx: &AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(15))
        .await;
}

pub async fn dispose(cx: &mut AsyncApp, window: WindowHandle<Empty>, raw: usize) {
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(raw) {
        assert!(
            Instant::now() < deadline,
            "owned native disposal did not settle"
        );
        pump(cx).await;
    }
}
