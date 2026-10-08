use super::*;

impl SyndicStorage {
    pub fn prepare_open_draft_editor_candidate_session_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        request: DraftEditorCandidateSessionOpenRequestV1,
    ) -> Result<PreparedDraftEditorCandidateSessionOpenV1, DraftEditorCandidateSessionCommandErrorV1>
    {
        let access = ReadAccess::Candidate(store);
        let revision = self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?;
        let prepared =
            self.prepare_open_draft_editor_candidate_session_with_access(access, request)?;
        let current = self
            .current_range_selector(access, request.selector().thread_id())
            .map_err(|error| match error {
                DraftPieceRangeSourceErrorV1::Operational(error) => {
                    DraftEditorCandidateSessionCommandErrorV1::Read(error)
                }
                _ => DraftEditorCandidateSessionCommandErrorV1::Invariant,
            })?;
        let frontier = self.point_with_access::<DraftEditHistoryFrontiersFamily>(
            access,
            DraftEditHistoryFrontierKeyV1::session(
                request.selector().draft_id(),
                request.session_id(),
            ),
            point_limit(),
        )?;
        let root = self.point_with_access::<DraftPieceRootsFamily>(
            access,
            request.selector().root().key(),
            point_limit(),
        )?;
        let receipt = self.point_with_access::<DraftEditorCandidateSessionsFamily>(
            access,
            receipt_key(request),
            point_limit(),
        )?;
        if !prepared.initially_absent
            || frontier.is_some()
            || receipt.is_some()
            || current != Some(request.selector())
            || !root.as_ref().is_some_and(|root| {
                root.reference() == request.selector().root()
                    && draft_piece_root_reference_is_locally_exact_v1(root.reference())
            })
        {
            return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
        }
        if self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?
            != revision
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "fresh editor opening preparation",
            }
            .into());
        }
        Ok(prepared)
    }

    pub fn reconcile_fresh_draft_editor_candidate_session_open_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftEditorCandidateSessionOpenV1,
        outcome: CommandOutcome,
    ) -> Result<DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionCommandErrorV1>
    {
        let access = ReadAccess::Candidate(store);
        let result = self
            .reconcile_draft_editor_candidate_session_open_with_access(access, prepared, outcome)?;
        if let DraftEditorCandidateSessionOpenOutcomeV1::Opened(head)
        | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) = &result
        {
            let revision = self
                .revision_with_access(access)
                .map_err(SyndicReadError::Read)?;
            let receipt = self.point_with_access::<DraftEditorCandidateSessionsFamily>(
                access,
                receipt_key(prepared.request),
                point_limit(),
            )?;
            let current = self
                .current_range_selector(access, prepared.request.selector().thread_id())
                .map_err(|error| match error {
                    DraftPieceRangeSourceErrorV1::Operational(error) => {
                        DraftEditorCandidateSessionCommandErrorV1::Read(error)
                    }
                    _ => DraftEditorCandidateSessionCommandErrorV1::Invariant,
                })?;
            if !matches!(receipt, Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(ref receipt))
                if receipt.head() == head && receipt.request_bytes() == prepared.canonical_request)
                || current != Some(prepared.request.selector())
                || !idle_candidate_closure_is_exact_with_access(self, access, head)?
                || !publication::fresh_opening_history_is_exact_with_access(self, access, head)?
            {
                return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
            }
            let after = self.point_with_access::<DraftEditorCandidateSessionsFamily>(
                access,
                head_key(prepared.request),
                point_limit(),
            )?;
            if !matches!(after, Some(DraftEditorCandidateSessionRecordV1::Head(ref after)) if after == head)
                || self
                    .revision_with_access(access)
                    .map_err(SyndicReadError::Read)?
                    != revision
            {
                return Err(SyndicReadError::ConcurrentChange {
                    operation: "fresh editor opening outcome",
                }
                .into());
            }
        }
        Ok(result)
    }

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
