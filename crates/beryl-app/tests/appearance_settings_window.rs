#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    env,
    ffi::OsString,
    panic::{self, AssertUnwindSafe},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

pub use beryl_app::{
    AgentPreferences, AppearanceButtonSettings, AppearanceButtonStateSettings,
    AppearanceForegroundSettings, AppearanceInputSettings, AppearanceRoleSettings,
    AppearanceSettings, AppearanceSettingsStore, AppearanceStatusLineSettings,
    AppearanceSurfaceSettings, AppearanceTranscriptShellSettings, ContextCompactionTimeoutError,
    GuiPreferences, GuiPreferencesStore, NotificationPreferences, NotificationSoundPathError,
    OperationPreferences, normalize_developer_instructions_text,
    parse_context_compaction_timeout_seconds_text, parse_notification_sound_path_text,
    validate_notification_sound_path,
};
use gpui_settings_window::{
    SettingsFieldId, SettingsFieldKind, SettingsRowActionId, SettingsSectionId,
};

#[allow(dead_code)]
#[path = "../src/shell/settings.rs"]
mod settings;

#[test]
fn settings_model_maps_appearance_roles_and_color_rows() {
    let state = settings_state(AppearanceSettings::default());
    let model = state.model();

    assert_eq!(model.sections().len(), 18);
    assert_eq!(model.selected_section_id().as_str(), "general_ui");

    let foreground = model
        .row(&SettingsFieldId::from("general_ui.foreground"))
        .expect("general UI foreground row should exist");
    assert_eq!(foreground.kind(), SettingsFieldKind::Color);
    assert_eq!(foreground.value(), "#e2e8f0");

    let code_font = model
        .row(&SettingsFieldId::from("code.font_family"))
        .expect("code font row should exist");
    assert_eq!(code_font.kind(), SettingsFieldKind::Text);
    assert_eq!(code_font.value(), "Consolas");

    let reasoning_foreground = model
        .row(&SettingsFieldId::from("transcript_reasoning.foreground"))
        .expect("transcript reasoning foreground row should exist");
    assert_eq!(reasoning_foreground.kind(), SettingsFieldKind::Color);
    assert_eq!(reasoning_foreground.value(), "#e2e8f0");

    let commentary_foreground = model
        .row(&SettingsFieldId::from("transcript_commentary.foreground"))
        .expect("transcript commentary foreground row should exist");
    assert_eq!(commentary_foreground.kind(), SettingsFieldKind::Color);
    assert_eq!(commentary_foreground.value(), "#cbd5e1");

    let primary_button_background = model
        .row(&SettingsFieldId::from("primary_button.normal_background"))
        .expect("primary button normal background row should exist");
    assert_eq!(primary_button_background.kind(), SettingsFieldKind::Color);

    let thread_strip_background = model
        .row(&SettingsFieldId::from(
            "chrome.conversation_thread_strip_background",
        ))
        .expect("conversation thread strip background row should exist");
    assert_eq!(
        thread_strip_background.label(),
        "Conversation thread strip background"
    );
    assert_eq!(thread_strip_background.kind(), SettingsFieldKind::Color);
    assert_eq!(thread_strip_background.value(), "#091220");
}

#[test]
fn settings_model_includes_notifications_sound_picker_row() {
    let state = settings_state(AppearanceSettings::default());
    let model = state.model();
    let section = model
        .sections()
        .iter()
        .find(|section| section.section_id().as_str() == "notifications")
        .expect("notifications section should exist");

    assert_eq!(section.label(), "Notifications");
    assert_eq!(section.rows().len(), 1);

    let field_id = state.notification_end_turn_sound_field_id();
    let row = model
        .row(&field_id)
        .expect("end-turn sound row should exist");
    assert_eq!(row.label(), "End-turn sound");
    assert_eq!(row.kind(), SettingsFieldKind::Text);
    assert_eq!(row.value(), "");
    assert_eq!(row.actions().len(), 2);
    assert_eq!(
        row.actions()[0].action_id(),
        &SettingsRowActionId::from("choose")
    );
    assert_eq!(row.actions()[0].label(), "Choose...");
    assert_eq!(
        row.actions()[1].action_id(),
        &SettingsRowActionId::from("clear")
    );
    assert_eq!(row.actions()[1].label(), "Clear");
}

#[test]
fn settings_model_includes_agent_developer_instructions_row() {
    let state = settings_state(AppearanceSettings::default());
    let model = state.model();
    let section = model
        .sections()
        .iter()
        .find(|section| section.section_id().as_str() == "agent")
        .expect("agent section should exist");

    assert_eq!(section.label(), "Agent");
    assert_eq!(section.rows().len(), 1);

    let field_id = state.developer_instructions_field_id();
    let row = model
        .row(&field_id)
        .expect("developer instructions row should exist");
    assert_eq!(row.label(), "Developer Instructions");
    assert_eq!(
        row.subtext(),
        Some("Sent as developer instructions with every user message.")
    );
    assert_eq!(row.kind(), SettingsFieldKind::MultilineText);
    assert_eq!(row.value(), "");
    assert!(row.actions().is_empty());
}

#[test]
fn settings_model_includes_operations_context_compaction_timeout_row() {
    let state = settings_state(AppearanceSettings::default());
    let model = state.model();
    let section = model
        .sections()
        .iter()
        .find(|section| section.section_id().as_str() == "operations")
        .expect("operations section should exist");

    assert_eq!(section.label(), "Operations");
    assert_eq!(section.rows().len(), 1);

    let field_id = context_compaction_timeout_field_id();
    let row = model
        .row(&field_id)
        .expect("context compaction timeout row should exist");
    assert_eq!(row.label(), "Context compaction timeout");
    assert_eq!(
        row.subtext(),
        Some("Seconds Beryl waits for backend-reported compaction completion.")
    );
    assert_eq!(row.kind(), SettingsFieldKind::Text);
    assert_eq!(row.value(), "180");
    assert!(row.actions().is_empty());
}

#[test]
fn settings_window_options_map_active_theme_to_visual_theme() {
    let mut active = AppearanceSettings::default();
    active.general_ui.background = "#101112".to_string();
    active.general_ui.foreground = "#edeff1".to_string();
    active.chrome.surfaces.panel_background = "#202122".to_string();
    active.chrome.surfaces.border = "#303132".to_string();
    active.chrome.primary_button.normal.background = "#404142".to_string();
    let state = settings_state(active);

    let theme = state.window_options().visual_theme().clone();

    assert_eq!(theme.window_background.to_hex(), "#101112");
    assert_eq!(theme.panel.background.to_hex(), "#202122");
    assert_eq!(theme.panel.foreground.to_hex(), "#edeff1");
    assert_eq!(theme.primary_button.normal.background.to_hex(), "#404142");
}

#[test]
fn settings_apply_stages_color_changes_and_normalizes_on_apply() {
    let mut active = AppearanceSettings::default();
    active.code.foreground = "#112233".to_string();
    let (mut state, shared, _notifications, root) = settings_state_with_temp_store(active);
    let field_id = SettingsFieldId::from("code.foreground");
    let commentary_field_id = SettingsFieldId::from("transcript_commentary.foreground");
    let thread_strip_field_id =
        SettingsFieldId::from("chrome.conversation_thread_strip_background");

    state.set_field_value(&field_id, "#AABBCC".to_string());
    state.set_field_value(&commentary_field_id, "#334455".to_string());
    state.set_field_value(&thread_strip_field_id, "#010203".to_string());
    assert_eq!(
        shared.lock().unwrap().code.foreground,
        "#112233",
        "field edits must not live-preview into active settings"
    );
    assert_eq!(
        state.model().row(&field_id).map(|row| row.value()),
        Some("#AABBCC")
    );

    assert!(state.apply());
    assert_eq!(shared.lock().unwrap().code.foreground, "#aabbcc");
    assert_eq!(
        shared.lock().unwrap().transcript_commentary.foreground,
        "#334455"
    );
    assert_eq!(
        shared
            .lock()
            .unwrap()
            .chrome
            .conversation_thread_strip_background,
        "#010203"
    );
    wait_for_save(&mut state);

    let loaded = AppearanceSettingsStore::new(&root)
        .load_or_default()
        .unwrap();
    assert_eq!(loaded.code.foreground, "#aabbcc");
    assert_eq!(loaded.transcript_commentary.foreground, "#334455");
    assert_eq!(
        loaded.chrome.conversation_thread_strip_background,
        "#010203"
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_persists_notification_preferences_separately_from_theme() {
    let (mut state, _appearance, notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let sound_path = root.join("turn-done.wav");

    state.set_notification_end_turn_sound_path(sound_path.display().to_string());
    assert_eq!(
        notifications
            .lock()
            .unwrap()
            .notifications
            .end_turn_sound_path,
        None,
        "notification edits must not live-preview into active preferences"
    );

    assert!(state.apply());
    assert_eq!(
        notifications
            .lock()
            .unwrap()
            .notifications
            .end_turn_sound_path
            .as_deref(),
        Some(sound_path.as_path())
    );
    wait_for_save(&mut state);

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(
        loaded_preferences
            .notifications
            .end_turn_sound_path
            .as_deref(),
        Some(sound_path.as_path())
    );
    assert!(AppearanceSettingsStore::new(&root).theme_path().exists());
    assert!(GuiPreferencesStore::new(&root).preferences_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_persists_operation_preferences() {
    let (mut state, _appearance, preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());

    state.set_field_value(&context_compaction_timeout_field_id(), "240".to_string());
    assert_eq!(
        preferences
            .lock()
            .unwrap()
            .operations
            .context_compaction_timeout_seconds,
        180,
        "operation edits must not live-preview into active preferences"
    );

    assert!(state.apply());
    assert_eq!(
        preferences
            .lock()
            .unwrap()
            .operations
            .context_compaction_timeout_seconds,
        240
    );
    wait_for_save(&mut state);

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(
        loaded_preferences
            .operations
            .context_compaction_timeout_seconds,
        240
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_notification_row_actions_choose_and_clear() {
    let (mut state, _appearance, _notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = state.notification_end_turn_sound_field_id();
    let sound_path = root.join("turn-done.wav");

    assert_eq!(
        state.handle_row_action(&field_id, &SettingsRowActionId::from("choose")),
        Some(settings::SettingsRowActionOutcome::PromptForEndTurnSoundPath)
    );

    state.set_notification_end_turn_sound_path(sound_path.display().to_string());
    let sound_path_text = sound_path.display().to_string();
    assert_eq!(
        state.model().row(&field_id).map(|row| row.value()),
        Some(sound_path_text.as_str())
    );

    assert_eq!(
        state.handle_row_action(&field_id, &SettingsRowActionId::from("clear")),
        Some(settings::SettingsRowActionOutcome::Updated)
    );
    assert_eq!(state.notification_end_turn_sound_path_value(), "");
    assert_eq!(
        state.handle_row_action(&field_id, &SettingsRowActionId::from("missing")),
        None
    );
    cleanup_temp_dir(root);
}

#[test]
fn settings_stage_notification_picker_path_validates_wav_extension() {
    let (mut state, _appearance, _notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = state.notification_end_turn_sound_field_id();
    let sound_path = root.join("turn-done.WAV");
    let text_path = root.join("turn-done.txt");

    state.stage_notification_end_turn_sound_path_from_picker(sound_path.clone());
    assert_eq!(
        state.notification_end_turn_sound_path_value(),
        sound_path.display().to_string()
    );
    assert!(state.field_error(&field_id).is_none());

    state.stage_notification_end_turn_sound_path_from_picker(text_path.clone());
    assert_eq!(
        state.notification_end_turn_sound_path_value(),
        text_path.display().to_string()
    );
    assert!(
        state
            .field_error(&field_id)
            .is_some_and(|error| error.contains(".wav"))
    );
    assert!(!state.apply());
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_persists_empty_notification_path_as_disabled() {
    let (mut state, _appearance, notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = state.notification_end_turn_sound_field_id();
    let sound_path = root.join("turn-done.wav");

    state.set_notification_end_turn_sound_path(sound_path.display().to_string());
    assert_eq!(
        state.handle_row_action(&field_id, &SettingsRowActionId::from("clear")),
        Some(settings::SettingsRowActionOutcome::Updated)
    );

    assert!(state.apply());
    assert_eq!(
        notifications
            .lock()
            .unwrap()
            .notifications
            .end_turn_sound_path,
        None
    );
    wait_for_save(&mut state);

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(loaded_preferences.notifications.end_turn_sound_path, None);
    cleanup_temp_dir(root);
}

#[test]
fn settings_save_uses_injected_root_when_environment_home_differs() {
    let env_home = unique_temp_dir();
    let (mut state, _appearance, _preferences, injected_root) =
        with_environment_home(&env_home, || {
            settings_state_with_temp_store(AppearanceSettings::default())
        });

    state.set_field_value(
        &SettingsFieldId::from("code.foreground"),
        "#010203".to_string(),
    );
    assert!(state.apply());
    wait_for_save(&mut state);

    assert!(
        AppearanceSettingsStore::new(&injected_root)
            .theme_path()
            .exists()
    );
    assert!(
        GuiPreferencesStore::new(&injected_root)
            .preferences_path()
            .exists()
    );
    assert!(!env_home.join(".beryl").exists());

    cleanup_temp_dir(injected_root);
    cleanup_temp_dir(env_home);
}

#[test]
fn settings_apply_rejects_invalid_notification_path_without_mutating_active_preferences() {
    let (mut state, _appearance, notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = state.notification_end_turn_sound_field_id();

    state.set_notification_end_turn_sound_path("relative/done.wav".to_string());

    assert!(!state.apply());
    assert_eq!(
        notifications
            .lock()
            .unwrap()
            .notifications
            .end_turn_sound_path,
        None
    );
    assert!(
        state
            .field_error(&field_id)
            .is_some_and(|error| error.contains("absolute"))
    );
    assert!(!GuiPreferencesStore::new(&root).preferences_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_rejects_invalid_operation_timeout_without_mutating_active_preferences() {
    let (mut state, _appearance, preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = context_compaction_timeout_field_id();

    state.set_field_value(&field_id, "0".to_string());

    assert!(!state.apply());
    assert_eq!(
        preferences
            .lock()
            .unwrap()
            .operations
            .context_compaction_timeout_seconds,
        180
    );
    assert!(
        state
            .field_error(&field_id)
            .is_some_and(|error| error.contains("at least"))
    );
    assert!(!GuiPreferencesStore::new(&root).preferences_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_rejects_invalid_color_draft_without_mutating_active_settings() {
    let mut active = AppearanceSettings::default();
    active.emphasis.background = "#010203".to_string();
    let (mut state, shared, _notifications, root) = settings_state_with_temp_store(active);
    let field_id = SettingsFieldId::from("emphasis.background");

    state.set_field_value(&field_id, "slate".to_string());

    assert!(!state.apply());
    assert_eq!(shared.lock().unwrap().emphasis.background, "#010203");
    assert!(
        state
            .model()
            .row(&field_id)
            .and_then(|row| row.error())
            .is_some_and(|error| error.contains("#rrggbb"))
    );
    assert!(!AppearanceSettingsStore::new(&root).theme_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn settings_reset_discards_unapplied_draft_and_preserves_selected_section() {
    let (mut state, _shared, _notifications, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let field_id = SettingsFieldId::from("general_ui.font_family");

    state.select_section(SettingsSectionId::from("code"));
    state.set_field_value(&field_id, "JetBrains Mono".to_string());
    state.set_notification_end_turn_sound_path(root.join("done.wav").display().to_string());
    state.set_developer_instructions("Use a staged draft.".to_string());
    let context_timeout_field_id = context_compaction_timeout_field_id();
    state.set_field_value(&context_timeout_field_id, "240".to_string());
    state.reset_draft_from_active();

    let model = state.model();
    assert_eq!(model.selected_section_id().as_str(), "code");
    assert_eq!(model.row(&field_id).map(|row| row.value()), Some("Inter"));
    assert_eq!(state.notification_end_turn_sound_path_value(), "");
    assert_eq!(state.developer_instructions_value(), "");
    assert_eq!(
        model.row(&context_timeout_field_id).map(|row| row.value()),
        Some("180")
    );
    cleanup_temp_dir(root);
}

fn settings_state(settings_value: AppearanceSettings) -> settings::SettingsState {
    settings_state_with_temp_store(settings_value).0
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
    tempdir_support::temp_dir("beryl-settings-window-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    root.close().unwrap();
}

fn context_compaction_timeout_field_id() -> SettingsFieldId {
    SettingsFieldId::from("operations.context_compaction_timeout_seconds")
}

fn with_environment_home<T>(home: &Path, action: impl FnOnce() -> T) -> T {
    let userprofile = env::var_os("USERPROFILE");
    let home_var = env::var_os("HOME");
    unsafe {
        env::set_var("USERPROFILE", home);
        env::set_var("HOME", home);
    }

    let result = panic::catch_unwind(AssertUnwindSafe(action));

    restore_env_var("USERPROFILE", userprofile);
    restore_env_var("HOME", home_var);

    match result {
        Ok(value) => value,
        Err(payload) => panic::resume_unwind(payload),
    }
}

fn restore_env_var(key: &str, value: Option<OsString>) {
    unsafe {
        if let Some(value) = value {
            env::set_var(key, value);
        } else {
            env::remove_var(key);
        }
    }
}
