#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::fs;

use beryl_app::{
    ACTIVATE_THEME_TOOL, BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE, BERYL_DYNAMIC_TOOL_NAMESPACE,
    BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE, BerylWorkspacePersistence,
    DIAGNOSTIC_CHILD_CLOSE_POPUPS_TOOL, DIAGNOSTIC_CHILD_CREATE_NEW_THREAD_TOOL,
    DIAGNOSTIC_CHILD_HARD_STOP_TURN_TOOL, DIAGNOSTIC_CHILD_LIST_WORKSPACE_THREADS_TOOL,
    DIAGNOSTIC_CHILD_PREPARE_RENDERER_WINDOW_TOOL, DIAGNOSTIC_CHILD_READ_MEDIA_EVENTS_TOOL,
    DIAGNOSTIC_CHILD_READ_MEMORY_TOOL, DIAGNOSTIC_CHILD_READ_PROCESS_TOOL,
    DIAGNOSTIC_CHILD_READ_RENDERER_TOOL, DIAGNOSTIC_CHILD_READ_RETAINED_STATE_TOOL,
    DIAGNOSTIC_CHILD_READ_TRANSCRIPT_FRAME_METRICS_TOOL, DIAGNOSTIC_CHILD_READ_UI_STATE_TOOL,
    DIAGNOSTIC_CHILD_READ_VISIBLE_MEDIA_TOOL, DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL,
    DIAGNOSTIC_CHILD_SOFT_STOP_TURN_TOOL, DIAGNOSTIC_CHILD_START_TOOL,
    DIAGNOSTIC_CHILD_START_TURN_TOOL, DIAGNOSTIC_CHILD_STATUS_TOOL, DIAGNOSTIC_CHILD_STOP_TOOL,
    DIAGNOSTIC_CHILD_SWITCH_THREAD_TOOL, DIAGNOSTIC_CHILD_SWITCH_WORKSPACE_TOOL,
    DIAGNOSTIC_CHILD_WAIT_FOR_STATE_TOOL, INSTALL_THEME_TOOL, LifecycleYieldOutcome,
    PREVIEW_THEME_TOOL, READ_CHECKLIST_TOOL, READ_GRAPH_NEIGHBORHOOD_TOOL, READ_GUI_SETTINGS_TOOL,
    READ_MEDIA_EVENTS_TOOL, READ_MEMORY_DIAGNOSTICS_TOOL, READ_PROCESS_DIAGNOSTICS_TOOL,
    READ_RENDERER_DIAGNOSTICS_TOOL, READ_RETAINED_STATE_SUMMARY_TOOL,
    READ_THEME_AUTHORING_GUIDE_TOOL, READ_THEME_REPOSITORY_TOOL, READ_THEME_SCHEMA_TOOL,
    READ_TRANSCRIPT_FRAME_METRICS_TOOL, READ_VISIBLE_MEDIA_TOOL, READ_WORKSPACE_GRAPH_SUMMARY_TOOL,
    SAVE_THEME_AS_TOOL, SET_CHECKLIST_ITEM_STATUS_TOOL, SET_GRAPH_NODE_PARENT_TOOL,
    STOP_THEME_PREVIEW_TOOL, UPDATE_GUI_SETTINGS_TOOL, UPDATE_THEME_TOOL, UPSERT_GRAPH_NODE_TOOL,
    UPSERT_GRAPH_SOFT_LINK_TOOL, VALIDATE_GUI_SETTINGS_UPDATE_TOOL, VALIDATE_THEME_DOCUMENT_TOOL,
    WorkspaceGraphToolService, YIELD_TOOL, beryl_diagnostic_child_dynamic_tool_specs,
    beryl_dynamic_tool_specs, beryl_lifecycle_dynamic_tool_specs, beryl_thread_start_options,
    beryl_user_thread_start_options, dispatch_beryl_dynamic_tool_call_with_metadata,
    dispatch_beryl_graph_dynamic_tool_call, dispatch_beryl_graph_dynamic_tool_call_with_metadata,
    dispatch_beryl_lifecycle_dynamic_tool_call_with_metadata, validate_unique_dynamic_tool_names,
};
use beryl_backend::{
    DynamicToolCallOutputContentItem, DynamicToolCallRequest, DynamicToolCallResponse,
    DynamicToolSpec, parse_dynamic_tool_call_request,
};
use beryl_model::{
    provenance::MutationSource,
    semantic_graph::{ChecklistItemStatus, SemanticNodeId, SoftLinkId},
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest},
};
use serde_json::{Value, json};

const READ_SETTINGS_WINDOW_DIAGNOSTICS_TOOL_NAME: &str = "read_settings_window_diagnostics";
const DIAGNOSTIC_CHILD_READ_SETTINGS_WINDOW_TOOL_NAME: &str = "read_settings_window";

#[test]
fn beryl_thread_start_options_register_graph_and_lifecycle_dynamic_tools() {
    let options = beryl_thread_start_options();
    let tools = options.dynamic_tools();
    let tool_keys: Vec<_> = tools
        .iter()
        .map(|tool| (tool.namespace.as_deref(), tool.name.as_str()))
        .collect();

    assert!(!options.is_ephemeral());
    assert_eq!(
        tool_keys,
        vec![
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_WORKSPACE_GRAPH_SUMMARY_TOOL
            ),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_GRAPH_NEIGHBORHOOD_TOOL
            ),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), READ_CHECKLIST_TOOL),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), UPSERT_GRAPH_NODE_TOOL),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                SET_GRAPH_NODE_PARENT_TOOL
            ),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                UPSERT_GRAPH_SOFT_LINK_TOOL
            ),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                SET_CHECKLIST_ITEM_STATUS_TOOL
            ),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), YIELD_TOOL),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_THEME_REPOSITORY_TOOL
            ),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), READ_THEME_SCHEMA_TOOL),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_THEME_AUTHORING_GUIDE_TOOL
            ),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                VALIDATE_THEME_DOCUMENT_TOOL
            ),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), PREVIEW_THEME_TOOL),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), STOP_THEME_PREVIEW_TOOL),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), INSTALL_THEME_TOOL),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), UPDATE_THEME_TOOL),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), SAVE_THEME_AS_TOOL),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), ACTIVATE_THEME_TOOL),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), READ_GUI_SETTINGS_TOOL),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                VALIDATE_GUI_SETTINGS_UPDATE_TOOL
            ),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), UPDATE_GUI_SETTINGS_TOOL),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_PROCESS_DIAGNOSTICS_TOOL
            ),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_MEMORY_DIAGNOSTICS_TOOL
            ),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_RENDERER_DIAGNOSTICS_TOOL
            ),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_RETAINED_STATE_SUMMARY_TOOL
            ),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), READ_VISIBLE_MEDIA_TOOL),
            (Some(BERYL_DYNAMIC_TOOL_NAMESPACE), READ_MEDIA_EVENTS_TOOL),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_TRANSCRIPT_FRAME_METRICS_TOOL
            ),
            (
                Some(BERYL_DYNAMIC_TOOL_NAMESPACE),
                READ_SETTINGS_WINDOW_DIAGNOSTICS_TOOL_NAME
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_START_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_STOP_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_STATUS_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_READ_PROCESS_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_READ_MEMORY_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_READ_RENDERER_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_PREPARE_RENDERER_WINDOW_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_READ_UI_STATE_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_READ_RETAINED_STATE_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_READ_VISIBLE_MEDIA_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_READ_MEDIA_EVENTS_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_READ_TRANSCRIPT_FRAME_METRICS_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_READ_SETTINGS_WINDOW_TOOL_NAME
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_LIST_WORKSPACE_THREADS_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_CREATE_NEW_THREAD_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_START_TURN_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_SOFT_STOP_TURN_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_HARD_STOP_TURN_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_WAIT_FOR_STATE_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_SWITCH_WORKSPACE_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_SWITCH_THREAD_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL
            ),
            (
                Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE),
                DIAGNOSTIC_CHILD_CLOSE_POPUPS_TOOL
            ),
        ]
    );
    assert!(tools.iter().all(|tool| tool.defer_loading == Some(false)));
    assert_api_valid_namespaces(tools);
    validate_unique_dynamic_tool_names(tools).unwrap();
}

#[test]
fn graph_parent_tool_schema_requires_explicit_root_placement() {
    let tools = beryl_dynamic_tool_specs();
    let parent_tool = tools
        .iter()
        .find(|tool| tool.name == SET_GRAPH_NODE_PARENT_TOOL)
        .expect("parent tool must be registered");

    assert_eq!(
        parent_tool.input_schema["required"],
        json!(["childId", "parentId"])
    );
    assert!(
        parent_tool.input_schema["properties"]["parentId"]["description"]
            .as_str()
            .unwrap()
            .contains("Use null to make the child root-level")
    );
}

#[test]
fn beryl_user_thread_start_options_include_dynamic_tools_without_developer_instructions() {
    let options = beryl_user_thread_start_options();

    assert!(!options.is_ephemeral());
    assert_eq!(options.developer_instructions(), None);
    assert!(
        options
            .dynamic_tools()
            .iter()
            .any(|tool| tool.name == YIELD_TOOL)
    );
    validate_unique_dynamic_tool_names(options.dynamic_tools()).unwrap();
}

#[test]
fn beryl_user_thread_start_options_include_graph_summary_tool() {
    let options = beryl_user_thread_start_options();

    assert_eq!(options.developer_instructions(), None);
    assert!(
        options
            .dynamic_tools()
            .iter()
            .any(|tool| tool.name == READ_WORKSPACE_GRAPH_SUMMARY_TOOL)
    );
}

#[test]
fn lifecycle_yield_tool_spec_accepts_only_outcome() {
    let tools = beryl_lifecycle_dynamic_tool_specs();
    let yield_tool = tools
        .iter()
        .find(|tool| tool.name == YIELD_TOOL)
        .expect("yield tool must be registered");

    assert_eq!(
        yield_tool.namespace.as_deref(),
        Some(BERYL_DYNAMIC_TOOL_NAMESPACE)
    );
    assert_eq!(yield_tool.defer_loading, Some(false));
    assert_eq!(yield_tool.input_schema["required"], json!(["outcome"]));
    assert_eq!(yield_tool.input_schema["additionalProperties"], false);
    assert_eq!(
        yield_tool.input_schema["properties"]["outcome"]["enum"],
        json!([
            "phase_needs_review",
            "blocked_needs_operator",
            "phase_continue",
            "plan_complete"
        ])
    );
    assert!(
        yield_tool
            .description
            .contains("semantic lifecycle outcome")
    );
}

#[test]
fn theme_authoring_and_validation_tool_specs_are_bounded() {
    let tools = beryl_dynamic_tool_specs();
    let guide_tool = tools
        .iter()
        .find(|tool| tool.name == READ_THEME_AUTHORING_GUIDE_TOOL)
        .expect("theme authoring guide tool must be registered");
    let validation_tool = tools
        .iter()
        .find(|tool| tool.name == VALIDATE_THEME_DOCUMENT_TOOL)
        .expect("theme validation tool must be registered");

    assert_eq!(
        guide_tool.namespace.as_deref(),
        Some(BERYL_DYNAMIC_TOOL_NAMESPACE)
    );
    assert_eq!(
        validation_tool.namespace.as_deref(),
        Some(BERYL_DYNAMIC_TOOL_NAMESPACE)
    );
    assert_eq!(guide_tool.defer_loading, Some(false));
    assert_eq!(validation_tool.defer_loading, Some(false));
    assert_eq!(guide_tool.input_schema["additionalProperties"], false);
    assert_eq!(validation_tool.input_schema["additionalProperties"], false);
    assert_eq!(
        guide_tool.input_schema["properties"]["section"]["enum"],
        json!([
            "all",
            "overview",
            "syntax",
            "inheritance",
            "role_groups",
            "transcript_roles",
            "code_roles",
            "settings_roles",
            "examples",
            "troubleshooting"
        ])
    );
    assert_eq!(
        validation_tool.input_schema["required"],
        json!(["document"])
    );
    assert_eq!(
        validation_tool.input_schema["properties"]["explainRoles"]["maxItems"],
        32
    );
}

#[test]
fn diagnostic_tool_specs_are_bounded_and_read_only() {
    let tools = beryl_dynamic_tool_specs();
    let visible = tools
        .iter()
        .find(|tool| tool.name == READ_VISIBLE_MEDIA_TOOL)
        .expect("visible-media diagnostics tool must be registered");
    let renderer = tools
        .iter()
        .find(|tool| tool.name == READ_RENDERER_DIAGNOSTICS_TOOL)
        .expect("renderer diagnostics tool must be registered");
    let events = tools
        .iter()
        .find(|tool| tool.name == READ_MEDIA_EVENTS_TOOL)
        .expect("media-events diagnostics tool must be registered");
    let frame_metrics = tools
        .iter()
        .find(|tool| tool.name == READ_TRANSCRIPT_FRAME_METRICS_TOOL)
        .expect("transcript-frame diagnostics tool must be registered");

    assert_eq!(
        visible.namespace.as_deref(),
        Some(BERYL_DYNAMIC_TOOL_NAMESPACE)
    );
    assert_eq!(visible.defer_loading, Some(false));
    assert_eq!(renderer.defer_loading, Some(false));
    assert_eq!(renderer.input_schema["additionalProperties"], false);
    assert_eq!(visible.input_schema["additionalProperties"], false);
    assert_eq!(visible.input_schema["properties"]["limit"]["maximum"], 64);
    assert_eq!(events.input_schema["additionalProperties"], false);
    assert_eq!(events.input_schema["properties"]["limit"]["maximum"], 128);
    assert!(events.input_schema["properties"]["afterSequence"].is_object());
    assert_eq!(frame_metrics.input_schema["additionalProperties"], false);
    assert_eq!(
        frame_metrics.input_schema["properties"]["limit"]["maximum"],
        64
    );
    assert!(frame_metrics.input_schema["properties"]["afterSequence"].is_object());
}

#[test]
fn diagnostic_child_tool_specs_use_separate_namespace() {
    let tools = beryl_diagnostic_child_dynamic_tool_specs();
    let start = tools
        .iter()
        .find(|tool| tool.name == DIAGNOSTIC_CHILD_START_TOOL)
        .expect("diagnostic child start tool must be registered");
    let switch_workspace = tools
        .iter()
        .find(|tool| tool.name == DIAGNOSTIC_CHILD_SWITCH_WORKSPACE_TOOL)
        .expect("diagnostic child switch_workspace tool must be registered");
    let frame_metrics = tools
        .iter()
        .find(|tool| tool.name == DIAGNOSTIC_CHILD_READ_TRANSCRIPT_FRAME_METRICS_TOOL)
        .expect("diagnostic child frame-metrics tool must be registered");

    assert!(tools.iter().all(|tool| {
        tool.namespace.as_deref() == Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE)
            && tool.defer_loading == Some(false)
    }));
    assert!(!BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE.contains('.'));
    assert_eq!(start.input_schema["required"], json!(["berylHomeDir"]));
    assert_eq!(
        switch_workspace.input_schema["required"],
        json!(["workspaceId"])
    );
    assert_eq!(
        frame_metrics.input_schema["properties"]["limit"]["maximum"],
        64
    );
    validate_unique_dynamic_tool_names(&tools).unwrap();
}

#[test]
fn local_supervisor_gui_control_tools_are_not_registered() {
    let tools = beryl_dynamic_tool_specs();

    for removed_tool in [
        "read_ui_state",
        "switch_thread",
        "scroll_transcript",
        "close_popups",
    ] {
        assert!(
            tools.iter().all(|tool| {
                tool.namespace.as_deref() != Some(BERYL_DYNAMIC_TOOL_NAMESPACE)
                    || tool.name != removed_tool
            }),
            "{removed_tool} must not be registered as a local supervisor dynamic tool"
        );
    }
}

#[test]
fn lifecycle_yield_call_accepts_supported_outcome() {
    let request = dynamic_tool_request(
        YIELD_TOOL,
        json!({
            "outcome": "phase_continue"
        }),
    );
    let dispatch = dispatch_beryl_lifecycle_dynamic_tool_call_with_metadata(&request);
    let payload = response_json(dispatch.response());

    assert_eq!(
        dispatch.outcome(),
        Some(LifecycleYieldOutcome::PhaseContinue)
    );
    assert!(dispatch.response().success);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["result"]["outcome"], "phase_continue");
}

#[test]
fn beryl_dynamic_tool_dispatch_routes_lifecycle_yield_without_graph_write() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("dynamic_dispatch").unwrap();
    let request = dynamic_tool_request(
        YIELD_TOOL,
        json!({
            "outcome": "phase_needs_review"
        }),
    );

    let dispatch =
        dispatch_beryl_dynamic_tool_call_with_metadata(&service, &workspace_id, &request);
    let payload = response_json(dispatch.response());

    assert!(dispatch.response().success);
    assert_eq!(
        dispatch.lifecycle_yield(),
        Some(LifecycleYieldOutcome::PhaseNeedsReview)
    );
    assert!(dispatch.graph_write().is_none());
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["result"]["outcome"], "phase_needs_review");

    root.close().unwrap();
}

#[test]
fn lifecycle_yield_call_rejects_malformed_outcome_arguments() {
    for arguments in [
        json!({}),
        json!({ "outcome": "compact" }),
        json!({
            "outcome": "phase_continue",
            "after": "compact"
        }),
    ] {
        let request = dynamic_tool_request(YIELD_TOOL, arguments);
        let dispatch = dispatch_beryl_lifecycle_dynamic_tool_call_with_metadata(&request);
        let payload = response_json(dispatch.response());

        assert_eq!(dispatch.outcome(), None);
        assert!(!dispatch.response().success);
        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["kind"], "invalid_arguments");
    }
}

#[test]
fn beryl_dynamic_tool_registry_rejects_duplicate_names() {
    let mut tools = beryl_dynamic_tool_specs();
    tools.push(
        DynamicToolSpec::new(
            YIELD_TOOL,
            "duplicate yield",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        )
        .with_namespace(BERYL_DYNAMIC_TOOL_NAMESPACE),
    );

    let error = validate_unique_dynamic_tool_names(&tools).unwrap_err();

    assert_eq!(error.namespace(), Some(BERYL_DYNAMIC_TOOL_NAMESPACE));
    assert_eq!(error.name(), YIELD_TOOL);
}

#[test]
fn dynamic_summary_call_reads_the_bound_workspace() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let request = dynamic_tool_request(READ_WORKSPACE_GRAPH_SUMMARY_TOOL, json!({}));
    let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["result"]["manifest"]["id"], "graph_dynamic");
    assert_eq!(payload["result"]["rootNodeCount"], 0);
    assert_eq!(payload["result"]["rootNodes"], json!([]));
    assert_eq!(payload["result"]["rootNodesTruncated"], false);
    assert_eq!(payload["result"]["nodeCount"], 0);

    root.close().unwrap();
}

#[test]
fn dynamic_summary_call_returns_ordered_root_snapshots() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    for request in [
        dynamic_tool_request(
            UPSERT_GRAPH_NODE_TOOL,
            json!({
                "nodeId": "root_a",
                "parentId": null,
                "title": "Root A",
                "summary": "Root A summary",
                "topic": true,
                "checklist": false,
                "checklistItem": false
            }),
        ),
        dynamic_tool_request(
            UPSERT_GRAPH_NODE_TOOL,
            json!({
                "nodeId": "root_b",
                "parentId": null,
                "title": "Root B",
                "summary": "Root B summary",
                "topic": true,
                "checklist": false,
                "checklistItem": false
            }),
        ),
    ] {
        let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
        assert!(response.success);
    }

    let request = dynamic_tool_request(READ_WORKSPACE_GRAPH_SUMMARY_TOOL, json!({}));
    let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["result"]["rootNodeCount"], 2);
    assert_eq!(payload["result"]["rootNodesTruncated"], false);
    assert_eq!(payload["result"]["rootNodes"][0]["id"], "root_a");
    assert_eq!(payload["result"]["rootNodes"][1]["id"], "root_b");

    root.close().unwrap();
}

#[test]
fn dynamic_graph_neighborhood_without_anchor_returns_root_level_response_shape() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    for request in [
        dynamic_tool_request(
            UPSERT_GRAPH_NODE_TOOL,
            json!({
                "nodeId": "root_a",
                "parentId": null,
                "title": "Root A",
                "summary": "Root A summary",
                "topic": true,
                "checklist": false,
                "checklistItem": false
            }),
        ),
        dynamic_tool_request(
            UPSERT_GRAPH_NODE_TOOL,
            json!({
                "nodeId": "root_b",
                "parentId": null,
                "title": "Root B",
                "summary": "Root B summary",
                "topic": true,
                "checklist": false,
                "checklistItem": false
            }),
        ),
    ] {
        let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
        assert!(response.success);
    }

    let request = dynamic_tool_request(READ_GRAPH_NEIGHBORHOOD_TOOL, json!({}));
    let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["result"]["anchorNodeId"], Value::Null);
    assert_eq!(payload["result"]["anchor"], Value::Null);
    assert_eq!(payload["result"]["lineage"], json!([]));
    assert_eq!(payload["result"]["summary"]["rootNodeCount"], 2);
    assert_eq!(payload["result"]["summary"]["rootNodes"][0]["id"], "root_a");
    assert_eq!(payload["result"]["summary"]["rootNodes"][1]["id"], "root_b");

    root.close().unwrap();
}

#[test]
fn dynamic_write_call_injects_app_server_call_provenance() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let request = dynamic_tool_request(
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
    );
    let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
    let payload = response_json(&response);
    let graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let provenance = graph
        .node(&SemanticNodeId::new("root").unwrap())
        .unwrap()
        .provenance()
        .created();

    assert!(response.success);
    assert_eq!(payload["result"]["summary"]["rootNodeCount"], 1);
    assert_eq!(payload["result"]["summary"]["rootNodes"][0]["id"], "root");
    assert_eq!(provenance.actor(), "codex");
    match provenance.source() {
        MutationSource::DynamicToolCall {
            thread_id,
            turn_id,
            tool_name,
            call_id,
        } => {
            assert_eq!(thread_id.as_str(), "thread_1");
            assert_eq!(turn_id.as_str(), "turn_1");
            assert_eq!(tool_name, UPSERT_GRAPH_NODE_TOOL);
            assert_eq!(call_id, "call_1");
        }
        other => panic!("expected dynamic tool provenance, got {other:?}"),
    }

    root.close().unwrap();
}

#[test]
fn explicit_dynamic_write_tools_apply_atomic_patches() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    for request in [
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
        ),
        dynamic_tool_request(
            UPSERT_GRAPH_NODE_TOOL,
            json!({
                "nodeId": "release_checklist",
                "parentId": "root",
                "title": "Release checklist",
                "summary": "Prepare the release.",
                "topic": true,
                "checklist": true,
                "checklistItem": false
            }),
        ),
        dynamic_tool_request(
            UPSERT_GRAPH_NODE_TOOL,
            json!({
                "nodeId": "draft",
                "parentId": "release_checklist",
                "title": "Draft release notes",
                "summary": "Write the release notes.",
                "topic": true,
                "checklist": false,
                "checklistItem": true,
                "checklistItemStatus": "todo"
            }),
        ),
        dynamic_tool_request(
            UPSERT_GRAPH_NODE_TOOL,
            json!({
                "nodeId": "archive_checklist",
                "parentId": "root",
                "title": "Archive checklist",
                "summary": "Preserve release artifacts.",
                "topic": true,
                "checklist": true,
                "checklistItem": false
            }),
        ),
        dynamic_tool_request(
            SET_GRAPH_NODE_PARENT_TOOL,
            json!({
                "childId": "draft",
                "parentId": "archive_checklist"
            }),
        ),
        dynamic_tool_request(
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
        ),
        dynamic_tool_request(
            UPSERT_GRAPH_SOFT_LINK_TOOL,
            json!({
                "linkId": "draft_depends_on_root",
                "sourceId": "draft",
                "targetId": "docs_root",
                "kind": "depends_on"
            }),
        ),
        dynamic_tool_request(
            SET_CHECKLIST_ITEM_STATUS_TOOL,
            json!({
                "nodeId": "draft",
                "status": "done"
            }),
        ),
    ] {
        let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
        assert!(
            response.success,
            "dynamic write failed for {}",
            request.tool()
        );
    }

    let graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let root_id = SemanticNodeId::new("root").unwrap();
    let docs_root_id = SemanticNodeId::new("docs_root").unwrap();
    let draft_id = SemanticNodeId::new("draft").unwrap();
    let archive_checklist_id = SemanticNodeId::new("archive_checklist").unwrap();
    let link_id = SoftLinkId::new("draft_depends_on_root").unwrap();

    assert_eq!(
        graph.root_node_ids(),
        &[root_id.clone(), docs_root_id.clone()]
    );
    assert_eq!(graph.parent_id_of(&draft_id), Some(&archive_checklist_id));
    assert_eq!(
        graph.node(&draft_id).unwrap().checklist_item_status(),
        Some(ChecklistItemStatus::Done)
    );
    assert_eq!(graph.soft_link(&link_id).unwrap().source_id(), &draft_id);
    assert_eq!(
        graph.soft_link(&link_id).unwrap().target_id(),
        &docs_root_id
    );

    root.close().unwrap();
}

#[test]
fn dynamic_parent_update_with_null_moves_child_to_root() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    for request in [
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
        ),
        dynamic_tool_request(
            UPSERT_GRAPH_NODE_TOOL,
            json!({
                "nodeId": "child",
                "parentId": "root",
                "title": "Child",
                "summary": "Child summary",
                "topic": true,
                "checklist": false,
                "checklistItem": false
            }),
        ),
    ] {
        let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
        assert!(response.success);
    }

    let request = dynamic_tool_request(
        SET_GRAPH_NODE_PARENT_TOOL,
        json!({
            "childId": "child",
            "parentId": null,
            "index": 0
        }),
    );
    let dispatch =
        dispatch_beryl_graph_dynamic_tool_call_with_metadata(&service, &workspace_id, &request);
    let payload = response_json(dispatch.response());
    let commit = dispatch.graph_write().unwrap().into_commit();
    let graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let root_id = SemanticNodeId::new("root").unwrap();

    assert!(dispatch.response().success);
    assert!(commit.changed);
    assert_eq!(commit.patch.operations().len(), 1);
    assert_eq!(payload["result"]["summary"]["rootNodeCount"], 2);
    assert_eq!(graph.parent_id_of(&child_id), None);
    assert_eq!(graph.root_node_ids(), &[child_id, root_id]);

    root.close().unwrap();
}

#[test]
fn dynamic_write_rejects_model_supplied_provenance() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let request = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "root",
            "parentId": null,
            "title": "Root",
            "summary": "Root summary",
            "topic": true,
            "checklist": false,
            "checklistItem": false,
            "provenance": { "actor": "untrusted" }
        }),
    );
    let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
    let payload = response_json(&response);
    let graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "invalid_arguments");
    assert!(graph.node(&SemanticNodeId::new("root").unwrap()).is_none());

    root.close().unwrap();
}

#[test]
fn dynamic_node_upsert_requires_explicit_parent_id() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let request = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "root",
            "title": "Root",
            "summary": "Root summary",
            "topic": true,
            "checklist": false,
            "checklistItem": false
        }),
    );
    let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
    let payload = response_json(&response);
    let graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "invalid_arguments");
    assert!(graph.node(&SemanticNodeId::new("root").unwrap()).is_none());

    root.close().unwrap();
}

#[test]
fn dynamic_parent_update_requires_explicit_parent_id() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let request = dynamic_tool_request(
        SET_GRAPH_NODE_PARENT_TOOL,
        json!({
            "childId": "root"
        }),
    );
    let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
    let payload = response_json(&response);
    let graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "invalid_arguments");
    assert!(graph.node(&SemanticNodeId::new("root").unwrap()).is_none());

    root.close().unwrap();
}

#[test]
fn dynamic_tool_call_reports_unsupported_tools_as_tool_errors() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let request = dynamic_tool_request("missing_tool", json!({}));
    let response = dispatch_beryl_graph_dynamic_tool_call(&service, &workspace_id, &request);
    let payload = response_json(&response);

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "unsupported_tool");

    root.close().unwrap();
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

fn response_json(response: &DynamicToolCallResponse) -> Value {
    let Some(DynamicToolCallOutputContentItem::InputText { text }) = response.content_items.first()
    else {
        panic!("expected a single text content item")
    };
    serde_json::from_str(text).unwrap()
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-workspace-graph-dynamic-tools-test-")
}

fn assert_api_valid_namespaces(tools: &[DynamicToolSpec]) {
    for tool in tools {
        if let Some(namespace) = tool.namespace.as_deref() {
            assert!(
                !namespace.contains('.'),
                "dynamic tool namespace {namespace:?} for {:?} must not contain dots",
                tool.name
            );
        }
    }
}
