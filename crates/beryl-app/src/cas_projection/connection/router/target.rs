use beryl_model::CasThreadId;

use super::{
    LiveEventTargetCloseReason, ProvenTerminalOutcome, RouterState, TargetEntry,
    TargetInvalidation, TargetTerminalSignal, state::advance_revision,
};

pub(super) fn invalidation(
    target: &TargetEntry,
    reason: LiveEventTargetCloseReason,
) -> TargetInvalidation {
    TargetInvalidation {
        key: target.key.clone(),
        owner: target.owner,
        loaded_generation: target.loaded_generation,
        reason,
    }
}

pub(super) fn close_target(
    state: &mut RouterState,
    thread_id: &CasThreadId,
    reason: LiveEventTargetCloseReason,
) -> bool {
    if state
        .targets
        .get(thread_id)
        .is_some_and(|target| !target.dynamic_tool_responses.is_empty())
    {
        retire_router_state(state, reason);
        return true;
    }
    let steering_attempt_in_flight = state
        .active_steering_attempt
        .as_ref()
        .is_some_and(|attempt| &attempt.thread_id == thread_id);
    let stop_election_in_flight = state
        .active_stop_election
        .as_ref()
        .is_some_and(|election| &election.thread_id == thread_id);
    if let Some(target) = state.targets.get_mut(thread_id)
        && (target.publication_in_flight.is_some()
            || steering_attempt_in_flight
            || stop_election_in_flight)
    {
        target.publication_closing.get_or_insert(reason);
        // The exact publication or steering owner still fences externally visible closure.
        // Its finish or loss-transfer path publishes the terminal signal after resolving the
        // in-flight authority.
        advance_revision(state);
        return false;
    }
    let retain_for_loss = state.targets.get(thread_id).is_some_and(|target| {
        target.pending_activation.is_some()
            && target.turn_state != super::TargetTurn::Terminal
            && reason != LiveEventTargetCloseReason::ReceiverAbandoned
    });
    if retain_for_loss {
        let target = state
            .targets
            .get_mut(thread_id)
            .expect("retained loss target remains registered");
        target.publication_closing.get_or_insert(reason);
        // A disconnected receiver observes `Open` as an unexpected worker stop, so publish the
        // exact close cause before dropping the last sender.
        set_terminal(&target.terminal, reason);
        target.sender.take();
        let connection_retired = fence_thread_lane(state, thread_id);
        if !connection_retired {
            advance_revision(state);
        }
        return connection_retired;
    }
    if let Some(target) = state.targets.remove(thread_id) {
        set_terminal(&target.terminal, reason);
        if fence_thread_lane(state, thread_id) {
            return true;
        }
        advance_revision(state);
    }
    false
}

pub(super) fn fence_thread_lane(state: &mut RouterState, thread_id: &CasThreadId) -> bool {
    if !state.retired_thread_lanes.contains(thread_id)
        && state.retired_thread_lanes.len() >= super::RETIRED_THREAD_LANE_LIMIT
    {
        retire_router_state(state, LiveEventTargetCloseReason::RetiredThreadLaneCapacity);
        return true;
    }
    state.retired_thread_lanes.insert(thread_id.clone());
    false
}

pub(super) fn retire_router_state(state: &mut RouterState, reason: LiveEventTargetCloseReason) {
    if state.retired.is_some() {
        return;
    }
    state.retired = Some(reason);
    let active_steering_thread = state
        .active_steering_attempt
        .as_ref()
        .map(|attempt| attempt.thread_id.clone());
    let active_stop_thread = state
        .active_stop_election
        .as_ref()
        .map(|election| election.thread_id.clone());
    for (thread_id, target) in &mut state.targets {
        let exact_owner_in_flight = target.publication_in_flight.is_some()
            || active_steering_thread.as_ref() == Some(thread_id)
            || active_stop_thread.as_ref() == Some(thread_id);
        if exact_owner_in_flight {
            target.publication_closing.get_or_insert(reason);
        } else {
            set_terminal(&target.terminal, reason);
            target.sender.take();
        }
        target.dynamic_tool_responses.clear();
    }
    state.targets.retain(|thread_id, target| {
        target.publication_in_flight.is_some()
            || active_steering_thread.as_ref() == Some(thread_id)
            || active_stop_thread.as_ref() == Some(thread_id)
            || (target.pending_activation.is_some()
                && target.turn_state != super::TargetTurn::Terminal)
    });
    advance_revision(state);
}

pub(super) fn set_proven_terminal(
    terminal: &std::sync::Mutex<TargetTerminalSignal>,
    outcome: ProvenTerminalOutcome,
) -> bool {
    let mut terminal = terminal
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *terminal != TargetTerminalSignal::Open {
        return false;
    }
    *terminal = TargetTerminalSignal::Proven(outcome);
    true
}

pub(super) fn set_terminal(
    terminal: &std::sync::Mutex<TargetTerminalSignal>,
    reason: LiveEventTargetCloseReason,
) {
    match terminal.lock() {
        Ok(mut terminal) => *terminal = TargetTerminalSignal::Closed(reason),
        Err(poisoned) => *poisoned.into_inner() = TargetTerminalSignal::Closed(reason),
    }
}
