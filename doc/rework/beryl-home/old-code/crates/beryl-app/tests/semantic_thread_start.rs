use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub use beryl_app::beryl_user_thread_start_options;
use beryl_app::{READ_WORKSPACE_GRAPH_SUMMARY_TOOL, UPSERT_GRAPH_NODE_TOOL, YIELD_TOOL};
use beryl_backend::{ThreadSessionResponse, ThreadStartOptions};
use beryl_model::workspace::WorkspaceId;
use serde_json::json;

#[allow(dead_code)]
#[path = "../src/shell/semantic_thread_start.rs"]
mod semantic_thread_start;

use semantic_thread_start::{
    SemanticThreadStartBackend, SemanticThreadStartSource, start_semantic_backend_thread,
};

#[test]
fn graph_node_thread_start_uses_standard_user_thread_options() {
    let mut backend = FakeSemanticThreadStartBackend::default();
    let execution_target = workspace();

    let started = start_semantic_backend_thread(
        &mut backend,
        SemanticThreadStartSource::GraphNode,
        &execution_target,
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(started.thread.summary().id, "semantic_thread");
    assert_eq!(backend.started_threads.len(), 1);
    let started = &backend.started_threads[0];
    assert_eq!(started.cwd, execution_target.canonical_path());
    assert_eq!(started.options.developer_instructions(), None);
    assert_standard_user_thread_options(&started.options);
}

#[test]
fn checklist_item_thread_start_uses_standard_user_thread_options() {
    let mut backend = FakeSemanticThreadStartBackend::default();
    let execution_target = workspace();

    start_semantic_backend_thread(
        &mut backend,
        SemanticThreadStartSource::ChecklistItem,
        &execution_target,
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(backend.started_threads.len(), 1);
    let started = &backend.started_threads[0];
    assert_eq!(started.options.developer_instructions(), None);
    assert_standard_user_thread_options(&started.options);
}

#[derive(Default)]
struct FakeSemanticThreadStartBackend {
    started_threads: Vec<StartedThread>,
}

struct StartedThread {
    cwd: PathBuf,
    options: ThreadStartOptions,
}

impl SemanticThreadStartBackend for FakeSemanticThreadStartBackend {
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
        Ok(thread_session_response())
    }
}

fn assert_standard_user_thread_options(options: &ThreadStartOptions) {
    let names: Vec<_> = options
        .dynamic_tools()
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();

    assert!(!options.is_ephemeral());
    assert!(names.contains(&READ_WORKSPACE_GRAPH_SUMMARY_TOOL));
    assert!(names.contains(&UPSERT_GRAPH_NODE_TOOL));
    assert!(names.contains(&YIELD_TOOL));
}

fn thread_session_response() -> ThreadSessionResponse {
    serde_json::from_value(json!({
        "thread": {
            "id": "semantic_thread",
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
