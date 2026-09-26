use super::*;

pub(super) fn references_are_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
    settlement: &DraftHistoricalRootAdoptionV1,
) -> Result<bool, R::Error> {
    if match settlement.request().direction() {
        DraftHistoricalRootDirectionV1::Undo => settlement.source_history().undo_head(),
        DraftHistoricalRootDirectionV1::Redo => settlement.source_history().redo_head(),
    } != Some(settlement.selected_transition().reference())
        || reader
            .point::<DraftEditHistoryTransitionsFamily>(&settlement.selected_transition().key())?
            .as_ref()
            != Some(settlement.selected_transition())
        || reader
            .point::<DraftPieceRootsFamily>(&settlement.target_root().reference().key())?
            .as_ref()
            != Some(settlement.target_root())
    {
        return Ok(false);
    }
    Ok(
        settlement.outcome() == DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed
            && settlement.successor_transition() == Some(transition)
            && settlement.successor_history() == Some(frontier)
            && settlement
                .successor_candidate()
                .is_some_and(|candidate| session::adopted_head_matches_current(candidate, head)),
    )
}
