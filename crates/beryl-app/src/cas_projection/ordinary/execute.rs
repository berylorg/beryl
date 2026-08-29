use beryl_home_store::HomeStore;
use beryl_state::AssetState;
use syndic_storage::SyndicStorage;

use super::{
    OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome,
    OrdinaryTurnExecutionRequest,
};
use crate::cas_projection::{
    CasProjectionCoordinator, LoadedCasProjection, ProjectionCancellationToken,
    service::ProjectionFlight,
};

mod capture_loop;
mod identity;
mod start;

impl CasProjectionCoordinator {
    /// Executes one exact pending ordinary turn on a non-GPUI worker until terminal or loss.
    ///
    /// Submitted text and image markers are replayed from their exact sealed Syndic and asset
    /// authorities without an app-owned complete descriptor sequence.
    #[allow(
        clippy::too_many_arguments,
        reason = "the public execution boundary keeps each durable authority and request-local handler explicit"
    )]
    pub fn execute_ordinary_turn(
        &self,
        store: &HomeStore,
        storage: &SyndicStorage,
        assets: &AssetState,
        projection: LoadedCasProjection,
        cancellation: &ProjectionCancellationToken,
        request: &OrdinaryTurnExecutionRequest,
        mut tools: OrdinaryDynamicToolHandlers<'_>,
    ) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure> {
        start::execute(
            self,
            store,
            storage,
            assets,
            projection,
            cancellation,
            request,
            &mut tools,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the scheduled execution boundary keeps each durable authority and request-local handler explicit"
    )]
    pub(in crate::cas_projection) fn execute_ordinary_turn_in_flight(
        &self,
        store: &HomeStore,
        storage: &SyndicStorage,
        assets: &AssetState,
        projection: LoadedCasProjection,
        cancellation: &ProjectionCancellationToken,
        request: &OrdinaryTurnExecutionRequest,
        mut tools: OrdinaryDynamicToolHandlers<'_>,
        flight: &ProjectionFlight,
    ) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure> {
        start::execute_in_flight(
            self,
            store,
            storage,
            assets,
            projection,
            cancellation,
            request,
            &mut tools,
            flight,
        )
    }
}
