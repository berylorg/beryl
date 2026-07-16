use beryl_model::CasThreadId;

use super::{
    LiveEventTargetCloseReason, RouterState, TargetEntry, TargetInvalidation,
    state::advance_revision,
};

pub(super) fn reserve_bytes(counter: &std::sync::atomic::AtomicUsize, amount: usize) -> bool {
    counter
        .fetch_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |current| {
                current
                    .checked_add(amount)
                    .filter(|next| *next <= super::LIVE_EVENT_TARGET_QUEUE_BYTE_LIMIT)
            },
        )
        .is_ok()
}

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
    if let Some(target) = state.targets.remove(thread_id) {
        set_terminal(&target.terminal, reason);
        if !state.retired_thread_lanes.contains(thread_id)
            && state.retired_thread_lanes.len() >= super::RETIRED_THREAD_LANE_LIMIT
        {
            retire_router_state(state, LiveEventTargetCloseReason::RetiredThreadLaneCapacity);
            return true;
        }
        state.retired_thread_lanes.insert(thread_id.clone());
        advance_revision(state);
    }
    false
}

pub(super) fn retire_router_state(state: &mut RouterState, reason: LiveEventTargetCloseReason) {
    if state.retired.is_some() {
        return;
    }
    state.retired = Some(reason);
    for (_, target) in state.targets.drain() {
        set_terminal(&target.terminal, reason);
    }
    advance_revision(state);
}

fn set_terminal(
    terminal: &std::sync::Mutex<Option<LiveEventTargetCloseReason>>,
    reason: LiveEventTargetCloseReason,
) {
    match terminal.lock() {
        Ok(mut terminal) => *terminal = Some(reason),
        Err(poisoned) => *poisoned.into_inner() = Some(reason),
    }
}
