use super::*;

#[inline(never)]
pub(super) fn read_head(
    reader: &DomainReader<'_, SyndicDomain>,
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> Result<DraftEditorCandidateSessionV1, SyndicMutationError> {
    let head = match required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id),
    )? {
        DraftEditorCandidateSessionRecordV1::Head(head) => head,
        DraftEditorCandidateSessionRecordV1::OpenReceipt(_) => {
            return Err(SyndicMutationError::IdentityCollision);
        }
    };
    Ok(head)
}

#[inline(never)]
pub(super) fn authenticate_open_receipt(
    reader: &DomainReader<'_, SyndicDomain>,
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    head: &DraftEditorCandidateSessionV1,
) -> Result<(), SyndicMutationError> {
    let receipt = required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            draft_id,
            session_id,
            head.open_operation_id(),
        ),
    )?;
    let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = receipt else {
        return Err(SyndicMutationError::IdentityCollision);
    };
    if !super::super::session::receipt_matches_head(&receipt, head) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

#[inline(never)]
pub(super) fn authenticate_active_custody(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
) -> Result<(), SyndicMutationError> {
    if let Some(custody) = head.active_operation() {
        if let Some(staging_receipt) = custody.staging_receipt() {
            let identity = staging_receipt.identity();
            let staging_head = required::<DraftMutationStagingHeadsFamily>(reader, &identity)?;
            let receipt =
                super::super::staging::authenticate_staging_head_reader(reader, &staging_head)?;
            if staging_head.receipt() != staging_receipt
                || custody.operation_id() != identity.operation_id().as_piece_operation()
                || custody.begin_digest() != Some(staging_head.begin_digest())
                || custody.predecessor_candidate_generation()
                    != staging_head.begin().predecessor_candidate_generation()
                || custody.predecessor_root() != staging_head.begin().predecessor_root()
                || custody.predecessor_history() != staging_head.begin().predecessor_history()
                || receipt.custody_after() != DraftMutationStagingCustodyTagV1::Staging
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
        } else {
            let key = DraftPieceSettlementKeyV1::new(
                head.draft_id(),
                head.session_id(),
                custody.operation_id(),
            );
            let build = required_build(reader, &key)?;
            if Some(build.proposal_digest()) != custody.proposal_digest()
                || build.predecessor_candidate_generation()
                    != custody.predecessor_candidate_generation()
                || build.predecessor_root() != custody.predecessor_root()
                || Some(build.progress_receipt()) != custody.build_receipt()
                || !matches!(
                    build.lifecycle(),
                    DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
                )
                || point::<DraftPieceSettlementsFamily>(reader, &key)?.is_some()
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            let next_ordinal = build
                .progress_receipt()
                .key()
                .transition_ordinal()
                .checked_add(1)
                .ok_or(SyndicMutationError::IdentityCollision)?;
            if point::<DraftPieceBuildProgressFamily>(
                reader,
                &DraftPieceBuildProgressReceiptKeyV1::new(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                    next_ordinal,
                ),
            )?
            .is_some()
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            if build.staged_fragment_count() < build.fragment_count()
                && point::<DraftPieceBuildFragmentsFamily>(
                    reader,
                    &DraftPieceBuildFragmentKeyV1::new(
                        build.draft_id(),
                        build.session_id(),
                        build.operation_id(),
                        build.staged_fragment_count() + 1,
                    ),
                )?
                .is_some()
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
    }
    Ok(())
}

#[inline(never)]
pub(super) fn authenticate_candidate_history(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
) -> Result<(), SyndicMutationError> {
    if head.newest_candidate_generation() == head.published_candidate_generation()
        && head.newest_candidate_generation() != 0
    {
        let published =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.published_history().key())?;
        let newest =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?;
        if published.reference() != head.published_history()
            || newest.reference() != head.newest_history()
            || !(published == newest
                && matches!(
                    published.reference().key(),
                    DraftEditHistoryFrontierKeyV1::Publication { session_id, .. }
                        if session_id == head.session_id()
                )
                || published.fork_session(head.session_id()).as_ref() == Some(&newest))
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
    } else if head.newest_candidate_generation() != head.published_candidate_generation() {
        let root = head.newest_root();
        let newest_history =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?;
        if newest_history.reference() != head.newest_history() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        authenticate_draft_edit_history_frontier_v1(reader, &newest_history)?;
        let journal_head = newest_history
            .journal_head()
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let newest_transition =
            required::<DraftEditHistoryTransitionsFamily>(reader, &journal_head.key())?;
        if newest_transition.reference() != journal_head {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if newest_transition.kind() != DraftEditHistoryTransitionKindV1::OrdinaryEdit {
            if !historical_candidate_session_is_exact(
                reader,
                head,
                newest_transition.operation_id(),
            )? {
                return Err(SyndicMutationError::IdentityCollision);
            }
            return Ok(());
        }
        let key = DraftPieceSettlementKeyV1::new(
            head.draft_id(),
            head.session_id(),
            newest_transition.operation_id(),
        );
        let stored_root = required::<DraftPieceRootsFamily>(reader, &root.key())?;
        let settlement = required::<DraftPieceSettlementsFamily>(reader, &key)?;
        let build = point_build(reader, &key)?;
        let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if stored_root.reference() != root
            || !settlement_closure_is_exact(&settlement)
            || !settlement_terminal_build_is_exact(&settlement, build.as_ref())
            || !super::super::session::adopted_head_matches_current(
                adoption.adopted_session(),
                head,
            )
            || !matches!(
                settlement.outcome(),
                DraftPieceSettlementOutcomeV1::Committed {
                    successor,
                    candidate_generation,
                    ..
                } if *successor == root
                    && *candidate_generation == head.newest_candidate_generation()
            )
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
    }
    Ok(())
}
