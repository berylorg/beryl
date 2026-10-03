use std::time::Duration;

use super::{AttentionTriggerState, PlatformAttentionState};

const LOCAL_INPUT_IDLE_THRESHOLD: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub(super) struct PlatformAttentionMonitor {
    #[cfg(target_os = "windows")]
    windows: WindowsPlatformAttentionMonitor,
}

impl PlatformAttentionMonitor {
    pub(super) fn reader(&self) -> PlatformAttentionReader {
        PlatformAttentionReader {
            windows: self.windows.reader(),
        }
    }

    pub(super) fn close(&self) {
        self.windows.close();
    }

    pub(super) fn take_worker(&mut self) -> Option<std::thread::JoinHandle<()>> {
        self.windows.take_worker()
    }
    #[cfg(test)]
    pub(super) fn is_finished(&self) -> bool {
        self.windows.is_finished()
    }
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

#[derive(Clone, Debug)]
pub(super) struct PlatformAttentionReader {
    windows: windows_attention::WindowsAttentionReader,
}

impl PlatformAttentionReader {
    pub(super) fn snapshot(&self) -> PlatformAttentionState {
        self.windows.snapshot()
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
#[path = "platform_attention/windows_attention.rs"]
mod windows_attention;

#[cfg(target_os = "windows")]
use windows_attention::WindowsPlatformAttentionMonitor;

#[cfg(test)]
pub(super) fn test_rejected_creation_balances_context() -> bool {
    windows_attention::test_rejected_creation_balances_context()
}

#[cfg(test)]
pub(super) fn test_stopped_before_start() {
    windows_attention::test_stopped_before_start();
}
