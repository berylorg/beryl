use beryl_model::{CasThreadId, CasTurnId};

/// Constructs one exact ordered thread-close operation for cross-crate lifecycle tests.
#[must_use]
pub fn thread_closed_operation(thread_id: CasThreadId) -> crate::OrderedTurnStreamOperation {
    crate::OrderedTurnStreamOperation::ThreadClosed(crate::ThreadClosed::decoded(thread_id))
}

/// Constructs a status-only normal terminal for cross-crate lifecycle tests.
#[must_use]
pub fn normal_turn_terminal(
    status: crate::NormalTurnTerminalStatus,
    thread_id: CasThreadId,
    turn_id: CasTurnId,
) -> crate::NormalTurnTerminal {
    crate::NormalTurnTerminal::decoded(
        thread_id,
        turn_id,
        status,
        None,
        crate::turn::NormalTurnTerminalDiagnostic::new(),
    )
}
