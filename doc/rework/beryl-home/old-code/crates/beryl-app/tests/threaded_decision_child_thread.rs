use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub use beryl_app::beryl_user_thread_start_options;
use beryl_app::{READ_WORKSPACE_GRAPH_SUMMARY_TOOL, RESOLVE_DECISION_BRANCH_TOOL};
use beryl_backend::{ThreadSessionResponse, ThreadStartOptions};
use beryl_model::workspace::WorkspaceId;
use serde_json::json;

#[path = "../src/threaded_decision_child_thread.rs"]
mod threaded_decision_child_thread;

use threaded_decision_child_thread::{
    DecisionChildThreadStartBackend, start_empty_decision_child_thread,
};

#[test]
fn decision_child_thread_start_uses_empty_persistent_user_thread() {
    let mut backend = FakeDecisionChildThreadStartBackend::new(Ok(thread_session_response(
        "decision_child",
        &[],
        false,
    )));
    let execution_target = workspace();

    let thread =
        start_empty_decision_child_thread(&mut backend, &execution_target, Duration::from_secs(1))
            .unwrap();

    assert_eq!(thread.summary().id, "decision_child");
    assert!(thread.turns.is_empty());
    assert_eq!(backend.started_threads.len(), 1);
    let started = &backend.started_threads[0];
    assert_eq!(started.cwd, execution_target.canonical_path());
    assert!(!started.options.is_ephemeral());
    let tool_names = started
        .options
        .dynamic_tools()
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&READ_WORKSPACE_GRAPH_SUMMARY_TOOL));
    assert!(tool_names.contains(&RESOLVE_DECISION_BRANCH_TOOL));
}

#[test]
fn decision_child_thread_start_rejects_inherited_turns() {
    let mut backend = FakeDecisionChildThreadStartBackend::new(Ok(thread_session_response(
        "decision_child",
        &["parent_turn"],
        false,
    )));

    let error =
        start_empty_decision_child_thread(&mut backend, &workspace(), Duration::from_secs(1))
            .unwrap_err();

    assert!(error.contains("already contained 1 turn"));
    assert!(error.contains("must start empty"));
}

#[test]
fn decision_child_thread_start_rejects_ephemeral_thread() {
    let mut backend = FakeDecisionChildThreadStartBackend::new(Ok(thread_session_response(
        "decision_child",
        &[],
        true,
    )));

    let error =
        start_empty_decision_child_thread(&mut backend, &workspace(), Duration::from_secs(1))
            .unwrap_err();

    assert!(error.contains("marked it ephemeral"));
}

struct FakeDecisionChildThreadStartBackend {
    response: Option<Result<ThreadSessionResponse, String>>,
    started_threads: Vec<StartedThread>,
}

struct StartedThread {
    cwd: PathBuf,
    options: ThreadStartOptions,
}

impl FakeDecisionChildThreadStartBackend {
    fn new(response: Result<ThreadSessionResponse, String>) -> Self {
        Self {
            response: Some(response),
            started_threads: Vec::new(),
        }
    }
}

impl DecisionChildThreadStartBackend for FakeDecisionChildThreadStartBackend {
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
        self.response
            .take()
            .expect("start_thread should only be called once")
    }
}

fn thread_session_response(id: &str, turn_ids: &[&str], ephemeral: bool) -> ThreadSessionResponse {
    serde_json::from_value(json!({
        "thread": {
            "id": id,
            "cwd": r"C:\work\beryl",
            "preview": "",
            "createdAt": 0,
            "updatedAt": 0,
            "modelProvider": "openai",
            "ephemeral": ephemeral,
            "status": { "type": "idle" },
            "turns": turn_ids.iter().map(|turn_id| {
                json!({
                    "id": turn_id,
                    "status": "completed",
                    "items": []
                })
            }).collect::<Vec<_>>()
        }
    }))
    .unwrap()
}

fn workspace() -> WorkspaceId {
    WorkspaceId::host_windows(PathBuf::from(r"C:\work\beryl"))
}
