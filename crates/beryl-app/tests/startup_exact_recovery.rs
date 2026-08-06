use std::time::Duration;

use beryl_backend::{ThreadSessionResponse, ThreadTurnsListOptions, ThreadTurnsListResponse};
use beryl_model::conversation::{
    ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState,
};
use beryl_model::workspace::WorkspaceId;
use serde_json::json;

#[path = "../src/memory_diagnostics.rs"]
mod memory_diagnostics;
#[path = "../src/shell/startup_initial_thread_load.rs"]
mod startup_initial_thread_load;
#[path = "../src/shell/thread_selection.rs"]
mod thread_selection;

mod shell {
    #[path = "../../src/shell/thread_activation.rs"]
    pub(super) mod thread_activation;
    #[path = "../../src/shell/thread_selection.rs"]
    pub(super) mod thread_selection;
    #[path = "../../src/shell/transcript_history.rs"]
    pub(super) mod transcript_history;
}

use shell::thread_activation::{
    ExistingThreadActivationBackend, activate_existing_thread_direct_with_fork_parent,
};
use shell::transcript_history::TranscriptHistoryBackend;
use startup_initial_thread_load::{
    StartupInitialThreadLoadAdapter, route_startup_initial_thread_load,
};
use thread_selection::{
    KnownThreadSelection, ThreadSelectionRequest,
    persisted_active_thread_disconnect_selection_request,
    persisted_active_thread_selection_request, resolve_known_thread_selection,
};

#[derive(Debug, PartialEq, Eq)]
enum StartupOperation {
    List,
    Activate(String),
}

#[derive(Default)]
struct RecordingStartupAdapter {
    operations: Vec<StartupOperation>,
}

impl StartupInitialThreadLoadAdapter for RecordingStartupAdapter {
    type Output = ();

    fn activate_exact(&mut self, request: &ThreadSelectionRequest) {
        let ThreadSelectionRequest::Exact { thread_id, .. } = request else {
            unreachable!();
        };
        self.operations
            .push(StartupOperation::Activate(thread_id.clone()));
    }

    fn persisted_unavailable(&mut self, _: &ThreadSelectionRequest) {}

    fn restore_preferred(&mut self, _: &ThreadSelectionRequest) {
        self.operations.push(StartupOperation::List);
    }
}

#[test]
fn production_router_enforces_three_way_startup_request_behavior() {
    let cases = [
        (
            ThreadSelectionRequest::exact("thread_exact", "Exact"),
            vec![StartupOperation::Activate("thread_exact".to_string())],
        ),
        (
            ThreadSelectionRequest::PersistedActiveRepairRequired {
                thread_id: "thread_invalid".to_string(),
                label: "Invalid".to_string(),
                detail: "Repair required".to_string(),
            },
            Vec::new(),
        ),
        (
            ThreadSelectionRequest::RestorePreferred(None),
            vec![StartupOperation::List],
        ),
    ];

    for (request, expected) in cases {
        let mut adapter = RecordingStartupAdapter::default();
        route_startup_initial_thread_load(&request, &mut adapter);
        assert_eq!(
            adapter.operations, expected,
            "unexpected route for {request:?}"
        );
    }
}

#[test]
fn eligible_persisted_request_activates_the_exact_backend_id() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let workspace_state = persisted_active_state(&execution_target, "thread_exact");
    let selection =
        persisted_active_thread_selection_request(&workspace_state, &execution_target).unwrap();
    let ThreadSelectionRequest::Exact {
        thread_id,
        label,
        expected_forked_from_id,
    } = selection
    else {
        panic!("eligible persisted registration must route to exact recovery");
    };
    let mut backend = RecordingActivationBackend::successful(&thread_id, &execution_target);

    let activation = activate_existing_thread_direct_with_fork_parent(
        &mut backend,
        &execution_target,
        &thread_id,
        &label,
        expected_forked_from_id.as_deref(),
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(thread_id, "thread_exact");
    assert_eq!(activation.thread.summary().id, "thread_exact");
    assert_eq!(backend.resume_calls, vec!["thread_exact"]);
    assert_eq!(backend.turn_calls, vec!["thread_exact"]);
}

#[test]
fn disconnect_phase_child_routes_exactly_without_inventory_and_keeps_root_expectation() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let root_id = ConversationThreadId::new("thread_root");
    let child_id = ConversationThreadId::new("thread_child");
    let mut workspace_state = persisted_active_state(&execution_target, child_id.as_str());
    workspace_state.remember_thread(RegisteredConversationThread::new(
        root_id.clone(),
        execution_target.clone(),
        "Root preview",
        None,
        1,
        2,
    ));
    workspace_state
        .record_thread_as_orchestration_root(&root_id)
        .unwrap();
    workspace_state
        .record_thread_orchestration_root(&child_id, &root_id)
        .unwrap();

    let selection = persisted_active_thread_disconnect_selection_request(
        &workspace_state,
        &execution_target,
        child_id.as_str(),
    )
    .unwrap();
    assert!(matches!(
        &selection,
        ThreadSelectionRequest::Exact {
            thread_id,
            expected_forked_from_id: Some(expected_root),
            ..
        } if thread_id == child_id.as_str() && expected_root == root_id.as_str()
    ));

    let mut adapter = RecordingStartupAdapter::default();
    route_startup_initial_thread_load(&selection, &mut adapter);
    assert_eq!(
        adapter.operations,
        vec![StartupOperation::Activate(child_id.as_str().to_string())]
    );
}

#[test]
fn invalid_persisted_request_fails_closed_without_inventory_substitute() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut workspace_state = persisted_active_state(&execution_target, "thread_invalid");
    workspace_state
        .mark_thread_rebind_required(
            &ConversationThreadId::new("thread_invalid"),
            "Persisted member binding is stale",
        )
        .unwrap();

    let selection =
        persisted_active_thread_selection_request(&workspace_state, &execution_target).unwrap();
    let ThreadSelectionRequest::PersistedActiveRepairRequired {
        thread_id,
        label: _,
        detail,
    } = &selection
    else {
        panic!("invalid persisted registration must require explicit repair");
    };
    assert_eq!(thread_id, "thread_invalid");
    assert!(detail.contains("Persisted member binding is stale"));

    let substitute = thread_response("thread_substitute", &execution_target)
        .thread
        .summary();
    assert_eq!(
        resolve_known_thread_selection(&[substitute], &execution_target, &selection),
        KnownThreadSelection::None
    );
}

#[test]
fn absent_persisted_request_leaves_bounded_discovery_eligible() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();

    assert_eq!(
        persisted_active_thread_selection_request(&workspace_state, &execution_target),
        None
    );
    let discovery_selection = ThreadSelectionRequest::RestorePreferred(None);
    let candidate = thread_response("thread_candidate", &execution_target)
        .thread
        .summary();
    assert_eq!(
        resolve_known_thread_selection(&[candidate], &execution_target, &discovery_selection),
        KnownThreadSelection::Selected {
            thread_id: "thread_candidate".to_string(),
            strict: false,
        }
    );
}

struct RecordingActivationBackend {
    resume_response: Option<ThreadSessionResponse>,
    resume_calls: Vec<String>,
    turn_calls: Vec<String>,
}

impl RecordingActivationBackend {
    fn successful(thread_id: &str, execution_target: &WorkspaceId) -> Self {
        Self {
            resume_response: Some(thread_response(thread_id, execution_target)),
            resume_calls: Vec::new(),
            turn_calls: Vec::new(),
        }
    }
}

impl ExistingThreadActivationBackend for RecordingActivationBackend {
    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        self.resume_calls.push(thread_id.to_string());
        self.resume_response
            .take()
            .ok_or_else(|| "unexpected repeated metadata resume".to_string())
    }
}

impl TranscriptHistoryBackend for RecordingActivationBackend {
    type Error = String;

    fn list_thread_turns(
        &mut self,
        thread_id: &str,
        _: &ThreadTurnsListOptions,
        _: Duration,
    ) -> Result<ThreadTurnsListResponse, Self::Error> {
        self.turn_calls.push(thread_id.to_string());
        Ok(ThreadTurnsListResponse {
            data: Vec::new(),
            next_cursor: None,
            backwards_cursor: None,
        })
    }
}

fn persisted_active_state(
    execution_target: &WorkspaceId,
    thread_id: &str,
) -> WorkspaceConversationState {
    let thread_id = ConversationThreadId::new(thread_id);
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(execution_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target.clone(),
        "Persisted preview",
        Some("Persisted title".to_string()),
        1,
        2,
    ));
    workspace_state.activate_thread(&thread_id).unwrap();
    workspace_state
}

fn thread_response(thread_id: &str, execution_target: &WorkspaceId) -> ThreadSessionResponse {
    serde_json::from_value(json!({
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": execution_target.canonical_path(),
            "ephemeral": false,
            "forkedFromId": null,
            "id": thread_id,
            "modelProvider": "openai",
            "preview": "Activation",
            "source": "appServer",
            "status": {
                "type": "active",
                "activeFlags": ["waitingOnUserInput"]
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap()
}
