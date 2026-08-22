use super::*;

pub(super) fn lifecycle_end_status(lifecycle: TurnLifecycle) -> Option<TurnEndStatus> {
    let outcome = match lifecycle {
        TurnLifecycle::Pending | TurnLifecycle::Active => return None,
        TurnLifecycle::Complete => TurnTerminalOutcome::Complete,
        TurnLifecycle::Interrupted => TurnTerminalOutcome::Interrupted,
        TurnLifecycle::Failed => TurnTerminalOutcome::Failed,
        TurnLifecycle::Incomplete => TurnTerminalOutcome::Incomplete,
        TurnLifecycle::UnknownTerminal => TurnTerminalOutcome::UnknownTerminal,
    };
    Some(turn_end_status(outcome))
}
