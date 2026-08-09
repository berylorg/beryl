#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    fs,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

pub use beryl_app::{
    ActiveThemeProjection, AgentPreferences, AppearanceButtonSettings,
    AppearanceButtonStateSettings, AppearanceForegroundSettings, AppearanceInputSettings,
    AppearanceRoleSettings, AppearanceSettings, AppearanceStatusLineSettings,
    AppearanceSurfaceSettings, AppearanceTranscriptShellSettings, BUILT_IN_INSTALLED_THEME_ID,
    BerylThemeProperty, BerylThemeRole, BerylWorkspacePersistence, ContextCompactionTimeoutError,
    GuiPreferences, GuiPreferencesStore, InstalledThemeId, NotificationPreferences,
    NotificationSoundPathError, OperationPreferences, StylePropertyId, StylePropertyKind,
    StylePropertySource, StylePropertyValue, StyleRoleId, ThemeDefinition, ThemeRepositorySnapshot,
    ThemeRepositoryStore, ThemeResolutionContext, ThemeResolver, ThemeRoleDefinition,
    ThemeRoleSchema, WorkspaceGraphUpkeepPolicy, built_in_theme_schema,
    normalize_developer_instructions_text, normalize_graph_upkeep_instructions_text,
    parse_context_compaction_timeout_seconds_text, parse_notification_sound_path_text,
    validate_notification_sound_path,
};
use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest};
use gpui_settings_window::SettingsFieldKind;

#[path = "../src/build_identity.rs"]
mod build_identity;

#[allow(dead_code)]
#[path = "../src/shell/settings.rs"]
mod settings;

#[test]
fn settings_model_includes_graph_upkeep_instructions_row() {
    let state = settings_state_with_temp_store(AppearanceSettings::default()).0;
    let model = state.model();
    let section = model
        .sections()
        .iter()
        .find(|section| section.section_id().as_str() == "graph")
        .expect("graph section should exist");

    assert_eq!(section.label(), "Graph");
    assert_eq!(section.rows().len(), 1);

    let field_id = state.graph_upkeep_instructions_field_id();
    let row = model
        .row(&field_id)
        .expect("graph upkeep instructions row should exist");
    assert_eq!(row.label(), "Graph Upkeep Instructions");
    assert_eq!(row.kind(), SettingsFieldKind::MultilineText);
    assert_eq!(row.value(), "");
    assert_eq!(
        row.error(),
        Some("Select a workspace before editing graph-upkeep instructions.")
    );
    assert!(row.actions().is_empty());
}

#[test]
fn settings_apply_persists_workspace_graph_upkeep_policy_after_save() {
    let (mut state, root, workspace_id) = settings_state_with_workspace_target();
    let instructions = "Keep plan nodes current.\r\nPrefer concise summaries.";

    state.set_graph_upkeep_instructions(instructions.to_string());
    assert_eq!(
        state
            .active_graph_upkeep_policy_snapshot()
            .and_then(|policy| policy.instructions().map(str::to_string)),
        None,
        "graph-upkeep edits must not live-preview into the active policy"
    );

    assert!(state.apply());
    assert_eq!(
        state
            .active_graph_upkeep_policy_snapshot()
            .and_then(|policy| policy.instructions().map(str::to_string)),
        None,
        "active graph-upkeep policy should update only after persistence succeeds"
    );
    wait_for_all_saves(&mut state);

    assert_eq!(
        state
            .active_graph_upkeep_policy_snapshot()
            .and_then(|policy| policy.instructions().map(str::to_string)),
        Some("Keep plan nodes current.\nPrefer concise summaries.".to_string())
    );
    let loaded = BerylWorkspacePersistence::new(&root)
        .load_workspace_graph_upkeep_policy(&workspace_id)
        .unwrap();
    assert_eq!(
        loaded.instructions(),
        Some("Keep plan nodes current.\nPrefer concise summaries.")
    );

    let preferences_text =
        fs::read_to_string(GuiPreferencesStore::new(&root).preferences_path()).unwrap_or_default();
    assert!(!preferences_text.contains("graph_upkeep"));
    assert!(!preferences_text.contains("graph-upkeep"));
    cleanup_temp_dir(root);
}

#[test]
fn settings_apply_persists_blank_graph_upkeep_policy_as_disabled() {
    let (mut state, root, workspace_id) = settings_state_with_workspace_target();

    state.set_graph_upkeep_instructions(" \r\n\t ".to_string());
    assert!(state.apply());
    wait_for_all_saves(&mut state);

    let loaded = BerylWorkspacePersistence::new(&root)
        .load_workspace_graph_upkeep_policy(&workspace_id)
        .unwrap();
    assert_eq!(loaded.instructions(), None);
    cleanup_temp_dir(root);
}

#[test]
fn settings_reset_discards_unapplied_graph_upkeep_draft() {
    let (mut state, root, workspace_id) = settings_state_with_workspace_target();
    let applied = WorkspaceGraphUpkeepPolicy::with_instructions(Some("Applied policy".to_string()));
    state.set_graph_workspace_target(workspace_id, applied.clone());

    state.set_graph_upkeep_instructions("Draft policy".to_string());
    state.reset_draft_from_active();

    assert_eq!(state.graph_upkeep_instructions_value(), "Applied policy");
    assert_eq!(state.active_graph_upkeep_policy_snapshot(), Some(applied));
    assert!(!state.has_pending_save());
    cleanup_temp_dir(root);
}

#[test]
fn workspace_switch_discards_unapplied_graph_upkeep_draft() {
    let (mut state, root, first_workspace_id) = settings_state_with_workspace_target();
    let second_workspace_id = BerylWorkspaceId::new("graph_settings_second").unwrap();
    let second_policy =
        WorkspaceGraphUpkeepPolicy::with_instructions(Some("Second policy".to_string()));

    state.set_graph_upkeep_instructions("Draft for first workspace".to_string());
    state.set_graph_workspace_target(second_workspace_id.clone(), second_policy.clone());

    assert_eq!(state.graph_upkeep_instructions_value(), "Second policy");
    assert_eq!(
        state.active_graph_upkeep_policy_snapshot(),
        Some(second_policy)
    );
    assert_ne!(
        state.graph_upkeep_instructions_value(),
        "Draft for first workspace"
    );
    assert_ne!(first_workspace_id, second_workspace_id);
    cleanup_temp_dir(root);
}

#[test]
fn active_graph_upkeep_policy_requires_matching_workspace_target() {
    let (mut state, root, first_workspace_id) = settings_state_with_workspace_target();
    let second_workspace_id = BerylWorkspaceId::new("graph_settings_second").unwrap();
    let policy = WorkspaceGraphUpkeepPolicy::with_instructions(Some("Second policy".to_string()));

    state.set_graph_workspace_target(second_workspace_id.clone(), policy.clone());

    assert_eq!(
        state.active_graph_upkeep_policy_for_workspace(&second_workspace_id),
        Some(policy)
    );
    assert_eq!(
        state.active_graph_upkeep_policy_for_workspace(&first_workspace_id),
        None
    );
    cleanup_temp_dir(root);
}

#[test]
fn workspace_rename_rebinds_graph_target_without_losing_draft() {
    let (mut state, root, old_workspace_id) = settings_state_with_workspace_target();
    let new_workspace_id = BerylWorkspaceId::new("renamed_graph_settings").unwrap();
    let new_manifest =
        BerylWorkspaceManifest::named(new_workspace_id.clone(), "Renamed Graph Settings", 42);
    let applied = WorkspaceGraphUpkeepPolicy::with_instructions(Some("Applied policy".to_string()));

    BerylWorkspacePersistence::new(&root)
        .save_workspace_manifest(&new_manifest)
        .unwrap();
    state.set_graph_workspace_target(old_workspace_id.clone(), applied.clone());
    state.set_graph_upkeep_instructions("Draft after rename".to_string());

    assert!(
        state.rebind_graph_workspace_target_after_rename(
            &old_workspace_id,
            new_workspace_id.clone()
        )
    );

    assert_eq!(
        state.graph_upkeep_instructions_value(),
        "Draft after rename"
    );
    assert_eq!(
        state.active_graph_upkeep_policy_for_workspace(&old_workspace_id),
        None
    );
    assert_eq!(
        state.active_graph_upkeep_policy_for_workspace(&new_workspace_id),
        Some(applied)
    );

    assert!(state.apply());
    wait_for_all_saves(&mut state);

    let loaded = BerylWorkspacePersistence::new(&root)
        .load_workspace_graph_upkeep_policy(&new_workspace_id)
        .unwrap();
    assert_eq!(loaded.instructions(), Some("Draft after rename"));

    cleanup_temp_dir(root);
}

#[test]
fn graph_upkeep_row_stays_unavailable_without_workspace_persistence() {
    let root = unique_temp_dir();
    let theme_store = ThemeRepositoryStore::new(&root);
    let theme_snapshot = theme_store.load_or_default().unwrap();
    let shared_theme = Arc::new(Mutex::new(theme_snapshot.active_projection().clone()));
    let shared_preferences = Arc::new(Mutex::new(GuiPreferences::default()));
    let mut state = settings::SettingsState::new_with_theme_repository(
        shared_theme,
        shared_preferences,
        GuiPreferencesStore::new(&root),
        theme_store,
        theme_snapshot,
    );
    let workspace_id = BerylWorkspaceId::new("graph_settings_unavailable").unwrap();

    state.set_graph_workspace_target(workspace_id, WorkspaceGraphUpkeepPolicy::default());
    state.set_graph_upkeep_instructions("Should not stage".to_string());

    let field_id = state.graph_upkeep_instructions_field_id();
    let model = state.model();
    let row = model.row(&field_id).unwrap();
    assert_eq!(row.value(), "");
    assert_eq!(
        row.error(),
        Some("Workspace settings storage is unavailable for graph-upkeep instructions.")
    );

    cleanup_temp_dir(root);
}

fn settings_state_with_workspace_target() -> (
    settings::SettingsState,
    tempdir_support::TestTempDir,
    BerylWorkspaceId,
) {
    let (mut state, root) = settings_state_with_temp_store(AppearanceSettings::default());
    let workspace_id = BerylWorkspaceId::new("graph_settings").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Settings", 42);
    BerylWorkspacePersistence::new(&root)
        .save_workspace_manifest(&manifest)
        .unwrap();
    state.set_graph_workspace_target(workspace_id.clone(), WorkspaceGraphUpkeepPolicy::default());
    (state, root, workspace_id)
}

fn settings_state_with_temp_store(
    settings_value: AppearanceSettings,
) -> (settings::SettingsState, tempdir_support::TestTempDir) {
    let root = unique_temp_dir();
    let theme_store = ThemeRepositoryStore::new(&root);
    let theme_snapshot = theme_store
        .save_as_theme("Test Theme", settings_value.to_theme_definition().unwrap())
        .unwrap();
    let shared_theme = Arc::new(Mutex::new(theme_snapshot.active_projection().clone()));
    let shared_preferences = Arc::new(Mutex::new(GuiPreferences::default()));
    let workspace_persistence = BerylWorkspacePersistence::new(&root);
    let state = settings::SettingsState::new_with_theme_repository_and_workspace_persistence(
        shared_theme,
        shared_preferences,
        GuiPreferencesStore::new(&root),
        workspace_persistence,
        theme_store,
        theme_snapshot,
    );
    (state, root)
}

fn wait_for_all_saves(state: &mut settings::SettingsState) {
    for _ in 0..100 {
        match state.poll_save() {
            settings::SettingsSavePoll::Failed(error) => panic!("settings save failed: {error}"),
            settings::SettingsSavePoll::Idle
            | settings::SettingsSavePoll::Pending
            | settings::SettingsSavePoll::Saved => {
                if !state.has_pending_save() {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    panic!("timed out waiting for settings save");
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-graph-upkeep-settings-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    root.close().unwrap();
}
