#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub use beryl_app::{
    BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE, BERYL_DYNAMIC_TOOL_NAMESPACE,
    BerylWorkspacePersistence, LifecycleYieldOutcome, NodeLeafDeleteRequest,
    NodeSubtreeDeleteRequest, ThreadRefUpsertRequest, UPSERT_GRAPH_NODE_TOOL,
    WorkspaceGraphMutationCommit, WorkspaceGraphRevision, WorkspaceGraphToolService,
    WorkspaceImageAsset, WorkspaceImageAssetStatus, WorkspacePersistenceError, YIELD_TOOL,
    beryl_diagnostic_child_dynamic_tool_shell_response_timeout, beryl_thread_start_options,
    beryl_user_thread_start_options, diagnostic_bridge_unavailable_response,
    dispatch_beryl_dynamic_tool_call_with_metadata, dispatch_beryl_graph_dynamic_tool_call,
    dispatch_beryl_graph_dynamic_tool_call_with_metadata, is_beryl_diagnostic_child_dynamic_tool,
    is_beryl_diagnostic_dynamic_tool,
};
use beryl_backend::{
    ApprovalRequest, DynamicToolCallOutputContentItem, DynamicToolCallRequest,
    DynamicToolCallResponse, ThreadSessionResponse, ThreadStartOptions, ThreadStatus, TurnInfo,
    TurnStatus, TurnStreamEvent, UserInput, parse_approval_request,
    parse_dynamic_tool_call_request,
};
use beryl_model::{
    conversation::WorkspaceConversationState,
    semantic_graph::SemanticNodeId,
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest, RuntimeMode, WorkspaceId},
};
use serde_json::json;

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
    #[path = "../../src/shell/workspace_members.rs"]
    pub(super) mod workspace_members;

    fn text_fragment(text: &str) -> execution_detail::UserInputFragment {
        execution_detail::UserInputFragment::text(text)
    }

    fn backend_fragment(
        text: &str,
        backend_input: Vec<beryl_backend::UserInput>,
    ) -> execution_detail::UserInputFragment {
        execution_detail::UserInputFragment::from_backend_input(text, backend_input)
    }

    pub(crate) fn sample_typed_fragment_backend_input() -> Vec<beryl_backend::UserInput> {
        let fragments = vec![
            text_fragment("First"),
            backend_fragment(
                "See [A]",
                vec![
                    beryl_backend::UserInput::text("See "),
                    beryl_backend::UserInput::text("Image A:"),
                    beryl_backend::UserInput::local_image("/tmp/a.png"),
                ],
            ),
        ];
        turn_worker::backend_input_for_user_input_fragments(&fragments)
    }

    pub(crate) fn fresh_workspace_new_thread_target(
        state: &beryl_model::conversation::WorkspaceConversationState,
        active_target: &beryl_model::workspace::WorkspaceId,
    ) -> beryl_model::workspace::WorkspaceId {
        workspace_members::resolve_new_thread_execution_target(state, active_target).unwrap()
    }
}

use shell::{
    thread_title::title_generation_turn_options,
    turn_worker::{
        ThreadActivationBackend, TurnStreamBackend, activate_thread,
        automatic_thread_title_generation_is_eligible, handle_beryl_dynamic_tool_call,
        handle_beryl_dynamic_tool_call_with_shell_tools,
        shell_dynamic_tool_request_channel_with_capacity_for_test, stream_active_turn_events,
    },
};

#[test]
fn stream_idle_before_completion_keeps_turn_pending() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let mut backend = FakeTurnStreamBackend::new([
        Ok(None),
        Ok(Some(TurnStreamEvent::AgentMessageDelta {
            thread_id: "thread_1".to_string(),
            turn_id: "turn_1".to_string(),
            item_id: "message_1".to_string(),
            delta: "still working".to_string(),
        })),
        Ok(Some(turn_completed("thread_1", "turn_1"))),
        Ok(None),
    ]);
    let mut emitted = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        unexpected_dynamic_tool_call,
        |_| panic!("test did not expect a lifecycle yield"),
        |event| {
            emitted.push(event);
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(emitted.len(), 2);
    assert_eq!(
        backend.polls,
        vec![idle_poll, idle_poll, idle_poll, completion_grace]
    );
}

#[test]
fn stream_status_after_completion_ends_without_waiting_for_idle_grace() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let mut backend = FakeTurnStreamBackend::new([
        Ok(Some(turn_completed("thread_1", "turn_1"))),
        Ok(Some(TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        })),
    ]);
    let mut emitted = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        unexpected_dynamic_tool_call,
        |_| panic!("test did not expect a lifecycle yield"),
        |event| {
            emitted.push(event);
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(emitted.len(), 2);
    assert_eq!(backend.polls, vec![idle_poll, completion_grace]);
}

#[test]
fn stream_stops_when_update_consumer_rejects_event() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let mut backend = FakeTurnStreamBackend::new([
        Ok(Some(TurnStreamEvent::AgentMessageDelta {
            thread_id: "thread_1".to_string(),
            turn_id: "turn_1".to_string(),
            item_id: "message_1".to_string(),
            delta: "still working".to_string(),
        })),
        Ok(Some(turn_completed("thread_1", "turn_1"))),
    ]);

    let error = stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        unexpected_dynamic_tool_call,
        |_| panic!("test did not expect a lifecycle yield"),
        |_| Err("receiver closed".to_string()),
    )
    .unwrap_err();

    assert_eq!(error, "receiver closed");
    assert_eq!(backend.polls, vec![idle_poll]);
}

#[test]
fn stream_auto_cancels_command_approval_and_waits_for_interruption() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let approval = command_approval_request();
    let mut backend = FakeTurnStreamBackend::new([
        Ok(Some(TurnStreamEvent::ApprovalRequested(approval.clone()))),
        Ok(Some(turn_interrupted("thread_1", "turn_1"))),
        Ok(Some(TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        })),
    ]);
    let mut emitted = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        unexpected_dynamic_tool_call,
        |_| panic!("test did not expect a lifecycle yield"),
        |event| {
            emitted.push(event);
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(backend.denied_approvals, vec![approval]);
    assert!(backend.interrupted_turns.is_empty());
    assert!(matches!(
        emitted.first(),
        Some(TurnStreamEvent::TurnCompleted { turn, .. })
            if turn.status == TurnStatus::Interrupted
    ));
}

#[test]
fn stream_interrupts_after_permission_approval_denial() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let approval = permissions_approval_request();
    let mut backend = FakeTurnStreamBackend::new([
        Ok(Some(TurnStreamEvent::ApprovalRequested(approval.clone()))),
        Ok(Some(turn_interrupted("thread_1", "turn_1"))),
        Ok(Some(TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        })),
    ]);
    let mut emitted = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        unexpected_dynamic_tool_call,
        |_| panic!("test did not expect a lifecycle yield"),
        |event| {
            emitted.push(event);
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(backend.denied_approvals, vec![approval]);
    assert_eq!(
        backend.interrupted_turns,
        vec![("thread_1".to_string(), "turn_1".to_string(), idle_poll)]
    );
}

#[test]
fn stream_dynamic_tool_call_responds_and_keeps_streaming() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let request = dynamic_tool_call_request(
        "read_workspace_graph_summary",
        json!({ "ignoredByTest": true }),
    );
    let mut backend = FakeTurnStreamBackend::new([
        Ok(Some(TurnStreamEvent::DynamicToolCallRequested(
            request.clone(),
        ))),
        Ok(Some(turn_completed("thread_1", "turn_1"))),
        Ok(None),
    ]);
    let mut handled_calls = Vec::new();
    let mut emitted = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        |request| {
            handled_calls.push(request.call_id().to_string());
            DynamicToolCallResponse::success_text("{\"ok\":true}")
        },
        |_| panic!("test did not expect a lifecycle yield"),
        |event| {
            emitted.push(event);
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(handled_calls, vec!["call_1"]);
    assert_eq!(backend.dynamic_tool_responses.len(), 1);
    assert_eq!(backend.dynamic_tool_responses[0].0, request);
    assert!(backend.dynamic_tool_responses[0].1.success);
    assert_eq!(emitted, vec![turn_completed("thread_1", "turn_1")]);
}

#[test]
fn shell_dynamic_tool_bridge_reports_busy_without_blocking_when_full() {
    let request = dynamic_tool_call_request("read_ui_state", json!({ "limit": 1 }));
    let (sender, _receiver) = shell_dynamic_tool_request_channel_with_capacity_for_test(0);
    let sender = sender.with_response_timeout_for_test(Duration::from_secs(60));

    let response = sender.request(&request);
    let payload = response_json(&response);

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "shell_unavailable");
    assert!(
        payload["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("busy"))
    );
}

#[test]
fn timed_out_shell_dynamic_tool_request_cannot_be_claimed_later() {
    let request = dynamic_tool_call_request("close_popups", json!({}));
    let (sender, receiver) = shell_dynamic_tool_request_channel_with_capacity_for_test(1);
    let sender = sender.with_response_timeout_for_test(Duration::from_millis(1));

    let response = sender.request(&request);
    let payload = response_json(&response);
    let queued_request = receiver
        .try_recv()
        .expect("timed-out request should still be queued for stale-drop testing");

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "shell_unavailable");
    assert!(!queued_request.try_claim());
}

#[test]
fn local_gui_control_tool_call_is_not_forwarded_to_supervisor_shell_bridge() {
    let request = dynamic_tool_call_request("read_ui_state", json!({ "limit": 1 }));
    let (sender, receiver) = shell_dynamic_tool_request_channel_with_capacity_for_test(1);
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("local_gui_control_removed").unwrap();
    let mut graph_updates = Vec::new();

    let handled = handle_beryl_dynamic_tool_call_with_shell_tools(
        &service,
        &workspace_id,
        Some(&sender),
        &request,
        |update| graph_updates.push(update),
    );
    let response = handled.into_response();
    let payload = response_json(&response);

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "unsupported_tool");
    assert!(receiver.try_recv().is_err());
    assert!(graph_updates.is_empty());

    root.close().unwrap();
}

#[test]
fn diagnostic_child_tool_call_is_forwarded_to_supervisor_shell_bridge() {
    let request = dynamic_tool_call_request_with_namespace(
        BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE,
        "status",
        json!({}),
    );
    let (sender, receiver) = shell_dynamic_tool_request_channel_with_capacity_for_test(1);
    let sender = sender.with_response_timeout_for_test(Duration::from_millis(1));
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("diagnostic_child_forwarded").unwrap();
    let mut graph_updates = Vec::new();

    let handled = handle_beryl_dynamic_tool_call_with_shell_tools(
        &service,
        &workspace_id,
        Some(&sender),
        &request,
        |update| graph_updates.push(update),
    );
    let response = handled.into_response();
    let payload = response_json(&response);
    let queued_request = receiver
        .try_recv()
        .expect("diagnostic child tool request should be queued for shell handling");

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "shell_unavailable");
    assert_eq!(
        queued_request.request().namespace(),
        Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE)
    );
    assert_eq!(queued_request.request().tool(), "status");
    assert!(graph_updates.is_empty());

    root.close().unwrap();
}

#[test]
fn diagnostic_child_stop_uses_extended_shell_response_timeout() {
    let request = dynamic_tool_call_request_with_namespace(
        BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE,
        "stop",
        json!({}),
    );
    let (sender, _receiver) = shell_dynamic_tool_request_channel_with_capacity_for_test(1);
    let sender = sender.with_response_timeout_for_test(Duration::from_millis(1));

    assert!(sender.response_timeout_for_request_for_test(&request) > Duration::from_secs(2));
}

#[test]
fn stream_lifecycle_yield_captures_correlated_outcome() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let request = dynamic_tool_call_request(
        YIELD_TOOL,
        json!({
            "outcome": "phase_continue"
        }),
    );
    let mut backend = FakeTurnStreamBackend::new([
        Ok(Some(TurnStreamEvent::DynamicToolCallRequested(
            request.clone(),
        ))),
        Ok(Some(turn_completed("thread_1", "turn_1"))),
        Ok(None),
    ]);
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("lifecycle_yield").unwrap();
    let mut graph_updates = Vec::new();
    let mut lifecycle_yields = Vec::new();
    let mut emitted = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        |request| {
            handle_beryl_dynamic_tool_call(&service, &workspace_id, request, |update| {
                graph_updates.push(update)
            })
        },
        |yielded| lifecycle_yields.push(yielded),
        |event| {
            emitted.push(event);
            Ok(())
        },
    )
    .unwrap();

    assert!(graph_updates.is_empty());
    assert_eq!(lifecycle_yields.len(), 1);
    assert_eq!(lifecycle_yields[0].thread_id, "thread_1");
    assert_eq!(lifecycle_yields[0].turn_id, "turn_1");
    assert_eq!(
        lifecycle_yields[0].outcome,
        LifecycleYieldOutcome::PhaseContinue
    );
    assert_eq!(backend.dynamic_tool_responses.len(), 1);
    assert_eq!(backend.dynamic_tool_responses[0].0, request);
    assert!(backend.dynamic_tool_responses[0].1.success);
    assert_eq!(
        response_json(&backend.dynamic_tool_responses[0].1)["result"]["outcome"],
        "phase_continue"
    );
    assert_eq!(emitted, vec![turn_completed("thread_1", "turn_1")]);

    root.close().unwrap();
}

#[test]
fn stream_malformed_lifecycle_yield_fails_without_capture() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let request = dynamic_tool_call_request(YIELD_TOOL, json!({}));
    let mut backend = FakeTurnStreamBackend::new([
        Ok(Some(TurnStreamEvent::DynamicToolCallRequested(
            request.clone(),
        ))),
        Ok(Some(turn_completed("thread_1", "turn_1"))),
        Ok(None),
    ]);
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("lifecycle_yield").unwrap();
    let mut lifecycle_yields = Vec::new();
    let mut emitted = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        |request| {
            handle_beryl_dynamic_tool_call(&service, &workspace_id, request, |_| {
                panic!("malformed yield must not publish a graph update")
            })
        },
        |yielded| lifecycle_yields.push(yielded),
        |event| {
            emitted.push(event);
            Ok(())
        },
    )
    .unwrap();

    assert!(lifecycle_yields.is_empty());
    assert_eq!(backend.dynamic_tool_responses.len(), 1);
    assert!(!backend.dynamic_tool_responses[0].1.success);
    assert_eq!(
        response_json(&backend.dynamic_tool_responses[0].1)["error"]["kind"],
        "invalid_arguments"
    );
    assert_eq!(emitted, vec![turn_completed("thread_1", "turn_1")]);

    root.close().unwrap();
}

#[test]
fn stream_duplicate_lifecycle_yield_keeps_first_outcome() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let first = dynamic_tool_call_request_with_identity(
        "thread_1",
        "turn_1",
        "call_1",
        YIELD_TOOL,
        json!({
            "outcome": "phase_continue"
        }),
    );
    let second = dynamic_tool_call_request_with_identity(
        "thread_1",
        "turn_1",
        "call_2",
        YIELD_TOOL,
        json!({
            "outcome": "plan_complete"
        }),
    );
    let mut backend = FakeTurnStreamBackend::new([
        Ok(Some(TurnStreamEvent::DynamicToolCallRequested(
            first.clone(),
        ))),
        Ok(Some(TurnStreamEvent::DynamicToolCallRequested(
            second.clone(),
        ))),
        Ok(Some(turn_completed("thread_1", "turn_1"))),
        Ok(None),
    ]);
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("lifecycle_yield").unwrap();
    let mut lifecycle_yields = Vec::new();
    let mut emitted = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        |request| {
            handle_beryl_dynamic_tool_call(&service, &workspace_id, request, |_| {
                panic!("lifecycle yield must not publish a graph update")
            })
        },
        |yielded| lifecycle_yields.push(yielded),
        |event| {
            emitted.push(event);
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(lifecycle_yields.len(), 1);
    assert_eq!(
        lifecycle_yields[0].outcome,
        LifecycleYieldOutcome::PhaseContinue
    );
    assert_eq!(backend.dynamic_tool_responses.len(), 2);
    assert_eq!(backend.dynamic_tool_responses[0].0, first);
    assert_eq!(backend.dynamic_tool_responses[1].0, second);
    assert!(backend.dynamic_tool_responses[0].1.success);
    assert!(backend.dynamic_tool_responses[1].1.success);
    assert_eq!(
        response_json(&backend.dynamic_tool_responses[1].1)["result"]["outcome"],
        "plan_complete"
    );
    assert_eq!(emitted, vec![turn_completed("thread_1", "turn_1")]);

    root.close().unwrap();
}

#[test]
fn stream_uncorrelated_lifecycle_yield_is_rejected_without_capture() {
    let idle_poll = Duration::from_secs(10);
    let completion_grace = Duration::from_millis(500);
    let request = dynamic_tool_call_request_with_identity(
        "other_thread",
        "turn_1",
        "call_1",
        YIELD_TOOL,
        json!({
            "outcome": "phase_continue"
        }),
    );
    let mut backend = FakeTurnStreamBackend::new([
        Ok(Some(TurnStreamEvent::DynamicToolCallRequested(
            request.clone(),
        ))),
        Ok(Some(turn_completed("thread_1", "turn_1"))),
        Ok(None),
    ]);
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("lifecycle_yield").unwrap();
    let mut lifecycle_yields = Vec::new();
    let mut emitted = Vec::new();

    stream_active_turn_events(
        &mut backend,
        "thread_1",
        "turn_1",
        idle_poll,
        completion_grace,
        |request| {
            handle_beryl_dynamic_tool_call(&service, &workspace_id, request, |_| {
                panic!("lifecycle yield must not publish a graph update")
            })
        },
        |yielded| lifecycle_yields.push(yielded),
        |event| {
            emitted.push(event);
            Ok(())
        },
    )
    .unwrap();

    assert!(lifecycle_yields.is_empty());
    assert_eq!(backend.dynamic_tool_responses.len(), 1);
    assert!(!backend.dynamic_tool_responses[0].1.success);
    assert_eq!(
        response_json(&backend.dynamic_tool_responses[0].1)["error"]["kind"],
        "uncorrelated_lifecycle_yield"
    );
    assert_eq!(emitted, vec![turn_completed("thread_1", "turn_1")]);

    root.close().unwrap();
}

#[test]
fn graph_dynamic_write_call_publishes_graph_commit_update() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic_refresh").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    let request = dynamic_tool_call_request(
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
    let mut graph_updates = Vec::new();

    let handled = handle_beryl_dynamic_tool_call(&service, &workspace_id, &request, |update| {
        graph_updates.push(update)
    });

    assert!(handled.clone().into_response().success);
    assert_eq!(handled.lifecycle_yield(), None);
    assert_eq!(graph_updates.len(), 1);
    let shell::graph::GraphMutationUpdate::Commit(update) = graph_updates.pop().unwrap() else {
        panic!("dynamic graph write must publish a graph commit update");
    };
    let updated_workspace_id = update.commit.workspace_id.clone();
    assert_eq!(updated_workspace_id, workspace_id);
    let root_node_id = SemanticNodeId::new("root").unwrap();
    assert!(update.commit.changed);
    assert_eq!(update.commit.manifest.id(), &updated_workspace_id);
    assert_eq!(
        update.commit.base_revision,
        WorkspaceGraphRevision::default()
    );
    assert_eq!(
        update.commit.committed_revision,
        WorkspaceGraphRevision::new(1)
    );
    assert_eq!(update.commit.patch.operations().len(), 2);
    let graph = persistence
        .load_workspace_graph_state(&updated_workspace_id)
        .unwrap();
    assert_eq!(graph.node(&root_node_id).unwrap().title(), "Root");

    root.close().unwrap();
}

#[test]
fn graph_dynamic_write_failure_publishes_graph_failure_update() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic_failure").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    let request = dynamic_tool_call_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "root",
            "parentId": null,
            "title": "Root",
            "summary": "Root summary",
            "topic": true,
            "checklist": false,
            "checklistItem": true
        }),
    );
    let mut graph_updates = Vec::new();

    let handled = handle_beryl_dynamic_tool_call(&service, &workspace_id, &request, |update| {
        graph_updates.push(update)
    });

    assert!(!handled.clone().into_response().success);
    assert_eq!(handled.lifecycle_yield(), None);
    assert_eq!(graph_updates.len(), 1);
    let shell::graph::GraphMutationUpdate::Failure(update) = graph_updates.pop().unwrap() else {
        panic!("dynamic graph write failure must publish a graph failure update");
    };
    assert_eq!(update.workspace_id, workspace_id);
    assert!(update.message.contains("checklist"));

    root.close().unwrap();
}

#[test]
fn graph_dynamic_read_call_does_not_publish_graph_refresh_update() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_dynamic_refresh").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Dynamic", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();
    let request = dynamic_tool_call_request("read_workspace_graph_summary", json!({}));
    let mut graph_updates = Vec::new();

    let handled = handle_beryl_dynamic_tool_call(&service, &workspace_id, &request, |update| {
        graph_updates.push(update)
    });

    assert!(handled.clone().into_response().success);
    assert_eq!(handled.lifecycle_yield(), None);
    assert!(graph_updates.is_empty());

    root.close().unwrap();
}

#[test]
fn thread_title_generation_uses_medium_reasoning_effort() {
    let options = title_generation_turn_options();

    assert_eq!(options.reasoning_effort(), Some("medium"));
}

#[test]
fn title_candidate_uses_normalized_backend_name_for_eligibility() {
    assert!(automatic_thread_title_generation_is_eligible(
        true,
        Some("   ")
    ));
    assert!(!automatic_thread_title_generation_is_eligible(
        true,
        Some("Named thread")
    ));
}

#[test]
fn title_candidate_requires_submit_path_eligibility() {
    assert!(!automatic_thread_title_generation_is_eligible(false, None));
    assert!(automatic_thread_title_generation_is_eligible(true, None));
}

#[test]
fn new_thread_activation_uses_dynamic_tools_without_developer_instructions() {
    let mut backend = FakeThreadActivationBackend::default();
    let workspace = workspace();

    let activation =
        activate_thread(&mut backend, &workspace, None, Duration::from_secs(1)).unwrap();

    assert_eq!(activation.thread_id, "created_thread");
    assert_eq!(backend.resumed_threads, Vec::<String>::new());
    assert_eq!(backend.started_threads.len(), 1);
    let started = &backend.started_threads[0];
    assert_eq!(started.cwd, workspace.canonical_path());
    assert_eq!(started.options.developer_instructions(), None);
    assert!(!started.options.is_ephemeral());
    assert!(started.options.dynamic_tools().iter().any(|tool| {
        tool.name == YIELD_TOOL && tool.namespace.as_deref() == Some(BERYL_DYNAMIC_TOOL_NAMESPACE)
    }));
}

#[test]
fn fresh_workspace_first_submit_starts_backend_thread_at_host_implicit_home_target() {
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .select_runtime(RuntimeMode::HostWindows)
        .unwrap();
    let implicit_home_target = WorkspaceId::host_windows(PathBuf::from(r"C:\Users\operator"));
    let submit_target =
        shell::fresh_workspace_new_thread_target(&workspace_state, &implicit_home_target);
    let mut backend = FakeThreadActivationBackend::default();

    let activation =
        activate_thread(&mut backend, &submit_target, None, Duration::from_secs(1)).unwrap();

    assert_eq!(activation.thread_id, "created_thread");
    assert_eq!(backend.resumed_threads, Vec::<String>::new());
    assert_eq!(backend.started_threads.len(), 1);
    assert_eq!(
        backend.started_threads[0].cwd,
        submit_target.canonical_path()
    );
    assert_eq!(
        submit_target.runtime_mode(),
        implicit_home_target.runtime_mode()
    );
}

#[test]
fn existing_thread_activation_resumes_without_thread_start_options() {
    let mut backend = FakeThreadActivationBackend::default();
    let workspace = workspace();

    let activation = activate_thread(
        &mut backend,
        &workspace,
        Some("existing_thread"),
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(activation.thread_id, "existing_thread");
    assert!(backend.started_threads.is_empty());
    assert_eq!(backend.resumed_threads, vec!["existing_thread".to_string()]);
}

#[test]
fn turn_worker_flattens_typed_fragments_to_ordered_backend_input() {
    assert_eq!(
        shell::sample_typed_fragment_backend_input(),
        vec![
            UserInput::text("First"),
            UserInput::text("See "),
            UserInput::text("Image A:"),
            UserInput::local_image("/tmp/a.png"),
        ]
    );
}

#[derive(Default)]
struct FakeThreadActivationBackend {
    started_threads: Vec<StartedThread>,
    resumed_threads: Vec<String>,
}

struct StartedThread {
    cwd: PathBuf,
    options: ThreadStartOptions,
}

impl ThreadActivationBackend for FakeThreadActivationBackend {
    type Error = String;

    fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        _: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        self.started_threads.push(StartedThread {
            cwd: cwd.to_path_buf(),
            options,
        });
        Ok(thread_session_response("created_thread"))
    }

    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        self.resumed_threads.push(thread_id.to_string());
        Ok(thread_session_response(thread_id))
    }
}

struct FakeTurnStreamBackend {
    events: VecDeque<Result<Option<TurnStreamEvent>, String>>,
    polls: Vec<Duration>,
    denied_approvals: Vec<ApprovalRequest>,
    dynamic_tool_responses: Vec<(DynamicToolCallRequest, DynamicToolCallResponse)>,
    interrupted_turns: Vec<(String, String, Duration)>,
}

impl FakeTurnStreamBackend {
    fn new<const N: usize>(events: [Result<Option<TurnStreamEvent>, String>; N]) -> Self {
        Self {
            events: VecDeque::from(events),
            polls: Vec::new(),
            denied_approvals: Vec::new(),
            dynamic_tool_responses: Vec::new(),
            interrupted_turns: Vec::new(),
        }
    }
}

impl TurnStreamBackend for FakeTurnStreamBackend {
    type Error = String;

    fn next_turn_stream_event(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error> {
        self.polls.push(idle_timeout);
        self.events
            .pop_front()
            .unwrap_or_else(|| Err("unexpected extra stream poll".to_string()))
    }

    fn deny_approval_request(&mut self, request: &ApprovalRequest) -> Result<(), Self::Error> {
        self.denied_approvals.push(request.clone());
        Ok(())
    }

    fn respond_dynamic_tool_call(
        &mut self,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), Self::Error> {
        self.dynamic_tool_responses
            .push((request.clone(), response.clone()));
        Ok(())
    }

    fn interrupt_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        timeout: Duration,
    ) -> Result<(), Self::Error> {
        self.interrupted_turns
            .push((thread_id.to_string(), turn_id.to_string(), timeout));
        Ok(())
    }
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

fn turn_interrupted(thread_id: &str, turn_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::TurnCompleted {
        thread_id: thread_id.to_string(),
        turn: TurnInfo {
            id: turn_id.to_string(),
            status: TurnStatus::Interrupted,
            items: Vec::new(),
            error: None,
        },
    }
}

fn command_approval_request() -> ApprovalRequest {
    parse_approval_request(
        json!(42),
        "item/commandExecution/requestApproval",
        Some(json!({
            "threadId": "thread_1",
            "turnId": "turn_1",
            "itemId": "cmd_1",
            "command": "Remove-Item important.txt",
            "cwd": "C:\\work\\beryl",
            "reason": "needs elevated execution"
        })),
    )
    .unwrap()
}

fn permissions_approval_request() -> ApprovalRequest {
    parse_approval_request(
        json!("approval-1"),
        "item/permissions/requestApproval",
        Some(json!({
            "threadId": "thread_1",
            "turnId": "turn_1",
            "itemId": "perm_1",
            "cwd": "C:\\work\\beryl",
            "permissions": {},
            "reason": "needs more permissions"
        })),
    )
    .unwrap()
}

fn unexpected_dynamic_tool_call(_: &DynamicToolCallRequest) -> DynamicToolCallResponse {
    panic!("test did not expect a dynamic tool call")
}

fn dynamic_tool_call_request(tool: &str, arguments: serde_json::Value) -> DynamicToolCallRequest {
    dynamic_tool_call_request_with_identity("thread_1", "turn_1", "call_1", tool, arguments)
}

fn dynamic_tool_call_request_with_identity(
    thread_id: &str,
    turn_id: &str,
    call_id: &str,
    tool: &str,
    arguments: serde_json::Value,
) -> DynamicToolCallRequest {
    dynamic_tool_call_request_with_namespace_and_identity(
        "beryl", thread_id, turn_id, call_id, tool, arguments,
    )
}

fn dynamic_tool_call_request_with_namespace(
    namespace: &str,
    tool: &str,
    arguments: serde_json::Value,
) -> DynamicToolCallRequest {
    dynamic_tool_call_request_with_namespace_and_identity(
        namespace, "thread_1", "turn_1", "call_1", tool, arguments,
    )
}

fn dynamic_tool_call_request_with_namespace_and_identity(
    namespace: &str,
    thread_id: &str,
    turn_id: &str,
    call_id: &str,
    tool: &str,
    arguments: serde_json::Value,
) -> DynamicToolCallRequest {
    parse_dynamic_tool_call_request(
        json!("dynamic-request-1"),
        "item/tool/call",
        Some(json!({
            "threadId": thread_id,
            "turnId": turn_id,
            "callId": call_id,
            "namespace": namespace,
            "tool": tool,
            "arguments": arguments
        })),
    )
    .unwrap()
    .unwrap()
}

fn response_json(response: &DynamicToolCallResponse) -> serde_json::Value {
    let Some(DynamicToolCallOutputContentItem::InputText { text }) = response.content_items.first()
    else {
        panic!("expected a single text content item")
    };
    serde_json::from_str(text).unwrap()
}

fn thread_session_response(thread_id: &str) -> ThreadSessionResponse {
    serde_json::from_value(json!({
        "thread": {
            "id": thread_id,
            "cwd": r"C:\work\beryl",
            "preview": "",
            "createdAt": 0,
            "updatedAt": 0,
            "modelProvider": "openai",
            "ephemeral": false,
            "status": { "type": "idle" },
            "turns": []
        }
    }))
    .unwrap()
}

fn workspace() -> WorkspaceId {
    WorkspaceId::host_windows(PathBuf::from(r"C:\work\beryl"))
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-turn-worker-test-")
}
