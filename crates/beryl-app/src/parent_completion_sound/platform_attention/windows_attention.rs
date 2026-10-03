use std::{
    mem::size_of,
    ptr::addr_of,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

use windows::{
    Win32::{
        Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::{
            LibraryLoader::GetModuleHandleW,
            Power::{
                HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification,
                UnregisterPowerSettingNotification,
            },
            RemoteDesktop::{
                NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
                WTSUnRegisterSessionNotification,
            },
            SystemInformation::GetTickCount,
            SystemServices::{GUID_LIDSWITCH_STATE_CHANGE, GUID_SESSION_DISPLAY_STATUS},
        },
        UI::{
            Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
            WindowsAndMessaging::{
                CREATESTRUCTW, CreateWindowExW, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW,
                DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetWindowLongPtrW, HWND_MESSAGE,
                MSG, PBT_POWERSETTINGCHANGE, PM_REMOVE, PeekMessageW, RegisterClassW,
                SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WM_NCCREATE, WM_NCDESTROY,
                WM_POWERBROADCAST, WM_QUIT, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_OVERLAPPED,
            },
        },
    },
    core::{PCWSTR, w},
};

use super::{
    AttentionTriggerState, LOCAL_INPUT_IDLE_THRESHOLD, MessageAttentionState,
    PlatformAttentionState, PowerSettingKind, idle_trigger_state_from_ticks,
    power_setting_attention_state, session_lock_attention_state,
};

#[derive(Debug)]
pub(super) struct WindowsPlatformAttentionMonitor {
    message_state: Arc<Mutex<MessageAttentionState>>,
    worker: Option<MessageWindowWorker>,
}

impl WindowsPlatformAttentionMonitor {
    pub(super) fn reader(&self) -> WindowsAttentionReader {
        WindowsAttentionReader {
            message_state: self.message_state.clone(),
        }
    }

    pub(super) fn close(&self) {
        if let Some(worker) = &self.worker {
            worker.request_stop();
        }
    }

    pub(super) fn take_worker(&mut self) -> Option<JoinHandle<()>> {
        self.close();
        self.worker.take().and_then(|mut worker| worker.join.take())
    }
    #[cfg(test)]
    pub(super) fn is_finished(&self) -> bool {
        self.worker.is_none()
    }
    pub(super) fn spawn() -> Self {
        let message_state = Arc::new(Mutex::new(MessageAttentionState::default()));
        let worker = MessageWindowWorker::spawn(message_state.clone());
        Self {
            message_state,
            worker: Some(worker),
        }
    }

    pub(super) fn snapshot(&self) -> PlatformAttentionState {
        let message_state = self
            .message_state
            .lock()
            .map(|state| *state)
            .unwrap_or_else(|_| MessageAttentionState::default());
        PlatformAttentionState {
            local_input_idle: local_input_idle_state(),
            session_locked: message_state.session_locked,
            lid_closed: message_state.lid_closed,
            display_inactive: message_state.display_inactive,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct WindowsAttentionReader {
    message_state: Arc<Mutex<MessageAttentionState>>,
}

impl WindowsAttentionReader {
    pub(super) fn snapshot(&self) -> PlatformAttentionState {
        let state = self
            .message_state
            .lock()
            .map(|state| *state)
            .unwrap_or_else(|_| MessageAttentionState::default());
        PlatformAttentionState {
            local_input_idle: local_input_idle_state(),
            session_locked: state.session_locked,
            lid_closed: state.lid_closed,
            display_inactive: state.display_inactive,
        }
    }
}

impl Drop for WindowsPlatformAttentionMonitor {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.stop();
        }
    }
}

fn local_input_idle_state() -> AttentionTriggerState {
    let mut last_input = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    let read = unsafe { GetLastInputInfo(&mut last_input).as_bool() };
    if !read {
        return AttentionTriggerState::Unknown;
    }
    let current_tick = unsafe { GetTickCount() };
    idle_trigger_state_from_ticks(current_tick, last_input.dwTime, LOCAL_INPUT_IDLE_THRESHOLD)
}

#[derive(Debug)]
struct MessageWindowWorker {
    stop_requested: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl MessageWindowWorker {
    fn spawn(message_state: Arc<Mutex<MessageAttentionState>>) -> Self {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let thread_stop_requested = stop_requested.clone();
        let thread_state = message_state.clone();
        let join = thread::Builder::new()
            .name("desktop-attention".into())
            .spawn(move || {
                run_message_window_worker(thread_state, thread_stop_requested);
            })
            .map_err(|error| {
                tracing::warn!(%error, "desktop attention worker unavailable");
                mark_message_notifications_unsupported(&message_state);
            })
            .ok();

        Self {
            stop_requested,
            join,
        }
    }

    fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
    }

    fn stop(mut self) {
        self.request_stop();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct WindowContext {
    message_state: Arc<Mutex<MessageAttentionState>>,
    #[cfg(test)]
    reject_creation: bool,
}

fn run_message_window_worker(
    message_state: Arc<Mutex<MessageAttentionState>>,
    stop_requested: Arc<AtomicBool>,
) {
    let Some((hwnd, mut registrations)) = create_message_window(message_state.clone()) else {
        mark_message_notifications_unsupported(&message_state);
        return;
    };

    if stop_requested.load(Ordering::Acquire) {
        cleanup_message_window(hwnd, &mut registrations);
        return;
    }

    let mut message = MSG::default();
    loop {
        if stop_requested.load(Ordering::Acquire) {
            break;
        }
        if unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() } {
            if message.message == WM_QUIT {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        } else {
            thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    cleanup_message_window(hwnd, &mut registrations);
}

fn create_message_window(
    message_state: Arc<Mutex<MessageAttentionState>>,
) -> Option<(HWND, MessageWindowRegistrations)> {
    create_message_window_with_context(Arc::new(WindowContext {
        message_state,
        #[cfg(test)]
        reject_creation: false,
    }))
}

fn create_message_window_with_context(
    context: Arc<WindowContext>,
) -> Option<(HWND, MessageWindowRegistrations)> {
    let hmodule = unsafe { GetModuleHandleW(PCWSTR::null()).ok()? };
    let hinstance = HINSTANCE(hmodule.0);
    let class_name = w!("BerylPlatformAttentionMessageWindow");
    let class = WNDCLASSW {
        lpfnWndProc: Some(message_window_proc),
        hInstance: hinstance,
        lpszClassName: class_name,
        ..Default::default()
    };
    unsafe {
        let _ = RegisterClassW(&class);
    }

    let context_ptr = Arc::as_ptr(&context);
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!(""),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            Some(context_ptr.cast()),
        )
    };

    let hwnd = match hwnd {
        Ok(hwnd) => hwnd,
        Err(_) => return None,
    };

    let registrations = register_message_notifications(hwnd);
    if let Some(context) = window_context(hwnd) {
        if let Ok(mut state) = context.message_state.lock() {
            if !registrations.session_registered {
                state.session_locked = AttentionTriggerState::Unsupported;
            }
            if registrations.lid_registration.is_none() {
                state.lid_closed = AttentionTriggerState::Unsupported;
            }
            if registrations.display_registration.is_none() {
                state.display_inactive = AttentionTriggerState::Unsupported;
            }
        }
    }

    Some((hwnd, registrations))
}

#[derive(Debug)]
struct MessageWindowRegistrations {
    session_registered: bool,
    lid_registration: Option<HPOWERNOTIFY>,
    display_registration: Option<HPOWERNOTIFY>,
}

fn register_message_notifications(hwnd: HWND) -> MessageWindowRegistrations {
    let session_registered =
        unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION).is_ok() };
    let recipient = HANDLE(hwnd.0);
    let lid_registration = unsafe {
        RegisterPowerSettingNotification(
            recipient,
            &GUID_LIDSWITCH_STATE_CHANGE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
        .ok()
    };
    let display_registration = unsafe {
        RegisterPowerSettingNotification(
            recipient,
            &GUID_SESSION_DISPLAY_STATUS,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
        .ok()
    };

    MessageWindowRegistrations {
        session_registered,
        lid_registration,
        display_registration,
    }
}

fn cleanup_message_window(hwnd: HWND, registrations: &mut MessageWindowRegistrations) {
    if let Some(registration) = registrations.lid_registration.take() {
        let _ = unsafe { UnregisterPowerSettingNotification(registration) };
    }
    if let Some(registration) = registrations.display_registration.take() {
        let _ = unsafe { UnregisterPowerSettingNotification(registration) };
    }
    if registrations.session_registered {
        let _ = unsafe { WTSUnRegisterSessionNotification(hwnd) };
        registrations.session_registered = false;
    }
    let _ = unsafe { DestroyWindow(hwnd) };
}

fn mark_message_notifications_unsupported(message_state: &Arc<Mutex<MessageAttentionState>>) {
    if let Ok(mut state) = message_state.lock() {
        state.session_locked = AttentionTriggerState::Unsupported;
        state.lid_closed = AttentionTriggerState::Unsupported;
        state.display_inactive = AttentionTriggerState::Unsupported;
    }
}

unsafe extern "system" fn message_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create_struct = lparam.0 as *const CREATESTRUCTW;
        if !create_struct.is_null() {
            let context = unsafe { (*create_struct).lpCreateParams as *mut WindowContext };
            if !context.is_null() {
                unsafe {
                    Arc::increment_strong_count(context);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, context as isize);
                }
                return LRESULT(1);
            }
        }
        return LRESULT(0);
    }

    #[cfg(test)]
    if message == windows::Win32::UI::WindowsAndMessaging::WM_CREATE
        && window_context(hwnd).is_some_and(|context| context.reject_creation)
    {
        return LRESULT(-1);
    }

    match message {
        WM_WTSSESSION_CHANGE => {
            update_session_lock_state(hwnd, wparam.0 as u32);
            LRESULT(0)
        }
        WM_POWERBROADCAST if wparam.0 as u32 == PBT_POWERSETTINGCHANGE => {
            update_power_setting_state(hwnd, lparam);
            LRESULT(1)
        }
        WM_NCDESTROY => {
            let context = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if context != 0 {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    drop(Arc::from_raw(context as *const WindowContext));
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
pub(super) fn test_rejected_creation_balances_context() -> bool {
    let context = Arc::new(WindowContext {
        message_state: Arc::new(Mutex::new(MessageAttentionState::default())),
        reject_creation: true,
    });
    assert!(create_message_window_with_context(context.clone()).is_none());
    Arc::strong_count(&context) == 1
}

#[cfg(test)]
pub(super) fn test_stopped_before_start() {
    run_message_window_worker(
        Arc::new(Mutex::new(MessageAttentionState::default())),
        Arc::new(AtomicBool::new(true)),
    );
}

fn update_session_lock_state(hwnd: HWND, event: u32) {
    let Some(state) = session_lock_attention_state(event) else {
        return;
    };
    let Some(context) = window_context(hwnd) else {
        return;
    };
    if let Ok(mut message_state) = context.message_state.lock() {
        message_state.session_locked = state;
    }
}

fn update_power_setting_state(hwnd: HWND, lparam: LPARAM) {
    let Some((kind, state)) = (unsafe { power_setting_from_lparam(lparam) }) else {
        return;
    };
    let Some(context) = window_context(hwnd) else {
        return;
    };
    if let Ok(mut message_state) = context.message_state.lock() {
        match kind {
            PowerSettingKind::LidSwitch => message_state.lid_closed = state,
            PowerSettingKind::SessionDisplay => message_state.display_inactive = state,
        }
    }
}

unsafe fn power_setting_from_lparam(
    lparam: LPARAM,
) -> Option<(PowerSettingKind, AttentionTriggerState)> {
    let setting = lparam.0 as *const POWERBROADCAST_SETTING;
    if setting.is_null() {
        return None;
    }
    let setting = unsafe { &*setting };
    let kind = if setting.PowerSetting == GUID_LIDSWITCH_STATE_CHANGE {
        PowerSettingKind::LidSwitch
    } else if setting.PowerSetting == GUID_SESSION_DISPLAY_STATUS {
        PowerSettingKind::SessionDisplay
    } else {
        return None;
    };

    let data_len = usize::try_from(setting.DataLength).ok()?;
    let data_ptr = addr_of!(setting.Data).cast::<u8>();
    let data = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
    Some((kind, power_setting_attention_state(kind, data)))
}

fn window_context(hwnd: HWND) -> Option<&'static WindowContext> {
    let context = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if context == 0 {
        None
    } else {
        Some(unsafe { &*(context as *const WindowContext) })
    }
}
