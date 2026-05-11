use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use beryl_backend::{TurnStartOptions, TurnStreamEvent};

#[path = "thread_title/backend.rs"]
mod backend;
#[path = "thread_title/cleanup.rs"]
mod cleanup;
#[path = "thread_title/generated.rs"]
mod generated;
#[allow(dead_code)]
#[path = "thread_title/task.rs"]
mod task;
#[path = "thread_title/types.rs"]
mod types;
#[path = "thread_title/worker.rs"]
mod worker;

pub(crate) use backend::ThreadTitleBackend;
#[allow(unused_imports)]
pub(super) use task::{
    ThreadTitleTask, ThreadTitleTaskOutcome, cancel_all_thread_title_tasks,
    cancel_thread_title_tasks_for_thread, poll_thread_title_tasks,
    thread_title_task_active_for_thread,
};
pub(crate) use types::{ThreadTitleResult, ThreadTitleUpdate};
pub(super) use worker::spawn_thread_title_worker;
#[allow(unused_imports)]
pub(crate) use worker::{run_thread_title_attempt, run_thread_title_attempt_with_cancellation};

const TITLE_DEVELOPER_INSTRUCTIONS: &str = r#"You generate concise conversation titles.
Return only the title text.
Use 2 to 6 words when possible.
Use title case unless preserving code, file names, or exact product names.
Do not use tools, markdown, quotation marks, trailing punctuation, or explanations."#;
const TITLE_GENERATION_TIMEOUT: Duration = Duration::from_secs(30);
const TITLE_GENERATION_STREAM_POLL_INTERVAL: Duration = Duration::from_millis(25);
const TITLE_PROMPT_INPUT_LIMIT: usize = 4_000;
const MAX_TITLE_CHARS: usize = 80;
const TITLE_REASONING_EFFORT: &str = "medium";

#[derive(Clone, Debug)]
pub(super) struct ThreadTitleCandidate {
    target_thread_id: String,
    user_input: String,
}

#[derive(Clone, Debug)]
pub(super) struct ThreadTitleCancellation {
    cancelled: Arc<AtomicBool>,
}

impl ThreadTitleCandidate {
    pub(super) fn new(
        target_thread_id: impl Into<String>,
        user_input: impl Into<String>,
    ) -> Option<Self> {
        let target_thread_id = target_thread_id.into();
        let user_input = user_input.into();
        if target_thread_id.trim().is_empty() || user_input.trim().is_empty() {
            return None;
        }

        Some(Self {
            target_thread_id,
            user_input,
        })
    }

    pub(super) fn target_thread_id(&self) -> &str {
        &self.target_thread_id
    }

    pub(super) fn user_input(&self) -> &str {
        &self.user_input
    }
}

impl ThreadTitleCancellation {
    pub(super) fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub(crate) fn title_generation_turn_options() -> TurnStartOptions {
    TurnStartOptions::default().with_reasoning_effort(TITLE_REASONING_EFFORT)
}

pub(super) fn thread_title_repair_candidate(
    target_thread_id: &str,
    automatic_title_generation_allowed: bool,
    backend_thread_name: Option<&str>,
    title_task_active: bool,
    earliest_known_user_input: Option<&str>,
    fallback_user_input: Option<&str>,
) -> Option<ThreadTitleCandidate> {
    if title_task_active
        || !automatic_title_generation_allowed
        || normalized_backend_thread_name(backend_thread_name).is_some()
    {
        return None;
    }

    let user_input = [earliest_known_user_input, fallback_user_input]
        .into_iter()
        .flatten()
        .find(|input| !input.trim().is_empty())?;
    ThreadTitleCandidate::new(target_thread_id.to_string(), user_input.to_string())
}

fn build_title_prompt(user_input: &str) -> String {
    let user_input = truncate_for_prompt(user_input.trim());
    format!(
        "Create a short title for a Codex conversation whose first user message is below.\n\
Return only the title.\n\n\
<first_user_message>\n{user_input}\n</first_user_message>"
    )
}

fn truncate_for_prompt(input: &str) -> String {
    input.chars().take(TITLE_PROMPT_INPUT_LIMIT).collect()
}

fn normalized_backend_thread_name(name: Option<&str>) -> Option<&str> {
    let name = name?.trim();
    (!name.is_empty()).then_some(name)
}

fn accepted_title(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let without_prefix = line
        .strip_prefix("Title:")
        .or_else(|| line.strip_prefix("title:"))
        .unwrap_or(line)
        .trim();
    let title = without_prefix
        .trim_matches(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '"' | '\'' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '{' | '}'
                )
        })
        .trim_end_matches(|ch: char| matches!(ch, '.' | ':' | ';'))
        .trim();
    if title.is_empty() {
        return None;
    }

    let title = clamp_chars(title, MAX_TITLE_CHARS);
    title
        .chars()
        .any(|ch| ch.is_alphanumeric())
        .then_some(title)
}

fn clamp_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let mut result = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        result = result.trim_end().to_string();
    }
    result
}

fn event_thread_id(event: &TurnStreamEvent) -> Option<&str> {
    match event {
        TurnStreamEvent::ThreadStarted { thread } => Some(thread.id.as_str()),
        TurnStreamEvent::AgentLabelUpdated { thread_id, .. }
        | TurnStreamEvent::ThreadStatusChanged { thread_id, .. }
        | TurnStreamEvent::ThreadClosed { thread_id }
        | TurnStreamEvent::TurnStarted { thread_id, .. }
        | TurnStreamEvent::TurnCompleted { thread_id, .. }
        | TurnStreamEvent::ItemStarted { thread_id, .. }
        | TurnStreamEvent::ItemCompleted { thread_id, .. }
        | TurnStreamEvent::AgentMessageDelta { thread_id, .. }
        | TurnStreamEvent::ReasoningSummaryPartAdded { thread_id, .. }
        | TurnStreamEvent::ReasoningSummaryTextDelta { thread_id, .. }
        | TurnStreamEvent::ReasoningTextDelta { thread_id, .. }
        | TurnStreamEvent::CommandExecutionOutputDelta { thread_id, .. }
        | TurnStreamEvent::FileChangeOutputDelta { thread_id, .. }
        | TurnStreamEvent::TokenUsageUpdated { thread_id, .. }
        | TurnStreamEvent::ThreadNameUpdated { thread_id, .. } => Some(thread_id.as_str()),
        TurnStreamEvent::ApprovalRequested(request) => request.thread_id(),
        TurnStreamEvent::DynamicToolCallRequested(request) => Some(request.thread_id()),
        TurnStreamEvent::AccountRateLimitsUpdated { .. }
        | TurnStreamEvent::ProtocolError { .. } => None,
    }
}
