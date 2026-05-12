#![allow(dead_code, private_interfaces, unused_imports)]

#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    cell::RefCell,
    collections::VecDeque,
    env,
    ffi::OsString,
    fs,
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

pub use beryl_app::{
    BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE, BerylWorkspacePersistence, LifecycleYieldOutcome,
    NodeLeafDeleteRequest, NodeSubtreeDeleteRequest, ThreadRefUpsertRequest,
    UPSERT_GRAPH_NODE_TOOL, WorkspaceGraphMutationCommit, WorkspaceGraphRevision,
    WorkspaceGraphToolService, WorkspaceImageAsset, WorkspaceImageAssetStatus,
    WorkspacePersistenceError, YIELD_TOOL, beryl_user_thread_start_options,
    dispatch_beryl_dynamic_tool_call_with_metadata,
};
use beryl_backend::{
    ApprovalRequest, DynamicToolCallOutputContentItem, DynamicToolCallRequest,
    DynamicToolCallResponse, TurnInfo, TurnStatus, TurnStreamEvent,
    parse_dynamic_tool_call_request,
};
use beryl_model::{
    semantic_graph::{
        SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp, SemanticNodeDraft,
        SemanticNodeFacets, SemanticNodeId,
    },
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest},
};
use redb::{Database, TableDefinition};
use serde_json::{Value, json};

const WORKSPACE_METADATA_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("workspace_metadata");

#[path = "../src/memory_diagnostics.rs"]
mod memory_diagnostics;

mod shell {
    #[path = "../../src/shell/column_selector.rs"]
    pub(super) mod column_selector;
    #[path = "../../src/shell/composer_draft.rs"]
    pub(super) mod composer_draft;
    #[path = "../../src/shell/composer_image_delivery.rs"]
    pub(super) mod composer_image_delivery;
    #[path = "../../src/shell/execution_detail.rs"]
    pub(crate) mod execution_detail;
    #[path = "../../src/shell/graph.rs"]
    pub(super) mod graph;
    #[path = "../../src/shell/graph_worker.rs"]
    pub(super) mod graph_worker;
    #[path = "../../src/shell/thread_activation.rs"]
    pub(super) mod thread_activation;
    #[path = "../../src/shell/thread_selection.rs"]
    pub(super) mod thread_selection;
    #[path = "../../src/shell/thread_title.rs"]
    pub(super) mod thread_title;
    #[path = "../../src/shell/transcript_history.rs"]
    pub(super) mod transcript_history;
    #[path = "../../src/shell/transcript_image_sources.rs"]
    pub(super) mod transcript_image_sources;
    #[path = "../../src/shell/turn_worker.rs"]
    pub(super) mod turn_worker;
}

use shell::turn_worker::{
    TurnStreamBackend, handle_beryl_dynamic_tool_call, stream_active_turn_events,
};

#[test]
fn graph_dynamic_noop_write_publishes_quiet_commit_update() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("turn_dynamic_noop").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Turn Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &root_graph())
        .unwrap();
    let request = upsert_root_request();
    let mut graph_updates = Vec::new();

    let handled = handle_beryl_dynamic_tool_call(&service, &workspace_id, &request, |update| {
        graph_updates.push(update)
    });

    assert!(handled.into_response().success);
    assert_eq!(graph_updates.len(), 1);
    let shell::graph::GraphMutationUpdate::Commit(update) = graph_updates.pop().unwrap() else {
        panic!("dynamic graph write must publish a graph commit update");
    };
    assert!(!update.commit.changed);
    assert!(update.no_op_message.is_empty());
    assert_eq!(
        update.commit.committed_revision,
        WorkspaceGraphRevision::new(1)
    );

    root.close().unwrap();
}

#[test]
fn graph_dynamic_write_response_carries_ordered_root_summary_for_turn_worker() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("turn_dynamic_multi_root").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Turn Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &root_graph())
        .unwrap();
    let request = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "docs_root",
            "parentId": null,
            "title": "Docs",
            "summary": "Documentation work.",
            "topic": true,
            "checklist": false,
            "checklistItem": false
        }),
    );
    let mut graph_updates = Vec::new();

    let handled = handle_beryl_dynamic_tool_call(&service, &workspace_id, &request, |update| {
        graph_updates.push(update)
    });
    let payload = response_json(&handled.clone().into_response());
    let stored = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(handled.into_response().success);
    assert_eq!(payload["result"]["summary"]["rootNodeCount"], 2);
    assert_eq!(payload["result"]["summary"]["rootNodes"][0]["id"], "root");
    assert_eq!(
        payload["result"]["summary"]["rootNodes"][1]["id"],
        "docs_root"
    );
    assert_eq!(
        stored.root_node_ids(),
        &[
            SemanticNodeId::new("root").unwrap(),
            SemanticNodeId::new("docs_root").unwrap()
        ]
    );
    assert_eq!(graph_updates.len(), 1);
    let shell::graph::GraphMutationUpdate::Commit(update) = graph_updates.pop().unwrap() else {
        panic!("dynamic graph write must publish a graph commit update");
    };
    assert!(update.commit.changed);
    assert_eq!(
        update.commit.committed_revision,
        WorkspaceGraphRevision::new(1)
    );

    root.close().unwrap();
}

#[test]
fn graph_tool_write_uses_injected_root_when_environment_home_differs() {
    let env_home = unique_temp_dir();
    let injected_root = unique_temp_dir();
    let workspace_id = BerylWorkspaceId::new("turn_dynamic_injected_root").unwrap();

    with_environment_home(&env_home, || {
        let persistence = BerylWorkspacePersistence::new(&injected_root);
        let service = WorkspaceGraphToolService::new(persistence.clone());
        let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Turn Dynamic", 42);
        persistence.save_workspace_manifest(&manifest).unwrap();

        let handled =
            handle_beryl_dynamic_tool_call(&service, &workspace_id, &upsert_root_request(), |_| {});

        assert!(handled.into_response().success);
        assert!(
            persistence
                .load_workspace_graph_state(&workspace_id)
                .unwrap()
                .node(&SemanticNodeId::new("root").unwrap())
                .is_some()
        );
    });

    assert!(
        injected_root
            .join("workspaces")
            .join(workspace_id.as_str())
            .join("workspace.redb")
            .exists()
    );
    assert!(!env_home.join(".beryl").exists());

    injected_root.close().unwrap();
    let _ = env_home.close();
}

#[test]
fn graph_dynamic_write_publishes_failure_when_persisted_graph_revision_is_missing() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("turn_dynamic_missing_revision").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Turn Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    write_graph_record_without_revision(&persistence, &workspace_id, &root_graph());
    let request = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "docs_root",
            "parentId": null,
            "title": "Docs",
            "summary": "Documentation work.",
            "topic": true,
            "checklist": false,
            "checklistItem": false
        }),
    );
    let mut graph_updates = Vec::new();

    let handled = handle_beryl_dynamic_tool_call(&service, &workspace_id, &request, |update| {
        graph_updates.push(update)
    });
    let payload = response_json(&handled.into_response());

    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "graph_tool_error");
    assert_eq!(graph_updates.len(), 1);
    let shell::graph::GraphMutationUpdate::Failure(update) = graph_updates.pop().unwrap() else {
        panic!("dynamic graph write failure must publish a graph failure update");
    };
    assert_eq!(update.workspace_id, workspace_id);
    assert!(update.message.contains("semantic graph revision"));

    root.close().unwrap();
}

#[test]
fn stream_dynamic_graph_write_publishes_update_before_success_response() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("turn_dynamic_stream").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Turn Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    let request = upsert_root_request();
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut backend = FakeTurnStreamBackend::new(
        [
            Ok(Some(TurnStreamEvent::DynamicToolCallRequested(
                request.clone(),
            ))),
            Ok(Some(turn_completed("thread_1", "turn_1"))),
            Ok(None),
        ],
        log.clone(),
    );
    let mut graph_updates = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        Duration::from_secs(10),
        Duration::from_millis(500),
        |request| {
            handle_beryl_dynamic_tool_call(&service, &workspace_id, request, |update| {
                log.borrow_mut().push("graph_update");
                graph_updates.push(update);
            })
        },
        |_| panic!("test did not expect a lifecycle yield"),
        |_| Ok(()),
    )
    .unwrap();

    assert_eq!(&*log.borrow(), &["graph_update", "response"]);
    assert_eq!(backend.dynamic_tool_responses.len(), 1);
    assert_eq!(backend.dynamic_tool_responses[0].0, request);
    assert!(backend.dynamic_tool_responses[0].1.success);
    assert_eq!(
        response_json(&backend.dynamic_tool_responses[0].1)["ok"],
        true
    );
    assert_eq!(graph_updates.len(), 1);
    let shell::graph::GraphMutationUpdate::Commit(update) = graph_updates.pop().unwrap() else {
        panic!("dynamic graph write must publish a graph commit update");
    };
    assert!(update.commit.changed);
    assert_eq!(
        update.commit.committed_revision,
        WorkspaceGraphRevision::new(1)
    );

    root.close().unwrap();
}

struct FakeTurnStreamBackend {
    events: VecDeque<Result<Option<TurnStreamEvent>, String>>,
    dynamic_tool_responses: Vec<(DynamicToolCallRequest, DynamicToolCallResponse)>,
    log: Rc<RefCell<Vec<&'static str>>>,
}

impl FakeTurnStreamBackend {
    fn new<const N: usize>(
        events: [Result<Option<TurnStreamEvent>, String>; N],
        log: Rc<RefCell<Vec<&'static str>>>,
    ) -> Self {
        Self {
            events: VecDeque::from(events),
            dynamic_tool_responses: Vec::new(),
            log,
        }
    }
}

impl TurnStreamBackend for FakeTurnStreamBackend {
    type Error = String;

    fn next_turn_stream_event(
        &mut self,
        _: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error> {
        self.events
            .pop_front()
            .unwrap_or_else(|| Err("unexpected extra stream poll".to_string()))
    }

    fn deny_approval_request(&mut self, _: &ApprovalRequest) -> Result<(), Self::Error> {
        Ok(())
    }

    fn respond_dynamic_tool_call(
        &mut self,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), Self::Error> {
        self.log.borrow_mut().push("response");
        self.dynamic_tool_responses
            .push((request.clone(), response.clone()));
        Ok(())
    }

    fn interrupt_turn(&mut self, _: &str, _: &str, _: Duration) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn upsert_root_request() -> DynamicToolCallRequest {
    dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "root",
            "parentId": null,
            "title": "Root",
            "summary": "Root summary",
            "topic": true,
            "checklist": false,
            "checklistItem": false
        }),
    )
}

fn dynamic_tool_request(tool: &str, arguments: Value) -> DynamicToolCallRequest {
    parse_dynamic_tool_call_request(
        json!("dynamic-request-1"),
        "item/tool/call",
        Some(json!({
            "threadId": "thread_1",
            "turnId": "turn_1",
            "callId": "call_1",
            "namespace": BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE,
            "tool": tool,
            "arguments": arguments
        })),
    )
    .unwrap()
    .unwrap()
}

fn root_graph() -> SemanticGraph {
    let root_id = SemanticNodeId::new("root").unwrap();
    let mut graph = SemanticGraph::default();
    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    root_id.clone(),
                    "Root",
                    "Root summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: beryl_model::provenance::MutationProvenance::new(
                    "operator",
                    1,
                    beryl_model::provenance::MutationSource::workspace_action("seed_graph")
                        .unwrap(),
                    Some(100),
                )
                .unwrap(),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id,
                parent_id: None,
                index: None,
                provenance: beryl_model::provenance::MutationProvenance::new(
                    "operator",
                    2,
                    beryl_model::provenance::MutationSource::workspace_action("seed_graph")
                        .unwrap(),
                    Some(100),
                )
                .unwrap(),
            },
        ]))
        .unwrap();
    graph
}

fn turn_completed(thread_id: &str, turn_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::TurnCompleted {
        thread_id: thread_id.to_string(),
        turn: TurnInfo {
            id: turn_id.to_string(),
            status: TurnStatus::Completed,
            items: Vec::new(),
            error: None,
        },
    }
}

fn response_json(response: &DynamicToolCallResponse) -> Value {
    let Some(DynamicToolCallOutputContentItem::InputText { text }) = response.content_items.first()
    else {
        panic!("expected a single text content item")
    };
    serde_json::from_str(text).unwrap()
}

fn write_graph_record_without_revision(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    graph: &SemanticGraph,
) {
    let database_path = persistence.workspace_database_path(workspace_id);
    let database = Database::open(&database_path).unwrap();
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(WORKSPACE_METADATA_TABLE).unwrap();
        let graph_bytes = serde_json::to_vec(graph).unwrap();
        table
            .insert("semantic_graph_state", graph_bytes.as_slice())
            .unwrap();
    }
    write_txn.commit().unwrap();
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-turn-worker-graph-dynamic-test-")
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
