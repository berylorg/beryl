use super::*;

impl SyndicStorage {
    pub fn qualify_fresh_draft_editor_candidate_session_open_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        expected_home_id: beryl_model::BerylHomeId,
        expected_disposal_operation: DraftPieceOperationIdV1,
        prepared: &PreparedDraftEditorCandidateSessionOpenV1,
    ) -> Result<DraftEditorCandidateSessionReadOutcomeV1, DraftEditorCandidateSessionCommandErrorV1>
    {
        let access = ReadAccess::Candidate(store);
        if access.home_id() != expected_home_id {
            return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
        }
        let revision = self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?;
        let result = qualify_original_opening(self, access, expected_disposal_operation, prepared);
        if self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?
            != revision
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "fresh editor opening recovery qualification",
            }
            .into());
        }
        result
    }
}

fn qualify_original_opening(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    expected_disposal_operation: DraftPieceOperationIdV1,
    prepared: &PreparedDraftEditorCandidateSessionOpenV1,
) -> Result<DraftEditorCandidateSessionReadOutcomeV1, DraftEditorCandidateSessionCommandErrorV1> {
    let request = prepared.request;
    if expected_disposal_operation == request.operation_id() {
        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
    }
    let first = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        head_key(request),
        point_limit(),
    )?;
    let receipt = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        receipt_key(request),
        point_limit(),
    )?;
    let disposal = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
            request.selector().draft_id(),
            request.session_id(),
            expected_disposal_operation,
        ),
        point_limit(),
    )?;
    let root = storage.point_with_access::<DraftPieceRootsFamily>(
        store,
        request.selector().root().key(),
        point_limit(),
    )?;
    let durable = storage.point_with_access::<DraftEditHistoryFrontiersFamily>(
        store,
        request.selector().history().key(),
        point_limit(),
    )?;
    if !root
        .as_ref()
        .is_some_and(|root| root.reference() == request.selector().root())
        || !match durable.as_ref() {
            Some(durable) => {
                durable.reference() == request.selector().history()
                    && draft_edit_history_frontier_is_authenticated_with_access(
                        storage, store, durable,
                    )?
            }
            None => false,
        }
    {
        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
    }
    let result = match (&first, receipt) {
        (None, None) => {
            let frontier = storage.point_with_access::<DraftEditHistoryFrontiersFamily>(
                store,
                DraftEditHistoryFrontierKeyV1::session(
                    request.selector().draft_id(),
                    request.session_id(),
                ),
                point_limit(),
            )?;
            if !prepared.initially_absent || frontier.is_some() || disposal.is_some() {
                return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
            }
            DraftEditorCandidateSessionReadOutcomeV1::Absent
        }
        (
            Some(DraftEditorCandidateSessionRecordV1::Head(head)),
            Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt)),
        ) => {
            if receipt.request_bytes() != prepared.canonical_request
                || !receipt_matches_head(&receipt, head)
                || head.open_operation_id() != request.operation_id()
            {
                return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
            }
            match head.lifecycle() {
                DraftEditorCandidateSessionLifecycleV1::Active => {
                    if receipt.head() != head
                        || disposal.is_some()
                        || head.active_operation().is_some()
                        || !checkpoint::has_opening_identity(head)
                        || !checkpoint::has_saved_identity(head)
                        || !publication::fresh_opening_history_is_exact_with_access(
                            storage, store, head,
                        )?
                    {
                        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                    }
                    DraftEditorCandidateSessionReadOutcomeV1::Active(head.clone())
                }
                DraftEditorCandidateSessionLifecycleV1::Disposed => {
                    if head.disposal_operation_id() != Some(expected_disposal_operation) {
                        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                    }
                    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(disposal)) = disposal
                    else {
                        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                    };
                    let Some(disposal) = disposal.disposal() else {
                        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                    };
                    if disposal.before_head() != receipt.head()
                        || receipt
                            .head()
                            .abandoned_fresh(expected_disposal_operation)
                            .as_ref()
                            != Some(head)
                        || !publication::validate_disposal_receipt_with_access(
                            storage, store, disposal,
                        )?
                    {
                        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                    }
                    DraftEditorCandidateSessionReadOutcomeV1::Disposed(head.clone())
                }
            }
        }
        _ => return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant),
    };
    let last = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        head_key(request),
        point_limit(),
    )?;
    if last != first {
        return Err(SyndicReadError::ConcurrentChange {
            operation: "fresh editor opening recovery qualification",
        }
        .into());
    }
    Ok(result)
}
