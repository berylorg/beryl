#![allow(dead_code)]

use std::time::Duration;

use beryl_backend::{ThreadItem, ThreadSessionResponse, TurnInfo, TurnItemsView, TurnStatus};
use beryl_model::workspace::WorkspaceId;
use serde_json::json;

#[path = "../src/memory_diagnostics.rs"]
mod memory_diagnostics;

mod shell {
    #[path = "../../src/shell/thread_activation.rs"]
    pub(super) mod thread_activation;
    #[allow(dead_code)]
    #[path = "../../src/shell/thread_selection.rs"]
    pub(super) mod thread_selection;
}

use shell::thread_activation::{
    ExistingThreadActivationBackend, ExistingThreadActivationError, ThreadActivationLoader,
};

#[test]
fn direct_activation_resumes_metadata_without_loading_turn_history() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::ok(thread_response(
        "thread_a",
        r"C:\work\alpha",
        vec![not_loaded_turn("turn_1")],
    ));

    let activation = ThreadActivationLoader::load_existing_thread(
        &mut backend,
        &execution_target,
        "thread_a",
        "Thread A",
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(backend.resume_calls, vec!["thread_a"]);
    assert_eq!(
        activation.session_metadata.model.as_deref(),
        Some("gpt-5.4")
    );
    assert_eq!(activation.thread.summary().id, "thread_a");
    assert_eq!(activation.thread.turns.len(), 1);
    assert_eq!(
        activation.thread.turns[0].items_view,
        TurnItemsView::NotLoaded
    );
    assert!(activation.thread.turns[0].items.is_empty());
}

#[test]
fn direct_activation_rejects_cwd_mismatch_as_rebind() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend =
        FakeActivationBackend::ok(thread_response("thread_a", r"C:\work\beta", Vec::new()));

    let error = ThreadActivationLoader::load_existing_thread(
        &mut backend,
        &execution_target,
        "thread_a",
        "Thread A",
        Duration::from_secs(5),
    )
    .unwrap_err();

    match error {
        ExistingThreadActivationError::RequiresRebind { detail } => {
            assert!(detail.contains("Thread A"));
            assert!(detail.contains(r"C:\work\beta"));
            assert!(detail.contains(r"C:\work\alpha"));
            assert!(detail.contains("Explicit rebinding is required"));
        }
        ExistingThreadActivationError::Failed { message } => {
            panic!("expected rebind error, got failure: {message}");
        }
    }
    assert_eq!(backend.resume_calls, vec!["thread_a"]);
}

#[test]
fn direct_activation_resume_error_is_reported_as_rebind_requirement() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::err("missing thread");

    let error = ThreadActivationLoader::load_existing_thread(
        &mut backend,
        &execution_target,
        "thread_a",
        "Thread A",
        Duration::from_secs(5),
    )
    .unwrap_err();

    match error {
        ExistingThreadActivationError::RequiresRebind { detail } => {
            assert!(detail.contains("Thread A"));
            assert!(detail.contains("missing thread"));
        }
        ExistingThreadActivationError::Failed { message } => {
            panic!("expected rebind error, got failure: {message}");
        }
    }
    assert_eq!(backend.resume_calls, vec!["thread_a"]);
}

struct FakeActivationBackend {
    resume_response: Option<Result<ThreadSessionResponse, String>>,
    resume_calls: Vec<String>,
}

impl FakeActivationBackend {
    fn ok(response: ThreadSessionResponse) -> Self {
        Self {
            resume_response: Some(Ok(response)),
            resume_calls: Vec::new(),
        }
    }

    fn err(message: &str) -> Self {
        Self {
            resume_response: Some(Err(message.to_string())),
            resume_calls: Vec::new(),
        }
    }
}

impl ExistingThreadActivationBackend for FakeActivationBackend {
    type Error = String;

    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        self.resume_calls.push(thread_id.to_string());
        self.resume_response
            .take()
            .expect("resume should be called once")
    }
}

fn thread_response(thread_id: &str, cwd: &str, turns: Vec<TurnInfo>) -> ThreadSessionResponse {
    serde_json::from_value(json!({
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": cwd,
            "ephemeral": false,
            "id": thread_id,
            "modelProvider": "openai",
            "preview": "Activation",
            "source": "appServer",
            "status": {
                "type": "active",
                "activeFlags": ["waitingOnUserInput"]
            },
            "turns": turns,
            "updatedAt": 2
        }
    }))
    .unwrap()
}

fn not_loaded_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: TurnItemsView::NotLoaded,
        items: Vec::<ThreadItem>::new(),
        error: None,
    }
}
