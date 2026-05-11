#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

pub use beryl_app::{
    AgentPreferences, AppearanceButtonSettings, AppearanceButtonStateSettings,
    AppearanceForegroundSettings, AppearanceInputSettings, AppearanceRoleSettings,
    AppearanceSettings, AppearanceSettingsStore, AppearanceStatusLineSettings,
    AppearanceSurfaceSettings, AppearanceTranscriptShellSettings, GuiPreferences,
    GuiPreferencesStore, NotificationPreferences, NotificationSoundPathError,
    normalize_developer_instructions_text, parse_notification_sound_path_text,
    validate_notification_sound_path,
};

#[allow(dead_code)]
#[path = "../src/shell/settings.rs"]
mod settings;

#[test]
fn settings_apply_persists_developer_instructions_preference() {
    let (mut state, _appearance, preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let instructions = "Use subagents for independent review.\nKeep architecture clean.";

    state.set_developer_instructions(instructions.to_string());
    assert_eq!(
        preferences
            .lock()
            .unwrap()
            .agent
            .developer_instructions
            .as_deref(),
        None,
        "developer-instructions edits must not live-preview into active preferences"
    );

    assert!(state.apply());
    assert_eq!(
        preferences
            .lock()
            .unwrap()
            .agent
            .developer_instructions
            .as_deref(),
        Some(instructions)
    );
    wait_for_save(&mut state);

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(
        loaded_preferences.agent.developer_instructions.as_deref(),
        Some(instructions)
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_persists_blank_developer_instructions_as_disabled() {
    let (mut state, _appearance, preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());

    state.set_developer_instructions(" \n\t ".to_string());

    assert!(state.apply());
    assert_eq!(
        preferences.lock().unwrap().agent.developer_instructions,
        None
    );
    wait_for_save(&mut state);

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(loaded_preferences.agent.developer_instructions, None);
    cleanup_temp_dir(root);
}

fn settings_state_with_temp_store(
    settings_value: AppearanceSettings,
) -> (
    settings::SettingsState,
    Arc<Mutex<AppearanceSettings>>,
    Arc<Mutex<GuiPreferences>>,
    tempdir_support::TestTempDir,
) {
    let root = unique_temp_dir();
    let shared_appearance = Arc::new(Mutex::new(settings_value));
    let shared_preferences = Arc::new(Mutex::new(GuiPreferences::default()));
    let state = settings::SettingsState::new_with_stores(
        shared_appearance.clone(),
        AppearanceSettingsStore::new(&root),
        shared_preferences.clone(),
        GuiPreferencesStore::new(&root),
    );
    (state, shared_appearance, shared_preferences, root)
}

fn wait_for_save(state: &mut settings::SettingsState) {
    for _ in 0..100 {
        match state.poll_save() {
            settings::SettingsSavePoll::Saved => return,
            settings::SettingsSavePoll::Pending => thread::sleep(Duration::from_millis(10)),
            settings::SettingsSavePoll::Idle => panic!("settings save should be pending"),
            settings::SettingsSavePoll::Failed(error) => panic!("settings save failed: {error}"),
        }
    }

    panic!("timed out waiting for settings save");
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-developer-instructions-settings-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    root.close().unwrap();
}
