use std::{error::Error, fmt};

use beryl_home_store::{CommandOutcome, HomeStore, ReconciliationFailure};

use crate::{SyndicReadError, SyndicStorage};

use super::super::super::{DraftPieceRootsFamily, point_limit};
use super::super::{
    DraftEditHistoryFrontiersFamily, DraftEditHistoryTransitionsFamily,
    draft_edit_history_frontier_is_authenticated_v1,
};
use super::{codec::*, model::*, mutation::PreparedDraftHistoricalRootAdoptionV1};

#[derive(Debug)]
pub enum DraftHistoricalRootAdoptionReconciliationErrorV1 {
    Read(SyndicReadError),
    Reconciliation(ReconciliationFailure),
}

impl fmt::Display for DraftHistoricalRootAdoptionReconciliationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => {
                write!(formatter, "historical-root adoption status failed: {error}")
            }
            Self::Reconciliation(error) => write!(
                formatter,
                "historical-root adoption reconciliation failed: {error}"
            ),
        }
    }
}

impl Error for DraftHistoricalRootAdoptionReconciliationErrorV1 {}

impl From<SyndicReadError> for DraftHistoricalRootAdoptionReconciliationErrorV1 {
    fn from(value: SyndicReadError) -> Self {
        Self::Read(value)
    }
}

impl SyndicStorage {
    pub fn draft_historical_root_adoption_status(
        &self,
        store: &HomeStore,
        request: DraftHistoricalRootAdoptionRequestV1,
    ) -> Result<DraftHistoricalRootAdoptionStatusV1, SyndicReadError> {
        let Some(settlement) =
            self.point::<DraftHistoricalRootAdoptionsFamily>(store, request.key(), point_limit())?
        else {
            return Ok(DraftHistoricalRootAdoptionStatusV1::Absent);
        };
        if settlement.request_bytes() != canonical_historical_root_adoption_request_bytes(request)
            || !settlement.is_locally_valid()
            || !draft_edit_history_frontier_is_authenticated_v1(
                self,
                store,
                settlement.source_history(),
            )?
            || match request.direction() {
                DraftHistoricalRootDirectionV1::Undo => settlement.source_history().undo_head(),
                DraftHistoricalRootDirectionV1::Redo => settlement.source_history().redo_head(),
            } != Some(settlement.selected_transition().reference())
            || self
                .point::<DraftEditHistoryTransitionsFamily>(
                    store,
                    settlement.selected_transition().key(),
                    point_limit(),
                )?
                .as_ref()
                != Some(settlement.selected_transition())
            || self
                .point::<DraftPieceRootsFamily>(
                    store,
                    settlement.target_root().reference().key(),
                    point_limit(),
                )?
                .as_ref()
                != Some(settlement.target_root())
        {
            return Ok(DraftHistoricalRootAdoptionStatusV1::Collision);
        }
        if settlement.outcome() == DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed {
            let (Some(transition), Some(history), Some(candidate)) = (
                settlement.successor_transition(),
                settlement.successor_history(),
                settlement.successor_candidate(),
            ) else {
                return Ok(DraftHistoricalRootAdoptionStatusV1::Collision);
            };
            if self
                .point::<DraftEditHistoryTransitionsFamily>(store, transition.key(), point_limit())?
                .as_ref()
                != Some(transition)
            {
                return Ok(DraftHistoricalRootAdoptionStatusV1::Collision);
            }
            let current_history = self.point::<DraftEditHistoryFrontiersFamily>(
                store,
                history.reference().key(),
                point_limit(),
            )?;
            if current_history.as_ref() != Some(history)
                || !draft_edit_history_frontier_is_authenticated_v1(self, store, history)?
                || candidate.newest_history() != history.reference()
            {
                return Ok(DraftHistoricalRootAdoptionStatusV1::Collision);
            }
        }
        Ok(DraftHistoricalRootAdoptionStatusV1::Settled(
            DraftHistoricalRootAdoptionOutcomeV1::from_settlement(settlement),
        ))
    }

    pub fn reconcile_draft_historical_root_adoption(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftHistoricalRootAdoptionV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftHistoricalRootAdoptionReconciliationV1,
        DraftHistoricalRootAdoptionReconciliationErrorV1,
    > {
        if let CommandOutcome::Indeterminate { reconciliation, .. } = outcome {
            let handle = reconciliation.install_and_handle();
            store
                .reconcile(&handle)
                .map_err(DraftHistoricalRootAdoptionReconciliationErrorV1::Reconciliation)?;
        }
        Ok(
            match self.draft_historical_root_adoption_status(store, prepared.request())? {
                DraftHistoricalRootAdoptionStatusV1::Absent => {
                    DraftHistoricalRootAdoptionReconciliationV1::ExactOld
                }
                DraftHistoricalRootAdoptionStatusV1::Settled(outcome) => {
                    DraftHistoricalRootAdoptionReconciliationV1::ExactNew(outcome)
                }
                DraftHistoricalRootAdoptionStatusV1::Collision => {
                    DraftHistoricalRootAdoptionReconciliationV1::Collision
                }
            },
        )
    }
}
