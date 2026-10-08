use super::*;
use beryl_home_store::HomeCandidateRecoveryAccess;

impl SyndicStorage {
    pub fn current_draft_piece_text_demand_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        thread_id: SyndicThreadId,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    ) -> Result<
        Option<DraftPieceCurrentRangeResultV1<DraftPieceTextDemandResultV1>>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.with_candidate_range_revision(store, || {
            self.stabilized_current_range(ReadAccess::Candidate(store), thread_id, |root| {
                self.draft_piece_text_demand_candidate(store, root, demand, max_bytes)
            })
        })
    }

    pub fn candidate_draft_piece_text_demand_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        binding: DraftEditorCandidateActivationBindingV1,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    ) -> Result<
        DraftPieceCandidateRangeResultV1<DraftPieceTextDemandResultV1>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.with_candidate_range_revision(store, || {
            self.stabilized_candidate_range(ReadAccess::Candidate(store), binding, |root| {
                self.draft_piece_text_demand_candidate(store, root, demand, max_bytes)
            })
        })
    }

    pub fn candidate_draft_piece_marker_demand_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        binding: DraftEditorCandidateActivationBindingV1,
        demand: DraftPieceMarkerDemandV1,
    ) -> Result<
        DraftPieceCandidateRangeResultV1<DraftPieceMarkerDemandResultV1>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.with_candidate_range_revision(store, || {
            self.stabilized_candidate_range(ReadAccess::Candidate(store), binding, |root| {
                self.draft_piece_marker_demand_candidate(store, root, demand)
            })
        })
    }

    pub fn candidate_draft_piece_marker_edge_proof_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        binding: DraftEditorCandidateActivationBindingV1,
        request: DraftPieceMarkerEdgeProofRequestV1,
        retained_byte_ceiling: usize,
    ) -> Result<
        DraftPieceCandidateRangeResultV1<Option<DraftPieceMarkerEdgeProofV1>>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.with_candidate_range_revision(store, || {
            self.stabilized_candidate_range(ReadAccess::Candidate(store), binding, |root| {
                self.draft_piece_marker_edge_proof_candidate(
                    store,
                    root,
                    request,
                    retained_byte_ceiling,
                )
            })
        })
    }

    fn with_candidate_range_revision<T>(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        read: impl FnOnce() -> Result<T, DraftPieceRangeSourceErrorV1>,
    ) -> Result<T, DraftPieceRangeSourceErrorV1> {
        let access = ReadAccess::Candidate(store);
        let revision = self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?;
        let result = read();
        if self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?
            != revision
        {
            return Err(DraftPieceRangeSourceErrorV1::ConcurrentChange);
        }
        result
    }

    pub(super) fn draft_editor_candidate_session_for_range_with_access(
        &self,
        store: ReadAccess<'_>,
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
    ) -> Result<DraftEditorCandidateSessionReadOutcomeV1, SyndicReadError> {
        if let ReadAccess::Ordinary(store) = store {
            return self.draft_editor_candidate_session(store, draft_id, session_id);
        }
        let key = DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id);
        let first = self.point_with_access::<DraftEditorCandidateSessionsFamily>(
            store,
            key,
            point_limit(),
        )?;
        let outcome = match &first {
            None => DraftEditorCandidateSessionReadOutcomeV1::Absent,
            Some(DraftEditorCandidateSessionRecordV1::Head(head))
                if head.draft_id() == draft_id && head.session_id() == session_id =>
            {
                match head.lifecycle() {
                    DraftEditorCandidateSessionLifecycleV1::Active => {
                        if !session::idle_candidate_closure_is_exact_with_access(self, store, head)?
                        {
                            return Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure);
                        }
                        DraftEditorCandidateSessionReadOutcomeV1::Active(head.clone())
                    }
                    DraftEditorCandidateSessionLifecycleV1::Disposed => {
                        let Some(operation) = head.disposal_operation_id() else {
                            return Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure);
                        };
                        let receipt = self
                            .point_with_access::<DraftEditorCandidateSessionsFamily>(
                                store,
                                DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
                                    draft_id, session_id, operation,
                                ),
                                point_limit(),
                            )?;
                        let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt)) =
                            receipt
                        else {
                            return Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure);
                        };
                        let Some(disposal) = receipt.disposal() else {
                            return Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure);
                        };
                        if disposal.after_head() != head
                            || !session::opening_receipt_is_exact_with_access(self, store, head)?
                            || !publication::validate_disposal_receipt_with_access(
                                self, store, disposal,
                            )?
                        {
                            return Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure);
                        }
                        DraftEditorCandidateSessionReadOutcomeV1::Disposed(head.clone())
                    }
                }
            }
            Some(_) => DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure,
        };
        if self.point_with_access::<DraftEditorCandidateSessionsFamily>(
            store,
            key,
            point_limit(),
        )? != first
        {
            return Ok(DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange);
        }
        Ok(outcome)
    }

    pub fn draft_piece_text_demand_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        root: DraftPieceRootReferenceV1,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    ) -> Result<DraftPieceTextDemandResultV1, DraftPieceRangeSourceErrorV1> {
        self.draft_piece_text_demand_with_access(
            ReadAccess::Candidate(store),
            root,
            demand,
            max_bytes,
        )
    }

    pub fn draft_piece_marker_demand_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        root: DraftPieceRootReferenceV1,
        demand: DraftPieceMarkerDemandV1,
    ) -> Result<DraftPieceMarkerDemandResultV1, DraftPieceRangeSourceErrorV1> {
        self.draft_piece_marker_demand_with_access(ReadAccess::Candidate(store), root, demand)
    }

    pub fn draft_piece_marker_edge_proof_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        root: DraftPieceRootReferenceV1,
        request: DraftPieceMarkerEdgeProofRequestV1,
        retained_byte_ceiling: usize,
    ) -> Result<Option<DraftPieceMarkerEdgeProofV1>, DraftPieceRangeSourceErrorV1> {
        self.draft_piece_marker_edge_proof_with_access(
            ReadAccess::Candidate(store),
            root,
            request,
            retained_byte_ceiling,
        )
    }

    pub fn validate_draft_piece_restoration_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        restoration: DraftPieceRestorationV1,
    ) -> Result<DraftPieceRestorationV1, DraftPiecePrepareErrorV1> {
        self.validate_draft_piece_restoration_with_access(ReadAccess::Candidate(store), restoration)
    }
}
