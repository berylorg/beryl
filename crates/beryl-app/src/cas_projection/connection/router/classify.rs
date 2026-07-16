use beryl_backend::{RateLimitSnapshot, TurnStreamEvent};
use beryl_model::{CasItemId, CasThreadId, CasTurnId, DynamicToolCallId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum EventScope {
    Account(RateLimitSnapshot),
    Thread {
        thread_id: CasThreadId,
        closes_target: bool,
    },
    Turn {
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        starts_turn: bool,
        completes_turn: bool,
    },
    ProtocolError,
}

pub(super) fn classify(event: &TurnStreamEvent) -> Result<EventScope, ()> {
    let scope = match event {
        TurnStreamEvent::AccountRateLimitsUpdated { rate_limits } => {
            EventScope::Account(rate_limits.clone())
        }
        TurnStreamEvent::ProtocolError { .. } => EventScope::ProtocolError,
        TurnStreamEvent::ThreadStarted { thread } => thread_scope(&thread.id, false)?,
        TurnStreamEvent::AgentLabelUpdated { thread_id, .. }
        | TurnStreamEvent::ThreadStatusChanged { thread_id, .. }
        | TurnStreamEvent::ThreadNameUpdated { thread_id, .. } => thread_scope(thread_id, false)?,
        TurnStreamEvent::ThreadClosed { thread_id } => thread_scope(thread_id, true)?,
        TurnStreamEvent::TurnStarted {
            thread_id, turn_id, ..
        } => turn_scope(thread_id, turn_id, true, false),
        TurnStreamEvent::TurnCompleted { thread_id, turn } => {
            turn_scope(thread_id, &turn.id, false, true)
        }
        TurnStreamEvent::ItemStarted {
            thread_id,
            turn_id,
            item,
        }
        | TurnStreamEvent::ItemCompleted {
            thread_id,
            turn_id,
            item,
        } => {
            let _ = item.id();
            turn_scope(thread_id, turn_id, false, false)
        }
        TurnStreamEvent::ItemDelta(delta) => {
            let _ = delta.item_id();
            turn_scope(delta.thread_id(), delta.turn_id(), false, false)
        }
        TurnStreamEvent::TokenUsageUpdated {
            thread_id, turn_id, ..
        } => turn_scope_strings(thread_id, turn_id, false, false)?,
        TurnStreamEvent::ApprovalRequested(request) => {
            let thread_id = request.thread_id().ok_or(())?;
            let turn_id = request.turn_id().ok_or(())?;
            if let Some(item_id) = request.item_id() {
                CasItemId::new(item_id).map_err(|_| ())?;
            }
            turn_scope_strings(thread_id, turn_id, false, false)?
        }
        TurnStreamEvent::DynamicToolCallRequested(request) => {
            DynamicToolCallId::new(request.call_id()).map_err(|_| ())?;
            turn_scope_strings(request.thread_id(), request.turn_id(), false, false)?
        }
    };
    Ok(scope)
}

fn thread_scope(thread_id: &str, closes_target: bool) -> Result<EventScope, ()> {
    Ok(EventScope::Thread {
        thread_id: CasThreadId::new(thread_id).map_err(|_| ())?,
        closes_target,
    })
}

fn turn_scope(
    thread_id: &CasThreadId,
    turn_id: &CasTurnId,
    starts_turn: bool,
    completes_turn: bool,
) -> EventScope {
    EventScope::Turn {
        thread_id: thread_id.clone(),
        turn_id: turn_id.clone(),
        starts_turn,
        completes_turn,
    }
}

fn turn_scope_strings(
    thread_id: &str,
    turn_id: &str,
    starts_turn: bool,
    completes_turn: bool,
) -> Result<EventScope, ()> {
    Ok(turn_scope(
        &CasThreadId::new(thread_id).map_err(|_| ())?,
        &CasTurnId::new(turn_id).map_err(|_| ())?,
        starts_turn,
        completes_turn,
    ))
}
