use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use beryl_backend::{ManagedBackendClientConnector, ThreadReadMetadata};
use tracing::warn;

const MAX_RESOLUTION_BATCH: usize = 8;
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub(super) struct ToolActivityNicknameResolver {
    receiver: Option<Receiver<ToolActivityNicknameUpdate>>,
    in_flight: HashSet<String>,
    retry_by_thread: HashMap<String, NicknameRetryState>,
}

#[derive(Debug)]
pub(super) enum ToolActivityNicknamePoll {
    Idle,
    Pending,
    Finished(Vec<ToolActivityNicknameOutcome>),
}

#[derive(Debug)]
pub(super) enum ToolActivityNicknameOutcome {
    Resolved { metadata: ThreadReadMetadata },
    Unresolved { thread_id: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ToolActivityNicknameResolutionTarget {
    pub(super) thread_id: String,
    pub(super) requires_nickname: bool,
}

#[derive(Debug)]
enum ToolActivityNicknameUpdate {
    Finished(Vec<ToolActivityNicknameOutcome>),
}

#[derive(Clone, Copy, Debug)]
struct NicknameRetryState {
    attempts: u32,
    retry_at: Instant,
}

impl ToolActivityNicknameResolver {
    pub(super) fn reset(&mut self) {
        self.receiver = None;
        self.in_flight.clear();
        self.retry_by_thread.clear();
    }

    pub(super) fn has_active_worker(&self) -> bool {
        self.receiver.is_some()
    }

    pub(super) fn has_retry_work(&self) -> bool {
        !self.retry_by_thread.is_empty()
    }

    pub(super) fn begin_if_needed(
        &mut self,
        resolution_targets: Vec<ToolActivityNicknameResolutionTarget>,
        connector: ManagedBackendClientConnector,
        timeout: Duration,
    ) -> bool {
        if self.receiver.is_some() {
            return false;
        }

        let now = Instant::now();
        let batch = self.eligible_resolution_batch(resolution_targets, now);
        if batch.is_empty() {
            return false;
        }
        self.in_flight
            .extend(batch.iter().map(|target| target.thread_id.clone()));

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let outcomes = resolve_tool_activity_nicknames(connector, batch, timeout);
            let _ = sender.send(ToolActivityNicknameUpdate::Finished(outcomes));
        });
        self.receiver = Some(receiver);
        true
    }

    fn eligible_resolution_batch(
        &self,
        resolution_targets: Vec<ToolActivityNicknameResolutionTarget>,
        now: Instant,
    ) -> Vec<ToolActivityNicknameResolutionTarget> {
        let mut selected = HashSet::new();
        let mut batch = Vec::new();
        for target in resolution_targets {
            let thread_id = target.thread_id.trim();
            if thread_id.is_empty()
                || self.in_flight.contains(thread_id)
                || selected.contains(thread_id)
            {
                continue;
            }
            if self
                .retry_by_thread
                .get(thread_id)
                .is_some_and(|retry| retry.retry_at > now)
            {
                continue;
            }
            selected.insert(thread_id.to_string());
            batch.push(ToolActivityNicknameResolutionTarget {
                thread_id: thread_id.to_string(),
                requires_nickname: target.requires_nickname,
            });
            if batch.len() >= MAX_RESOLUTION_BATCH {
                break;
            }
        }
        batch
    }

    pub(super) fn poll(&mut self) -> ToolActivityNicknamePoll {
        let Some(receiver) = self.receiver.as_ref() else {
            return ToolActivityNicknamePoll::Idle;
        };

        match receiver.try_recv() {
            Ok(ToolActivityNicknameUpdate::Finished(outcomes)) => {
                self.receiver = None;
                self.finish_outcomes(&outcomes);
                ToolActivityNicknamePoll::Finished(outcomes)
            }
            Err(TryRecvError::Empty) => ToolActivityNicknamePoll::Pending,
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                let outcomes = self
                    .in_flight
                    .drain()
                    .map(|thread_id| ToolActivityNicknameOutcome::Unresolved {
                        thread_id,
                        message: "The nickname resolver worker stopped before returning a result."
                            .to_string(),
                    })
                    .collect::<Vec<_>>();
                self.finish_outcomes(&outcomes);
                ToolActivityNicknamePoll::Finished(outcomes)
            }
        }
    }

    fn finish_outcomes(&mut self, outcomes: &[ToolActivityNicknameOutcome]) {
        let now = Instant::now();
        for outcome in outcomes {
            match outcome {
                ToolActivityNicknameOutcome::Resolved { metadata } => {
                    self.in_flight.remove(&metadata.thread.id);
                    self.retry_by_thread.remove(&metadata.thread.id);
                }
                ToolActivityNicknameOutcome::Unresolved { thread_id, .. } => {
                    self.in_flight.remove(thread_id);
                    let attempts = self
                        .retry_by_thread
                        .get(thread_id)
                        .map_or(1, |retry| retry.attempts.saturating_add(1));
                    self.retry_by_thread.insert(
                        thread_id.clone(),
                        NicknameRetryState {
                            attempts,
                            retry_at: now + retry_delay(attempts),
                        },
                    );
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn eligible_resolution_batch_for_test(
        &self,
        resolution_targets: Vec<ToolActivityNicknameResolutionTarget>,
        now: Instant,
    ) -> Vec<ToolActivityNicknameResolutionTarget> {
        self.eligible_resolution_batch(resolution_targets, now)
    }

    #[cfg(test)]
    pub(super) fn mark_in_flight_for_test(&mut self, thread_id: impl Into<String>) {
        self.in_flight.insert(thread_id.into());
    }

    #[cfg(test)]
    pub(super) fn mark_retry_for_test(
        &mut self,
        thread_id: impl Into<String>,
        retry_at: Instant,
        attempts: u32,
    ) {
        self.retry_by_thread
            .insert(thread_id.into(), NicknameRetryState { attempts, retry_at });
    }
}

fn resolve_tool_activity_nicknames(
    connector: ManagedBackendClientConnector,
    resolution_targets: Vec<ToolActivityNicknameResolutionTarget>,
    timeout: Duration,
) -> Vec<ToolActivityNicknameOutcome> {
    let mut session = match connector.connect_request_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            let message = format!("Beryl could not connect to the managed backend: {error}");
            return resolution_targets
                .into_iter()
                .map(|target| ToolActivityNicknameOutcome::Unresolved {
                    thread_id: target.thread_id,
                    message: message.clone(),
                })
                .collect();
        }
    };

    resolution_targets
        .into_iter()
        .map(
            |target| match session.read_thread_metadata_details(&target.thread_id, timeout) {
                Ok(metadata)
                    if !target.requires_nickname
                        || metadata
                            .thread
                            .agent_nickname
                            .as_deref()
                            .is_some_and(|label| !label.trim().is_empty()) =>
                {
                    ToolActivityNicknameOutcome::Resolved { metadata }
                }
                Ok(_) => ToolActivityNicknameOutcome::Unresolved {
                    thread_id: target.thread_id,
                    message: "Backend thread metadata did not include a subagent nickname."
                        .to_string(),
                },
                Err(error) => {
                    warn!(
                        thread_id = target.thread_id.as_str(),
                        error = %error,
                        "failed to resolve tool-activity subagent nickname"
                    );
                    ToolActivityNicknameOutcome::Unresolved {
                        thread_id: target.thread_id,
                        message: error.to_string(),
                    }
                }
            },
        )
        .collect()
}

fn retry_delay(attempts: u32) -> Duration {
    INITIAL_RETRY_DELAY
        .saturating_mul(1_u32 << attempts.saturating_sub(1).min(4))
        .min(MAX_RETRY_DELAY)
}
