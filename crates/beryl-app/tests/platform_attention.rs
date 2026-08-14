#![allow(dead_code)]

#[path = "../src/shell/notification_policy.rs"]
mod notification_policy;
#[path = "../src/shell/notifications.rs"]
mod notifications;
#[path = "../src/shell/platform_attention.rs"]
mod platform_attention;

use std::time::Duration;

use notification_policy::AttentionTriggerState;
use platform_attention::{
    PowerSettingKind, idle_trigger_state_from_ticks, message_registration_state,
    power_setting_attention_state, session_lock_attention_state,
};

#[test]
fn idle_tick_math_handles_active_inactive_and_wraparound() {
    let threshold = Duration::from_secs(30);

    assert_eq!(
        idle_trigger_state_from_ticks(29_999, 0, threshold),
        AttentionTriggerState::Inactive
    );
    assert_eq!(
        idle_trigger_state_from_ticks(30_000, 0, threshold),
        AttentionTriggerState::Active
    );
    assert_eq!(
        idle_trigger_state_from_ticks(20, u32::MAX - 10, threshold),
        AttentionTriggerState::Inactive
    );
    assert_eq!(
        idle_trigger_state_from_ticks(30_005, u32::MAX - 10, threshold),
        AttentionTriggerState::Active
    );
}

#[test]
fn malformed_power_data_stays_unknown() {
    assert_eq!(
        power_setting_attention_state(PowerSettingKind::LidSwitch, &[]),
        AttentionTriggerState::Unknown
    );
    assert_eq!(
        power_setting_attention_state(PowerSettingKind::SessionDisplay, &[0, 0, 0]),
        AttentionTriggerState::Unknown
    );
}

#[test]
fn lid_power_values_map_closed_open_and_unknown() {
    assert_eq!(
        power_setting_attention_state(PowerSettingKind::LidSwitch, &0u32.to_le_bytes()),
        AttentionTriggerState::Active
    );
    assert_eq!(
        power_setting_attention_state(PowerSettingKind::LidSwitch, &1u32.to_le_bytes()),
        AttentionTriggerState::Inactive
    );
    assert_eq!(
        power_setting_attention_state(PowerSettingKind::LidSwitch, &2u32.to_le_bytes()),
        AttentionTriggerState::Unknown
    );
}

#[test]
fn display_power_values_map_off_on_dim_and_unknown() {
    assert_eq!(
        power_setting_attention_state(PowerSettingKind::SessionDisplay, &0u32.to_le_bytes()),
        AttentionTriggerState::Active
    );
    assert_eq!(
        power_setting_attention_state(PowerSettingKind::SessionDisplay, &1u32.to_le_bytes()),
        AttentionTriggerState::Inactive
    );
    assert_eq!(
        power_setting_attention_state(PowerSettingKind::SessionDisplay, &2u32.to_le_bytes()),
        AttentionTriggerState::Active
    );
    assert_eq!(
        power_setting_attention_state(PowerSettingKind::SessionDisplay, &3u32.to_le_bytes()),
        AttentionTriggerState::Unknown
    );
}

#[test]
fn wts_lock_unlock_events_update_session_attention() {
    assert_eq!(
        session_lock_attention_state(7),
        Some(AttentionTriggerState::Active)
    );
    assert_eq!(
        session_lock_attention_state(8),
        Some(AttentionTriggerState::Inactive)
    );
    assert_eq!(session_lock_attention_state(5), None);
}

#[test]
fn registration_failures_are_represented_as_unsupported() {
    let state = message_registration_state(false, true, false);

    assert_eq!(state.session_locked, AttentionTriggerState::Unsupported);
    assert_eq!(state.lid_closed, AttentionTriggerState::Unknown);
    assert_eq!(state.display_inactive, AttentionTriggerState::Unsupported);
}

#[cfg(not(target_os = "windows"))]
#[test]
fn non_windows_monitor_reports_unsupported_without_active_triggers() {
    let monitor = platform_attention::PlatformAttentionMonitor::spawn();
    let snapshot = monitor.snapshot();

    assert_eq!(
        snapshot.local_input_idle,
        AttentionTriggerState::Unsupported
    );
    assert_eq!(snapshot.session_locked, AttentionTriggerState::Unsupported);
    assert_eq!(snapshot.lid_closed, AttentionTriggerState::Unsupported);
    assert_eq!(
        snapshot.display_inactive,
        AttentionTriggerState::Unsupported
    );
}
