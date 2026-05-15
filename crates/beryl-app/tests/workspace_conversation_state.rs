#[path = "support/tempdir.rs"]
mod tempdir_support;

#[allow(dead_code)]
#[path = "../src/shell/token_usage_snapshot.rs"]
mod token_usage_snapshot_adapter;

use beryl_app::{
    BerylWorkspacePersistence, WorkspaceActivityPanelMode, WorkspaceImageAssetStatus,
    WorkspaceUiState,
};
use beryl_backend::{ThreadTokenUsage, TokenUsageBreakdown};
use beryl_model::conversation::{
    ConversationThreadId, ConversationThreadMemberBinding, ConversationThreadTitleSource,
    ConversationThreadTokenUsageSnapshot, ConversationTokenUsageBreakdown, ConversationTurnId,
    PrimaryWorkspaceMember, RegisteredConversationThread, ThreadAutomaticTitleGenerationState,
    WorkspaceConversationState,
};
use beryl_model::workspace::{
    BerylWorkspaceId, BerylWorkspaceManifest, BerylWorkspaceTitleSource, RuntimeMode, WorkspaceId,
    WorkspaceMemberAvailability,
};
use gpui::ImageFormat;
use redb::{Database, TableDefinition};
use serde_json::json;

const WORKSPACE_METADATA_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("workspace_metadata");
const WORKSPACE_CONVERSATION_STATE_KEY: &str = "conversation_state";
const WORKSPACE_UI_STATE_KEY: &str = "ui_state";

#[test]
fn workspace_state_roundtrips_runtime_members_and_active_thread() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graphics_learning").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graphics Learning", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();
    let thread = RegisteredConversationThread::new(
        ConversationThreadId::new("thread_1"),
        execution_target.clone(),
        "Explain the renderer",
        Some("Renderer".to_string()),
        1,
        2,
    );

    persistence.save_workspace_manifest(&manifest).unwrap();
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(thread);
    state.activate_thread(&ConversationThreadId::new("thread_1"));
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let loaded = persistence.load_workspace_state(&workspace_id).unwrap();

    assert_eq!(loaded.active_thread().unwrap().as_str(), "thread_1");
    let loaded_thread = loaded
        .thread_registration(&ConversationThreadId::new("thread_1"))
        .unwrap();
    assert_eq!(loaded_thread.title(), Some("Renderer"));
    assert!(loaded_thread.gui_title().is_none());
    assert!(matches!(
        loaded_thread.member_binding(),
        Some(ConversationThreadMemberBinding::Explicit { .. })
    ));
    assert_eq!(loaded.selected_runtime(), Some(&RuntimeMode::HostWindows));
    assert_eq!(
        loaded.primary_explicit_member().unwrap().canonical_path(),
        execution_target.canonical_path()
    );

    root.close().unwrap();
}

#[test]
fn workspace_state_roundtrips_runtime_bound_member_availability() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("member_availability").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Availability", 42);
    let host_target = WorkspaceId::host_windows(r"C:\work\missing");
    let wsl_target = WorkspaceId::wsl_linux("Debian", r"\work\available");
    let mut state = WorkspaceConversationState::default();

    persistence.save_workspace_manifest(&manifest).unwrap();
    state
        .designate_primary_execution_target(&host_target)
        .unwrap();
    state.attach_execution_target(&wsl_target).unwrap();
    let missing_member_id = state.explicit_members()[0].id().clone();
    let available_member_id = state.explicit_members()[1].id().clone();
    state
        .mark_explicit_member_path_not_found(&missing_member_id)
        .unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let loaded = persistence.load_workspace_state(&workspace_id).unwrap();

    assert_eq!(loaded.explicit_members().len(), 2);
    assert_eq!(
        loaded.explicit_members()[0].runtime_mode(),
        host_target.runtime_mode()
    );
    assert_eq!(
        loaded.explicit_members()[0].availability(),
        WorkspaceMemberAvailability::PathNotFound
    );
    assert_eq!(
        loaded.explicit_members()[1].runtime_mode(),
        wsl_target.runtime_mode()
    );
    assert_eq!(
        loaded.durable_primary_explicit_member_id(),
        Some(&available_member_id)
    );

    root.close().unwrap();
}

#[test]
fn legacy_selected_runtime_members_load_as_runtime_bound_available_members() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("legacy_runtime_members").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Legacy Members", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();
    write_raw_workspace_conversation_state(
        &persistence,
        &workspace_id,
        json!({
            "selected_runtime": "HostWindows",
            "explicit_members": [
                {
                    "id": "member_1",
                    "canonical_path": "C:\\work\\beryl"
                }
            ],
            "primary_explicit_member_id": "member_1"
        }),
    );

    let loaded = persistence.load_workspace_state(&workspace_id).unwrap();

    assert_eq!(loaded.default_runtime(), Some(&RuntimeMode::HostWindows));
    assert_eq!(loaded.explicit_members().len(), 1);
    assert_eq!(
        loaded.explicit_members()[0].runtime_mode(),
        &RuntimeMode::HostWindows
    );
    assert_eq!(
        loaded.explicit_members()[0].availability(),
        WorkspaceMemberAvailability::Available
    );
    assert_eq!(
        loaded.primary_explicit_member().unwrap().canonical_path(),
        WorkspaceId::host_windows(r"C:\work\beryl").canonical_path()
    );

    root.close().unwrap();
}

#[test]
fn active_thread_state_persists_only_after_successful_activation_update() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("activation_persistence").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Activation", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_a = ConversationThreadId::new("thread_a");
    let thread_b = ConversationThreadId::new("thread_b");
    let mut state = WorkspaceConversationState::default();

    persistence.save_workspace_manifest(&manifest).unwrap();
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        thread_a.clone(),
        execution_target.clone(),
        "Active preview",
        Some("Active".to_string()),
        1,
        2,
    ));
    state.activate_thread(&thread_a);
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    assert!(
        state
            .activate_thread(&ConversationThreadId::new("missing_thread"))
            .is_none()
    );
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();
    let failed_attempt = persistence.load_workspace_state(&workspace_id).unwrap();
    assert_eq!(failed_attempt.active_thread(), Some(&thread_a));

    state.remember_thread(RegisteredConversationThread::new(
        thread_b.clone(),
        execution_target,
        "Selected preview",
        Some("Selected".to_string()),
        3,
        4,
    ));
    state.activate_thread(&thread_b);
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let successful_attempt = persistence.load_workspace_state(&workspace_id).unwrap();
    assert_eq!(successful_attempt.active_thread(), Some(&thread_b));

    root.close().unwrap();
}

#[test]
fn touching_workspace_manifest_updates_last_updated_without_losing_runtime_state() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graphics_learning").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graphics Learning", 42);
    let mut state = WorkspaceConversationState::default();

    state
        .select_runtime(RuntimeMode::HostWindows)
        .expect("host runtime selection is valid");

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let touched = persistence.touch_workspace_manifest(&workspace_id).unwrap();
    let loaded_state = persistence.load_workspace_state(&workspace_id).unwrap();

    assert!(touched.last_updated_at_millis() >= manifest.last_updated_at_millis());
    match loaded_state.primary_member().unwrap() {
        PrimaryWorkspaceMember::ImplicitHome(RuntimeMode::HostWindows) => {}
        other => panic!("expected implicit host home member, got {other:?}"),
    }

    root.close().unwrap();
}

#[test]
fn generated_workspace_title_is_persisted_without_overwriting_existing_title() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let manifest = BerylWorkspaceManifest::untitled(1, 42);
    let workspace_id = manifest.id().clone();

    persistence.save_workspace_manifest(&manifest).unwrap();

    let generated = persistence
        .set_workspace_generated_title_if_untitled(&workspace_id, " Renderer notes ")
        .unwrap()
        .unwrap();
    let second = persistence
        .set_workspace_generated_title_if_untitled(generated.id(), "Second title")
        .unwrap();
    let stored = persistence
        .load_workspace_manifest(generated.id())
        .unwrap()
        .unwrap();

    assert_eq!(generated.id().as_str(), "renderer-notes");
    assert_eq!(generated.title(), "Renderer notes");
    assert_eq!(
        generated.title_source(),
        Some(BerylWorkspaceTitleSource::FirstCompletedTurn)
    );
    assert!(generated.last_updated_at_millis() >= manifest.last_updated_at_millis());
    assert!(second.is_none());
    assert_eq!(stored, generated);
    assert!(
        persistence
            .load_workspace_manifest(&workspace_id)
            .unwrap()
            .is_none()
    );

    root.close().unwrap();
}

#[test]
fn manual_workspace_title_overrides_generated_title_and_touches_manifest() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let manifest = BerylWorkspaceManifest::untitled(2, 42);
    let workspace_id = manifest.id().clone();

    persistence.save_workspace_manifest(&manifest).unwrap();
    let generated = persistence
        .set_workspace_generated_title_if_untitled(&workspace_id, "Generated notes")
        .unwrap()
        .unwrap();
    let manual = persistence
        .set_workspace_manual_title(generated.id(), " Manual notes ")
        .unwrap()
        .unwrap();

    assert_eq!(generated.id().as_str(), "generated-notes");
    assert_eq!(manual.id().as_str(), "manual-notes");
    assert_eq!(manual.title(), "Manual notes");
    assert_eq!(
        manual.title_source(),
        Some(BerylWorkspaceTitleSource::Manual)
    );
    assert!(manual.last_updated_at_millis() >= generated.last_updated_at_millis());
    assert!(
        persistence
            .set_workspace_generated_title_if_untitled(manual.id(), "Late generated")
            .unwrap()
            .is_none()
    );

    root.close().unwrap();
}

#[test]
fn workspace_title_change_moves_conversation_ui_and_image_asset_state() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graphics_learning").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graphics Learning", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();
    let thread = RegisteredConversationThread::new(
        ConversationThreadId::new("thread_1"),
        execution_target.clone(),
        "Inspect renderer",
        Some("Renderer".to_string()),
        10,
        11,
    );

    persistence.save_workspace_manifest(&manifest).unwrap();
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(thread);
    state.activate_thread(&ConversationThreadId::new("thread_1"));
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();
    persistence
        .save_workspace_ui_state(
            &workspace_id,
            &WorkspaceUiState::new(WorkspaceActivityPanelMode::On, 144.0),
        )
        .unwrap();
    let asset = persistence
        .create_workspace_image_asset(&workspace_id, ImageFormat::Png, b"png bytes")
        .unwrap();
    let old_dir = persistence.workspace_dir(&workspace_id);

    let renamed = persistence
        .set_workspace_manual_title(&workspace_id, "Renderer Notes")
        .unwrap()
        .unwrap();
    let new_id = BerylWorkspaceId::new("renderer-notes").unwrap();
    let loaded_state = persistence.load_workspace_state(&new_id).unwrap();
    let loaded_ui_state = persistence.load_workspace_ui_state(&new_id).unwrap();
    let loaded_assets = persistence.load_workspace_image_assets(&new_id).unwrap();

    assert_eq!(renamed.id(), &new_id);
    assert_eq!(renamed.title(), "Renderer Notes");
    assert!(!old_dir.exists());
    assert!(persistence.workspace_dir(&new_id).exists());
    assert!(
        persistence
            .load_workspace_manifest(&workspace_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(loaded_state, state);
    assert_eq!(
        loaded_ui_state.tool_activity_panel_mode(),
        WorkspaceActivityPanelMode::On
    );
    assert_eq!(loaded_ui_state.tool_activity_panel_height_px(), 144.0);
    assert_eq!(loaded_assets.len(), 1);
    assert_eq!(loaded_assets[0].id(), asset.id());
    assert_eq!(
        loaded_assets[0].status(),
        WorkspaceImageAssetStatus::Available
    );
    assert_eq!(
        persistence
            .read_workspace_image_asset_bytes(&new_id, asset.id())
            .unwrap(),
        b"png bytes"
    );

    root.close().unwrap();
}

#[test]
fn gui_owned_thread_titles_and_rebind_requirement_roundtrip_in_workspace_state() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graphics_learning").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graphics Learning", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let generated_thread_id = ConversationThreadId::new("thread_generated");
    let manual_thread_id = ConversationThreadId::new("thread_manual");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        generated_thread_id.clone(),
        execution_target.clone(),
        "Generated preview",
        None,
        1,
        2,
    ));
    state.remember_thread(RegisteredConversationThread::new(
        manual_thread_id.clone(),
        execution_target,
        "Manual preview",
        Some("Backend title".to_string()),
        3,
        4,
    ));
    state
        .set_thread_generated_title_if_absent(&generated_thread_id, " Generated title ", 5)
        .unwrap();
    state
        .set_thread_manual_title(&manual_thread_id, " Manual title ", 6)
        .unwrap();
    state
        .mark_thread_rebind_required(&manual_thread_id, "Original member detached")
        .unwrap();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let loaded = persistence.load_workspace_state(&workspace_id).unwrap();
    let generated = loaded
        .thread_registration(&generated_thread_id)
        .expect("generated thread should roundtrip");
    let manual = loaded
        .thread_registration(&manual_thread_id)
        .expect("manual thread should roundtrip");

    assert_eq!(generated.title(), Some("Generated title"));
    assert_eq!(
        generated.gui_title().unwrap().source(),
        ConversationThreadTitleSource::FirstCompletedTurn
    );
    assert_eq!(manual.backend_name(), Some("Backend title"));
    assert_eq!(manual.title(), Some("Manual title"));
    assert_eq!(
        manual.gui_title().unwrap().source(),
        ConversationThreadTitleSource::Manual
    );
    assert_eq!(
        manual.rebind_required().unwrap().detail(),
        "Original member detached"
    );

    root.close().unwrap();
}

#[test]
fn beryl_created_thread_title_generation_state_roundtrips_as_repairable_after_restart() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("thread_title_state").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Thread Titles", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let beryl_thread_id = ConversationThreadId::new("thread_beryl");
    let external_thread_id = ConversationThreadId::new("thread_external");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(
        RegisteredConversationThread::new(
            beryl_thread_id.clone(),
            execution_target.clone(),
            "",
            None,
            1,
            2,
        )
        .with_beryl_created(),
    );
    state.remember_thread(RegisteredConversationThread::new(
        external_thread_id.clone(),
        execution_target.clone(),
        "",
        None,
        3,
        4,
    ));

    assert!(state.thread_automatic_title_generation_eligible(&beryl_thread_id));
    assert!(!state.thread_automatic_title_generation_eligible(&external_thread_id));
    assert!(
        state
            .mark_thread_automatic_title_generation_started(&beryl_thread_id)
            .unwrap()
    );
    assert!(!state.thread_automatic_title_generation_eligible(&beryl_thread_id));

    state.remember_thread(RegisteredConversationThread::new(
        beryl_thread_id.clone(),
        execution_target,
        "Refreshed preview",
        None,
        5,
        6,
    ));

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let loaded = persistence.load_workspace_state(&workspace_id).unwrap();
    let loaded_thread = loaded.thread_registration(&beryl_thread_id).unwrap();

    assert!(loaded_thread.beryl_created());
    assert!(loaded_thread.automatic_title_generation_attempted());
    assert_eq!(
        loaded_thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::Abandoned
    );
    assert!(loaded.thread_automatic_title_generation_eligible(&beryl_thread_id));

    root.close().unwrap();
}

#[test]
fn legacy_attempted_thread_title_generation_state_loads_as_repairable() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("legacy_thread_title_state").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Legacy Titles", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    write_raw_workspace_conversation_state(
        &persistence,
        &workspace_id,
        json!({
            "threads": [
                {
                    "thread_id": "thread_legacy",
                    "execution_target": {
                        "runtime_mode": "HostWindows",
                        "canonical_path": "C:\\work\\beryl"
                    },
                    "preview": "Legacy preview",
                    "beryl_created": true,
                    "automatic_title_generation_attempted": true,
                    "created_at_millis": 1,
                    "updated_at_millis": 2
                }
            ],
            "active_thread": "thread_legacy"
        }),
    );

    let thread_id = ConversationThreadId::new("thread_legacy");
    let loaded = persistence.load_workspace_state(&workspace_id).unwrap();
    let thread = loaded.thread_registration(&thread_id).unwrap();

    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::Abandoned
    );
    assert!(loaded.thread_automatic_title_generation_eligible(&thread_id));

    root.close().unwrap();
}

#[test]
fn suppressed_automatic_thread_title_backend_name_roundtrips() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("suppressed_thread_title_name").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Suppressed Titles", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_branch");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(
        RegisteredConversationThread::new(
            thread_id.clone(),
            execution_target,
            "Branch preview",
            None,
            1,
            2,
        )
        .with_beryl_created()
        .with_ignored_backend_name_for_automatic_title(Some("Source title".to_string())),
    );

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let loaded = persistence.load_workspace_state(&workspace_id).unwrap();
    let thread = loaded.thread_registration(&thread_id).unwrap();

    assert_eq!(thread.backend_name(), None);
    assert_eq!(
        thread.ignored_backend_name_for_automatic_title(),
        Some("Source title")
    );
    assert!(loaded.thread_automatic_title_generation_eligible(&thread_id));

    root.close().unwrap();
}

#[test]
fn backend_thread_name_snapshot_roundtrips_separately_from_generated_fallback() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("thread_names").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Thread Names", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_generated");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Generated preview",
        None,
        1,
        2,
    ));
    state
        .set_thread_generated_title_if_absent(&thread_id, "Generated title", 3)
        .unwrap();
    state
        .set_thread_backend_name(&thread_id, Some("Backend title".to_string()))
        .unwrap();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let loaded = persistence.load_workspace_state(&workspace_id).unwrap();
    let thread = loaded
        .thread_registration(&thread_id)
        .expect("thread should roundtrip");

    assert_eq!(thread.backend_name(), Some("Backend title"));
    assert_eq!(thread.title(), Some("Backend title"));
    assert_eq!(
        thread.gui_title().unwrap().source(),
        ConversationThreadTitleSource::FirstCompletedTurn
    );

    root.close().unwrap();
}

#[test]
fn token_usage_snapshots_record_replace_and_roundtrip_in_workspace_state() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("usage_snapshots").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Usage", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_usage");
    let first = token_usage_snapshot("turn_1", 240, Some(200_000), 100);
    let replacement = token_usage_snapshot("turn_2", 320, Some(200_000), 200);
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Usage preview",
        None,
        1,
        2,
    ));
    state
        .record_thread_token_usage_snapshot(&thread_id, first)
        .unwrap();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let mut loaded = persistence.load_workspace_state(&workspace_id).unwrap();
    assert_eq!(
        loaded
            .thread_token_usage_snapshot(&thread_id)
            .unwrap()
            .turn_id()
            .as_str(),
        "turn_1"
    );

    loaded
        .record_thread_token_usage_snapshot(&thread_id, replacement.clone())
        .unwrap();
    persistence
        .save_workspace_state(&workspace_id, &loaded)
        .unwrap();

    let replaced = persistence.load_workspace_state(&workspace_id).unwrap();
    assert_eq!(
        replaced.thread_token_usage_snapshot(&thread_id),
        Some(&replacement)
    );

    root.close().unwrap();
}

#[test]
fn token_usage_notification_snapshot_persists_without_touching_workspace_manifest() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("usage_notification").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Usage", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_usage");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Usage preview",
        None,
        1,
        2,
    ));

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let snapshot = token_usage_snapshot_adapter::thread_token_usage_snapshot(
        "turn_usage",
        &backend_token_usage(),
        900,
    );
    assert!(
        persistence
            .record_thread_token_usage_snapshot(&workspace_id, &thread_id, snapshot)
            .unwrap()
    );
    assert!(matches!(
        persistence.record_thread_token_usage_snapshot(
            &workspace_id,
            &ConversationThreadId::new("missing_thread"),
            token_usage_snapshot_adapter::thread_token_usage_snapshot(
                "turn_missing",
                &backend_token_usage(),
                901,
            ),
        ),
        Err(beryl_app::WorkspacePersistenceError::RecordThreadTokenUsageSnapshot { .. })
    ));

    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_state = persistence.load_workspace_state(&workspace_id).unwrap();
    let snapshot = stored_state
        .thread_token_usage_snapshot(&thread_id)
        .expect("token usage snapshot should persist");

    assert_eq!(stored_manifest.last_updated_at_millis(), 42);
    assert_eq!(snapshot.turn_id().as_str(), "turn_usage");
    assert_eq!(snapshot.last().cached_input_tokens(), 10);
    assert_eq!(snapshot.last().input_tokens(), 240);
    assert_eq!(snapshot.total().total_tokens(), 520);
    assert_eq!(snapshot.model_context_window(), Some(200_000));
    assert_eq!(snapshot.observed_at_millis(), 900);

    root.close().unwrap();
}

#[test]
fn workspace_ui_state_roundtrips_separately_from_conversation_state() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("workspace_ui").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace UI", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut conversation = WorkspaceConversationState::default();
    let ui_state = WorkspaceUiState::new(WorkspaceActivityPanelMode::On, 176.5);

    conversation
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &conversation)
        .unwrap();
    persistence
        .save_workspace_ui_state(&workspace_id, &ui_state)
        .unwrap();

    let loaded_ui_state = persistence.load_workspace_ui_state(&workspace_id).unwrap();
    assert_eq!(
        loaded_ui_state.tool_activity_panel_mode(),
        WorkspaceActivityPanelMode::On
    );
    assert_eq!(loaded_ui_state.tool_activity_panel_height_px(), 176.5);

    let mut loaded_conversation = persistence.load_workspace_state(&workspace_id).unwrap();
    loaded_conversation.clear_active_thread();
    persistence
        .save_workspace_state(&workspace_id, &loaded_conversation)
        .unwrap();

    assert_eq!(
        persistence.load_workspace_ui_state(&workspace_id).unwrap(),
        ui_state
    );

    root.close().unwrap();
}

#[test]
fn missing_workspace_ui_state_loads_activity_auto_default() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("workspace_ui_default").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace UI", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();

    let loaded_ui_state = persistence.load_workspace_ui_state(&workspace_id).unwrap();
    assert_eq!(
        loaded_ui_state.tool_activity_panel_mode(),
        WorkspaceActivityPanelMode::Auto
    );
    assert_eq!(loaded_ui_state.tool_activity_panel_height_px(), 112.0);

    root.close().unwrap();
}

#[test]
fn legacy_workspace_ui_state_enabled_boolean_maps_to_modes() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let enabled_workspace_id = BerylWorkspaceId::new("workspace_ui_legacy_enabled").unwrap();
    let disabled_workspace_id = BerylWorkspaceId::new("workspace_ui_legacy_disabled").unwrap();
    let enabled_manifest =
        BerylWorkspaceManifest::named(enabled_workspace_id.clone(), "Enabled UI", 42);
    let disabled_manifest =
        BerylWorkspaceManifest::named(disabled_workspace_id.clone(), "Disabled UI", 42);

    persistence
        .save_workspace_manifest(&enabled_manifest)
        .unwrap();
    persistence
        .save_workspace_manifest(&disabled_manifest)
        .unwrap();
    write_raw_workspace_ui_state(
        &persistence,
        &enabled_workspace_id,
        json!({
            "tool_activity_panel_enabled": true,
            "tool_activity_panel_height_px": 150.0
        }),
    );
    write_raw_workspace_ui_state(
        &persistence,
        &disabled_workspace_id,
        json!({
            "tool_activity_panel_enabled": false,
            "tool_activity_panel_height_px": 160.0
        }),
    );

    let enabled = persistence
        .load_workspace_ui_state(&enabled_workspace_id)
        .unwrap();
    let disabled = persistence
        .load_workspace_ui_state(&disabled_workspace_id)
        .unwrap();

    assert_eq!(
        enabled.tool_activity_panel_mode(),
        WorkspaceActivityPanelMode::On
    );
    assert_eq!(enabled.tool_activity_panel_height_px(), 150.0);
    assert_eq!(
        disabled.tool_activity_panel_mode(),
        WorkspaceActivityPanelMode::Off
    );
    assert_eq!(disabled.tool_activity_panel_height_px(), 160.0);

    root.close().unwrap();
}

#[test]
fn workspace_ui_state_uses_auto_when_no_mode_or_legacy_boolean_is_stored() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("workspace_ui_absent_mode").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace UI", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();
    write_raw_workspace_ui_state(
        &persistence,
        &workspace_id,
        json!({
            "tool_activity_panel_height_px": 170.0
        }),
    );

    let loaded = persistence.load_workspace_ui_state(&workspace_id).unwrap();
    assert_eq!(
        loaded.tool_activity_panel_mode(),
        WorkspaceActivityPanelMode::Auto
    );
    assert_eq!(loaded.tool_activity_panel_height_px(), 170.0);

    root.close().unwrap();
}

#[test]
fn workspace_ui_state_mode_field_wins_over_legacy_boolean() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("workspace_ui_mode_wins").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace UI", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();
    write_raw_workspace_ui_state(
        &persistence,
        &workspace_id,
        json!({
            "tool_activity_panel_mode": "auto",
            "tool_activity_panel_enabled": true,
            "tool_activity_panel_height_px": 180.0
        }),
    );

    let loaded = persistence.load_workspace_ui_state(&workspace_id).unwrap();
    assert_eq!(
        loaded.tool_activity_panel_mode(),
        WorkspaceActivityPanelMode::Auto
    );
    assert_eq!(loaded.tool_activity_panel_height_px(), 180.0);

    root.close().unwrap();
}

#[test]
fn workspace_activity_panel_mode_cycles_in_toolbar_order() {
    assert_eq!(
        WorkspaceActivityPanelMode::Auto.next(),
        WorkspaceActivityPanelMode::On
    );
    assert_eq!(
        WorkspaceActivityPanelMode::On.next(),
        WorkspaceActivityPanelMode::Off
    );
    assert_eq!(
        WorkspaceActivityPanelMode::Off.next(),
        WorkspaceActivityPanelMode::Auto
    );
    assert_eq!(WorkspaceActivityPanelMode::Auto.label(), "Activity Auto");
    assert_eq!(WorkspaceActivityPanelMode::On.label(), "Activity On");
    assert_eq!(WorkspaceActivityPanelMode::Off.label(), "Activity Off");
    assert_eq!(WorkspaceActivityPanelMode::Auto.value_label(), "Auto");
    assert_eq!(WorkspaceActivityPanelMode::On.value_label(), "On");
    assert_eq!(WorkspaceActivityPanelMode::Off.value_label(), "Off");
}

#[test]
fn activity_auto_shows_for_accepted_parent_turn_and_hides_when_it_ends() {
    assert!(WorkspaceActivityPanelMode::Auto.panel_visible(true, false));
    assert!(!WorkspaceActivityPanelMode::Auto.panel_visible(false, false));
}

#[test]
fn activity_auto_shows_during_selected_thread_context_compaction() {
    assert!(WorkspaceActivityPanelMode::Auto.panel_visible(false, true));
    assert!(WorkspaceActivityPanelMode::Auto.panel_visible(true, true));
}

#[test]
fn activity_on_and_off_override_current_work_state() {
    assert!(WorkspaceActivityPanelMode::On.panel_visible(false, false));
    assert!(WorkspaceActivityPanelMode::On.panel_visible(true, true));
    assert!(!WorkspaceActivityPanelMode::Off.panel_visible(false, false));
    assert!(!WorkspaceActivityPanelMode::Off.panel_visible(true, true));
}

#[test]
fn workspace_ui_state_persists_without_touching_workspace_manifest() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("workspace_ui_manifest").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace UI", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_ui_state(
            &workspace_id,
            &WorkspaceUiState::new(WorkspaceActivityPanelMode::On, 144.0),
        )
        .unwrap();

    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();

    assert_eq!(stored_manifest.last_updated_at_millis(), 42);

    root.close().unwrap();
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-workspace-conversation-state-test-")
}

fn write_raw_workspace_ui_state(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    value: serde_json::Value,
) {
    write_raw_workspace_record(persistence, workspace_id, WORKSPACE_UI_STATE_KEY, value);
}

fn write_raw_workspace_conversation_state(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    value: serde_json::Value,
) {
    write_raw_workspace_record(
        persistence,
        workspace_id,
        WORKSPACE_CONVERSATION_STATE_KEY,
        value,
    );
}

fn write_raw_workspace_record(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    key: &str,
    value: serde_json::Value,
) {
    let database_path = persistence.workspace_database_path(workspace_id);
    let database = Database::open(&database_path).unwrap();
    let record_bytes = serde_json::to_vec(&value).unwrap();
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(WORKSPACE_METADATA_TABLE).unwrap();
        table.insert(key, record_bytes.as_slice()).unwrap();
    }
    write_txn.commit().unwrap();
}

fn token_usage_snapshot(
    turn_id: &str,
    input_tokens: i64,
    model_context_window: Option<i64>,
    observed_at_millis: u64,
) -> ConversationThreadTokenUsageSnapshot {
    ConversationThreadTokenUsageSnapshot::new(
        ConversationTurnId::new(turn_id),
        ConversationTokenUsageBreakdown::new(2, input_tokens, 5, 7, input_tokens + 14),
        ConversationTokenUsageBreakdown::new(3, input_tokens + 20, 11, 13, input_tokens + 47),
        model_context_window,
        observed_at_millis,
    )
}

fn backend_token_usage() -> ThreadTokenUsage {
    ThreadTokenUsage {
        last: TokenUsageBreakdown {
            cached_input_tokens: 10,
            input_tokens: 240,
            output_tokens: 30,
            reasoning_output_tokens: 40,
            total_tokens: 310,
        },
        total: TokenUsageBreakdown {
            cached_input_tokens: 20,
            input_tokens: 420,
            output_tokens: 50,
            reasoning_output_tokens: 30,
            total_tokens: 520,
        },
        model_context_window: Some(200_000),
    }
}
