use beryl_home_store::HomeStore;
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CompleteTerminalHistory, InputGateRecord, InputGateState, SyndicPointReadLimit, SyndicStorage,
    TranscriptViewHeadRecord, TurnStateRecord,
};

use super::super::OrdinaryTurnExecutionError;
use super::command;

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionSnapshot {
    gate: InputGateRecord,
    state: TurnStateRecord,
    transcript: TranscriptViewHeadRecord,
}

pub(super) fn complete(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    let before = snapshot(store, storage, thread_id, turn_id, limit)?;
    if before.gate.state() != &InputGateState::FinalizingHistory(turn_id)
        || !before.state.lifecycle().is_proven_terminal()
        || before.state.turn_id() != turn_id
    {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "terminal-history completion source is not finalizing",
        ));
    }
    let request = CompleteTerminalHistory::new(
        thread_id,
        turn_id,
        before.gate.clone(),
        before.state.revision(),
        before.transcript.generation(),
        before.transcript.revision(),
    );
    command::dispatch(store, storage.current_complete_terminal_history(request))
}

fn snapshot(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    limit: SyndicPointReadLimit,
) -> Result<CompletionSnapshot, OrdinaryTurnExecutionError> {
    let gate = storage.input_gate(store, thread_id, limit)?.ok_or(
        OrdinaryTurnExecutionError::Invariant("terminal-history completion gate is missing"),
    )?;
    let state =
        storage
            .turn_state(store, turn_id, limit)?
            .ok_or(OrdinaryTurnExecutionError::Invariant(
                "terminal-history completion turn state is missing",
            ))?;
    let transcript = storage
        .transcript_view_head(store, thread_id, limit)?
        .ok_or(OrdinaryTurnExecutionError::Invariant(
            "terminal-history completion transcript head is missing",
        ))?;
    Ok(CompletionSnapshot {
        gate,
        state,
        transcript,
    })
}
