use std::time::Duration;

use super::notification_policy::{AttentionTriggerState, PlatformAttentionState};

const LOCAL_INPUT_IDLE_THRESHOLD: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub(super) struct PlatformAttentionMonitor {
    #[cfg(target_os = "windows")]
    windows: WindowsPlatformAttentionMonitor,
}

impl PlatformAttentionMonitor {
    pub(super) fn spawn() -> Self {
        #[cfg(target_os = "windows")]
        {
            return Self {
                windows: WindowsPlatformAttentionMonitor::spawn(),
            };
        }

        #[cfg(not(target_os = "windows"))]
        {
            Self {}
        }
    }

    pub(super) fn snapshot(&self) -> PlatformAttentionState {
        #[cfg(target_os = "windows")]
        {
            return self.windows.snapshot();
        }

        #[cfg(not(target_os = "windows"))]
        {
            unsupported_platform_attention_state()
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn unsupported_platform_attention_state() -> PlatformAttentionState {
    PlatformAttentionState {
        local_input_idle: AttentionTriggerState::Unsupported,
        session_locked: AttentionTriggerState::Unsupported,
        lid_closed: AttentionTriggerState::Unsupported,
        display_inactive: AttentionTriggerState::Unsupported,
    }
}

pub(super) fn idle_trigger_state_from_ticks(
    current_tick: u32,
    last_input_tick: u32,
    threshold: Duration,
) -> AttentionTriggerState {
    let elapsed = current_tick.wrapping_sub(last_input_tick);
    let threshold_millis = threshold.as_millis().min(u128::from(u32::MAX)) as u32;
    if elapsed >= threshold_millis {
        AttentionTriggerState::Active
    } else {
        AttentionTriggerState::Inactive
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PowerSettingKind {
    LidSwitch,
    SessionDisplay,
}

pub(super) fn power_setting_attention_state(
    kind: PowerSettingKind,
    data: &[u8],
) -> AttentionTriggerState {
    let Some(value) = dword_from_power_setting_data(data) else {
        return AttentionTriggerState::Unknown;
    };

    match kind {
        PowerSettingKind::LidSwitch => lid_switch_attention_state(value),
        PowerSettingKind::SessionDisplay => session_display_attention_state(value),
    }
}

fn dword_from_power_setting_data(data: &[u8]) -> Option<u32> {
    let bytes: [u8; 4] = data.get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn lid_switch_attention_state(value: u32) -> AttentionTriggerState {
    match value {
        0 => AttentionTriggerState::Active,
        1 => AttentionTriggerState::Inactive,
        _ => AttentionTriggerState::Unknown,
    }
}

fn session_display_attention_state(value: u32) -> AttentionTriggerState {
    match value {
        0 | 2 => AttentionTriggerState::Active,
        1 => AttentionTriggerState::Inactive,
        _ => AttentionTriggerState::Unknown,
    }
}

pub(super) fn session_lock_attention_state(event: u32) -> Option<AttentionTriggerState> {
    match event {
        WTS_SESSION_LOCK_EVENT => Some(AttentionTriggerState::Active),
        WTS_SESSION_UNLOCK_EVENT => Some(AttentionTriggerState::Inactive),
        _ => None,
    }
}

#[cfg(test)]
pub(super) fn message_registration_state(
    session_notifications_supported: bool,
    lid_notifications_supported: bool,
    display_notifications_supported: bool,
) -> MessageAttentionState {
    MessageAttentionState {
        session_locked: registration_initial_state(session_notifications_supported),
        lid_closed: registration_initial_state(lid_notifications_supported),
        display_inactive: registration_initial_state(display_notifications_supported),
    }
}

#[cfg(test)]
fn registration_initial_state(supported: bool) -> AttentionTriggerState {
    if supported {
        AttentionTriggerState::Unknown
    } else {
        AttentionTriggerState::Unsupported
    }
}

#[cfg(target_os = "windows")]
const WTS_SESSION_LOCK_EVENT: u32 = windows::Win32::UI::WindowsAndMessaging::WTS_SESSION_LOCK;
#[cfg(not(target_os = "windows"))]
const WTS_SESSION_LOCK_EVENT: u32 = 7;

#[cfg(target_os = "windows")]
const WTS_SESSION_UNLOCK_EVENT: u32 = windows::Win32::UI::WindowsAndMessaging::WTS_SESSION_UNLOCK;
#[cfg(not(target_os = "windows"))]
const WTS_SESSION_UNLOCK_EVENT: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MessageAttentionState {
    pub(super) session_locked: AttentionTriggerState,
    pub(super) lid_closed: AttentionTriggerState,
    pub(super) display_inactive: AttentionTriggerState,
}

impl Default for MessageAttentionState {
    fn default() -> Self {
        Self {
            session_locked: AttentionTriggerState::Unknown,
            lid_closed: AttentionTriggerState::Unknown,
            display_inactive: AttentionTriggerState::Unknown,
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_attention {
    use std::{
        mem::size_of,
        ptr::addr_of,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicIsize, Ordering},
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
                    DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW,
                    HWND_MESSAGE, MSG, PBT_POWERSETTINGCHANGE, PostMessageW, RegisterClassW,
                    SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WM_NCCREATE,
                    WM_NCDESTROY, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSW,
                    WS_OVERLAPPED,
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

    const STOP_MESSAGE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 91;

    #[derive(Debug)]
    pub(super) struct WindowsPlatformAttentionMonitor {
        message_state: Arc<Mutex<MessageAttentionState>>,
        worker: Option<MessageWindowWorker>,
    }

    impl WindowsPlatformAttentionMonitor {
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
        hwnd: Arc<AtomicIsize>,
        stop_requested: Arc<AtomicBool>,
        join: Option<JoinHandle<()>>,
    }

    impl MessageWindowWorker {
        fn spawn(message_state: Arc<Mutex<MessageAttentionState>>) -> Self {
            let hwnd = Arc::new(AtomicIsize::new(0));
            let stop_requested = Arc::new(AtomicBool::new(false));
            let thread_hwnd = hwnd.clone();
            let thread_stop_requested = stop_requested.clone();
            let join = thread::spawn(move || {
                run_message_window_worker(message_state, thread_hwnd, thread_stop_requested);
            });

            Self {
                hwnd,
                stop_requested,
                join: Some(join),
            }
        }

        fn stop(mut self) {
            self.stop_requested.store(true, Ordering::Release);
            let hwnd = self.hwnd.load(Ordering::Acquire);
            if hwnd != 0 {
                let _ = unsafe {
                    PostMessageW(
                        Some(HWND(hwnd as *mut _)),
                        STOP_MESSAGE,
                        WPARAM(0),
                        LPARAM(0),
                    )
                };
            }

            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    struct WindowContext {
        message_state: Arc<Mutex<MessageAttentionState>>,
    }

    fn run_message_window_worker(
        message_state: Arc<Mutex<MessageAttentionState>>,
        hwnd_slot: Arc<AtomicIsize>,
        stop_requested: Arc<AtomicBool>,
    ) {
        let Some((hwnd, mut registrations)) = create_message_window(message_state.clone()) else {
            mark_message_notifications_unsupported(&message_state);
            return;
        };

        hwnd_slot.store(hwnd.0 as isize, Ordering::Release);
        if stop_requested.load(Ordering::Acquire) {
            cleanup_message_window(hwnd, &mut registrations);
            hwnd_slot.store(0, Ordering::Release);
            return;
        }

        let mut message = MSG::default();
        loop {
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
            if result.0 <= 0 {
                break;
            }
            if message.hwnd == hwnd && message.message == STOP_MESSAGE {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        cleanup_message_window(hwnd, &mut registrations);
        hwnd_slot.store(0, Ordering::Release);
    }

    fn create_message_window(
        message_state: Arc<Mutex<MessageAttentionState>>,
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

        let context = Box::new(WindowContext { message_state });
        let context_ptr = Box::into_raw(context);
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
            Err(_) => {
                unsafe {
                    drop(Box::from_raw(context_ptr));
                }
                return None;
            }
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
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, context as isize);
                    }
                    return LRESULT(1);
                }
            }
            return LRESULT(0);
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
                        drop(Box::from_raw(context as *mut WindowContext));
                    }
                }
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
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
}

#[cfg(target_os = "windows")]
use windows_attention::WindowsPlatformAttentionMonitor;
