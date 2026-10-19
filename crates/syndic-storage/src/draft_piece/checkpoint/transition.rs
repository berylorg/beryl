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
        reader.boxed_point::<DraftEditHistoryTransitionsFamily>(&journal_head.key())?
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

#[inline(never)]
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
    let settlement = reader.boxed_point::<DraftPieceSettlementsFamily>(&key)?;
    if let Some(settlement) = settlement.as_deref() {
        if let DraftPieceSettlementClosureV1::OrdinaryRootReplacement(c) = settlement.closure() {
            let a = c.adoption();
            if !settlement_closure_is_exact(settlement)
                || !session::adopted_head_matches_current(a.adopted_session(), head)
                || a.adopted_history() != frontier
                || a.transition() != transition
            {
                return Ok(false);
            }
            let root =
                reader.point::<DraftPieceRootsFamily>(&a.adopted_root().reference().key())?;
            if root.as_ref() != Some(a.adopted_root()) {
                return Ok(false);
            }
            if settlement.terminal_source().is_none() {
                return Ok(settlement.terminal_progress_receipt().is_none());
            }
            let build = reader.boxed_point::<DraftPieceBuildsFamily>(&key)?;
            return match build {
                Some(build) => Ok(build_progress_is_exact(reader, &build)?
                    && settlement_terminal_build_is_exact(settlement, Some(&build))),
                None => Ok(false),
            };
        }
    }
    ordinary_build_is_exact(
        reader,
        head,
        frontier,
        transition,
        &key,
        settlement.as_deref(),
    )
}

#[inline(never)]
fn ordinary_build_is_exact<R: CheckpointReader>(
    reader: &R,
    head: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
    key: &DraftPieceSettlementKeyV1,
    settlement: Option<&DraftPieceSettlementV1>,
) -> Result<bool, R::Error> {
    let build = reader.boxed_point::<DraftPieceBuildsFamily>(key)?;
    let (Some(settlement), Some(build)) = (settlement, build.as_deref()) else {
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

#[inline(never)]
fn build_progress_is_exact<R: CheckpointReader>(
    reader: &R,
    build: &DraftPieceBuildRecordV1,
) -> Result<bool, R::Error> {
    let Some(receipt) =
        reader.boxed_point::<DraftPieceBuildProgressFamily>(&build.progress_receipt().key())?
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
    if reader.contains::<DraftPieceBuildProgressFamily>(
        &DraftPieceBuildProgressReceiptKeyV1::new(
            build.draft_id(),
            build.session_id(),
            build.operation_id(),
            next_ordinal,
        ),
    )? || !progress_receipt_matches_build(&receipt, build)
        || !reader.authenticate_progress(&receipt)?
    {
        return Ok(false);
    }
    Ok(true)
}
