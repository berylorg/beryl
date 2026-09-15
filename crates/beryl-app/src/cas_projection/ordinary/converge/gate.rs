use super::access::HistoryAccess;
use beryl_home_store::HomeGeneration;
use beryl_model::{BerylHomeId, HomeRevision, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CompleteTerminalHistory, InputGateRecord, InputGateState, SyndicPointReadLimit, SyndicStorage,
    TranscriptViewHeadRecord, TurnLifecycle, TurnStateRecord,
};

use super::super::OrdinaryTurnExecutionError;
use super::command;
use crate::cas_projection::service::TerminalCompletionPublisher;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalHistoryCompletion {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    home_revision: HomeRevision,
    lifecycle: TurnLifecycle,
}

impl TerminalHistoryCompletion {
    pub(crate) fn home_id(&self) -> BerylHomeId {
        self.home_id
    }
    pub(crate) fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }
    pub(crate) fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    pub(crate) fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    pub(crate) fn home_revision(&self) -> HomeRevision {
        self.home_revision
    }
    pub(crate) fn lifecycle(&self) -> TurnLifecycle {
        self.lifecycle
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionSnapshot {
    gate: InputGateRecord,
    state: TurnStateRecord,
    transcript: TranscriptViewHeadRecord,
}

pub(super) fn complete(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    limit: SyndicPointReadLimit,
    completion: Option<&TerminalCompletionPublisher>,
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
    let outcome =
        command::dispatch_with_receipt(store, storage.current_complete_terminal_history(request));
    if let Some(completion) = completion {
        let receipt = match &outcome {
            Ok(receipt) | Err(OrdinaryTurnExecutionError::HomeCommandCommitted { receipt, .. }) => {
                Some(receipt)
            }
            _ => None,
        };
        if let Some(receipt) = receipt {
            #[cfg(feature = "test-faults")]
            crate::cas_projection::test_faults::pause_terminal_history(
                thread_id,
                crate::cas_projection::test_faults::TerminalHistoryBarrierStage::AfterGateCommit,
            );
            completion.publish(TerminalHistoryCompletion {
                home_id: store.home_id(),
                home_generation: receipt.generation(),
                thread_id,
                turn_id,
                home_revision: receipt.home_revision(),
                lifecycle: before.state.lifecycle(),
            })?;
        }
    }
    outcome.map(|_| ())
}

fn snapshot(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    limit: SyndicPointReadLimit,
) -> Result<CompletionSnapshot, OrdinaryTurnExecutionError> {
    let gate = store.input_gate(storage, thread_id, limit)?.ok_or(
        OrdinaryTurnExecutionError::Invariant("terminal-history completion gate is missing"),
    )?;
    let state =
        store
            .turn_state(storage, turn_id, limit)?
            .ok_or(OrdinaryTurnExecutionError::Invariant(
                "terminal-history completion turn state is missing",
            ))?;
    let transcript = store
        .transcript_view_head(storage, thread_id, limit)?
        .ok_or(OrdinaryTurnExecutionError::Invariant(
            "terminal-history completion transcript head is missing",
        ))?;
    Ok(CompletionSnapshot {
        gate,
        state,
        transcript,
    })
}
