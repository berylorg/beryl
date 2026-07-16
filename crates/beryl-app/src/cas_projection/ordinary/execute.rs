use beryl_home_store::HomeStore;
use syndic_storage::SyndicStorage;

use super::{
    OrdinaryDynamicToolHandler, OrdinaryTurnExecutionError, OrdinaryTurnExecutionOutcome,
    OrdinaryTurnExecutionRequest,
};
use crate::cas_projection::{CasProjectionCoordinator, LoadedCasProjection};

mod capture_loop;
mod cleanup;
mod identity;
mod start;

impl CasProjectionCoordinator {
    /// Executes one exact pending ordinary turn on a non-GPUI worker until terminal or loss.
    pub fn execute_ordinary_turn(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        projection: LoadedCasProjection,
        request: &OrdinaryTurnExecutionRequest,
        tools: &mut impl OrdinaryDynamicToolHandler,
    ) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
        start::execute(self, store, storage, projection, request, tools)
    }
}
