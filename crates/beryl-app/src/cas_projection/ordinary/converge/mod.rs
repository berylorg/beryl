//! Durable completion of one proven-terminal ordinary turn's derived history.

mod access;
mod command;
mod gate;
mod item;
mod snapshot;
mod transcript;
pub(crate) use gate::TerminalHistoryCompletion;

use access::HistoryAccess;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeStore};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{SyndicPointReadLimit, SyndicStorage, SyndicTimestamp};

use super::OrdinaryTurnExecutionError;

pub(in crate::cas_projection) fn converge_terminal_history(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
    completion: Option<&crate::cas_projection::service::TerminalCompletionPublisher>,
) -> Result<(), OrdinaryTurnExecutionError> {
    converge_terminal_history_with_access(
        HistoryAccess::Ordinary(store),
        storage,
        thread_id,
        turn_id,
        minimum_observed_at,
        limit,
        completion,
    )
}

pub(in crate::cas_projection) fn converge_terminal_history_candidate(
    store: &HomeCandidateRecoveryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    converge_terminal_history_with_access(
        HistoryAccess::Candidate(store),
        storage,
        thread_id,
        turn_id,
        minimum_observed_at,
        limit,
        None,
    )
}

fn converge_terminal_history_with_access(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
    completion: Option<&crate::cas_projection::service::TerminalCompletionPublisher>,
) -> Result<(), OrdinaryTurnExecutionError> {
    item::converge_turn_items(
        store,
        storage,
        thread_id,
        turn_id,
        minimum_observed_at,
        limit,
    )?;
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_terminal_history(
        thread_id,
        crate::cas_projection::test_faults::TerminalHistoryBarrierStage::AfterItems,
    );
    transcript::converge_selected_transcript(store, storage, thread_id, limit)?;
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_terminal_history(
        thread_id,
        crate::cas_projection::test_faults::TerminalHistoryBarrierStage::BeforeGateRelease,
    );
    gate::complete(store, storage, thread_id, turn_id, limit, completion)?;
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_terminal_history(
        thread_id,
        crate::cas_projection::test_faults::TerminalHistoryBarrierStage::AfterGateRelease,
    );
    Ok(())
}
