use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use beryl_backend::{
    ManagedBackendClientConnector, ThreadItem, ThreadStartOptions, TurnStatus, TurnStreamEvent,
};
use beryl_model::workspace::WorkspaceId;
use tracing::warn;

use super::{
    TITLE_DEVELOPER_INSTRUCTIONS, TITLE_GENERATION_STREAM_POLL_INTERVAL, TITLE_GENERATION_TIMEOUT,
    ThreadTitleBackend, ThreadTitleCancellation, ThreadTitleCandidate, ThreadTitleResult,
    ThreadTitleUpdate, build_title_prompt, cleanup::cleanup_maintenance_thread, event_thread_id,
    generated::GeneratedTitleText, title_generation_turn_options,
};

struct ThreadTitleAttempt {
    candidate: ThreadTitleCandidate,
    cancellation: ThreadTitleCancellation,
    maintenance_thread_id: String,
    maintenance_turn_id: String,
    started_at: Instant,
    generation_timeout: Duration,
    generated_text: GeneratedTitleText,
    terminal_status: Option<TurnStatus>,
    cleanup_requested: bool,
}

enum ThreadTitleStartError {
    Cancelled,
    Failed(String),
}

pub(crate) fn spawn_thread_title_worker(
    connector: ManagedBackendClientConnector,
    execution_target: WorkspaceId,
    candidate: ThreadTitleCandidate,
    cancellation: ThreadTitleCancellation,
    timeout: Duration,
) -> Receiver<ThreadTitleUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let thread_id = candidate.target_thread_id().to_string();
        let result = run_thread_title_worker(
            connector,
            execution_target,
            candidate,
            cancellation,
            timeout,
        );
        let _ = sender.send(ThreadTitleUpdate::Finished { thread_id, result });
    });
    receiver
}

fn run_thread_title_worker(
    connector: ManagedBackendClientConnector,
    execution_target: WorkspaceId,
    candidate: ThreadTitleCandidate,
    cancellation: ThreadTitleCancellation,
    timeout: Duration,
) -> ThreadTitleResult {
    if cancellation.is_cancelled() {
        return ThreadTitleResult::Cancelled;
    }

    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            return ThreadTitleResult::Failed {
                message: format!("Beryl could not connect to the managed backend: {error}"),
            };
        }
    };

    run_thread_title_attempt_with_cancellation(
        &mut session,
        &execution_target,
        candidate,
        cancellation,
        timeout,
        TITLE_GENERATION_TIMEOUT,
    )
}

#[allow(dead_code)]
pub(crate) fn run_thread_title_attempt<B>(
    backend: &mut B,
    execution_target: &WorkspaceId,
    candidate: ThreadTitleCandidate,
    request_timeout: Duration,
    generation_timeout: Duration,
) -> ThreadTitleResult
where
    B: ThreadTitleBackend,
{
    run_thread_title_attempt_with_cancellation(
        backend,
        execution_target,
        candidate,
        ThreadTitleCancellation::new(),
        request_timeout,
        generation_timeout,
    )
}

pub(crate) fn run_thread_title_attempt_with_cancellation<B>(
    backend: &mut B,
    execution_target: &WorkspaceId,
    candidate: ThreadTitleCandidate,
    cancellation: ThreadTitleCancellation,
    request_timeout: Duration,
    generation_timeout: Duration,
) -> ThreadTitleResult
where
    B: ThreadTitleBackend,
{
    if cancellation.is_cancelled() {
        return ThreadTitleResult::Cancelled;
    }

    match ThreadTitleAttempt::start(
        backend,
        execution_target,
        candidate,
        cancellation,
        request_timeout,
        generation_timeout,
    ) {
        Ok(mut attempt) => attempt.run(backend, request_timeout),
        Err(ThreadTitleStartError::Cancelled) => ThreadTitleResult::Cancelled,
        Err(ThreadTitleStartError::Failed(message)) => ThreadTitleResult::Failed { message },
    }
}

impl ThreadTitleAttempt {
    fn start<B>(
        backend: &mut B,
        execution_target: &WorkspaceId,
        candidate: ThreadTitleCandidate,
        cancellation: ThreadTitleCancellation,
        request_timeout: Duration,
        generation_timeout: Duration,
    ) -> Result<Self, ThreadTitleStartError>
    where
        B: ThreadTitleBackend,
    {
        let response = match backend.start_thread_with_options(
            execution_target.canonical_path(),
            ThreadStartOptions::ephemeral()
                .with_developer_instructions(TITLE_DEVELOPER_INSTRUCTIONS),
            request_timeout,
        ) {
            Ok(response) => response,
            Err(error) => {
                warn!(
                    thread_id = candidate.target_thread_id(),
                    error = %error,
                    "failed to start ephemeral thread-title generation thread"
                );
                return Err(ThreadTitleStartError::Failed(format!(
                    "Beryl could not start the title-generation maintenance thread: {error}"
                )));
            }
        };

        let summary = response.thread.summary();
        if !summary.ephemeral {
            warn!(
                thread_id = candidate.target_thread_id(),
                maintenance_thread_id = summary.id.as_str(),
                "app-server did not create an ephemeral thread for title generation"
            );
            cleanup_maintenance_thread(backend, &summary.id, request_timeout);
            return Err(ThreadTitleStartError::Failed(
                "Beryl could not generate a title because the backend did not create an ephemeral maintenance thread."
                    .to_string(),
            ));
        }

        let maintenance_thread_id = summary.id;
        if cancellation.is_cancelled() {
            cleanup_maintenance_thread(backend, &maintenance_thread_id, request_timeout);
            return Err(ThreadTitleStartError::Cancelled);
        }

        let prompt = build_title_prompt(candidate.user_input());
        let turn = match backend.start_turn_with_options(
            &maintenance_thread_id,
            &prompt,
            title_generation_turn_options(),
            request_timeout,
        ) {
            Ok(response) => response.turn,
            Err(error) => {
                warn!(
                    thread_id = candidate.target_thread_id(),
                    maintenance_thread_id = maintenance_thread_id.as_str(),
                    error = %error,
                    "failed to start thread-title generation turn"
                );
                cleanup_maintenance_thread(backend, &maintenance_thread_id, request_timeout);
                return Err(ThreadTitleStartError::Failed(format!(
                    "Beryl could not start the title-generation turn: {error}"
                )));
            }
        };
        let maintenance_turn_id = turn.id.clone();
        let turn_status = turn.status;
        let turn_terminal = turn.is_terminal();

        let mut attempt = Self {
            candidate,
            cancellation,
            maintenance_thread_id,
            maintenance_turn_id,
            started_at: Instant::now(),
            generation_timeout,
            generated_text: GeneratedTitleText::default(),
            terminal_status: None,
            cleanup_requested: false,
        };
        attempt.observe_turn_items(turn.items);
        if turn_terminal {
            return Ok(attempt.with_terminal_status(turn_status));
        }
        Ok(attempt)
    }

    fn with_terminal_status(mut self, status: TurnStatus) -> Self {
        self.observe_terminal_status(status);
        self
    }

    fn run<B>(&mut self, backend: &mut B, request_timeout: Duration) -> ThreadTitleResult
    where
        B: ThreadTitleBackend,
    {
        loop {
            if self.cancelled() {
                self.cleanup(backend, request_timeout);
                return ThreadTitleResult::Cancelled;
            }

            if self.timed_out() {
                warn!(
                    thread_id = self.candidate.target_thread_id(),
                    maintenance_thread_id = self.maintenance_thread_id.as_str(),
                    "thread-title generation timed out"
                );
                self.cleanup(backend, request_timeout);
                return ThreadTitleResult::Failed {
                    message: "Title generation timed out.".to_string(),
                };
            }

            if let Some(status) = self.take_terminal_status() {
                return self.finish_title_turn(backend, status, request_timeout);
            }

            let event_timeout = self
                .remaining_timeout()
                .map(|remaining| remaining.min(TITLE_GENERATION_STREAM_POLL_INTERVAL))
                .unwrap_or(TITLE_GENERATION_STREAM_POLL_INTERVAL);
            let event = match backend.next_turn_stream_event(event_timeout) {
                Ok(Some(TurnStreamEvent::ProtocolError { error })) => {
                    self.cleanup(backend, request_timeout);
                    return ThreadTitleResult::Failed {
                        message: format!(
                            "Beryl received a protocol error while generating a title: {}",
                            error.message
                        ),
                    };
                }
                Ok(Some(event)) => event,
                Ok(None) => continue,
                Err(error) => {
                    self.cleanup(backend, request_timeout);
                    return ThreadTitleResult::Failed {
                        message: format!("Beryl lost the title-generation stream: {error}"),
                    };
                }
            };

            if self.event_belongs_to_maintenance_thread(&event) {
                self.observe_maintenance_event(event);
            }
        }
    }

    fn remaining_timeout(&self) -> Option<Duration> {
        self.generation_timeout
            .checked_sub(self.started_at.elapsed())
    }

    fn timed_out(&self) -> bool {
        self.remaining_timeout().is_none()
    }

    fn event_belongs_to_maintenance_thread(&self, event: &TurnStreamEvent) -> bool {
        event_thread_id(event) == Some(self.maintenance_thread_id.as_str())
    }

    fn observe_maintenance_event(&mut self, event: TurnStreamEvent) {
        match event {
            TurnStreamEvent::TurnStarted { turn, .. } => {
                if turn.id == self.maintenance_turn_id {
                    let turn_status = turn.status;
                    let turn_terminal = turn.is_terminal();
                    self.observe_turn_items(turn.items);
                    if turn_terminal {
                        self.observe_terminal_status(turn_status);
                    }
                }
            }
            TurnStreamEvent::TurnCompleted { turn, .. } => {
                if turn.id == self.maintenance_turn_id {
                    self.observe_turn_items(turn.items);
                    self.observe_terminal_status(turn.status);
                }
            }
            TurnStreamEvent::ItemStarted { turn_id, item, .. }
            | TurnStreamEvent::ItemCompleted { turn_id, item, .. } => {
                if turn_id == self.maintenance_turn_id {
                    self.generated_text.observe_thread_item(item);
                }
            }
            TurnStreamEvent::AgentMessageDelta {
                turn_id,
                item_id,
                delta,
                ..
            } => {
                if turn_id == self.maintenance_turn_id {
                    self.generated_text
                        .observe_agent_message_delta(item_id, delta);
                }
            }
            TurnStreamEvent::ThreadClosed { .. }
            | TurnStreamEvent::AgentLabelUpdated { .. }
            | TurnStreamEvent::ThreadStarted { .. }
            | TurnStreamEvent::ThreadStatusChanged { .. }
            | TurnStreamEvent::TokenUsageUpdated { .. }
            | TurnStreamEvent::ReasoningSummaryPartAdded { .. }
            | TurnStreamEvent::ReasoningSummaryTextDelta { .. }
            | TurnStreamEvent::ReasoningTextDelta { .. }
            | TurnStreamEvent::CommandExecutionOutputDelta { .. }
            | TurnStreamEvent::FileChangeOutputDelta { .. }
            | TurnStreamEvent::ThreadNameUpdated { .. }
            | TurnStreamEvent::AccountRateLimitsUpdated { .. }
            | TurnStreamEvent::ApprovalRequested(_)
            | TurnStreamEvent::DynamicToolCallRequested(_)
            | TurnStreamEvent::ProtocolError { .. } => {}
        }
    }

    fn observe_terminal_status(&mut self, status: TurnStatus) {
        self.terminal_status = Some(status);
    }

    fn take_terminal_status(&mut self) -> Option<TurnStatus> {
        self.terminal_status.take()
    }

    fn observe_turn_items(&mut self, items: Vec<ThreadItem>) {
        self.generated_text.observe_turn_items(items);
    }

    fn finish_title_turn<B>(
        &mut self,
        backend: &mut B,
        status: TurnStatus,
        request_timeout: Duration,
    ) -> ThreadTitleResult
    where
        B: ThreadTitleBackend,
    {
        if status != TurnStatus::Completed {
            warn!(
                thread_id = self.candidate.target_thread_id(),
                maintenance_thread_id = self.maintenance_thread_id.as_str(),
                ?status,
                "thread-title generation turn did not complete successfully"
            );
            self.cleanup(backend, request_timeout);
            return ThreadTitleResult::Failed {
                message: "The title-generation turn did not complete successfully.".to_string(),
            };
        }

        let Some(title) = self.generated_title() else {
            warn!(
                thread_id = self.candidate.target_thread_id(),
                maintenance_thread_id = self.maintenance_thread_id.as_str(),
                "thread-title generation produced no acceptable title"
            );
            self.cleanup(backend, request_timeout);
            return ThreadTitleResult::Failed {
                message: "Title generation produced no acceptable title.".to_string(),
            };
        };

        if self.cancelled() {
            self.cleanup(backend, request_timeout);
            return ThreadTitleResult::Cancelled;
        }

        match backend.set_thread_name(self.candidate.target_thread_id(), &title, request_timeout) {
            Ok(()) => {
                self.cleanup(backend, request_timeout);
                ThreadTitleResult::Applied { title }
            }
            Err(error) => {
                warn!(
                    thread_id = self.candidate.target_thread_id(),
                    maintenance_thread_id = self.maintenance_thread_id.as_str(),
                    error = %error,
                    "failed to publish generated backend thread title"
                );
                self.cleanup(backend, request_timeout);
                ThreadTitleResult::Failed {
                    message: format!("Beryl could not publish the generated thread title: {error}"),
                }
            }
        }
    }

    fn generated_title(&self) -> Option<String> {
        self.generated_text.generated_title()
    }

    fn cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    fn cleanup<B>(&mut self, backend: &mut B, timeout: Duration)
    where
        B: ThreadTitleBackend,
    {
        if self.cleanup_requested {
            return;
        }

        self.cleanup_requested = true;
        cleanup_maintenance_thread(backend, &self.maintenance_thread_id, timeout);
    }
}
