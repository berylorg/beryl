#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{
    BerylWorkspacePersistence, WorkspaceGraphUpkeepPolicy, normalize_graph_upkeep_instructions_text,
};
use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest};
use redb::{Database, TableDefinition};
use serde_json::json;

const WORKSPACE_METADATA_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("workspace_metadata");
const WORKSPACE_GRAPH_UPKEEP_POLICY_KEY: &str = "graph_upkeep_policy";

#[test]
fn missing_graph_upkeep_policy_loads_disabled_default() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_upkeep_default").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Upkeep", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();

    let policy = persistence
        .load_workspace_graph_upkeep_policy(&workspace_id)
        .unwrap();
    assert_eq!(policy.instructions(), None);

    root.close().unwrap();
}

#[test]
fn graph_upkeep_policy_roundtrips_without_touching_manifest_timestamp() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_upkeep_roundtrip").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Upkeep", 42);
    let policy = WorkspaceGraphUpkeepPolicy::with_instructions(Some(
        "Keep plan nodes current.\r\nPrefer concise summaries.".to_string(),
    ));

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_upkeep_policy(&workspace_id, &policy)
        .unwrap();

    let loaded = persistence
        .load_workspace_graph_upkeep_policy(&workspace_id)
        .unwrap();
    let loaded_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();

    assert_eq!(
        loaded.instructions(),
        Some("Keep plan nodes current.\nPrefer concise summaries.")
    );
    assert_eq!(loaded_manifest.last_updated_at_millis(), 42);

    root.close().unwrap();
}

#[test]
fn graph_upkeep_policy_save_rejects_missing_workspace_manifest() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_upkeep_missing_manifest").unwrap();
    let policy = WorkspaceGraphUpkeepPolicy::with_instructions(Some("Track plan nodes.".into()));

    let error = persistence
        .save_workspace_graph_upkeep_policy(&workspace_id, &policy)
        .expect_err("missing workspace manifest should reject graph-upkeep policy save");

    assert!(error.to_string().contains("workspace manifest"));
    assert!(!persistence.workspace_dir(&workspace_id).exists());

    root.close().unwrap();
}

#[test]
fn workspace_title_change_moves_graph_upkeep_policy() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_upkeep_rename").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Upkeep", 42);
    let policy = WorkspaceGraphUpkeepPolicy::with_instructions(Some(
        "Preserve this policy across rename.".to_string(),
    ));

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_upkeep_policy(&workspace_id, &policy)
        .unwrap();

    let renamed = persistence
        .set_workspace_manual_title(&workspace_id, "Renamed Graph Upkeep")
        .unwrap()
        .unwrap();
    let loaded = persistence
        .load_workspace_graph_upkeep_policy(renamed.id())
        .unwrap();

    assert_eq!(
        loaded.instructions(),
        Some("Preserve this policy across rename.")
    );
    assert!(
        persistence
            .load_workspace_manifest(&workspace_id)
            .unwrap()
            .is_none()
    );

    root.close().unwrap();
}

#[test]
fn blank_graph_upkeep_policy_is_disabled() {
    assert_eq!(normalize_graph_upkeep_instructions_text(" \n\t "), None);

    let policy = WorkspaceGraphUpkeepPolicy::with_instructions(Some(" \r\n\t ".to_string()));

    assert_eq!(policy.instructions(), None);
}

#[test]
fn malformed_graph_upkeep_policy_record_falls_back_to_disabled() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_upkeep_malformed").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Upkeep", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();
    write_raw_graph_upkeep_policy(
        &persistence,
        &workspace_id,
        json!({ "instructions": ["legacy", "bad"] }),
    );

    let loaded = persistence
        .load_workspace_graph_upkeep_policy(&workspace_id)
        .unwrap();

    assert_eq!(loaded.instructions(), None);

    root.close().unwrap();
}

fn write_raw_graph_upkeep_policy(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    value: serde_json::Value,
) {
    let database_path = persistence.workspace_database_path(workspace_id);
    let database = Database::open(&database_path).unwrap();
    let record_bytes = serde_json::to_vec(&value).unwrap();
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(WORKSPACE_METADATA_TABLE).unwrap();
        table
            .insert(WORKSPACE_GRAPH_UPKEEP_POLICY_KEY, record_bytes.as_slice())
            .unwrap();
    }
    write_txn.commit().unwrap();
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-workspace-graph-upkeep-policy-test-")
}
