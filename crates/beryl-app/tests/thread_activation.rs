use std::time::Duration;

use beryl_backend::{
    AgentMessageItem, ImageGenerationItem, JsonRpcError, ProtocolPhase, ThreadItem,
    ThreadSessionResponse, ThreadTurnsListOptions, ThreadTurnsListResponse, TurnInfo, TurnStatus,
    UsageTreeAccountingStatus, UsageTreeReadOutcome, UsageTreeSelfUsage, UsageTreeSnapshot,
    UsageTreeTokenUsageBreakdown,
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
    ExistingThreadActivationBackend, ExistingThreadActivationError,
    activate_existing_thread_direct, activate_existing_thread_direct_with_fork_parent,
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
    assert_eq!(backend.usage_tree_calls, vec!["thread_a"]);
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
    assert!(activation.history_window.has_older_pages());
    assert!(activation.usage_tree_snapshot.is_some());
}

#[test]
fn direct_activation_carries_complete_or_partial_usage_tree_snapshot_nonfatally() {
    for status in [
        UsageTreeAccountingStatus::Complete,
        UsageTreeAccountingStatus::LegacyPartial,
    ] {
        let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
        let mut backend = FakeActivationBackend::new(
            thread_response("thread_a", r"C:\work\alpha"),
            Ok(empty_turn_page()),
        );
        backend.usage_tree_response = Ok(UsageTreeReadOutcome::Snapshot(usage_tree_snapshot(
            "thread_a", 3, status,
        )));

        let activation = activate_existing_thread_direct(
            &mut backend,
            &execution_target,
            "thread_a",
            "Thread A",
            Duration::from_secs(5),
        )
        .unwrap();

        assert_eq!(
            activation
                .usage_tree_snapshot
                .as_ref()
                .map(|snapshot| snapshot.accounting_status),
            Some(status)
        );
    }
}

#[test]
fn unsupported_or_failed_usage_tree_read_does_not_fail_activation() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let responses = [
        Ok(UsageTreeReadOutcome::UnsupportedMethod {
            error: JsonRpcError {
                code: -32601,
                message: "method not found".to_string(),
                data: None,
            },
        }),
        Err("usage read unavailable".to_string()),
    ];

    for response in responses {
        let mut backend = FakeActivationBackend::new(
            thread_response("thread_a", r"C:\work\alpha"),
            Ok(empty_turn_page()),
        );
        backend.usage_tree_response = response;

        let activation = activate_existing_thread_direct(
            &mut backend,
            &execution_target,
            "thread_a",
            "Thread A",
            Duration::from_secs(5),
        )
        .unwrap();

        assert!(activation.usage_tree_snapshot.is_none());
        assert_eq!(backend.usage_tree_calls, vec!["thread_a"]);
    }
}

#[test]
fn direct_activation_failed_resume_can_retry_the_same_exact_thread() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::new(
        thread_response("thread_a", r"C:\work\alpha"),
        Ok(empty_turn_page()),
    );
    backend.resume_error_once = Some("backend temporarily unavailable".to_string());

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
            assert!(message.contains("could not reopen the requested thread"));
            assert!(message.contains("backend temporarily unavailable"));
        }
        ExistingThreadActivationError::RequiresRebind { detail } => {
            panic!("expected resume failure, got rebind: {detail}");
        }
    }
    assert_eq!(backend.resume_calls, vec!["thread_a"]);
    assert!(backend.turn_calls.is_empty());
    assert!(backend.usage_tree_calls.is_empty());

    let activation = activate_existing_thread_direct(
        &mut backend,
        &execution_target,
        "thread_a",
        "Thread A",
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(activation.thread.summary().id, "thread_a");
    assert_eq!(backend.resume_calls, vec!["thread_a", "thread_a"]);
    assert_eq!(
        backend
            .turn_calls
            .iter()
            .map(|(thread_id, _)| thread_id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread_a"]
    );
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
fn direct_activation_rejects_backend_identity_mismatch_before_history_load() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::new(
        thread_response("thread_other", r"C:\work\alpha"),
        Ok(ThreadTurnsListResponse {
            data: Vec::new(),
            next_cursor: None,
            backwards_cursor: None,
        }),
    );

    let error = activate_existing_thread_direct(
        &mut backend,
        &execution_target,
        "thread_requested",
        "Requested thread",
        Duration::from_secs(5),
    )
    .unwrap_err();

    match error {
        ExistingThreadActivationError::Failed { message } => {
            assert!(message.contains("thread_requested"));
            assert!(message.contains("thread_other"));
        }
        ExistingThreadActivationError::RequiresRebind { detail } => {
            panic!("expected identity failure, got rebind: {detail}");
        }
    }
    assert!(backend.turn_calls.is_empty());
}

#[test]
fn persisted_phase_child_activation_requires_exact_backend_root_parent() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::new(
        thread_response_with_parent("thread_child", r"C:\work\alpha", Some("other_root")),
        Ok(ThreadTurnsListResponse {
            data: Vec::new(),
            next_cursor: None,
            backwards_cursor: None,
        }),
    );

    let error = activate_existing_thread_direct_with_fork_parent(
        &mut backend,
        &execution_target,
        "thread_child",
        "Phase child",
        Some("thread_root"),
        Duration::from_secs(5),
    )
    .unwrap_err();

    match error {
        ExistingThreadActivationError::Failed { message } => {
            assert!(message.contains("other_root"));
            assert!(message.contains("thread_root"));
        }
        ExistingThreadActivationError::RequiresRebind { detail } => {
            panic!("expected lineage failure, got rebind: {detail}");
        }
    }
    assert!(backend.turn_calls.is_empty());
}

#[test]
fn persisted_phase_child_activation_rejects_missing_backend_root_parent() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut backend = FakeActivationBackend::new(
        thread_response_with_parent("thread_child", r"C:\work\alpha", None),
        Ok(ThreadTurnsListResponse {
            data: Vec::new(),
            next_cursor: None,
            backwards_cursor: None,
        }),
    );

    let error = activate_existing_thread_direct_with_fork_parent(
        &mut backend,
        &execution_target,
        "thread_child",
        "Phase child",
        Some("thread_root"),
        Duration::from_secs(5),
    )
    .unwrap_err();

    match error {
        ExistingThreadActivationError::Failed { message } => {
            assert!(message.contains("no fork parent"));
            assert!(message.contains("thread_root"));
        }
        ExistingThreadActivationError::RequiresRebind { detail } => {
            panic!("expected lineage failure, got rebind: {detail}");
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
    resume_error_once: Option<String>,
    turn_response: Result<ThreadTurnsListResponse, String>,
    resume_calls: Vec<String>,
    turn_calls: Vec<(String, ThreadTurnsListOptions)>,
    usage_tree_response: Result<UsageTreeReadOutcome, String>,
    usage_tree_calls: Vec<String>,
}

impl FakeActivationBackend {
    fn new(
        resume_response: ThreadSessionResponse,
        turn_response: Result<ThreadTurnsListResponse, String>,
    ) -> Self {
        Self {
            resume_response: Some(resume_response),
            resume_error_once: None,
            turn_response,
            resume_calls: Vec::new(),
            turn_calls: Vec::new(),
            usage_tree_response: Ok(UsageTreeReadOutcome::Snapshot(usage_tree_snapshot(
                "thread_a",
                1,
                UsageTreeAccountingStatus::Complete,
            ))),
            usage_tree_calls: Vec::new(),
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
        if let Some(error) = self.resume_error_once.take() {
            return Err(error);
        }
        self.resume_response
            .take()
            .ok_or_else(|| "resume called more than once".to_string())
    }

    fn read_token_usage_tree(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<UsageTreeReadOutcome, Self::Error> {
        self.usage_tree_calls.push(thread_id.to_string());
        self.usage_tree_response.clone()
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
    thread_response_with_parent(thread_id, cwd, None)
}

fn thread_response_with_parent(
    thread_id: &str,
    cwd: &str,
    forked_from_id: Option<&str>,
) -> ThreadSessionResponse {
    serde_json::from_value(json!({
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": cwd,
            "ephemeral": false,
            "forkedFromId": forked_from_id,
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
            saved_path: Some(saved_path.to_string()),
        })],
        error: None,
    }
}

fn empty_turn_page() -> ThreadTurnsListResponse {
    ThreadTurnsListResponse {
        data: Vec::new(),
        next_cursor: None,
        backwards_cursor: None,
    }
}

fn usage_tree_snapshot(
    root_thread_id: &str,
    revision: u64,
    accounting_status: UsageTreeAccountingStatus,
) -> UsageTreeSnapshot {
    let breakdown = UsageTreeTokenUsageBreakdown {
        total_tokens: 30,
        input_tokens: 20,
        cached_input_tokens: 5,
        cache_write_input_tokens: 0,
        output_tokens: 10,
        reasoning_output_tokens: 2,
    };
    UsageTreeSnapshot {
        schema_version: 1,
        root_thread_id: root_thread_id.to_string(),
        revision,
        self_usage: UsageTreeSelfUsage {
            total: breakdown.clone(),
            last: breakdown.clone(),
            model_context_window: Some(200_000),
        },
        descendants_total: breakdown.clone(),
        tree_total: breakdown,
        accounting_status,
    }
}
