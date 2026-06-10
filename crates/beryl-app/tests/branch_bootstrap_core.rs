use std::{collections::VecDeque, path::PathBuf, time::Duration};

use beryl_backend::{
    ApprovalRequest, DynamicToolCallOutputContentItem, DynamicToolCallRequest,
    DynamicToolCallResponse, ThreadInfo, ThreadItem, ThreadSummary, TurnInfo, TurnStartOptions,
    TurnStartResponse, TurnStatus, TurnStreamEvent, UserInput, parse_dynamic_tool_call_request,
};
use beryl_model::conversation::{ConversationThreadId, ConversationTurnId};
use serde_json::json;

#[path = "../src/branch_bootstrap_core.rs"]
mod branch_bootstrap_core;

use branch_bootstrap_core::{
    BranchBootstrapBackend, BranchBootstrapError, BranchBootstrapMessageInput,
    beryl_thread_link_destination, branch_bootstrap_message, parse_beryl_thread_link,
    prove_branch_thread_completed_bootstrap_from_history, start_branch_bootstrap_turn,
    start_branch_bootstrap_turn_only,
};

#[test]
fn bootstrap_message_records_visible_parent_link_and_context() {
    let parent_thread_id = ConversationThreadId::new("parent thread/alpha");

    let message = branch_bootstrap_message(BranchBootstrapMessageInput {
        parent_thread_id: &parent_thread_id,
        parent_thread_title: Some(r#" Parent [draft] \ "quoted" "#),
        branch_context: Some("\nDecision context:\nUse the visible context.\n"),
    });

    assert_eq!(
        message,
        concat!(
            r#"Branched from [Parent \[draft\] \\ "quoted"](beryl_threadid://parent%20thread%2Falpha), no response required."#,
            "\n\nDecision context:\nUse the visible context."
        )
    );
}

#[test]
fn bootstrap_message_uses_untitled_fallback() {
    let parent_thread_id = ConversationThreadId::new("parent");

    assert_eq!(
        branch_bootstrap_message(BranchBootstrapMessageInput {
            parent_thread_id: &parent_thread_id,
            parent_thread_title: Some("   "),
            branch_context: None,
        }),
        "Branched from [Untitled thread](beryl_threadid://parent), no response required."
    );
}

#[test]
fn thread_link_destination_roundtrips_percent_encoded_thread_ids() {
    let thread_id = ConversationThreadId::new("thread 1/alpha:β");
    let destination = beryl_thread_link_destination(&thread_id);

    assert_eq!(destination, "beryl_threadid://thread%201%2Falpha%3A%CE%B2");
    assert_eq!(
        parse_beryl_thread_link(&destination),
        Some(ConversationThreadId::new("thread 1/alpha:β"))
    );
    assert_eq!(parse_beryl_thread_link("https://example.invalid"), None);
    assert_eq!(parse_beryl_thread_link("beryl_threadid://"), None);
    assert_eq!(parse_beryl_thread_link("beryl_threadid://bad%XX"), None);
}

#[test]
fn bootstrap_turn_starts_without_hidden_developer_context_then_proves_durable() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(turn_completed("branch_thread", "bootstrap_turn"))))
        .with_read_thread(Ok(thread_info_with_bootstrap(
            "branch_thread",
            "bootstrap_turn",
            "Branched from [Parent](beryl_threadid://parent), no response required.",
        )));

    let result = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(result.thread().id, "branch_thread");
    assert_eq!(
        result.bootstrap_turn_id(),
        Some(&ConversationTurnId::new("bootstrap_turn"))
    );
    assert_eq!(backend.start_calls.len(), 1);
    assert_eq!(backend.start_calls[0].thread_id, "branch_thread");
    assert_eq!(
        backend.start_calls[0].text,
        "Branched from [Parent](beryl_threadid://parent), no response required."
    );
    assert!(backend.start_calls[0].options.model().is_none());
    assert!(backend.start_calls[0].options.reasoning_effort().is_none());
    assert!(
        backend.start_calls[0]
            .options
            .developer_instructions_context()
            .is_none()
    );
    assert_eq!(backend.stream_polls, 1);
    assert_eq!(backend.read_calls, vec!["branch_thread".to_string()]);
}

#[test]
fn bootstrap_turn_start_only_returns_exact_turn_without_terminal_proof() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend =
        FakeBootstrapBackend::new().with_start_turn(Ok(turn_start_response("bootstrap_turn")));

    let result = start_branch_bootstrap_turn_only(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(result.turn().id, "bootstrap_turn");
    assert_eq!(
        result.bootstrap_turn_id(),
        &ConversationTurnId::new("bootstrap_turn")
    );
    assert_eq!(backend.start_calls.len(), 1);
    assert_eq!(backend.stream_polls, 0);
    assert!(backend.read_calls.is_empty());
}

#[test]
fn bootstrap_turn_uses_history_proof_when_idle_precedes_completion_event() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(thread_status_idle("branch_thread"))))
        .with_read_thread(Ok(thread_info_with_bootstrap(
            "branch_thread",
            "bootstrap_turn",
            "Branched from [Parent](beryl_threadid://parent), no response required.",
        )));

    let result = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(result.thread().id, "branch_thread");
    assert_eq!(
        result.bootstrap_turn_id(),
        Some(&ConversationTurnId::new("bootstrap_turn"))
    );
    assert_eq!(backend.stream_polls, 1);
    assert_eq!(backend.read_calls, vec!["branch_thread".to_string()]);
}

#[test]
fn foreground_bootstrap_history_probe_returns_completed_turn_after_idle() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let bootstrap_turn_id = ConversationTurnId::new("bootstrap_turn");
    let mut backend = FakeBootstrapBackend::new().with_read_thread(Ok(thread_info_with_bootstrap(
        "branch_thread",
        "bootstrap_turn",
        "Branched from [Parent](beryl_threadid://parent), no response required.",
    )));

    let completion = prove_branch_thread_completed_bootstrap_from_history(
        &mut backend,
        &thread_id,
        &bootstrap_turn_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap()
    .expect("completed history should prove the bootstrap turn");

    assert_eq!(completion.thread().id, "branch_thread");
    assert_eq!(completion.turn().id, "bootstrap_turn");
    assert_eq!(backend.read_calls, vec!["branch_thread".to_string()]);
}

#[test]
fn foreground_bootstrap_history_probe_waits_when_turn_is_still_active() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let bootstrap_turn_id = ConversationTurnId::new("bootstrap_turn");
    let mut backend =
        FakeBootstrapBackend::new().with_read_thread(Ok(thread_info_with_summary_and_turns(
            thread_summary("branch_thread", false),
            vec![in_progress_bootstrap_turn(
                "bootstrap_turn",
                "Branched from [Parent](beryl_threadid://parent), no response required.",
            )],
        )));

    let completion = prove_branch_thread_completed_bootstrap_from_history(
        &mut backend,
        &thread_id,
        &bootstrap_turn_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap();

    assert!(completion.is_none());
    assert_eq!(backend.read_calls, vec!["branch_thread".to_string()]);
}

#[test]
fn bootstrap_turn_continues_after_idle_when_history_still_has_active_turn() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(thread_status_idle("branch_thread"))))
        .with_stream_event(Ok(Some(turn_completed("branch_thread", "bootstrap_turn"))))
        .with_read_thread(Ok(thread_info_with_summary_and_turns(
            thread_summary("branch_thread", false),
            vec![in_progress_bootstrap_turn(
                "bootstrap_turn",
                "Branched from [Parent](beryl_threadid://parent), no response required.",
            )],
        )))
        .with_read_thread(Ok(thread_info_with_bootstrap(
            "branch_thread",
            "bootstrap_turn",
            "Branched from [Parent](beryl_threadid://parent), no response required.",
        )));

    let result = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(result.thread().id, "branch_thread");
    assert_eq!(backend.stream_polls, 2);
    assert_eq!(
        backend.read_calls,
        vec!["branch_thread".to_string(), "branch_thread".to_string()]
    );
}

#[test]
fn bootstrap_turn_rejects_idle_history_without_visible_message() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(thread_status_idle("branch_thread"))))
        .with_read_thread(Ok(thread_info_with_summary_and_turns(
            thread_summary("branch_thread", false),
            vec![completed_bootstrap_turn("bootstrap_turn", "Different text")],
        )));

    let error = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        BranchBootstrapError::BootstrapTurnMissingVisibleMessage { .. }
    ));
    assert_eq!(backend.stream_polls, 1);
    assert_eq!(backend.read_calls, vec!["branch_thread".to_string()]);
}

#[test]
fn bootstrap_turn_failure_does_not_run_durability_read() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new().with_start_turn(Err("turn rejected".to_string()));

    let error = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        BranchBootstrapError::TurnStartFailed { .. }
    ));
    assert!(error.to_string().contains("turn rejected"));
    assert!(backend.read_calls.is_empty());
    assert_eq!(backend.stream_polls, 0);
}

#[test]
fn bootstrap_turn_reports_durability_read_failures() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(turn_completed("branch_thread", "bootstrap_turn"))))
        .with_read_thread(Err("no rollout found".to_string()));

    let error = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        BranchBootstrapError::DurabilityProofFailed { .. }
    ));
    assert!(error.to_string().contains("no rollout found"));
}

#[test]
fn bootstrap_turn_rejects_mismatched_or_ephemeral_durable_threads() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut mismatched = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(turn_completed("branch_thread", "bootstrap_turn"))))
        .with_read_thread(Ok(thread_info_with_summary_and_turns(
            thread_summary("other_thread", false),
            vec![completed_bootstrap_turn(
                "bootstrap_turn",
                "Branched from [Parent](beryl_threadid://parent), no response required.",
            )],
        )));

    assert!(matches!(
        start_branch_bootstrap_turn(
            &mut mismatched,
            &thread_id,
            "Branched from [Parent](beryl_threadid://parent), no response required.",
            Duration::from_secs(5),
        )
        .unwrap_err(),
        BranchBootstrapError::DurableThreadIdMismatch { .. }
    ));

    let mut ephemeral = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(turn_completed("branch_thread", "bootstrap_turn"))))
        .with_read_thread(Ok(thread_info_with_summary_and_turns(
            thread_summary("branch_thread", true),
            vec![completed_bootstrap_turn(
                "bootstrap_turn",
                "Branched from [Parent](beryl_threadid://parent), no response required.",
            )],
        )));

    assert!(matches!(
        start_branch_bootstrap_turn(
            &mut ephemeral,
            &thread_id,
            "Branched from [Parent](beryl_threadid://parent), no response required.",
            Duration::from_secs(5),
        )
        .unwrap_err(),
        BranchBootstrapError::DurableThreadMarkedEphemeral { .. }
    ));
}

#[test]
fn bootstrap_turn_rejects_missing_turn_id_before_streaming() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("   ")))
        .with_read_thread(Ok(thread_info_with_bootstrap(
            "branch_thread",
            "bootstrap_turn",
            "Branched from [Parent](beryl_threadid://parent), no response required.",
        )));

    let error = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        BranchBootstrapError::BootstrapTurnMissingId { .. }
    ));
    assert_eq!(backend.stream_polls, 0);
    assert!(backend.read_calls.is_empty());
}

#[test]
fn bootstrap_turn_failed_terminal_state_fails_before_durability_read() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(turn_failed("branch_thread", "bootstrap_turn"))))
        .with_read_thread(Ok(thread_info_with_bootstrap(
            "branch_thread",
            "bootstrap_turn",
            "Branched from [Parent](beryl_threadid://parent), no response required.",
        )));

    let error = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        BranchBootstrapError::BootstrapTurnFailed {
            status: TurnStatus::Failed,
            ..
        }
    ));
    assert!(backend.read_calls.is_empty());
}

#[test]
fn bootstrap_turn_fails_when_final_history_omits_completed_turn() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(turn_completed("branch_thread", "bootstrap_turn"))))
        .with_read_thread(Ok(thread_info_with_summary_and_turns(
            thread_summary("branch_thread", false),
            Vec::new(),
        )));

    let error = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        BranchBootstrapError::BootstrapTurnMissingFromHistory { .. }
    ));
}

#[test]
fn bootstrap_turn_fails_when_final_history_omits_visible_message() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(turn_completed("branch_thread", "bootstrap_turn"))))
        .with_read_thread(Ok(thread_info_with_summary_and_turns(
            thread_summary("branch_thread", false),
            vec![completed_bootstrap_turn("bootstrap_turn", "Different text")],
        )));

    let error = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        BranchBootstrapError::BootstrapTurnMissingVisibleMessage { .. }
    ));
}

#[test]
fn bootstrap_turn_returns_unavailable_response_for_dynamic_tool_and_fails_publication() {
    let thread_id = ConversationThreadId::new("branch_thread");
    let request = dynamic_tool_call_request("branch_thread", "bootstrap_turn", "upsert_graph_node");
    let mut backend = FakeBootstrapBackend::new()
        .with_start_turn(Ok(turn_start_response("bootstrap_turn")))
        .with_stream_event(Ok(Some(TurnStreamEvent::DynamicToolCallRequested(
            request.clone(),
        ))))
        .with_read_thread(Ok(thread_info_with_bootstrap(
            "branch_thread",
            "bootstrap_turn",
            "Branched from [Parent](beryl_threadid://parent), no response required.",
        )));

    let error = start_branch_bootstrap_turn(
        &mut backend,
        &thread_id,
        "Branched from [Parent](beryl_threadid://parent), no response required.",
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        BranchBootstrapError::BootstrapUnexpectedDynamicToolRequest { .. }
    ));
    assert!(backend.read_calls.is_empty());
    assert_eq!(backend.dynamic_tool_responses.len(), 1);
    assert_eq!(backend.dynamic_tool_responses[0].0, request);
    assert!(!backend.dynamic_tool_responses[0].1.success);
    assert!(
        response_text(&backend.dynamic_tool_responses[0].1)
            .contains("branch_bootstrap_tool_unavailable")
    );
}

#[derive(Clone, Debug)]
struct StartCall {
    thread_id: String,
    text: String,
    options: TurnStartOptions,
}

struct FakeBootstrapBackend {
    start_response: Option<Result<TurnStartResponse, String>>,
    read_responses: VecDeque<Result<ThreadInfo, String>>,
    stream_events: VecDeque<Result<Option<TurnStreamEvent>, String>>,
    start_calls: Vec<StartCall>,
    read_calls: Vec<String>,
    stream_polls: usize,
    approval_denials: Vec<ApprovalRequest>,
    dynamic_tool_responses: Vec<(DynamicToolCallRequest, DynamicToolCallResponse)>,
}

impl FakeBootstrapBackend {
    fn new() -> Self {
        Self {
            start_response: None,
            read_responses: VecDeque::new(),
            stream_events: VecDeque::new(),
            start_calls: Vec::new(),
            read_calls: Vec::new(),
            stream_polls: 0,
            approval_denials: Vec::new(),
            dynamic_tool_responses: Vec::new(),
        }
    }

    fn with_start_turn(mut self, response: Result<TurnStartResponse, String>) -> Self {
        self.start_response = Some(response);
        self
    }

    fn with_read_thread(mut self, response: Result<ThreadInfo, String>) -> Self {
        self.read_responses.push_back(response);
        self
    }

    fn with_stream_event(mut self, event: Result<Option<TurnStreamEvent>, String>) -> Self {
        self.stream_events.push_back(event);
        self
    }
}

impl BranchBootstrapBackend for FakeBootstrapBackend {
    type Error = String;

    fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        _: Duration,
    ) -> Result<TurnStartResponse, Self::Error> {
        self.start_calls.push(StartCall {
            thread_id: thread_id.to_string(),
            text: text.to_string(),
            options,
        });
        self.start_response
            .take()
            .expect("start response should be configured")
    }

    fn read_thread_metadata(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<ThreadSummary, Self::Error> {
        self.read_thread_with_turns(thread_id, Duration::from_secs(0))
            .map(|thread| thread.summary())
    }

    fn read_thread_with_turns(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<ThreadInfo, Self::Error> {
        self.read_calls.push(thread_id.to_string());
        self.read_responses
            .pop_front()
            .expect("read response should be configured")
    }

    fn next_turn_stream_event(
        &mut self,
        _: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error> {
        self.stream_polls += 1;
        self.stream_events
            .pop_front()
            .expect("stream event should be configured")
    }

    fn deny_approval_request(&mut self, request: &ApprovalRequest) -> Result<(), Self::Error> {
        self.approval_denials.push(request.clone());
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
}

fn turn_start_response(turn_id: &str) -> TurnStartResponse {
    TurnStartResponse {
        turn: TurnInfo {
            id: turn_id.to_string(),
            status: TurnStatus::InProgress,
            items_view: beryl_backend::TurnItemsView::Full,
            items: Vec::new(),
            error: None,
        },
    }
}

fn turn_completed(thread_id: &str, turn_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::TurnCompleted {
        thread_id: thread_id.to_string(),
        turn: completed_bootstrap_turn(
            turn_id,
            "Branched from [Parent](beryl_threadid://parent), no response required.",
        ),
    }
}

fn turn_failed(thread_id: &str, turn_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::TurnCompleted {
        thread_id: thread_id.to_string(),
        turn: TurnInfo {
            id: turn_id.to_string(),
            status: TurnStatus::Failed,
            items_view: beryl_backend::TurnItemsView::Full,
            items: Vec::new(),
            error: None,
        },
    }
}

fn thread_status_idle(thread_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::ThreadStatusChanged {
        thread_id: thread_id.to_string(),
        status: beryl_backend::ThreadStatus::Idle,
    }
}

fn thread_summary(thread_id: &str, ephemeral: bool) -> ThreadSummary {
    ThreadSummary {
        id: thread_id.to_string(),
        forked_from_id: Some("parent_thread".to_string()),
        cwd: PathBuf::from(r"C:\work\alpha"),
        preview: "Branch preview".to_string(),
        name: None,
        agent_nickname: None,
        path: None,
        created_at: 10,
        updated_at: 20,
        model_provider: "openai".to_string(),
        ephemeral,
    }
}

fn thread_info_with_bootstrap(thread_id: &str, turn_id: &str, message: &str) -> ThreadInfo {
    thread_info_with_summary_and_turns(
        thread_summary(thread_id, false),
        vec![completed_bootstrap_turn(turn_id, message)],
    )
}

fn thread_info_with_summary_and_turns(summary: ThreadSummary, turns: Vec<TurnInfo>) -> ThreadInfo {
    let turns = turns.iter().map(turn_json).collect::<Vec<_>>();
    serde_json::from_value(json!({
        "createdAt": summary.created_at,
        "cwd": summary.cwd.display().to_string(),
        "ephemeral": summary.ephemeral,
        "forkedFromId": summary.forked_from_id,
        "id": summary.id,
        "modelProvider": summary.model_provider,
        "preview": summary.preview,
        "status": { "type": "idle" },
        "turns": turns,
        "updatedAt": summary.updated_at
    }))
    .unwrap()
}

fn turn_json(turn: &TurnInfo) -> serde_json::Value {
    json!({
        "id": turn.id,
        "status": turn_status_wire(turn.status),
        "items": turn.items.iter().map(thread_item_json).collect::<Vec<_>>(),
        "error": turn.error
    })
}

fn thread_item_json(item: &ThreadItem) -> serde_json::Value {
    match item {
        ThreadItem::UserMessage(user_message) => json!({
            "type": "userMessage",
            "id": user_message.id,
            "content": user_message.content
        }),
        other => serde_json::to_value(other).unwrap(),
    }
}

fn turn_status_wire(status: TurnStatus) -> &'static str {
    match status {
        TurnStatus::Completed => "completed",
        TurnStatus::Interrupted => "interrupted",
        TurnStatus::Failed => "failed",
        TurnStatus::InProgress => "inProgress",
    }
}

fn completed_bootstrap_turn(turn_id: &str, message: &str) -> TurnInfo {
    TurnInfo {
        id: turn_id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
        items: vec![ThreadItem::UserMessage(beryl_backend::UserMessageItem {
            id: "user_message".to_string(),
            content: vec![UserInput::Text {
                text: message.to_string(),
            }],
        })],
        error: None,
    }
}

fn in_progress_bootstrap_turn(turn_id: &str, message: &str) -> TurnInfo {
    TurnInfo {
        id: turn_id.to_string(),
        status: TurnStatus::InProgress,
        items_view: beryl_backend::TurnItemsView::Full,
        items: vec![ThreadItem::UserMessage(beryl_backend::UserMessageItem {
            id: "user_message".to_string(),
            content: vec![UserInput::Text {
                text: message.to_string(),
            }],
        })],
        error: None,
    }
}

fn dynamic_tool_call_request(thread_id: &str, turn_id: &str, tool: &str) -> DynamicToolCallRequest {
    parse_dynamic_tool_call_request(
        json!("dynamic-request-1"),
        "item/tool/call",
        Some(json!({
            "threadId": thread_id,
            "turnId": turn_id,
            "callId": "call_1",
            "namespace": "beryl",
            "tool": tool,
            "arguments": {}
        })),
    )
    .unwrap()
    .unwrap()
}

fn response_text(response: &DynamicToolCallResponse) -> &str {
    let Some(DynamicToolCallOutputContentItem::InputText { text }) = response.content_items.first()
    else {
        panic!("expected a text dynamic tool response")
    };
    text
}
