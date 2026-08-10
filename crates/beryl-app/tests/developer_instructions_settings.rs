#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    fs,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

pub use beryl_app::{
    ActiveThemeProjection, ActivityDiagnosticCaptureErrorCategory,
    ActivityDiagnosticCaptureRuntimeState, ActivityDiagnosticCaptureStatus, AgentPreferences,
    AppearanceButtonSettings, AppearanceButtonStateSettings, AppearanceForegroundSettings,
    AppearanceInputSettings, AppearanceRoleSettings, AppearanceSettings,
    AppearanceStatusLineSettings, AppearanceSurfaceSettings, AppearanceTranscriptShellSettings,
    BUILT_IN_INSTALLED_THEME_ID, BerylThemeProperty, BerylThemeRole, BerylWorkspacePersistence,
    ContextCompactionTimeoutError, DiagnosticPreferences, GuiPreferences, GuiPreferencesStore,
    InstalledThemeId, NotificationPreferences, NotificationSoundPathError, OperationPreferences,
    StylePropertyId, StylePropertyKind, StylePropertySource, StylePropertyValue, StyleRoleId,
    ThemeDefinition, ThemeRepositorySnapshot, ThemeRepositoryStore, ThemeResolutionContext,
    ThemeResolver, ThemeRoleDefinition, ThemeRoleSchema, WorkspaceGraphUpkeepPolicy,
    built_in_theme_schema, normalize_developer_instructions_text,
    normalize_graph_upkeep_instructions_text, parse_context_compaction_timeout_seconds_text,
    parse_notification_sound_path_text, validate_notification_sound_path,
};

#[path = "../src/build_identity.rs"]
mod build_identity;

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
    assert!(!state.has_pending_save());

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(loaded_preferences.agent.developer_instructions, None);
    cleanup_temp_dir(root);
}

#[test]
fn external_settings_apply_noop_does_not_enqueue_save() {
    let (mut state, _appearance, _preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let current = state.active_preferences_snapshot();

    let changed = state.apply_preferences_from_external(current).unwrap();

    assert!(!changed);
    assert!(!state.has_pending_save());
    cleanup_temp_dir(root);
}

#[test]
fn external_settings_apply_rejects_unapplied_drafts() {
    let (mut state, _appearance, preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    state.set_developer_instructions("draft instructions".to_string());
    let next = GuiPreferences {
        agent: AgentPreferences::with_developer_instructions(Some(
            "external instructions".to_string(),
        )),
        ..GuiPreferences::default()
    };

    let error = state.apply_preferences_from_external(next).unwrap_err();

    assert!(error.contains("unapplied settings drafts"));
    assert_eq!(
        preferences.lock().unwrap().agent.developer_instructions,
        None
    );
    assert!(!state.has_pending_save());
    cleanup_temp_dir(root);
}

#[test]
fn external_settings_apply_persists_through_preferences_store() {
    let (mut state, _appearance, preferences, root) =
        settings_state_with_temp_store(AppearanceSettings::default());
    let next = GuiPreferences {
        agent: AgentPreferences::with_developer_instructions(Some(
            "external instructions".to_string(),
        )),
        ..GuiPreferences::default()
    };

    let changed = state.apply_preferences_from_external(next).unwrap();

    assert!(changed);
    assert_eq!(
        preferences
            .lock()
            .unwrap()
            .agent
            .developer_instructions
            .as_deref(),
        Some("external instructions")
    );
    wait_for_save(&mut state);

    let loaded_preferences = GuiPreferencesStore::new(&root).load_or_default().unwrap();
    assert_eq!(
        loaded_preferences.agent.developer_instructions.as_deref(),
        Some("external instructions")
    );
    cleanup_temp_dir(root);
}

#[test]
fn unavailable_preferences_store_rejects_gui_and_external_changes_before_mutation() {
    let active_theme = Arc::new(Mutex::new(
        AppearanceSettings::default()
            .to_active_theme_projection()
            .unwrap(),
    ));
    let gui_preferences = Arc::new(Mutex::new(GuiPreferences::default()));
    let mut gui_state = settings::SettingsState::new_without_stores(
        active_theme.clone(),
        gui_preferences.clone(),
        "private path details must not escape".to_string(),
    );
    let field_id = gui_state.activity_diagnostic_capture_field_id();
    gui_state.set_field_value(&field_id, "enabled".to_string());

    assert!(!gui_state.apply());
    assert!(!gui_state.has_pending_save());
    assert!(
        !gui_preferences
            .lock()
            .unwrap()
            .diagnostics
            .activity_diagnostic_capture_enabled
    );
    let model = gui_state.model();
    let feedback = model
        .row(&field_id)
        .and_then(|row| row.error())
        .expect("unavailable storage should produce localized field feedback");
    assert!(feedback.contains("settings storage is unavailable"));
    assert!(!feedback.contains("private path"));

    let external_preferences = Arc::new(Mutex::new(GuiPreferences::default()));
    let mut external_state = settings::SettingsState::new_without_stores(
        active_theme,
        external_preferences.clone(),
        "another private path".to_string(),
    );
    let next = diagnostic_preferences(true);
    let error = external_state
        .apply_preferences_from_external(next)
        .expect_err("changed external settings require an available store");
    assert!(error.contains("settings storage is unavailable"));
    assert!(!error.contains("private path"));
    assert!(!external_state.has_pending_save());
    assert_eq!(
        *external_preferences.lock().unwrap(),
        GuiPreferences::default()
    );
}

#[test]
fn failed_enable_and_disable_saves_restore_previous_active_preferences() {
    for initially_enabled in [false, true] {
        let initial = diagnostic_preferences(initially_enabled);
        let (mut state, shared_preferences, root) =
            settings_state_with_failing_store(initial.clone());
        let attempted = diagnostic_preferences(!initially_enabled);

        assert!(
            state
                .apply_preferences_from_external(attempted.clone())
                .unwrap()
        );
        assert_eq!(*shared_preferences.lock().unwrap(), attempted);

        let (_error, restored) = wait_for_failed_save(&mut state);
        assert_eq!(restored, Some(initial.clone()));
        assert_eq!(*shared_preferences.lock().unwrap(), initial);
        let field_id = state.activity_diagnostic_capture_field_id();
        let model = state.model();
        let feedback = model
            .row(&field_id)
            .and_then(|row| row.error())
            .expect("failed capture preference save should produce localized feedback");
        assert!(feedback.contains("previous value was restored"));
        assert!(!state.has_pending_save());
        cleanup_temp_dir(root);
    }
}

#[test]
fn queued_save_failure_never_rolls_back_to_a_superseded_snapshot() {
    let initial = diagnostic_preferences(false);
    let (mut state, shared_preferences, root) = settings_state_with_failing_store(initial.clone());
    let enabled = diagnostic_preferences(true);

    assert!(
        state
            .apply_preferences_from_external(enabled.clone())
            .unwrap()
    );
    assert!(
        state
            .apply_preferences_from_external(initial.clone())
            .unwrap()
    );
    assert_eq!(*shared_preferences.lock().unwrap(), initial);

    let (_first_error, first_restored) = wait_for_failed_save(&mut state);
    assert_eq!(
        first_restored, None,
        "a newer queued save suppresses stale rollback"
    );
    assert_eq!(*shared_preferences.lock().unwrap(), initial);

    let (_second_error, second_restored) = wait_for_failed_save(&mut state);
    assert_eq!(second_restored, Some(initial.clone()));
    assert_eq!(*shared_preferences.lock().unwrap(), initial);
    assert!(!state.has_pending_save());
    cleanup_temp_dir(root);
}

fn settings_state_with_temp_store(
    settings_value: AppearanceSettings,
) -> (
    settings::SettingsState,
    Arc<Mutex<ActiveThemeProjection>>,
    Arc<Mutex<GuiPreferences>>,
    tempdir_support::TestTempDir,
) {
    let root = unique_temp_dir();
    let shared_theme = Arc::new(Mutex::new(
        settings_value.to_active_theme_projection().unwrap(),
    ));
    let shared_preferences = Arc::new(Mutex::new(GuiPreferences::default()));
    let state = settings::SettingsState::new_with_stores(
        shared_theme.clone(),
        shared_preferences.clone(),
        GuiPreferencesStore::new(&root),
    );
    (state, shared_theme, shared_preferences, root)
}

fn settings_state_with_failing_store(
    initial: GuiPreferences,
) -> (
    settings::SettingsState,
    Arc<Mutex<GuiPreferences>>,
    tempdir_support::TestTempDir,
) {
    let root = unique_temp_dir();
    let blocked_root = root.join("not-a-directory");
    fs::write(&blocked_root, b"blocks preference directory creation").unwrap();
    let active_theme = Arc::new(Mutex::new(
        AppearanceSettings::default()
            .to_active_theme_projection()
            .unwrap(),
    ));
    let shared_preferences = Arc::new(Mutex::new(initial));
    let state = settings::SettingsState::new_with_stores(
        active_theme,
        shared_preferences.clone(),
        GuiPreferencesStore::new(blocked_root),
    );
    (state, shared_preferences, root)
}

fn diagnostic_preferences(enabled: bool) -> GuiPreferences {
    GuiPreferences {
        diagnostics: DiagnosticPreferences {
            activity_diagnostic_capture_enabled: enabled,
        },
        ..GuiPreferences::default()
    }
}

fn wait_for_failed_save(state: &mut settings::SettingsState) -> (String, Option<GuiPreferences>) {
    for _ in 0..100 {
        match state.poll_save() {
            settings::SettingsSavePoll::Failed {
                error,
                restored_preferences,
            } => return (error, restored_preferences),
            settings::SettingsSavePoll::Pending => thread::sleep(Duration::from_millis(10)),
            settings::SettingsSavePoll::Idle | settings::SettingsSavePoll::Saved => {
                panic!("failing settings store must report a save failure")
            }
        }
    }

    panic!("timed out waiting for settings save failure");
}

fn wait_for_save(state: &mut settings::SettingsState) {
    for _ in 0..100 {
        match state.poll_save() {
            settings::SettingsSavePoll::Saved => return,
            settings::SettingsSavePoll::Pending => thread::sleep(Duration::from_millis(10)),
            settings::SettingsSavePoll::Idle => panic!("settings save should be pending"),
            settings::SettingsSavePoll::Failed { error, .. } => {
                panic!("settings save failed: {error}")
            }
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
