#![allow(dead_code, private_interfaces, unused_imports)]

use std::{collections::VecDeque, path::PathBuf, time::Duration};

use beryl_backend::{
    ApprovalRequest, DynamicToolCallRequest, DynamicToolCallResponse, ThreadForkResponse,
    ThreadInfo, ThreadItem, ThreadRollbackResponse, ThreadSummary, TurnInfo, TurnStartOptions,
    TurnStartResponse, TurnStatus, TurnStreamEvent, UserInput,
};
use beryl_model::conversation::{
    BranchThreadTitleRetitleState, ConversationThreadId, ConversationTurnId,
    RegisteredConversationThread, WorkspaceConversationState,
};
use beryl_model::workspace::WorkspaceId;
use serde_json::json;

#[path = "../src/branch_bootstrap_core.rs"]
mod branch_bootstrap_core;

mod shell {
    #[path = "../../src/shell/composer_draft.rs"]
    mod composer_draft;
    #[path = "../../src/shell/composer_image_labels.rs"]
    mod composer_image_labels;
    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/transcript_branch_core.rs"]
    pub(super) mod transcript_branch_core;
    #[path = "../../src/shell/transcript_branch_menu_state.rs"]
    pub(super) mod transcript_branch_menu_state;
    #[path = "../../src/shell/transcript_edit_menu_state.rs"]
    mod transcript_edit_menu_state;
    #[path = "../../src/shell/transcript_presentation.rs"]
    mod transcript_presentation;
    #[path = "../../src/shell/transcript_projection.rs"]
    mod transcript_projection;
    #[allow(dead_code)]
    #[path = "../../src/shell/virtual_list/mod.rs"]
    mod virtual_list;

    pub(super) use self::transcript_branch_core::{
        TranscriptBranchActivationBlocker, TranscriptBranchActivationGate, TranscriptBranchBackend,
        TranscriptBranchOutcome, prepare_transcript_branch, register_transcript_branch_thread,
        run_transcript_branch, transcript_branch_activation_blocker,
    };
    pub(super) use self::transcript_branch_menu_state::{
        TranscriptBranchAction, TranscriptBranchRequest, TranscriptBranchTarget,
    };
}

use branch_bootstrap_core::BranchBootstrapBackend;
use shell::{
    TranscriptBranchAction, TranscriptBranchActivationBlocker, TranscriptBranchActivationGate,
    TranscriptBranchBackend, TranscriptBranchOutcome, TranscriptBranchRequest,
    TranscriptBranchTarget, prepare_transcript_branch, register_transcript_branch_thread,
    run_transcript_branch, transcript_branch_activation_blocker,
};

#[test]
fn branch_worker_forks_and_rolls_back_trailing_turns() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2", "turn_3"],
    ))))
    .with_rollback(Ok(rollback_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2"],
    ))));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::SwitchTo, "turn_2"),
        Duration::from_secs(1),
    );

    assert_eq!(backend.fork_calls, vec!["source_thread".to_string()]);
    assert_eq!(
        backend.rollback_calls,
        vec![("branch_thread".to_string(), 1)]
    );
    match outcome {
        TranscriptBranchOutcome::Branched {
            action,
            source_thread_id,
            source_turn_id,
            title_seed,
            thread,
            durable_summary,
            bootstrap_turn_id,
        } => {
            assert_eq!(action, TranscriptBranchAction::SwitchTo);
            assert_eq!(source_thread_id, "source_thread");
            assert_eq!(source_turn_id, "turn_2");
            assert_eq!(title_seed, "Clicked prompt");
            assert_eq!(thread.summary().id, "branch_thread");
            assert_eq!(durable_summary.id, "branch_thread");
            assert_eq!(bootstrap_turn_id.unwrap().as_str(), "bootstrap_turn");
            assert_eq!(
                thread
                    .turns
                    .iter()
                    .map(|turn| turn.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["turn_1", "turn_2"]
            );
        }
        TranscriptBranchOutcome::Failed { message, .. } => {
            panic!("expected successful branch, got failure: {message}");
        }
    }
    assert_eq!(
        backend.operations,
        vec![
            "fork:source_thread".to_string(),
            "rollback:branch_thread:1".to_string(),
            "bootstrap:branch_thread".to_string(),
            "stream".to_string(),
            "read:branch_thread".to_string()
        ]
    );
    assert!(
        backend.bootstrap_calls[0]
            .1
            .contains("Branched from [Untitled thread](beryl_threadid://source_thread)")
    );
}

#[test]
fn branch_prepare_stops_before_bootstrap_for_foreground_start() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2", "turn_3"],
    ))))
    .with_rollback(Ok(rollback_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2"],
    ))));

    let prepared = prepare_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::SwitchTo, "turn_2"),
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(prepared.branch_thread_id().as_str(), "branch_thread");
    assert_eq!(prepared.title_seed(), "Clicked prompt");
    assert!(
        prepared
            .bootstrap_message()
            .contains("Branched from [Untitled thread](beryl_threadid://source_thread)")
    );
    assert_eq!(
        backend.operations,
        vec![
            "fork:source_thread".to_string(),
            "rollback:branch_thread:1".to_string(),
        ]
    );
    assert!(backend.bootstrap_calls.is_empty());
    assert!(backend.read_metadata_calls.is_empty());
}

#[test]
fn branch_worker_skips_rollback_when_selected_turn_is_fork_tail() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2"],
    ))));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::Background, "turn_2"),
        Duration::from_secs(1),
    );

    assert!(matches!(
        outcome,
        TranscriptBranchOutcome::Branched {
            action: TranscriptBranchAction::Background,
            ..
        }
    ));
    assert!(backend.rollback_calls.is_empty());
}

#[test]
fn branch_worker_keeps_ordered_clicked_turn_fragments_as_provisional_title_seed() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2"],
    ))));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request_with_title_fragments(
            TranscriptBranchAction::Background,
            "turn_2",
            vec!["First clicked fragment", "Second clicked fragment"],
        ),
        Duration::from_secs(1),
    );

    match outcome {
        TranscriptBranchOutcome::Branched { title_seed, .. } => {
            assert_eq!(
                title_seed,
                "First clicked fragment\n\nSecond clicked fragment"
            );
        }
        TranscriptBranchOutcome::Failed { message, .. } => {
            panic!("expected successful branch, got failure: {message}");
        }
    }
}

#[test]
fn branch_worker_fails_when_fork_fails() {
    let mut backend = FakeBranchBackend::new(Err("fork unavailable".to_string()));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::Background, "turn_2"),
        Duration::from_secs(1),
    );

    match outcome {
        TranscriptBranchOutcome::Failed {
            action,
            source_thread_id,
            source_turn_id,
            message,
        } => {
            assert_eq!(action, TranscriptBranchAction::Background);
            assert_eq!(source_thread_id, "source_thread");
            assert_eq!(source_turn_id, "turn_2");
            assert!(message.contains("fork unavailable"));
        }
        TranscriptBranchOutcome::Branched { .. } => panic!("expected fork failure"),
    }
}

#[test]
fn branch_worker_fails_when_selected_turn_is_missing_from_fork() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_3"],
    ))));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::SwitchTo, "turn_2"),
        Duration::from_secs(1),
    );

    match outcome {
        TranscriptBranchOutcome::Failed { message, .. } => {
            assert!(message.contains("selected turn turn_2"));
        }
        TranscriptBranchOutcome::Branched { .. } => panic!("expected missing-turn failure"),
    }
    assert!(backend.rollback_calls.is_empty());
}

#[test]
fn branch_worker_fails_when_rollback_fails_after_fork() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2", "turn_3"],
    ))))
    .with_rollback(Err("rollback rejected".to_string()));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::SwitchTo, "turn_2"),
        Duration::from_secs(1),
    );

    match outcome {
        TranscriptBranchOutcome::Failed { message, .. } => {
            assert!(message.contains("branch_thread"));
            assert!(message.contains("rollback rejected"));
        }
        TranscriptBranchOutcome::Branched { .. } => panic!("expected rollback failure"),
    }
    assert_eq!(
        backend.rollback_calls,
        vec![("branch_thread".to_string(), 1)]
    );
}

#[test]
fn branch_worker_fails_when_bootstrap_turn_fails_after_fork_and_rollback() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2", "turn_3"],
    ))))
    .with_rollback(Ok(rollback_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2"],
    ))))
    .with_bootstrap(Err("turn rejected".to_string()));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::Background, "turn_2"),
        Duration::from_secs(1),
    );

    match outcome {
        TranscriptBranchOutcome::Failed { message, .. } => {
            assert!(message.contains("branch_thread"));
            assert!(message.contains("turn rejected"));
        }
        TranscriptBranchOutcome::Branched { .. } => panic!("expected bootstrap failure"),
    }
    assert_eq!(
        backend.operations,
        vec![
            "fork:source_thread".to_string(),
            "rollback:branch_thread:1".to_string(),
            "bootstrap:branch_thread".to_string(),
        ]
    );
    assert!(backend.read_metadata_calls.is_empty());
}

#[test]
fn branch_worker_fails_when_bootstrap_durability_read_fails() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2"],
    ))))
    .with_read_metadata(Err("not found yet".to_string()));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::Background, "turn_2"),
        Duration::from_secs(1),
    );

    match outcome {
        TranscriptBranchOutcome::Failed { message, .. } => {
            assert!(message.contains("branch_thread"));
            assert!(message.contains("not found yet"));
        }
        TranscriptBranchOutcome::Branched { .. } => panic!("expected durability failure"),
    }
    assert_eq!(
        backend.operations,
        vec![
            "fork:source_thread".to_string(),
            "bootstrap:branch_thread".to_string(),
            "stream".to_string(),
            "read:branch_thread".to_string(),
        ]
    );
}

#[test]
fn branch_finish_rejects_source_removed_while_worker_was_in_flight() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2"],
    ))));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::Background, "turn_2"),
        Duration::from_secs(1),
    );
    let TranscriptBranchOutcome::Branched {
        source_thread_id,
        thread,
        ..
    } = outcome
    else {
        panic!("expected successful backend branch");
    };

    let mut state = WorkspaceConversationState::default();
    let error = register_transcript_branch_thread(
        &mut state,
        &ConversationThreadId::new(source_thread_id),
        &ConversationTurnId::new("turn_2"),
        &thread.summary(),
        Some(ConversationTurnId::new("bootstrap_turn")),
    )
    .unwrap_err();

    assert!(error.contains("source thread source_thread"));
    assert!(
        state
            .thread_registration(&ConversationThreadId::new("branch_thread"))
            .is_none()
    );
}

#[test]
fn branch_finish_rejects_source_target_changed_while_worker_was_in_flight() {
    let mut backend = FakeBranchBackend::new(Ok(fork_response(thread_info(
        "branch_thread",
        r"C:\work\alpha",
        &["turn_1", "turn_2"],
    ))));

    let outcome = run_transcript_branch(
        &mut backend,
        branch_request(TranscriptBranchAction::Background, "turn_2"),
        Duration::from_secs(1),
    );
    let TranscriptBranchOutcome::Branched {
        source_thread_id,
        thread,
        ..
    } = outcome
    else {
        panic!("expected successful backend branch");
    };

    let changed_target = WorkspaceId::host_windows(r"C:\work\beta");
    let mut state = WorkspaceConversationState::default();
    state.attach_execution_target(&changed_target).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new(source_thread_id.clone()),
        changed_target,
        "Source",
        None,
        1,
        2,
    ));

    let error = register_transcript_branch_thread(
        &mut state,
        &ConversationThreadId::new(source_thread_id),
        &ConversationTurnId::new("turn_2"),
        &thread.summary(),
        Some(ConversationTurnId::new("bootstrap_turn")),
    )
    .unwrap_err();

    assert!(error.contains(r"C:\work\alpha"));
    assert!(error.contains(r"C:\work\beta"));
    assert!(
        state
            .thread_registration(&ConversationThreadId::new("branch_thread"))
            .is_none()
    );
}

#[test]
fn branch_switch_activation_blocker_keeps_registered_branch_after_creation() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut state = WorkspaceConversationState::default();
    state.attach_execution_target(&execution_target).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("source_thread"),
        execution_target,
        "Source",
        None,
        1,
        2,
    ));

    register_transcript_branch_thread(
        &mut state,
        &ConversationThreadId::new("source_thread"),
        &ConversationTurnId::new("turn_2"),
        &thread_summary("branch_thread", r"C:\work\alpha", None),
        Some(ConversationTurnId::new("bootstrap_turn")),
    )
    .unwrap();

    let blocker = transcript_branch_activation_blocker(TranscriptBranchActivationGate {
        activation_in_progress: true,
        workspace_ready: true,
        execution_target_matches_branch: true,
        backend_available: true,
    });

    assert_eq!(
        blocker,
        Some(TranscriptBranchActivationBlocker::ActivationInProgress)
    );
    assert!(
        blocker
            .unwrap()
            .notice_detail()
            .contains("another thread activation")
    );
    assert!(
        state
            .thread_registration(&ConversationThreadId::new("branch_thread"))
            .is_some()
    );
}

#[test]
fn branch_switch_activation_gate_reports_stale_workspace_targets() {
    assert_eq!(
        transcript_branch_activation_blocker(TranscriptBranchActivationGate {
            activation_in_progress: false,
            workspace_ready: false,
            execution_target_matches_branch: false,
            backend_available: true,
        }),
        Some(TranscriptBranchActivationBlocker::WorkspaceNotReady)
    );
    assert_eq!(
        transcript_branch_activation_blocker(TranscriptBranchActivationGate {
            activation_in_progress: false,
            workspace_ready: true,
            execution_target_matches_branch: false,
            backend_available: true,
        }),
        Some(TranscriptBranchActivationBlocker::ExecutionTargetChanged)
    );
    assert_eq!(
        transcript_branch_activation_blocker(TranscriptBranchActivationGate {
            activation_in_progress: false,
            workspace_ready: true,
            execution_target_matches_branch: true,
            backend_available: false,
        }),
        Some(TranscriptBranchActivationBlocker::BackendUnavailable)
    );
    assert_eq!(
        transcript_branch_activation_blocker(TranscriptBranchActivationGate {
            activation_in_progress: false,
            workspace_ready: true,
            execution_target_matches_branch: true,
            backend_available: true,
        }),
        None
    );
}

#[test]
fn branch_registration_copies_source_binding_and_marks_branch_title_eligible() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut state = WorkspaceConversationState::default();
    state.attach_execution_target(&execution_target).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("source_thread"),
        execution_target.clone(),
        "Source",
        None,
        1,
        2,
    ));

    let (registered_target, changed) = register_transcript_branch_thread(
        &mut state,
        &ConversationThreadId::new("source_thread"),
        &ConversationTurnId::new("turn_2"),
        &thread_summary("branch_thread", r"C:\work\alpha", None),
        Some(ConversationTurnId::new("bootstrap_turn")),
    )
    .unwrap();

    assert!(changed);
    assert_eq!(registered_target, execution_target);
    let branch = state
        .thread_registration(&ConversationThreadId::new("branch_thread"))
        .expect("branch should be registered");
    assert!(branch.beryl_created());
    assert!(branch.member_binding().is_some());
    assert_eq!(
        branch.branch_parent_thread_id().unwrap().as_str(),
        "source_thread"
    );
    assert_eq!(branch.branch_source_turn_id().unwrap().as_str(), "turn_2");
    assert_eq!(
        branch.branch_bootstrap_turn_id().unwrap().as_str(),
        "bootstrap_turn"
    );
    assert_eq!(
        branch.branch_title_retitle_state(),
        BranchThreadTitleRetitleState::AwaitingFirstRealUserTurn
    );
    assert!(
        state.thread_automatic_title_generation_eligible(&ConversationThreadId::new(
            "branch_thread"
        ))
    );
}

#[test]
fn branch_registration_ignores_copied_source_name_for_title_eligibility() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut state = WorkspaceConversationState::default();
    state.attach_execution_target(&execution_target).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("source_thread"),
        execution_target.clone(),
        "Source",
        Some("Conversation Branching Test".to_string()),
        1,
        2,
    ));

    let (_, changed) = register_transcript_branch_thread(
        &mut state,
        &ConversationThreadId::new("source_thread"),
        &ConversationTurnId::new("turn_2"),
        &thread_summary(
            "branch_thread",
            r"C:\work\alpha",
            Some("Conversation Branching Test"),
        ),
        Some(ConversationTurnId::new("bootstrap_turn")),
    )
    .unwrap();

    assert!(changed);
    let branch_id = ConversationThreadId::new("branch_thread");
    let branch = state
        .thread_registration(&branch_id)
        .expect("branch should be registered");
    assert_eq!(branch.backend_name(), None);
    assert!(state.thread_automatic_title_generation_eligible(&branch_id));
}

#[test]
fn branch_registration_preserves_distinct_backend_name() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut state = WorkspaceConversationState::default();
    state.attach_execution_target(&execution_target).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("source_thread"),
        execution_target.clone(),
        "Source",
        None,
        1,
        2,
    ));

    let (_, changed) = register_transcript_branch_thread(
        &mut state,
        &ConversationThreadId::new("source_thread"),
        &ConversationTurnId::new("turn_2"),
        &thread_summary("branch_thread", r"C:\work\alpha", Some("Backend Fork Name")),
        Some(ConversationTurnId::new("bootstrap_turn")),
    )
    .unwrap();

    assert!(changed);
    let branch_id = ConversationThreadId::new("branch_thread");
    let branch = state
        .thread_registration(&branch_id)
        .expect("branch should be registered");
    assert_eq!(branch.backend_name(), Some("Backend Fork Name"));
    assert!(!state.thread_automatic_title_generation_eligible(&branch_id));
}

#[test]
fn branch_registration_rejects_missing_source_and_cwd_mismatch() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut state = WorkspaceConversationState::default();

    let missing_source = register_transcript_branch_thread(
        &mut state,
        &ConversationThreadId::new("source_thread"),
        &ConversationTurnId::new("turn_2"),
        &thread_summary("branch_thread", r"C:\work\alpha", None),
        Some(ConversationTurnId::new("bootstrap_turn")),
    )
    .unwrap_err();
    assert!(missing_source.contains("source thread source_thread"));

    state.attach_execution_target(&execution_target).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("source_thread"),
        execution_target,
        "Source",
        None,
        1,
        2,
    ));
    let mismatch = register_transcript_branch_thread(
        &mut state,
        &ConversationThreadId::new("source_thread"),
        &ConversationTurnId::new("turn_2"),
        &thread_summary("branch_thread", r"C:\work\beta", None),
        Some(ConversationTurnId::new("bootstrap_turn")),
    )
    .unwrap_err();
    assert!(mismatch.contains(r"C:\work\beta"));
    assert!(mismatch.contains(r"C:\work\alpha"));
}

struct FakeBranchBackend {
    fork_response: Option<Result<ThreadForkResponse, String>>,
    rollback_response: Option<Result<ThreadRollbackResponse, String>>,
    bootstrap_response: Option<Result<TurnStartResponse, String>>,
    read_thread_response: Option<Result<ThreadInfo, String>>,
    stream_events: VecDeque<Result<Option<TurnStreamEvent>, String>>,
    fork_calls: Vec<String>,
    rollback_calls: Vec<(String, u32)>,
    bootstrap_calls: Vec<(String, String, TurnStartOptions)>,
    read_metadata_calls: Vec<String>,
    operations: Vec<String>,
}

impl FakeBranchBackend {
    fn new(fork_response: Result<ThreadForkResponse, String>) -> Self {
        Self {
            fork_response: Some(fork_response),
            rollback_response: None,
            bootstrap_response: Some(Ok(turn_start_response("bootstrap_turn"))),
            read_thread_response: Some(Ok(thread_info_with_bootstrap(
                "branch_thread",
                r"C:\work\alpha",
                "bootstrap_turn",
                "Branched from [Untitled thread](beryl_threadid://source_thread), no response required.",
            ))),
            stream_events: VecDeque::from([Ok(Some(turn_completed(
                "branch_thread",
                "bootstrap_turn",
            )))]),
            fork_calls: Vec::new(),
            rollback_calls: Vec::new(),
            bootstrap_calls: Vec::new(),
            read_metadata_calls: Vec::new(),
            operations: Vec::new(),
        }
    }

    fn with_rollback(mut self, rollback_response: Result<ThreadRollbackResponse, String>) -> Self {
        self.rollback_response = Some(rollback_response);
        self
    }

    fn with_bootstrap(mut self, response: Result<TurnStartResponse, String>) -> Self {
        self.bootstrap_response = Some(response);
        self
    }

    fn with_read_metadata(mut self, response: Result<ThreadInfo, String>) -> Self {
        self.read_thread_response = Some(response);
        self
    }
}

impl TranscriptBranchBackend for FakeBranchBackend {
    fn fork_thread(&mut self, thread_id: &str, _: Duration) -> Result<ThreadForkResponse, String> {
        self.operations.push(format!("fork:{thread_id}"));
        self.fork_calls.push(thread_id.to_string());
        self.fork_response
            .take()
            .expect("fork should only be called once")
    }

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        _: Duration,
    ) -> Result<ThreadRollbackResponse, String> {
        self.operations
            .push(format!("rollback:{thread_id}:{num_turns}"));
        self.rollback_calls.push((thread_id.to_string(), num_turns));
        self.rollback_response
            .take()
            .expect("rollback response should be provided")
    }
}

impl BranchBootstrapBackend for FakeBranchBackend {
    type Error = String;

    fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        _: Duration,
    ) -> Result<TurnStartResponse, Self::Error> {
        self.operations.push(format!("bootstrap:{thread_id}"));
        self.bootstrap_calls
            .push((thread_id.to_string(), text.to_string(), options));
        self.bootstrap_response
            .take()
            .expect("bootstrap response should be provided")
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
        self.operations.push(format!("read:{thread_id}"));
        self.read_metadata_calls.push(thread_id.to_string());
        self.read_thread_response
            .take()
            .expect("read thread response should be provided")
    }

    fn next_turn_stream_event(
        &mut self,
        _: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error> {
        self.operations.push("stream".to_string());
        self.stream_events
            .pop_front()
            .expect("stream event should be provided")
    }

    fn deny_approval_request(&mut self, _: &ApprovalRequest) -> Result<(), Self::Error> {
        Ok(())
    }

    fn respond_dynamic_tool_call(
        &mut self,
        _: &DynamicToolCallRequest,
        _: &DynamicToolCallResponse,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn branch_request(action: TranscriptBranchAction, source_turn_id: &str) -> TranscriptBranchRequest {
    branch_request_with_title_fragments(action, source_turn_id, vec!["Clicked prompt"])
}

fn branch_request_with_title_fragments(
    action: TranscriptBranchAction,
    source_turn_id: &str,
    title_seed_fragments: Vec<&str>,
) -> TranscriptBranchRequest {
    TranscriptBranchRequest::for_test(
        action,
        TranscriptBranchTarget::for_test(
            "source_thread",
            source_turn_id,
            0,
            title_seed_fragments
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
    )
}

fn fork_response(thread: ThreadInfo) -> ThreadForkResponse {
    ThreadForkResponse {
        thread,
        model: Some("gpt-5.4".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("medium".to_string()),
    }
}

fn rollback_response(thread: ThreadInfo) -> ThreadRollbackResponse {
    ThreadRollbackResponse { thread }
}

fn turn_start_response(id: &str) -> TurnStartResponse {
    TurnStartResponse {
        turn: TurnInfo {
            id: id.to_string(),
            status: TurnStatus::InProgress,
            items_view: beryl_backend::TurnItemsView::Full,
            items: Vec::new(),
            error: None,
        },
    }
}

fn thread_info(id: &str, cwd: &str, turn_ids: &[&str]) -> ThreadInfo {
    serde_json::from_value(json!({
        "cliVersion": "0.128.0",
        "createdAt": 10,
        "cwd": cwd,
        "ephemeral": false,
        "id": id,
        "modelProvider": "openai",
        "preview": "Branch preview",
        "source": "appServer",
        "status": { "type": "idle" },
        "turns": turn_ids.iter().map(|turn_id| {
            json!({
                "id": turn_id,
                "status": "completed",
                "items": []
            })
        }).collect::<Vec<_>>(),
        "updatedAt": 20
    }))
    .unwrap()
}

fn thread_summary(id: &str, cwd: &str, name: Option<&str>) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        forked_from_id: None,
        cwd: PathBuf::from(cwd),
        preview: "Branch preview".to_string(),
        name: name.map(str::to_string),
        agent_nickname: None,
        path: None,
        created_at: 10,
        updated_at: 20,
        model_provider: "openai".to_string(),
        ephemeral: false,
    }
}

fn thread_info_with_bootstrap(id: &str, cwd: &str, turn_id: &str, message: &str) -> ThreadInfo {
    serde_json::from_value(json!({
        "cliVersion": "0.128.0",
        "createdAt": 10,
        "cwd": cwd,
        "ephemeral": false,
        "id": id,
        "modelProvider": "openai",
        "preview": "Branch preview",
        "source": "appServer",
        "status": { "type": "idle" },
        "turns": [completed_bootstrap_turn_json(turn_id, message)],
        "updatedAt": 20
    }))
    .unwrap()
}

#[allow(dead_code)]
fn turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
        items: Vec::new(),
        error: None,
    }
}

fn turn_completed(thread_id: &str, turn_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::TurnCompleted {
        thread_id: thread_id.to_string(),
        turn: completed_bootstrap_turn(
            turn_id,
            "Branched from [Untitled thread](beryl_threadid://source_thread), no response required.",
        ),
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

fn completed_bootstrap_turn_json(turn_id: &str, message: &str) -> serde_json::Value {
    json!({
        "id": turn_id,
        "status": "completed",
        "items": [{
            "type": "userMessage",
            "id": "user_message",
            "content": [{
                "type": "text",
                "text": message
            }]
        }]
    })
}
