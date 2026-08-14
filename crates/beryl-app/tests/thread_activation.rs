use std::time::Duration;

use beryl_backend::{
    AgentMessageItem, ImageGenerationItem, ProtocolPhase, ThreadItem, ThreadSessionResponse,
    ThreadTurnsListOptions, ThreadTurnsListResponse, TurnInfo, TurnStatus,
};
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
    #[path = "../../src/shell/transcript_history.rs"]
    pub(super) mod transcript_history;
}

use shell::thread_activation::{
    ExistingThreadActivationBackend, ExistingThreadActivationError, activate_existing_thread_direct,
};
use shell::transcript_history::{
    THREAD_HISTORY_PAGE_LIMIT, TranscriptHistoryBackend, initial_thread_history_page_options,
};

#[test]
fn direct_activation_uses_metadata_resume_and_bounded_latest_turn_page() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::new(
        thread_response("thread_a", r"C:\work\alpha"),
        Ok(ThreadTurnsListResponse {
            data: vec![turn("turn_3"), turn("turn_2")],
            next_cursor: Some("older".to_string()),
            backwards_cursor: None,
        }),
    );

    let activation = activate_existing_thread_direct(
        &mut backend,
        &execution_target,
        "thread_a",
        "Thread A",
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(backend.resume_calls, vec!["thread_a"]);
    assert_eq!(backend.turn_calls.len(), 1);
    assert_eq!(backend.turn_calls[0].0, "thread_a");
    assert_eq!(
        backend.turn_calls[0].1,
        initial_thread_history_page_options()
    );
    assert_eq!(
        activation.session_metadata.model.as_deref(),
        Some("gpt-5.4")
    );
    assert!(activation.history_window.has_older_pages());
    assert_eq!(activation.thread.turns[0].id, "turn_2");
    assert_eq!(activation.thread.turns[1].id, "turn_3");
}

#[test]
fn direct_activation_preserves_generated_image_saved_path_from_latest_page() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::new(
        thread_response("thread_a", r"C:\work\alpha"),
        Ok(ThreadTurnsListResponse {
            data: vec![
                generated_image_turn("turn_3", "image_3", r"C:\work\alpha\generated-3.png"),
                turn("turn_2"),
            ],
            next_cursor: Some("older".to_string()),
            backwards_cursor: None,
        }),
    );

    let activation = activate_existing_thread_direct(
        &mut backend,
        &execution_target,
        "thread_a",
        "Thread A",
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(activation.thread.turns[0].id, "turn_2");
    assert_eq!(activation.thread.turns[1].id, "turn_3");
    let [ThreadItem::ImageGeneration(item)] = activation.thread.turns[1].items.as_slice() else {
        panic!("expected generated-image item in activated history page");
    };
    assert_eq!(item.id, "image_3");
    assert_eq!(
        item.saved_path.as_deref(),
        Some(r"C:\work\alpha\generated-3.png")
    );
    assert!(item.result.is_none());
    assert!(activation.history_window.has_older_pages());
}

#[test]
fn direct_activation_rejects_cwd_mismatch_as_rebind() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::new(
        thread_response("thread_a", r"C:\work\beta"),
        Ok(ThreadTurnsListResponse {
            data: Vec::new(),
            next_cursor: None,
            backwards_cursor: None,
        }),
    );

    let error = activate_existing_thread_direct(
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
    assert!(backend.turn_calls.is_empty());
}

#[test]
fn direct_activation_fails_when_initial_history_page_cannot_load() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::new(
        thread_response("thread_a", r"C:\work\alpha"),
        Err("page unavailable".to_string()),
    );

    let error = activate_existing_thread_direct(
        &mut backend,
        &execution_target,
        "thread_a",
        "Thread A",
        Duration::from_secs(5),
    )
    .unwrap_err();

    match error {
        ExistingThreadActivationError::Failed { message } => {
            assert!(message.contains("page unavailable"));
        }
        ExistingThreadActivationError::RequiresRebind { detail } => {
            panic!("expected page-load failure, got rebind: {detail}");
        }
    }
    assert_eq!(backend.resume_calls, vec!["thread_a"]);
    assert_eq!(backend.turn_calls.len(), 1);
}

#[test]
fn initial_history_page_options_request_latest_bounded_page() {
    let options = initial_thread_history_page_options();

    assert_eq!(options.limit, Some(THREAD_HISTORY_PAGE_LIMIT));
    assert_eq!(
        options.sort_direction,
        Some(beryl_backend::SortDirection::Desc)
    );
    assert_eq!(options.cursor, None);
}

struct FakeActivationBackend {
    resume_response: Option<ThreadSessionResponse>,
    turn_response: Result<ThreadTurnsListResponse, String>,
    resume_calls: Vec<String>,
    turn_calls: Vec<(String, ThreadTurnsListOptions)>,
}

impl FakeActivationBackend {
    fn new(
        resume_response: ThreadSessionResponse,
        turn_response: Result<ThreadTurnsListResponse, String>,
    ) -> Self {
        Self {
            resume_response: Some(resume_response),
            turn_response,
            resume_calls: Vec::new(),
            turn_calls: Vec::new(),
        }
    }
}

impl ExistingThreadActivationBackend for FakeActivationBackend {
    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        self.resume_calls.push(thread_id.to_string());
        self.resume_response
            .take()
            .ok_or_else(|| "resume called more than once".to_string())
    }
}

impl TranscriptHistoryBackend for FakeActivationBackend {
    type Error = String;

    fn list_thread_turns(
        &mut self,
        thread_id: &str,
        options: &ThreadTurnsListOptions,
        _: Duration,
    ) -> Result<ThreadTurnsListResponse, Self::Error> {
        self.turn_calls
            .push((thread_id.to_string(), options.clone()));
        self.turn_response.clone()
    }
}

fn thread_response(thread_id: &str, cwd: &str) -> ThreadSessionResponse {
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
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap()
}

fn turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::AgentMessage(AgentMessageItem {
            id: format!("{id}_message"),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: format!("Answer for {id}"),
        })],
        error: None,
    }
}

fn generated_image_turn(id: &str, image_id: &str, saved_path: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::ImageGeneration(ImageGenerationItem {
            id: image_id.to_string(),
            status: Some("completed".to_string()),
            revised_prompt: Some("A generated activation image".to_string()),
            result: None,
            saved_path: Some(saved_path.to_string()),
        })],
        error: None,
    }
}
