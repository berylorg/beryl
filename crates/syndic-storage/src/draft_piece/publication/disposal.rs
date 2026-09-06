use super::*;

pub(crate) struct PreparedCandidateDisposal {
    request: DraftEditorCandidateSessionDisposeRequestV1,
    after: DraftEditorCandidateSessionV1,
    receipt: DraftEditorCandidateSessionDisposeReceiptV1,
}

pub(crate) fn prepare_candidate_disposal(
    reader: &DomainReader<'_, SyndicDomain>,
    request: DraftEditorCandidateSessionDisposeRequestV1,
    head: DraftEditorCandidateSessionV1,
    frontier: DraftEditHistoryFrontierV1,
) -> Result<PreparedCandidateDisposal, SyndicMutationError> {
    if !disposal_request_matches_head(request, &head)
        || head.active_operation().is_some()
        || point::<DraftEditorCandidateSessionsFamily>(reader, &disposal_key(request))?.is_some()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let selector = current_selector(reader, head.thread_id())?;
    if selector.draft_id() != head.draft_id()
        || selector.selector_revision() != head.published_selector_revision()
        || selector.root() != head.published_root()
        || selector.history() != head.published_history()
    {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    let open = required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            head.draft_id(),
            head.session_id(),
            head.open_operation_id(),
        ),
    )?;
    let DraftEditorCandidateSessionRecordV1::OpenReceipt(open) = open else {
        return Err(SyndicMutationError::IdentityCollision);
    };
    if !session::receipt_matches_head(&open, &head)
        || !checkpoint::saved_checkpoint_is_exact_in_transaction(reader, &head, &frontier)?
        || frontier.reference() != head.newest_history()
        || required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?
            != frontier
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_draft_edit_history_frontier_v1(reader, &frontier)?;
    let after = checkpoint::disposed_saved_checkpoint(&head, request.operation_id())
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let receipt = DraftEditorCandidateSessionDisposeReceiptV1::new(
        canonical_candidate_disposal_request_bytes(request),
        head,
        after.clone(),
        frontier,
    );
    Ok(PreparedCandidateDisposal {
        request,
        after,
        receipt,
    })
}

impl PreparedCandidateDisposal {
    pub(crate) fn contribute(
        &self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &session_key(self.after.draft_id(), self.after.session_id()),
            &DraftEditorCandidateSessionRecordV1::Head(self.after.clone()),
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &disposal_key(self.request),
            &DraftEditorCandidateSessionRecordV1::OpenReceipt(
                DraftEditorCandidateSessionOpenReceiptV1::from_disposal(self.receipt.clone()),
            ),
        )?;
        Ok(())
    }
}
