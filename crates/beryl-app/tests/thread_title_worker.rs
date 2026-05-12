use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use beryl_backend::{
    AgentMessageItem, ProtocolPhase, ThreadItem, ThreadSessionResponse, ThreadStartOptions,
    ThreadUnsubscribeResponse, ThreadUnsubscribeStatus, TurnInfo, TurnStartOptions,
    TurnStartResponse, TurnStatus, TurnStreamEvent,
};
use beryl_model::workspace::WorkspaceId;
use serde_json::json;

#[allow(dead_code, unused_imports)]
#[path = "../src/shell/thread_title.rs"]
mod thread_title;

use thread_title::{
    ThreadTitleBackend, ThreadTitleCancellation, ThreadTitleCandidate, ThreadTitleResult,
    ThreadTitleTask, ThreadTitleTaskOutcome, ThreadTitleUpdate, cancel_all_thread_title_tasks,
    run_thread_title_attempt, run_thread_title_attempt_with_cancellation,
    thread_title_repair_candidate,
};

#[test]
fn title_worker_publishes_generated_title_without_foreground_gate() {
    let candidate = candidate();
    let mut backend = FakeTitleBackend::with_turn(completed_title_turn("Improve Thread Titles"));

    let result = run_thread_title_attempt(
        &mut backend,
        &workspace(),
        candidate,
        Duration::from_secs(1),
        Duration::from_secs(1),
    );

    assert_eq!(
        result,
        ThreadTitleResult::Applied {
            title: "Improve Thread Titles".to_string()
        }
    );
    assert_eq!(
        backend.set_names,
        vec![(
            "target_thread".to_string(),
            "Improve Thread Titles".to_string()
        )]
    );
    assert_eq!(backend.unsubscribed, vec!["maintenance_thread".to_string()]);
}

#[test]
fn title_worker_times_out_and_cleans_up() {
    let candidate = candidate();
    let mut backend = FakeTitleBackend::with_turn(in_progress_title_turn());

    let result = run_thread_title_attempt(
        &mut backend,
        &workspace(),
        candidate,
        Duration::from_secs(1),
        Duration::ZERO,
    );

    assert!(matches!(result, ThreadTitleResult::Failed { .. }));
    assert!(backend.set_names.is_empty());
    assert_eq!(backend.unsubscribed, vec!["maintenance_thread".to_string()]);
}

#[test]
fn title_worker_fails_when_generation_turn_fails() {
    let candidate = candidate();
    let mut backend = FakeTitleBackend::with_turn(failed_title_turn());

    let result = run_thread_title_attempt(
        &mut backend,
        &workspace(),
        candidate,
        Duration::from_secs(1),
        Duration::from_secs(1),
    );

    assert!(matches!(result, ThreadTitleResult::Failed { .. }));
    assert!(backend.set_names.is_empty());
    assert_eq!(backend.unsubscribed, vec!["maintenance_thread".to_string()]);
}

#[test]
fn title_worker_reports_name_setting_failure() {
    let candidate = candidate();
    let mut backend = FakeTitleBackend::with_turn(completed_title_turn("Useful Title"));
    backend.set_name_error = Some("name set failed".to_string());

    let result = run_thread_title_attempt(
        &mut backend,
        &workspace(),
        candidate,
        Duration::from_secs(1),
        Duration::from_secs(1),
    );

    assert!(matches!(result, ThreadTitleResult::Failed { .. }));
    assert_eq!(
        backend.set_names,
        vec![("target_thread".to_string(), "Useful Title".to_string())]
    );
    assert_eq!(backend.unsubscribed, vec!["maintenance_thread".to_string()]);
}

#[test]
fn title_worker_skips_backend_work_when_cancelled_before_start() {
    let candidate = candidate();
    let cancellation = ThreadTitleCancellation::new();
    cancellation.cancel();
    let mut backend = FakeTitleBackend::with_turn(completed_title_turn("Ignored Title"));

    let result = run_thread_title_attempt_with_cancellation(
        &mut backend,
        &workspace(),
        candidate,
        cancellation,
        Duration::from_secs(1),
        Duration::from_secs(1),
    );

    assert_eq!(result, ThreadTitleResult::Cancelled);
    assert_eq!(backend.started_threads, 0);
    assert_eq!(backend.started_turns, 0);
    assert!(backend.set_names.is_empty());
    assert!(backend.unsubscribed.is_empty());
}

#[test]
fn title_worker_cancellation_while_generation_in_progress_skips_name_setting() {
    let candidate = candidate();
    let cancellation = ThreadTitleCancellation::new();
    let cancellation_signal = cancellation.clone();
    let cancel_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        cancellation_signal.cancel();
    });
    let mut backend = FakeTitleBackend::with_turn(in_progress_title_turn());
    backend.empty_poll_delay = Duration::from_millis(5);

    let result = run_thread_title_attempt_with_cancellation(
        &mut backend,
        &workspace(),
        candidate,
        cancellation,
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    cancel_thread.join().unwrap();

    assert_eq!(result, ThreadTitleResult::Cancelled);
    assert!(backend.set_names.is_empty());
    assert_eq!(backend.unsubscribed, vec!["maintenance_thread".to_string()]);
}

#[test]
fn title_worker_keeps_applied_result_when_cancelled_during_name_setting() {
    let candidate = candidate();
    let cancellation = ThreadTitleCancellation::new();
    let mut backend = FakeTitleBackend::with_turn(completed_title_turn("Useful Title"));
    backend.cancel_on_set_name = Some(cancellation.clone());

    let result = run_thread_title_attempt_with_cancellation(
        &mut backend,
        &workspace(),
        candidate,
        cancellation,
        Duration::from_secs(1),
        Duration::from_secs(1),
    );

    assert_eq!(
        result,
        ThreadTitleResult::Applied {
            title: "Useful Title".to_string()
        }
    );
    assert_eq!(
        backend.set_names,
        vec![("target_thread".to_string(), "Useful Title".to_string())]
    );
    assert_eq!(backend.unsubscribed, vec!["maintenance_thread".to_string()]);
}

#[test]
fn cancelling_thread_title_task_drains_finished_applied_result() {
    let cancellation = ThreadTitleCancellation::new();
    let (sender, receiver) = mpsc::channel();
    sender
        .send(ThreadTitleUpdate::Finished {
            thread_id: "target_thread".to_string(),
            result: ThreadTitleResult::Applied {
                title: "Useful Title".to_string(),
            },
        })
        .unwrap();
    let mut tasks = vec![ThreadTitleTask::new(
        "target_thread".to_string(),
        cancellation.clone(),
        receiver,
    )];

    let outcomes = cancel_all_thread_title_tasks(&mut tasks);

    assert!(tasks.is_empty());
    assert!(!cancellation.is_cancelled());
    assert_eq!(
        outcomes,
        vec![ThreadTitleTaskOutcome::Finished {
            thread_id: "target_thread".to_string(),
            result: ThreadTitleResult::Applied {
                title: "Useful Title".to_string()
            }
        }]
    );
}

#[test]
fn cancelling_unfinished_thread_title_task_abandons_without_waiting() {
    let cancellation = ThreadTitleCancellation::new();
    let (_sender, receiver) = mpsc::channel();
    let mut tasks = vec![ThreadTitleTask::new(
        "target_thread".to_string(),
        cancellation.clone(),
        receiver,
    )];

    let outcomes = cancel_all_thread_title_tasks(&mut tasks);

    assert!(tasks.is_empty());
    assert!(cancellation.is_cancelled());
    assert_eq!(
        outcomes,
        vec![ThreadTitleTaskOutcome::Abandoned {
            thread_id: "target_thread".to_string()
        }]
    );
}

#[test]
fn repair_candidate_prefers_earliest_known_input_over_later_fallback() {
    let candidate = thread_title_repair_candidate(
        "target_thread",
        true,
        None,
        false,
        Some("First submitted prompt"),
        Some("Later repair turn"),
    )
    .unwrap();

    assert_eq!(candidate.target_thread_id(), "target_thread");
    assert_eq!(candidate.user_input(), "First submitted prompt");
}

#[test]
fn repair_candidate_uses_activation_loaded_input_without_turn_fallback() {
    let candidate = thread_title_repair_candidate(
        "target_thread",
        true,
        None,
        false,
        Some("Loaded history prompt"),
        None,
    )
    .unwrap();

    assert_eq!(candidate.target_thread_id(), "target_thread");
    assert_eq!(candidate.user_input(), "Loaded history prompt");
}

#[test]
fn repair_candidate_uses_later_fallback_when_no_earlier_input_is_known() {
    let candidate = thread_title_repair_candidate(
        "target_thread",
        true,
        None,
        false,
        None,
        Some("Later repair turn"),
    )
    .unwrap();

    assert_eq!(candidate.target_thread_id(), "target_thread");
    assert_eq!(candidate.user_input(), "Later repair turn");
}

#[test]
fn title_candidate_truncates_retained_user_input_seed() {
    let input = "x".repeat(4_500);
    let candidate = ThreadTitleCandidate::new("target_thread", input).unwrap();

    assert_eq!(candidate.user_input().len(), 4_000);
}

#[test]
fn repair_candidate_respects_title_and_task_guards() {
    assert!(
        thread_title_repair_candidate("target_thread", false, None, false, Some("Prompt"), None,)
            .is_none()
    );
    assert!(
        thread_title_repair_candidate(
            "target_thread",
            true,
            Some("Named thread"),
            false,
            Some("Prompt"),
            None,
        )
        .is_none()
    );
    assert!(
        thread_title_repair_candidate("target_thread", true, None, true, Some("Prompt"), None,)
            .is_none()
    );
    assert!(
        thread_title_repair_candidate("target_thread", true, None, false, None, None).is_none()
    );
}

struct FakeTitleBackend {
    turn_response: TurnStartResponse,
    events: VecDeque<Result<Option<TurnStreamEvent>, String>>,
    set_name_error: Option<String>,
    cancel_on_set_name: Option<ThreadTitleCancellation>,
    empty_poll_delay: Duration,
    started_threads: usize,
    started_turns: usize,
    set_names: Vec<(String, String)>,
    unsubscribed: Vec<String>,
}

impl FakeTitleBackend {
    fn with_turn(turn_response: TurnStartResponse) -> Self {
        Self {
            turn_response,
            events: VecDeque::new(),
            set_name_error: None,
            cancel_on_set_name: None,
            empty_poll_delay: Duration::ZERO,
            started_threads: 0,
            started_turns: 0,
            set_names: Vec::new(),
            unsubscribed: Vec::new(),
        }
    }
}

impl ThreadTitleBackend for FakeTitleBackend {
    type Error = String;

    fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        _: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        self.started_threads += 1;
        assert_eq!(cwd, workspace().canonical_path());
        assert!(options.is_ephemeral());
        assert!(options.dynamic_tools().is_empty());
        assert_eq!(
            options.developer_instructions(),
            Some(
                "You generate concise conversation titles.\nReturn only the title text.\nUse 2 to 6 words when possible.\nUse title case unless preserving code, file names, or exact product names.\nDo not use tools, markdown, quotation marks, trailing punctuation, or explanations."
            )
        );
        Ok(thread_session_response("maintenance_thread", true))
    }

    fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        _: Duration,
    ) -> Result<TurnStartResponse, Self::Error> {
        self.started_turns += 1;
        assert_eq!(thread_id, "maintenance_thread");
        assert!(text.contains("<first_user_message>"));
        assert_eq!(options.reasoning_effort(), Some("medium"));
        Ok(self.turn_response.clone())
    }

    fn next_turn_stream_event(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error> {
        self.events.pop_front().unwrap_or_else(|| {
            thread::sleep(self.empty_poll_delay.min(idle_timeout));
            Ok(None)
        })
    }

    fn set_thread_name(
        &mut self,
        thread_id: &str,
        name: &str,
        _: Duration,
    ) -> Result<(), Self::Error> {
        self.set_names
            .push((thread_id.to_string(), name.to_string()));
        if let Some(cancellation) = &self.cancel_on_set_name {
            cancellation.cancel();
        }
        if let Some(error) = &self.set_name_error {
            Err(error.clone())
        } else {
            Ok(())
        }
    }

    fn unsubscribe_thread(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<ThreadUnsubscribeResponse, Self::Error> {
        self.unsubscribed.push(thread_id.to_string());
        Ok(ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        })
    }
}

fn candidate() -> ThreadTitleCandidate {
    ThreadTitleCandidate::new("target_thread", "please improve the thread title").unwrap()
}

fn workspace() -> WorkspaceId {
    WorkspaceId::host_windows(PathBuf::from(r"C:\work\beryl"))
}

fn completed_title_turn(title: &str) -> TurnStartResponse {
    TurnStartResponse {
        turn: TurnInfo {
            id: "maintenance_turn".to_string(),
            status: TurnStatus::Completed,
            items: vec![ThreadItem::AgentMessage(AgentMessageItem {
                id: "message_1".to_string(),
                text: title.to_string(),
                phase: Some(ProtocolPhase::FinalAnswer),
            })],
            error: None,
        },
    }
}

fn in_progress_title_turn() -> TurnStartResponse {
    TurnStartResponse {
        turn: TurnInfo {
            id: "maintenance_turn".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    }
}

fn failed_title_turn() -> TurnStartResponse {
    TurnStartResponse {
        turn: TurnInfo {
            id: "maintenance_turn".to_string(),
            status: TurnStatus::Failed,
            items: Vec::new(),
            error: None,
        },
    }
}

fn thread_session_response(thread_id: &str, ephemeral: bool) -> ThreadSessionResponse {
    serde_json::from_value(json!({
        "thread": {
            "id": thread_id,
            "cwd": r"C:\work\beryl",
            "preview": "",
            "createdAt": 0,
            "updatedAt": 0,
            "modelProvider": "openai",
            "ephemeral": ephemeral,
            "status": { "type": "idle" },
            "turns": []
        }
    }))
    .unwrap()
}
