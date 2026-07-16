//! Durable completion of one proven-terminal ordinary turn's derived history.

mod command;
mod item;
mod snapshot;
mod transcript;

use beryl_home_store::HomeStore;
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{SyndicPointReadLimit, SyndicStorage, SyndicTimestamp};

use super::OrdinaryTurnExecutionError;

pub(super) fn converge_terminal_history(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    item::converge_turn_items(
        store,
        storage,
        thread_id,
        turn_id,
        minimum_observed_at,
        limit,
    )?;
    transcript::converge_selected_transcript(store, storage, thread_id, limit)
}
