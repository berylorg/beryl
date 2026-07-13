#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::fs;

use beryl_app::{
    BerylWorkspacePersistence, StartupMetadata, StartupPersistence, WorkspacePersistenceError,
    create_fresh_untitled_workspace, delete_workspace_and_resolve_active_replacement,
    resolve_startup_state,
};
use beryl_model::{
    conversation::{PrimaryWorkspaceMember, WorkspaceConversationState},
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest, BerylWorkspaceTitleError, RuntimeMode},
};

#[test]
fn startup_metadata_roundtrips_recent_workspaces() {
    let root = unique_temp_dir();
    let persistence = StartupPersistence::new(&root);
    let host_workspace = BerylWorkspaceId::new("graphics_learning").unwrap();
    let wsl_workspace = BerylWorkspaceId::new("solver_debugging").unwrap();

    let mut metadata = StartupMetadata::default();
    metadata.remember_workspace(host_workspace.clone());
    metadata.remember_workspace(wsl_workspace.clone());

    persistence.save(&metadata).unwrap();
    let loaded = persistence.load().unwrap();

    assert_eq!(
        loaded.recent_workspaces(),
        &[wsl_workspace.clone(), host_workspace.clone()]
    );
    assert_eq!(loaded.last_opened_workspace(), Some(&wsl_workspace));

    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn startup_metadata_failed_persist_preserves_existing_metadata_file() {
    let root = unique_temp_dir();
    let persistence = StartupPersistence::new(&root);
    let original_workspace = BerylWorkspaceId::new("graphics_learning").unwrap();
    let replacement_workspace = BerylWorkspaceId::new("solver_debugging").unwrap();
    let mut original = StartupMetadata::default();
    let mut replacement = StartupMetadata::default();

    original.allocate_untitled_workspace_sequence();
    original.remember_workspace(original_workspace);
    replacement.remember_workspace(replacement_workspace);

    persistence.save(&original).unwrap();
    let metadata_path = root.join("startup-state.json");
    let original_text = fs::read_to_string(&metadata_path).unwrap();
    let lock = tempdir_support::lock_file_against_replacement(&metadata_path).unwrap();

    assert!(persistence.save(&replacement).is_err());
    drop(lock);

    assert_eq!(persistence.load().unwrap(), original);
    assert_eq!(fs::read_to_string(metadata_path).unwrap(), original_text);
    root.close().unwrap();
}

#[test]
fn startup_metadata_replaces_renamed_workspace_without_reordering_recents() {
    let old_workspace = BerylWorkspaceId::new("graphics_learning").unwrap();
    let sibling = BerylWorkspaceId::new("solver_debugging").unwrap();
    let renamed = BerylWorkspaceId::new("beryl").unwrap();
    let mut metadata = StartupMetadata::default();

    metadata.remember_workspace(sibling.clone());
    metadata.remember_workspace(old_workspace.clone());
    metadata.replace_workspace(&old_workspace, renamed.clone());

    assert_eq!(
        metadata.recent_workspaces(),
        &[renamed.clone(), sibling.clone()]
    );
    assert_eq!(metadata.last_opened_workspace(), Some(&renamed));
}

#[test]
fn workspace_title_rename_rewrites_startup_metadata_references() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let active = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("graphics_learning").unwrap(),
        "Graphics Learning",
        42,
    );
    let sibling = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("solver_debugging").unwrap(),
        "Solver Debugging",
        84,
    );
    let mut metadata = StartupMetadata::default();

    workspaces.save_workspace_manifest(&active).unwrap();
    workspaces.save_workspace_manifest(&sibling).unwrap();
    metadata.remember_workspace(sibling.id().clone());
    metadata.remember_workspace(active.id().clone());
    startup.save(&metadata).unwrap();

    let renamed = workspaces
        .set_workspace_manual_title(active.id(), "Beryl")
        .unwrap()
        .unwrap();
    let persisted = startup.load().unwrap();

    assert_eq!(renamed.id().as_str(), "beryl");
    assert_eq!(persisted.last_opened_workspace(), Some(renamed.id()));
    assert_eq!(persisted.recent_workspaces().first(), Some(renamed.id()));
    assert_eq!(persisted.recent_workspaces().get(1), Some(sibling.id()));

    root.close().unwrap();
}

#[test]
fn workspace_title_rename_refuses_slug_collision_without_moving_old_state() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let active = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("graphics_learning").unwrap(),
        "Graphics Learning",
        42,
    );
    let collision =
        BerylWorkspaceManifest::named(BerylWorkspaceId::new("beryl").unwrap(), "Beryl", 84);
    let mut metadata = StartupMetadata::default();

    workspaces.save_workspace_manifest(&active).unwrap();
    workspaces.save_workspace_manifest(&collision).unwrap();
    metadata.remember_workspace(active.id().clone());
    startup.save(&metadata).unwrap();

    let error = workspaces
        .set_workspace_manual_title(active.id(), "Beryl")
        .unwrap_err();
    let persisted = startup.load().unwrap();

    assert!(matches!(
        error,
        WorkspacePersistenceError::WorkspaceTitle {
            source: BerylWorkspaceTitleError::SlugEquivalentCollision { slug },
        } if slug.as_str() == "beryl"
    ));
    assert!(workspaces.workspace_dir(active.id()).exists());
    assert!(workspaces.workspace_dir(collision.id()).exists());
    assert_eq!(
        workspaces.load_workspace_manifest(active.id()).unwrap(),
        Some(active.clone())
    );
    assert_eq!(
        workspaces.load_workspace_manifest(collision.id()).unwrap(),
        Some(collision)
    );
    assert_eq!(persisted.last_opened_workspace(), Some(active.id()));

    root.close().unwrap();
}

#[test]
fn workspace_title_rename_refuses_legacy_title_slug_collision() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let active = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("graphics_learning").unwrap(),
        "Graphics Learning",
        42,
    );
    let legacy_collision =
        BerylWorkspaceManifest::named(BerylWorkspaceId::new("legacy_beryl").unwrap(), "Beryl", 84);
    let mut metadata = StartupMetadata::default();

    workspaces.save_workspace_manifest(&active).unwrap();
    workspaces
        .save_workspace_manifest(&legacy_collision)
        .unwrap();
    metadata.remember_workspace(active.id().clone());
    startup.save(&metadata).unwrap();

    let error = workspaces
        .set_workspace_manual_title(active.id(), "Beryl")
        .unwrap_err();
    let persisted = startup.load().unwrap();

    assert!(matches!(
        error,
        WorkspacePersistenceError::WorkspaceTitle {
            source: BerylWorkspaceTitleError::SlugEquivalentCollision { slug },
        } if slug.as_str() == "beryl"
    ));
    assert!(workspaces.workspace_dir(active.id()).exists());
    assert!(workspaces.workspace_dir(legacy_collision.id()).exists());
    assert!(
        !workspaces
            .workspace_dir(&BerylWorkspaceId::new("beryl").unwrap())
            .exists()
    );
    assert_eq!(
        workspaces.load_workspace_manifest(active.id()).unwrap(),
        Some(active.clone())
    );
    assert_eq!(
        workspaces
            .load_workspace_manifest(legacy_collision.id())
            .unwrap(),
        Some(legacy_collision)
    );
    assert_eq!(persisted.last_opened_workspace(), Some(active.id()));

    root.close().unwrap();
}

#[test]
fn startup_resolution_recovers_interrupted_workspace_directory_rename() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let old_manifest = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("graphics_learning").unwrap(),
        "Graphics Learning",
        42,
    );
    let mut new_manifest = old_manifest.clone();
    let mut metadata = StartupMetadata::default();

    new_manifest.set_manual_title("Beryl").unwrap();
    metadata.remember_workspace(old_manifest.id().clone());
    workspaces.save_workspace_manifest(&old_manifest).unwrap();
    startup.save(&metadata).unwrap();
    fs::write(
        root.join("workspace-rename-transaction.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "old_workspace_id": old_manifest.id(),
            "new_workspace_id": new_manifest.id(),
            "old_manifest": old_manifest,
            "new_manifest": new_manifest,
        }))
        .unwrap(),
    )
    .unwrap();
    fs::rename(
        workspaces.workspace_dir(&BerylWorkspaceId::new("graphics_learning").unwrap()),
        workspaces.workspace_dir(&BerylWorkspaceId::new("beryl").unwrap()),
    )
    .unwrap();

    let resolved = resolve_startup_state(&startup, &workspaces).unwrap();
    let persisted = startup.load().unwrap();

    assert_eq!(resolved.active_workspace().id().as_str(), "beryl");
    assert_eq!(resolved.active_workspace().title(), "Beryl");
    assert_eq!(
        persisted.last_opened_workspace(),
        Some(resolved.active_workspace().id())
    );
    assert!(
        workspaces
            .load_workspace_manifest(&BerylWorkspaceId::new("graphics_learning").unwrap())
            .unwrap()
            .is_none()
    );
    assert!(
        workspaces
            .load_workspace_manifest(&BerylWorkspaceId::new("beryl").unwrap())
            .unwrap()
            .is_some()
    );
    assert!(!root.join("workspace-rename-transaction.json").exists());

    root.close().unwrap();
}

#[test]
fn startup_resolution_preserves_newer_manifest_during_stale_rename_marker_recovery() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let old_manifest = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("graphics_learning").unwrap(),
        "Graphics Learning",
        42,
    );
    let mut stale_new_manifest = old_manifest.clone();
    let current_manifest =
        BerylWorkspaceManifest::named(BerylWorkspaceId::new("beryl").unwrap(), "Beryl", 777);
    let mut metadata = StartupMetadata::default();

    stale_new_manifest.set_manual_title("Beryl").unwrap();
    metadata.remember_workspace(old_manifest.id().clone());
    workspaces.save_workspace_manifest(&old_manifest).unwrap();
    startup.save(&metadata).unwrap();
    fs::write(
        root.join("workspace-rename-transaction.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "old_workspace_id": old_manifest.id(),
            "new_workspace_id": stale_new_manifest.id(),
            "old_manifest": old_manifest,
            "new_manifest": stale_new_manifest,
        }))
        .unwrap(),
    )
    .unwrap();
    fs::rename(
        workspaces.workspace_dir(&BerylWorkspaceId::new("graphics_learning").unwrap()),
        workspaces.workspace_dir(&BerylWorkspaceId::new("beryl").unwrap()),
    )
    .unwrap();
    workspaces
        .save_workspace_manifest(&current_manifest)
        .unwrap();

    let resolved = resolve_startup_state(&startup, &workspaces).unwrap();
    let persisted_manifest = workspaces
        .load_workspace_manifest(&BerylWorkspaceId::new("beryl").unwrap())
        .unwrap()
        .unwrap();
    let persisted_metadata = startup.load().unwrap();

    assert_eq!(resolved.active_workspace().id().as_str(), "beryl");
    assert_eq!(persisted_manifest, current_manifest);
    assert_eq!(
        persisted_metadata.last_opened_workspace(),
        Some(current_manifest.id())
    );
    assert!(!root.join("workspace-rename-transaction.json").exists());

    root.close().unwrap();
}

#[test]
fn startup_state_uses_last_opened_semantic_workspace_when_it_exists() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let named = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("graphics_learning").unwrap(),
        "Graphics Learning",
        42,
    );
    let mut metadata = StartupMetadata::default();

    workspaces.save_workspace_manifest(&named).unwrap();
    metadata.remember_workspace(named.id().clone());
    startup.save(&metadata).unwrap();

    let resolved = resolve_startup_state(&startup, &workspaces).unwrap();

    assert_eq!(resolved.active_workspace(), &named);
    assert_eq!(
        workspaces
            .load_workspace_state(named.id())
            .unwrap()
            .selected_runtime(),
        None
    );
    assert!(
        resolved
            .known_workspaces()
            .iter()
            .any(|workspace| workspace == &named)
    );
    assert_eq!(resolved.startup_warning(), None);

    root.close().unwrap();
}

#[test]
fn startup_state_creates_host_runtime_untitled_workspace_for_fresh_app_home() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);

    let resolved = resolve_startup_state(&startup, &workspaces).unwrap();
    let persisted = startup.load().unwrap();
    let state = workspaces
        .load_workspace_state(resolved.active_workspace().id())
        .unwrap();

    assert!(resolved.active_workspace().is_untitled());
    assert_host_implicit_home(&state);
    assert_eq!(
        persisted.last_opened_workspace(),
        Some(resolved.active_workspace().id())
    );
    assert_eq!(persisted.next_untitled_workspace_sequence(), 2);

    root.close().unwrap();
}

#[test]
fn startup_state_known_workspaces_are_sorted_by_recent_use() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let graphics = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("graphics_learning").unwrap(),
        "Graphics Learning",
        42,
    );
    let solver = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("solver_debugging").unwrap(),
        "Solver Debugging",
        84,
    );
    let mut metadata = StartupMetadata::default();

    workspaces.save_workspace_manifest(&graphics).unwrap();
    workspaces.save_workspace_manifest(&solver).unwrap();
    metadata.remember_workspace(graphics.id().clone());
    metadata.remember_workspace(solver.id().clone());
    startup.save(&metadata).unwrap();

    let resolved = resolve_startup_state(&startup, &workspaces).unwrap();

    assert_eq!(resolved.active_workspace().id(), solver.id());
    assert_eq!(resolved.known_workspaces()[0].id(), solver.id());
    assert_eq!(resolved.known_workspaces()[1].id(), graphics.id());

    root.close().unwrap();
}

#[test]
fn startup_state_creates_untitled_workspace_when_last_opened_is_missing() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let missing_workspace = BerylWorkspaceId::new("missing_workspace").unwrap();
    let mut metadata = StartupMetadata::default();

    metadata.remember_workspace(missing_workspace.clone());
    startup.save(&metadata).unwrap();

    let resolved = resolve_startup_state(&startup, &workspaces).unwrap();
    let persisted = startup.load().unwrap();
    let state = workspaces
        .load_workspace_state(resolved.active_workspace().id())
        .unwrap();

    assert!(resolved.active_workspace().is_untitled());
    assert_ne!(resolved.active_workspace().id(), &missing_workspace);
    assert_host_implicit_home(&state);
    assert!(resolved.startup_warning().is_some());
    assert_eq!(
        persisted.last_opened_workspace(),
        Some(resolved.active_workspace().id())
    );
    assert_eq!(persisted.next_untitled_workspace_sequence(), 2);

    root.close().unwrap();
}

#[test]
fn fresh_untitled_workspace_is_materialized_in_redb_storage() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);

    let manifest = persistence.create_untitled_workspace(1).unwrap().unwrap();
    let stored = persistence
        .load_workspace_manifest(manifest.id())
        .unwrap()
        .unwrap();
    let state = persistence.load_workspace_state(manifest.id()).unwrap();

    assert_eq!(stored, manifest);
    assert!(stored.is_untitled());
    assert_host_implicit_home(&state);
    assert!(persistence.workspace_database_path(manifest.id()).exists());

    root.close().unwrap();
}

#[test]
fn fresh_untitled_workspace_skips_unlisted_partial_creation_artifact() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let partial_id = BerylWorkspaceId::untitled(1);

    fs::create_dir_all(workspaces.workspace_dir(&partial_id)).unwrap();

    let fresh = create_fresh_untitled_workspace(&startup, &workspaces).unwrap();
    let persisted = startup.load().unwrap();
    let listed = workspaces.list_workspace_manifests().unwrap();
    let state = workspaces.load_workspace_state(fresh.id()).unwrap();

    assert_eq!(fresh.id(), &BerylWorkspaceId::untitled(2));
    assert_host_implicit_home(&state);
    assert_eq!(listed, vec![fresh.clone()]);
    assert!(
        workspaces
            .load_workspace_manifest(&partial_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(persisted.last_opened_workspace(), Some(fresh.id()));
    assert_eq!(persisted.next_untitled_workspace_sequence(), 3);

    root.close().unwrap();
}

#[test]
fn fresh_untitled_workspace_creation_updates_recent_picker_metadata() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let existing = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("existing_workspace").unwrap(),
        "Existing Workspace",
        42,
    );
    let mut metadata = StartupMetadata::default();

    workspaces.save_workspace_manifest(&existing).unwrap();
    metadata.remember_workspace(existing.id().clone());
    startup.save(&metadata).unwrap();

    let fresh = create_fresh_untitled_workspace(&startup, &workspaces).unwrap();
    let persisted = startup.load().unwrap();
    let state = workspaces.load_workspace_state(fresh.id()).unwrap();

    assert!(fresh.is_untitled());
    assert_host_implicit_home(&state);
    assert_eq!(persisted.last_opened_workspace(), Some(fresh.id()));
    assert_eq!(persisted.recent_workspaces().first(), Some(fresh.id()));
    assert_eq!(persisted.next_untitled_workspace_sequence(), 2);

    root.close().unwrap();
}

#[test]
fn workspace_listing_does_not_materialize_legacy_scratchpad() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);

    let manifests = persistence.list_workspace_manifests().unwrap();

    assert!(manifests.is_empty());

    root.close().unwrap();
}

#[test]
fn workspace_listing_roundtrips_named_workspace_metadata() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let named = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("graphics_learning").unwrap(),
        "Graphics Learning",
        42,
    );

    persistence.save_workspace_manifest(&named).unwrap();

    let manifests = persistence.list_workspace_manifests().unwrap();

    assert!(manifests.iter().any(|manifest| manifest == &named));

    root.close().unwrap();
}

#[test]
fn active_workspace_deletion_opens_fresh_untitled_workspace() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let active = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("active_workspace").unwrap(),
        "Active Workspace",
        42,
    );
    let sibling = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("sibling_workspace").unwrap(),
        "Sibling Workspace",
        84,
    );
    let mut metadata = StartupMetadata::default();

    workspaces.save_workspace_manifest(&active).unwrap();
    workspaces.save_workspace_manifest(&sibling).unwrap();
    metadata.remember_workspace(sibling.id().clone());
    metadata.remember_workspace(active.id().clone());
    startup.save(&metadata).unwrap();

    let outcome = delete_workspace_and_resolve_active_replacement(
        &startup,
        &workspaces,
        active.id(),
        active.id(),
    )
    .unwrap();
    let replacement = outcome.replacement_workspace().unwrap();
    let persisted = startup.load().unwrap();
    let replacement_state = workspaces.load_workspace_state(replacement.id()).unwrap();

    assert!(outcome.deleted());
    assert!(replacement.is_untitled());
    assert_host_implicit_home(&replacement_state);
    assert_eq!(persisted.last_opened_workspace(), Some(replacement.id()));
    assert_eq!(persisted.next_untitled_workspace_sequence(), 2);
    assert!(
        workspaces
            .load_workspace_manifest(active.id())
            .unwrap()
            .is_none()
    );
    assert!(
        workspaces
            .load_workspace_manifest(sibling.id())
            .unwrap()
            .is_some()
    );
    assert!(
        outcome
            .known_workspaces()
            .iter()
            .any(|workspace| workspace.id() == replacement.id())
    );

    root.close().unwrap();
}

#[test]
fn inactive_workspace_deletion_keeps_current_workspace_active() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let active = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("active_workspace").unwrap(),
        "Active Workspace",
        42,
    );
    let inactive = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("inactive_workspace").unwrap(),
        "Inactive Workspace",
        84,
    );
    let mut metadata = StartupMetadata::default();

    workspaces.save_workspace_manifest(&active).unwrap();
    workspaces.save_workspace_manifest(&inactive).unwrap();
    metadata.remember_workspace(inactive.id().clone());
    startup.save(&metadata).unwrap();

    let outcome = delete_workspace_and_resolve_active_replacement(
        &startup,
        &workspaces,
        inactive.id(),
        active.id(),
    )
    .unwrap();
    let persisted = startup.load().unwrap();

    assert!(outcome.deleted());
    assert!(outcome.replacement_workspace().is_none());
    assert_eq!(persisted.last_opened_workspace(), Some(active.id()));
    assert!(
        workspaces
            .load_workspace_manifest(inactive.id())
            .unwrap()
            .is_none()
    );
    assert!(
        workspaces
            .load_workspace_manifest(active.id())
            .unwrap()
            .is_some()
    );

    root.close().unwrap();
}

#[test]
fn untitled_workspace_sequence_is_not_reused_after_deletion() {
    let root = unique_temp_dir();
    let startup = StartupPersistence::new(&root);
    let workspaces = BerylWorkspacePersistence::new(&root);
    let mut metadata = StartupMetadata::default();

    let first = workspaces.create_untitled_workspace(1).unwrap().unwrap();
    metadata.allocate_untitled_workspace_sequence();
    metadata.remember_workspace(first.id().clone());
    startup.save(&metadata).unwrap();

    delete_workspace_and_resolve_active_replacement(&startup, &workspaces, first.id(), first.id())
        .unwrap();
    let persisted = startup.load().unwrap();

    assert_eq!(
        persisted.last_opened_workspace(),
        Some(&BerylWorkspaceId::untitled(2))
    );
    assert_eq!(persisted.next_untitled_workspace_sequence(), 3);
    assert!(
        workspaces
            .load_workspace_manifest(&BerylWorkspaceId::untitled(1))
            .unwrap()
            .is_none()
    );
    assert!(
        workspaces
            .load_workspace_manifest(&BerylWorkspaceId::untitled(2))
            .unwrap()
            .is_some()
    );
    assert_host_implicit_home(
        &workspaces
            .load_workspace_state(&BerylWorkspaceId::untitled(2))
            .unwrap(),
    );

    root.close().unwrap();
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-startup-persistence-test-")
}

fn assert_host_implicit_home(state: &WorkspaceConversationState) {
    assert_eq!(state.selected_runtime(), Some(&RuntimeMode::HostWindows));
    assert!(state.explicit_members().is_empty());
    assert!(matches!(
        state.primary_member(),
        Some(PrimaryWorkspaceMember::ImplicitHome(
            RuntimeMode::HostWindows
        ))
    ));
}
