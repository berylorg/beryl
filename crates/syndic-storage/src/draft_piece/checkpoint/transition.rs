use super::*;

pub(super) fn transition_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, R::Error> {
    let Some(journal_head) = frontier.journal_head() else {
        return Ok(false);
    };
    let Some(transition) =
        reader.point::<DraftEditHistoryTransitionsFamily>(&journal_head.key())?
    else {
        return Ok(false);
    };
    if transition.reference() != journal_head {
        return Ok(false);
    }
    if transition.kind() != DraftEditHistoryTransitionKindV1::OrdinaryEdit {
        return historical_is_exact(reader, head, frontier, &transition);
    }
    ordinary_transition_is_exact(reader, head, frontier, &transition)
}

fn ordinary_transition_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
) -> Result<bool, R::Error> {
    let key = DraftPieceSettlementKeyV1::new(
        head.draft_id(),
        head.session_id(),
        transition.operation_id(),
    );
    let settlement = reader.point::<DraftPieceSettlementsFamily>(&key)?;
    ordinary_build_is_exact(
        reader,
        head,
        frontier,
        transition,
        &key,
        settlement.as_ref(),
    )
}

fn ordinary_build_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
    key: &DraftPieceSettlementKeyV1,
    settlement: Option<&DraftPieceSettlementV1>,
) -> Result<bool, R::Error> {
    let build = reader.point::<DraftPieceBuildsFamily>(key)?;
    let (Some(settlement), Some(build)) = (settlement, build.as_ref()) else {
        return Ok(false);
    };
    if !build_progress_is_exact(reader, build)? {
        return Ok(false);
    }
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        return Ok(false);
    };
    Ok(settlement_closure_is_exact(settlement)
        && settlement_terminal_build_is_exact(settlement, Some(build))
        && session::adopted_head_matches_current(adoption.adopted_session(), head)
        && adoption.adopted_root().reference() == head.newest_root()
        && adoption.adopted_history() == frontier
        && adoption.transition() == transition)
}

fn build_progress_is_exact<R: CheckpointReader>(
    reader: &R,
    build: &DraftPieceBuildRecordV1,
) -> Result<bool, R::Error> {
    let Some(receipt) =
        reader.point::<DraftPieceBuildProgressFamily>(&build.progress_receipt().key())?
    else {
        return Ok(false);
    };
    let Some(next_ordinal) = build
        .progress_receipt()
        .key()
        .transition_ordinal()
        .checked_add(1)
    else {
        return Ok(false);
    };
    if reader
        .point::<DraftPieceBuildProgressFamily>(&DraftPieceBuildProgressReceiptKeyV1::new(
            build.draft_id(),
            build.session_id(),
            build.operation_id(),
            next_ordinal,
        ))?
        .is_some()
        || !progress_receipt_matches_build(&receipt, build)
        || !reader.authenticate_progress(&receipt)?
    {
        return Ok(false);
    }
    Ok(true)
}
