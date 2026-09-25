#![cfg(target_os = "windows")]

use gpui::{
    App, AppContext, Application, AsyncApp, Empty, TitlebarOptions, WindowHandle, WindowOptions,
    WindowsHiddenWindowLease, with_windows_window_destruction_observer_for_test,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        System::Com::{
            CLSCTX_ALL, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED, CoCreateInstance,
            CoInitializeEx, CoUninitialize,
        },
        UI::{
            Shell::{IVirtualDesktopManager, VirtualDesktopManager},
            WindowsAndMessaging::{
                EnumWindows, FindWindowW, GetForegroundWindow, IsWindow, IsWindowVisible,
            },
        },
    },
    core::{BOOL, GUID, PCWSTR},
};

type NativeResult<T> = Result<T, u32>;

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[derive(Debug)]
struct ComResult<T> {
    initialize: u32,
    create: Option<u32>,
    value: Option<T>,
}

fn with_manager<T>(f: impl FnOnce(&IVirtualDesktopManager) -> T) -> ComResult<T> {
    let initialize = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED | COINIT_DISABLE_OLE1DDE) };
    if initialize.is_err() {
        return ComResult {
            initialize: initialize.0 as u32,
            create: None,
            value: None,
        };
    }
    let _apartment = Apartment;
    let manager: windows::core::Result<IVirtualDesktopManager> =
        unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL) };
    match manager {
        Ok(manager) => ComResult {
            initialize: initialize.0 as u32,
            create: Some(0),
            value: Some(f(&manager)),
        },
        Err(error) => ComResult {
            initialize: initialize.0 as u32,
            create: Some(error.code().0 as u32),
            value: None,
        },
    }
}

fn hwnd(raw: usize) -> HWND {
    HWND(raw as *mut _)
}

fn alive(raw: usize) -> bool {
    unsafe { IsWindow(Some(hwnd(raw))) }.as_bool()
}

fn visible(raw: usize) -> bool {
    unsafe { IsWindowVisible(hwnd(raw)) }.as_bool()
}

#[derive(Debug)]
struct DesktopFacts {
    desktop: NativeResult<GUID>,
    current: NativeResult<bool>,
}

fn facts(manager: &IVirtualDesktopManager, raw: usize) -> DesktopFacts {
    DesktopFacts {
        desktop: unsafe { manager.GetWindowDesktopId(hwnd(raw)) }
            .map_err(|error| error.code().0 as u32),
        current: unsafe { manager.IsWindowOnCurrentVirtualDesktop(hwnd(raw)) }
            .map(|value| value.as_bool())
            .map_err(|error| error.code().0 as u32),
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

#[derive(Debug)]
struct DesktopSources {
    control: DesktopFacts,
    alternate: Option<GUID>,
    visited: usize,
    enumeration: Option<u32>,
}

fn sources(control: usize) -> ComResult<DesktopSources> {
    with_manager(|manager| {
        let control = facts(manager, control);
        let mut alternate = None;
        let mut visited = 0;
        let mut enumeration = None;
        if let Ok(current) = control.desktop {
            if current != GUID::zeroed() && control.current == Ok(true) {
                let mut search = DesktopSearch {
                    manager,
                    current,
                    alternate: None,
                    visited: 0,
                };
                let result = unsafe {
                    EnumWindows(
                        Some(find_alternate),
                        LPARAM(&mut search as *mut DesktopSearch<'_> as isize),
                    )
                };
                alternate = search.alternate;
                visited = search.visited;
                if alternate.is_none() && visited < 256 {
                    enumeration = Some(result.err().map_or(0, |error| error.code().0 as u32));
                }
            }
        }
        DesktopSources {
            control,
            alternate,
            visited,
            enumeration,
        }
    })
}

#[derive(Debug)]
struct HiddenObservation {
    before: DesktopFacts,
    movement: Option<u32>,
    after: DesktopFacts,
}

fn prepare_hidden(
    token: WindowsHiddenWindowLease,
    requested: Option<GUID>,
) -> ComResult<HiddenObservation> {
    let raw = token.raw_handle();
    let result = with_manager(|manager| {
        let before = facts(manager, raw);
        let movement = requested.map(|desktop| {
            unsafe { manager.MoveWindowToDesktop(hwnd(raw), &desktop) }
                .err()
                .map_or(0, |error| error.code().0 as u32)
        });
        HiddenObservation {
            before,
            movement,
            after: facts(manager, raw),
        }
    });
    drop(token);
    result
}

fn open(cx: &mut App, name: &str) -> (WindowHandle<Empty>, usize) {
    let title = format!("Beryl desktop qualification {} {name}", std::process::id());
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

async fn pump(cx: &AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(15))
        .await;
}

async fn dispose(cx: &mut AsyncApp, window: WindowHandle<Empty>, raw: usize) {
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

async fn qualify(
    cx: &mut AsyncApp,
    name: &'static str,
    requested: Option<GUID>,
    control: usize,
    failures: &mut Vec<String>,
) -> usize {
    let (window, raw) = cx.update(|cx| open(cx, name)).unwrap();
    let foreground = unsafe { GetForegroundWindow() };
    let (token, released) = window
        .update(cx, |_, window, _| window.lease_hidden_windows_window())
        .unwrap()
        .unwrap();
    let hidden = cx
        .background_executor()
        .spawn(async move { prepare_hidden(token, requested) })
        .await;
    let settled = released.await.unwrap();
    let hidden_safe = alive(raw)
        && !visible(raw)
        && !settled.close_requested
        && !settled.native_destroyed
        && unsafe { GetForegroundWindow() } == foreground;
    if !hidden_safe {
        failures.push(format!(
            "{name}: hidden preparation changed native state or activation"
        ));
    }
    let published = window
        .update(cx, |_, window, cx| window.publish(cx))
        .unwrap();
    if published.is_err() {
        failures.push(format!("{name}: publication failed: {published:?}"));
    }
    pump(cx).await;
    // The fixture retains the published root until this read-only worker has completed.
    let shown = cx
        .background_executor()
        .spawn(
            async move { with_manager(|manager| (facts(manager, raw), facts(manager, control))) },
        )
        .await;
    let publication_safe =
        alive(raw) && visible(raw) && unsafe { GetForegroundWindow() } == foreground;
    if !publication_safe {
        failures.push(format!(
            "{name}: publication changed activation or lost the owned window"
        ));
    }
    println!(
        "desktop_qualification case={name} requested={requested:?} hidden={hidden:?} shown_target_and_control={shown:?} hidden_safe={hidden_safe} publication_safe={publication_safe}"
    );
    dispose(cx, window, raw).await;
    raw
}

#[test]
fn owned_hidden_desktop_assignment_before_first_publication() {
    let completed = Arc::new(Mutex::new(None));
    let captured = completed.clone();
    let destroyed = Arc::new(Mutex::new(Vec::new()));
    let observed = destroyed.clone();
    let gui_thread = std::thread::current().id();
    with_windows_window_destruction_observer_for_test(
        move |raw| {
            observed
                .lock()
                .unwrap()
                .push((raw, std::thread::current().id()))
        },
        || {
            Application::new().run(move |cx| {
                let (control_window, control) = open(cx, "control");
                let foreground = unsafe { GetForegroundWindow() };
                control_window.update(cx, |_, window, cx| window.publish(cx).unwrap()).unwrap();
                cx.spawn(async move |cx| {
                    let started = Instant::now();
                    let mut failures = Vec::new();
                    let mut expected = Vec::new();
                    pump(cx).await;
                    if unsafe { GetForegroundWindow() } != foreground {
                        failures.push("control publication activated the window".to_owned());
                    }
                    let source = cx.background_executor().spawn(async move { sources(control) }).await;
                    let known = source.value.as_ref().and_then(|source| {
                        source.alternate.or_else(|| source.control.desktop.ok().filter(|id| *id != GUID::zeroed()))
                    });
                    let alternate = source.value.as_ref().and_then(|source| source.alternate).is_some();
                    println!("desktop_qualification sources={source:?} alternate_available={alternate} known_case_scope={}", if alternate { "alternate_desktop" } else { "current_desktop_only_or_unavailable" });
                    if let Some(known) = known {
                        expected.push(qualify(cx, "known", Some(known), control, &mut failures).await);
                    } else {
                        println!("desktop_qualification case=known skipped=no_known_desktop");
                    }
                    let mut bytes = [0; 16];
                    getrandom::fill(&mut bytes).unwrap();
                    let nonexistent = GUID::from_u128(u128::from_be_bytes(bytes));
                    expected.push(qualify(cx, "nonexistent", Some(nonexistent), control, &mut failures).await);
                    expected.push(qualify(cx, "untouched", None, control, &mut failures).await);
                    if started.elapsed() > Duration::from_secs(30) {
                        failures.push("native qualification exceeded its 30 second budget".to_owned());
                    }
                    expected.push(control);
                    *captured.lock().unwrap() = Some((expected, failures));
                    control_window.update(cx, |_, window, _| window.remove_window()).unwrap();
                }).detach();
            });
        },
    );
    let (expected, failures) = completed
        .lock()
        .unwrap()
        .take()
        .expect("qualification completed");
    assert_eq!(
        *destroyed.lock().unwrap(),
        expected
            .iter()
            .map(|raw| (*raw, gui_thread))
            .collect::<Vec<_>>()
    );
    assert!(expected.into_iter().all(|raw| !alive(raw)));
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
