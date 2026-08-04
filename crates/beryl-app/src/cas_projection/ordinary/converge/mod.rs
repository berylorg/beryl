//! Durable completion of one proven-terminal ordinary turn's derived history.

mod command;
mod gate;
mod item;
mod snapshot;
mod transcript;

use beryl_home_store::HomeStore;
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{SyndicPointReadLimit, SyndicStorage, SyndicTimestamp};

use super::OrdinaryTurnExecutionError;
use crate::cas_projection::stop::StopCoordinator;

pub(in crate::cas_projection) fn converge_terminal_history(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
    live_stop_coordinator: Option<&StopCoordinator>,
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
    if let Some(coordinator) = live_stop_coordinator {
        coordinator
            .wait_for_finalization_release(thread_id, turn_id)
            .map_err(|source| OrdinaryTurnExecutionError::BrokerLoss {
                source: Box::new(source),
            })?;
    }
    gate::complete(store, storage, thread_id, turn_id, limit)?;
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_terminal_history(
        thread_id,
        crate::cas_projection::test_faults::TerminalHistoryBarrierStage::AfterGateRelease,
    );
    Ok(())
}
