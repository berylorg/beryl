//! Receipt-aware observer engine with independent bounded diagnostic capture.
//! Success uses fresh metadata idle confirmation because RPCs can buffer old stream idle notifications.

#[path = "compaction_observer/adapter.rs"]
mod adapter;
#[path = "compaction_observer/diagnostics.rs"]
mod diagnostics;
#[path = "compaction_observer/state.rs"]
mod state;
#[path = "compaction_observer/task.rs"]
mod task;
#[path = "compaction_observer/types.rs"]
mod types;
use crate::compaction_diagnostics::{
    CompactionDiagnosticAcceptance as Acceptance, CompactionDiagnosticCategory as Category,
    CompactionDiagnosticHandle, CompactionDiagnosticStage as Stage,
};
pub(crate) use adapter::{classify_start_error, connector_target_matches};
use beryl_backend::{
    CompactionReceiptState, CompactionUnknownReason, ThreadStatus, TurnStatus, TurnStreamEvent,
};
use state::Observation;
use std::time::Duration;
pub(crate) use task::{Cancellation, ObserverTask, observer_channel, spawn_observer};
pub(crate) use types::*;

pub(crate) const UPDATE_CAPACITY: usize = 32;
pub(crate) const RECONCILE_INTERVAL: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const ERROR_BYTES: usize = 4096;

/// Synchronous engine. Injected time and ports allow deterministic long-wait tests.
/// Terminal outcomes end the run exactly once. Cancellation emits no outcome.
pub(crate) fn run_observer(
    backend: &mut impl BackendPort,
    clock: &mut impl Clock,
    sink: &mut impl UpdateSink,
    target: ObserverTarget,
    request_timeout: Duration,
    warning_threshold: Duration,
    cancel: &Cancellation,
    diagnostics: Option<CompactionDiagnosticHandle>,
) {
    let mut state = Observation {
        target,
        operation: None,
        turn_id: None,
        item_completed: false,
        turn_completed: false,
        success_proven: false,
        last_uncertainty: None,
        diagnostics: diagnostics::Capture::new(diagnostics),
    };
    run_inner(
        backend,
        clock,
        sink,
        &mut state,
        request_timeout,
        warning_threshold,
        cancel,
    );
    backend.disconnect();
    state.diagnostics.at(clock.now());
    state.diagnostics.finish(state.turn_id.as_deref());
}

fn run_inner(
    backend: &mut impl BackendPort,
    clock: &mut impl Clock,
    sink: &mut impl UpdateSink,
    state: &mut Observation,
    timeout: Duration,
    warning: Duration,
    cancel: &Cancellation,
) {
    if cancel.is_cancelled() {
        return;
    }
    let setup = (|| {
        state.diagnostics.at(clock.now());
        state
            .diagnostics
            .record(Stage::Transport, Category::Requested, None);
        let session = backend.connect(timeout)?;
        state.diagnostics.at(clock.now());
        state
            .diagnostics
            .record(Stage::Transport, Category::Succeeded, None);
        if cancel.is_cancelled() {
            return Err(PortError::Unavailable);
        }
        let operation = backend.prepare(&state.target.thread_id)?;
        if operation.thread_id != state.target.thread_id
            || operation.observation_session_id != session
        {
            return Err(PortError::Rejected(
                "Compaction target identity did not match the backend.".into(),
            ));
        }
        state.diagnostics.bind_operation(&operation);
        state.operation = Some(operation);
        if cancel.is_cancelled() {
            return Err(PortError::Unavailable);
        }
        state
            .diagnostics
            .record(Stage::Subscription, Category::Requested, None);
        let result = backend.subscribe(&state.target.thread_id, timeout);
        state.diagnostics.at(clock.now());
        state.diagnostics.record(
            Stage::Subscription,
            if result.is_ok() {
                Category::Succeeded
            } else {
                Category::Rejected
            },
            None,
        );
        result
    })();
    state.diagnostics.at(clock.now());
    if let Err(error) = setup {
        state.emit(
            UpdateKind::Finished(Outcome::Rejected {
                message: rejection_message(error),
            }),
            sink,
            cancel,
        );
        return;
    }
    if !state.emit(UpdateKind::Prepared, sink, cancel) {
        return;
    }
    // Retain the operation BEFORE dispatch. Every path below observes only.
    let mut started = clock.now();
    let mut connected = true;
    state
        .diagnostics
        .record(Stage::Start, Category::Requested, None);
    let start_result = backend.start(timeout);
    state.diagnostics.at(clock.now());
    match start_result {
        Ok(receipt) => {
            started = clock.now();
            match state.receipt(receipt, Stage::Start, sink, cancel) {
                Ok(Some(outcome)) => {
                    state.emit(UpdateKind::Finished(outcome), sink, cancel);
                    return;
                }
                Err(()) => return,
                Ok(None) => {}
            }
        }
        Err(PortError::Rejected(message)) => {
            state.emit(
                UpdateKind::Finished(Outcome::Rejected {
                    message: bounded(message),
                }),
                sink,
                cancel,
            );
            return;
        }
        Err(PortError::Unavailable) => {
            state.diagnostics.acceptance(Acceptance::Indeterminate);
            state
                .diagnostics
                .record(Stage::Start, Category::Indeterminate, None);
            backend.disconnect();
            connected = false;
            if !state.uncertain(UnconfirmedReason::Unavailable, sink, cancel) {
                return;
            }
        }
    }
    let warning_at = started.saturating_add(warning);
    let mut next_attempt = warning_at;
    let mut next_connect = clock.now().saturating_add(RECONCILE_INTERVAL);
    let mut warned = false;
    let mut early_idle_read = false;
    let mut runtime_changed = false;
    loop {
        if cancel.is_cancelled() {
            return;
        }
        let now = clock.now();
        state.diagnostics.at(now);
        if !warned && now >= warning_at {
            warned = true;
            if !state.emit(UpdateKind::Warning, sink, cancel) {
                return;
            }
        }
        if !connected && !runtime_changed && now >= next_connect {
            state.diagnostics.record(
                Stage::Transport,
                Category::ReconnectRequested,
                state.turn_id.as_deref(),
            );
            next_connect = now.saturating_add(RECONCILE_INTERVAL);
            let reconnect_result = backend.connect(timeout);
            state.diagnostics.at(clock.now());
            match reconnect_result {
                Ok(session)
                    if Some(session.as_str())
                        == state
                            .operation
                            .as_ref()
                            .map(|op| op.observation_session_id.as_str()) =>
                {
                    if cancel.is_cancelled() {
                        return;
                    }
                    state.diagnostics.record(
                        Stage::Subscription,
                        Category::Requested,
                        state.turn_id.as_deref(),
                    );
                    connected = backend.subscribe(&state.target.thread_id, timeout).is_ok();
                    state.diagnostics.at(clock.now());
                    state.diagnostics.record(
                        Stage::Subscription,
                        if connected {
                            Category::Succeeded
                        } else {
                            Category::Unavailable
                        },
                        state.turn_id.as_deref(),
                    );
                    if connected {
                        state.diagnostics.record(
                            Stage::Transport,
                            Category::Reconnected,
                            state.turn_id.as_deref(),
                        );
                    }
                    if !connected {
                        backend.disconnect();
                    }
                }
                Ok(_) => {
                    runtime_changed = true;
                    backend.disconnect();
                    if !state.uncertain(
                        UnconfirmedReason::Receipt(CompactionUnknownReason::RuntimeChange),
                        sink,
                        cancel,
                    ) {
                        return;
                    }
                }
                Err(_) => {
                    state.diagnostics.record(
                        Stage::Transport,
                        Category::Unavailable,
                        state.turn_id.as_deref(),
                    );
                    backend.disconnect();
                }
            }
            if cancel.is_cancelled() {
                return;
            }
        }
        let scheduled = warned && clock.now() >= next_attempt;
        let early = !warned && state.success_proven && !early_idle_read;
        if connected && (scheduled || early) {
            if scheduled {
                next_attempt = clock.now().saturating_add(RECONCILE_INTERVAL);
                state.diagnostics.at(clock.now());
                state.diagnostics.record(
                    Stage::Receipt,
                    Category::Requested,
                    state.turn_id.as_deref(),
                );
                let read_result = backend.read(state.turn_id.as_deref(), timeout);
                state.diagnostics.at(clock.now());
                match read_result {
                    Ok(receipt) => {
                        let changed_runtime = matches!(
                            receipt.state,
                            CompactionReceiptState::Unknown {
                                reason: CompactionUnknownReason::RuntimeChange
                            }
                        ) && state.operation.as_ref()
                            == Some(&receipt.operation);
                        match state.receipt(receipt, Stage::Receipt, sink, cancel) {
                            Ok(Some(outcome)) => {
                                state.emit(UpdateKind::Finished(outcome), sink, cancel);
                                return;
                            }
                            Err(()) => return,
                            Ok(None) => {}
                        }
                        if changed_runtime {
                            runtime_changed = true;
                            connected = false;
                            state.diagnostics.at(clock.now());
                            state.diagnostics.record(
                                Stage::Transport,
                                Category::Disconnected,
                                state.turn_id.as_deref(),
                            );
                            backend.disconnect();
                        }
                    }
                    Err(_) => {
                        state.diagnostics.record(
                            Stage::Receipt,
                            Category::Unavailable,
                            state.turn_id.as_deref(),
                        );
                        connected = false;
                        state.diagnostics.at(clock.now());
                        state.diagnostics.record(
                            Stage::Transport,
                            Category::Disconnected,
                            state.turn_id.as_deref(),
                        );
                        backend.disconnect();
                        next_connect = clock.now().saturating_add(RECONCILE_INTERVAL);
                        if !state.uncertain(UnconfirmedReason::Unavailable, sink, cancel) {
                            return;
                        }
                    }
                }
            } else {
                early_idle_read = true;
            }
            if cancel.is_cancelled() {
                return;
            }
            if connected {
                state.diagnostics.at(clock.now());
                state.diagnostics.record(
                    Stage::Status,
                    Category::Requested,
                    state.turn_id.as_deref(),
                );
                let status_result = backend.status(&state.target.thread_id, timeout);
                state.diagnostics.at(clock.now());
                match status_result {
                    Ok((thread_id, status)) if thread_id == state.target.thread_id => {
                        state.diagnostics.record(
                            Stage::Status,
                            if status == ThreadStatus::Idle {
                                Category::Idle
                            } else {
                                Category::Active
                            },
                            state.turn_id.as_deref(),
                        );
                        if state.success_proven && status == ThreadStatus::Idle {
                            state.emit(UpdateKind::Finished(Outcome::Succeeded), sink, cancel);
                            return;
                        }
                    }
                    Ok(_) => {
                        state.diagnostics.record(
                            Stage::Status,
                            Category::Invalid,
                            state.turn_id.as_deref(),
                        );
                        if !state.uncertain(UnconfirmedReason::InvalidEvidence, sink, cancel) {
                            return;
                        }
                    }
                    Err(_) => {
                        state.diagnostics.record(
                            Stage::Status,
                            Category::Unavailable,
                            state.turn_id.as_deref(),
                        );
                        connected = false;
                        state.diagnostics.at(clock.now());
                        state.diagnostics.record(
                            Stage::Transport,
                            Category::Disconnected,
                            state.turn_id.as_deref(),
                        );
                        backend.disconnect();
                        next_connect = clock.now().saturating_add(RECONCILE_INTERVAL);
                        if !state.uncertain(UnconfirmedReason::Unavailable, sink, cancel) {
                            return;
                        }
                    }
                }
            }
        }
        if cancel.is_cancelled() {
            return;
        }
        if !connected {
            clock.wait(POLL_INTERVAL, cancel);
            continue;
        }
        let before_poll = clock.now();
        let event = match backend.poll(POLL_INTERVAL) {
            Ok(Some(event)) => event,
            Ok(None) => {
                // Ports may return immediately; do not spin on a quiet stream.
                if clock.now() == before_poll {
                    clock.wait(POLL_INTERVAL, cancel);
                }
                continue;
            }
            Err(_) => {
                connected = false;
                state.diagnostics.at(clock.now());
                state.diagnostics.record(
                    Stage::Transport,
                    Category::Disconnected,
                    state.turn_id.as_deref(),
                );
                backend.disconnect();
                next_connect = clock.now().saturating_add(RECONCILE_INTERVAL);
                if !state.uncertain(UnconfirmedReason::Unavailable, sink, cancel) {
                    return;
                }
                continue;
            }
        };
        state.diagnostics.at(clock.now());
        if cancel.is_cancelled() {
            return;
        }
        if let TurnStreamEvent::ApprovalRequested(request) = &event {
            if backend
                .deny_approval(
                    request,
                    &state.target.thread_id,
                    state.turn_id.as_deref(),
                    timeout,
                )
                .is_err()
            {
                connected = false;
                state.diagnostics.at(clock.now());
                state.diagnostics.record(
                    Stage::Transport,
                    Category::Disconnected,
                    state.turn_id.as_deref(),
                );
                backend.disconnect();
                next_connect = clock.now().saturating_add(RECONCILE_INTERVAL);
                if !state.uncertain(UnconfirmedReason::Unavailable, sink, cancel) {
                    return;
                }
            }
            continue;
        }
        let mut activity = None;
        let mut outcome = None;
        match event {
            TurnStreamEvent::TokenUsageUpdated {
                thread_id,
                turn_id,
                token_usage,
            } if exact_turn(state, &thread_id, &turn_id) => {
                if !state.emit(
                    UpdateKind::TokenUsage {
                        turn_id,
                        usage: token_usage,
                    },
                    sink,
                    cancel,
                ) {
                    return;
                }
            }
            // RPCs queue notifications without receive provenance. Even an idle
            // dequeued after terminal proof may predate that proof or the latest
            // metadata snapshot. Only a fresh metadata read can confirm idle.
            TurnStreamEvent::TurnStarted { thread_id, turn }
                if exact_turn(state, &thread_id, &turn.id) =>
            {
                activity = Some(Activity::Started)
            }
            TurnStreamEvent::ItemStarted {
                thread_id,
                turn_id,
                item,
            } if exact_turn(state, &thread_id, &turn_id)
                && item.item_type() == "contextCompaction" =>
            {
                activity = Some(Activity::CompactionItemStarted)
            }
            TurnStreamEvent::ItemCompleted {
                thread_id,
                turn_id,
                item,
            } if exact_turn(state, &thread_id, &turn_id)
                && item.item_type() == "contextCompaction" =>
            {
                state.item_completed = true;
                if state.turn_completed {
                    state
                        .diagnostics
                        .lifecycle(Category::ItemCompleted, Some(&turn_id));
                }
                activity = Some(Activity::CompactionItemCompleted);
            }
            TurnStreamEvent::TurnCompleted { thread_id, turn }
                if exact_turn(state, &thread_id, &turn.id) =>
            {
                state.diagnostics.lifecycle(
                    match turn.status {
                        TurnStatus::Completed if turn.error.is_none() => Category::Completed,
                        TurnStatus::Failed | TurnStatus::Completed => Category::Failed,
                        TurnStatus::Interrupted => Category::Interrupted,
                        TurnStatus::InProgress => Category::Started,
                    },
                    Some(&turn.id),
                );
                match turn.status {
                    TurnStatus::Completed if turn.error.is_none() => state.turn_completed = true,
                    TurnStatus::Failed | TurnStatus::Completed => {
                        outcome = Some(Outcome::Failed {
                            message: bounded(
                                turn.error
                                    .map(|error| error.message)
                                    .unwrap_or_else(|| "Backend compaction failed.".into()),
                            ),
                        })
                    }
                    TurnStatus::Interrupted => outcome = Some(Outcome::Interrupted),
                    TurnStatus::InProgress => {}
                }
            }
            TurnStreamEvent::TurnError {
                thread_id,
                turn_id,
                will_retry: true,
                ..
            } if exact_turn(state, &thread_id, &turn_id) => activity = Some(Activity::Retrying),
            TurnStreamEvent::ProtocolError { .. } => {
                connected = false;
                state.diagnostics.at(clock.now());
                state.diagnostics.record(
                    Stage::Transport,
                    Category::Disconnected,
                    state.turn_id.as_deref(),
                );
                backend.disconnect();
                next_connect = clock.now().saturating_add(RECONCILE_INTERVAL);
                if !state.uncertain(UnconfirmedReason::Unavailable, sink, cancel) {
                    return;
                }
            }
            _ => {}
        }
        if let Some(outcome) = outcome {
            state.emit(UpdateKind::Finished(outcome), sink, cancel);
            return;
        }
        if !state.success_proven && state.item_completed && state.turn_completed {
            state.success_proven = true;
            activity = Some(Activity::CompletedAwaitingIdle);
        }
        if let Some(activity) = activity
            && !state.emit(UpdateKind::Activity(activity), sink, cancel)
        {
            return;
        }
    }
}

fn exact_turn(state: &Observation, thread_id: &str, turn_id: &str) -> bool {
    thread_id == state.target.thread_id && state.turn_id.as_deref() == Some(turn_id)
}

fn bounded(message: String) -> String {
    let mut end = message.len().min(ERROR_BYTES);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}

fn rejection_message(error: PortError) -> String {
    match error {
        PortError::Rejected(message) => bounded(message),
        PortError::Unavailable => {
            "Beryl could not prepare and subscribe before context compaction.".into()
        }
    }
}
