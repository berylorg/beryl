#![allow(dead_code, private_interfaces, unused_imports)]

use std::{path::PathBuf, time::Duration};

use beryl_backend::{
    ThreadForkResponse, ThreadInfo, ThreadRollbackResponse, ThreadSummary, TurnInfo, TurnStatus,
};
use beryl_model::conversation::{
    ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState,
};
use beryl_model::workspace::WorkspaceId;
use serde_json::json;

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
        TranscriptBranchOutcome, register_transcript_branch_thread, run_transcript_branch,
        transcript_branch_activation_blocker,
    };
    pub(super) use self::transcript_branch_menu_state::{
        TranscriptBranchAction, TranscriptBranchRequest, TranscriptBranchTarget,
    };
}

use shell::{
    TranscriptBranchAction, TranscriptBranchActivationBlocker, TranscriptBranchActivationGate,
    TranscriptBranchBackend, TranscriptBranchOutcome, TranscriptBranchRequest,
    TranscriptBranchTarget, register_transcript_branch_thread, run_transcript_branch,
    transcript_branch_activation_blocker,
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
        } => {
            assert_eq!(action, TranscriptBranchAction::SwitchTo);
            assert_eq!(source_thread_id, "source_thread");
            assert_eq!(source_turn_id, "turn_2");
            assert_eq!(title_seed, "Clicked prompt");
            assert_eq!(thread.summary().id, "branch_thread");
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
        &thread.summary(),
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
        &thread.summary(),
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
        &thread_summary("branch_thread", r"C:\work\alpha", None),
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
        &thread_summary("branch_thread", r"C:\work\alpha", None),
    )
    .unwrap();

    assert!(changed);
    assert_eq!(registered_target, execution_target);
    let branch = state
        .thread_registration(&ConversationThreadId::new("branch_thread"))
        .expect("branch should be registered");
    assert!(branch.beryl_created());
    assert!(branch.member_binding().is_some());
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
        &thread_summary(
            "branch_thread",
            r"C:\work\alpha",
            Some("Conversation Branching Test"),
        ),
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
        &thread_summary("branch_thread", r"C:\work\alpha", Some("Backend Fork Name")),
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
        &thread_summary("branch_thread", r"C:\work\alpha", None),
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
        &thread_summary("branch_thread", r"C:\work\beta", None),
    )
    .unwrap_err();
    assert!(mismatch.contains(r"C:\work\beta"));
    assert!(mismatch.contains(r"C:\work\alpha"));
}

struct FakeBranchBackend {
    fork_response: Option<Result<ThreadForkResponse, String>>,
    rollback_response: Option<Result<ThreadRollbackResponse, String>>,
    fork_calls: Vec<String>,
    rollback_calls: Vec<(String, u32)>,
}

impl FakeBranchBackend {
    fn new(fork_response: Result<ThreadForkResponse, String>) -> Self {
        Self {
            fork_response: Some(fork_response),
            rollback_response: None,
            fork_calls: Vec::new(),
            rollback_calls: Vec::new(),
        }
    }

    fn with_rollback(mut self, rollback_response: Result<ThreadRollbackResponse, String>) -> Self {
        self.rollback_response = Some(rollback_response);
        self
    }
}

impl TranscriptBranchBackend for FakeBranchBackend {
    type Error = String;

    fn fork_thread(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<ThreadForkResponse, Self::Error> {
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
    ) -> Result<ThreadRollbackResponse, Self::Error> {
        self.rollback_calls.push((thread_id.to_string(), num_turns));
        self.rollback_response
            .take()
            .expect("rollback response should be provided")
    }
}

fn branch_request(action: TranscriptBranchAction, source_turn_id: &str) -> TranscriptBranchRequest {
    TranscriptBranchRequest::for_test(
        action,
        TranscriptBranchTarget::for_test(
            "source_thread",
            source_turn_id,
            0,
            vec!["Clicked prompt".to_string()],
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

#[allow(dead_code)]
fn turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: Vec::new(),
        error: None,
    }
}
