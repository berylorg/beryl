use super::*;

impl SyndicStorage {
    pub fn prepare_dispose_draft_editor_candidate_session(
        &self,
        store: &HomeStore,
        request: DraftEditorCandidateSessionDisposeRequestV1,
    ) -> Result<
        PreparedDraftEditorCandidateSessionDisposeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        self.prepare_dispose_draft_editor_candidate_session_with_access(
            ReadAccess::Ordinary(store),
            request,
        )
    }

    pub fn prepare_dispose_draft_editor_candidate_session_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        request: DraftEditorCandidateSessionDisposeRequestV1,
    ) -> Result<
        PreparedDraftEditorCandidateSessionDisposeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        self.prepare_dispose_draft_editor_candidate_session_with_access(
            ReadAccess::Candidate(store),
            request,
        )
    }

    fn prepare_dispose_draft_editor_candidate_session_with_access(
        &self,
        store: ReadAccess<'_>,
        request: DraftEditorCandidateSessionDisposeRequestV1,
    ) -> Result<
        PreparedDraftEditorCandidateSessionDisposeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let revision = self
            .revision_with_access(store)
            .map_err(SyndicReadError::Read)?;
        let result = self.prepare_disposal_with_access(store, request);
        disposal::confirm_revision(self, store, revision)?;
        result
    }

    fn prepare_disposal_with_access(
        &self,
        store: ReadAccess<'_>,
        request: DraftEditorCandidateSessionDisposeRequestV1,
    ) -> Result<
        PreparedDraftEditorCandidateSessionDisposeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        if !request.expected_pair().is_coherent() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let limit = point_limit();
        if let Some(record) = self.point_with_access::<DraftEditorCandidateSessionsFamily>(
            store,
            disposal_key(request),
            limit,
        )? {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(occupied) = record else {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            };
            let occupied = occupied
                .disposal()
                .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
            if !validate_disposal_receipt_with_access(self, store, occupied)? {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            }
            return Ok(PreparedDraftEditorCandidateSessionDisposeV1 {
                home_id: store.home_id(),
                canonical_path: disposal::canonical_path(store).to_owned(),
                request,
                canonical_request: canonical_candidate_disposal_request_bytes(request),
                frontier: occupied.frontier().clone(),
                initially_absent: false,
            });
        }
        let head =
            match disposal::read_session(self, store, request.draft_id(), request.session_id())? {
                DraftEditorCandidateSessionReadOutcomeV1::Active(head)
                | DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => head,
                _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
            };
        if disposal_request_matches_head(request, &head) && head.active_operation().is_some() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::ActiveOperation);
        }
        let frontier = self
            .point_with_access::<DraftEditHistoryFrontiersFamily>(
                store,
                head.newest_history().key(),
                limit,
            )?
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if frontier.reference() != head.newest_history()
            || !draft_edit_history_frontier_is_authenticated_with_access(self, store, &frontier)?
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        Ok(PreparedDraftEditorCandidateSessionDisposeV1 {
            home_id: store.home_id(),
            canonical_path: disposal::canonical_path(store).to_owned(),
            request,
            canonical_request: canonical_candidate_disposal_request_bytes(request),
            frontier,
            initially_absent: true,
        })
    }

    pub fn dispose_draft_editor_candidate_session(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftEditorCandidateSessionDisposeV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, DisposalMutation { prepared })
    }

    pub fn dispose_draft_editor_candidate_session_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftEditorCandidateSessionDisposeV1,
    ) -> Result<MutationContribution, DraftEditorCandidatePublicationCommandErrorV1> {
        if prepared.home_id != store.home_id() || prepared.canonical_path != store.canonical_path()
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        disposal::confirm_revision(self, ReadAccess::Candidate(store), expected_domain_revision)?;
        Ok(self.dispose_draft_editor_candidate_session(expected_domain_revision, prepared))
    }

    pub fn reconcile_draft_editor_candidate_session_disposal(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftEditorCandidateSessionDisposeV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidateSessionDisposeOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        self.reconcile_draft_editor_candidate_session_disposal_with_access(
            ReadAccess::Ordinary(store),
            prepared,
            outcome,
        )
    }

    pub fn reconcile_draft_editor_candidate_session_disposal_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftEditorCandidateSessionDisposeV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidateSessionDisposeOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        self.reconcile_draft_editor_candidate_session_disposal_with_access(
            ReadAccess::Candidate(store),
            prepared,
            outcome,
        )
    }

    fn reconcile_draft_editor_candidate_session_disposal_with_access(
        &self,
        store: ReadAccess<'_>,
        prepared: &PreparedDraftEditorCandidateSessionDisposeV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidateSessionDisposeOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        if prepared.home_id != store.home_id()
            || prepared.canonical_path != disposal::canonical_path(store)
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let committed = match outcome {
            CommandOutcome::NotCommitted { .. } => false,
            CommandOutcome::Committed { .. } => true,
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                let handle = reconciliation.install_and_handle();
                match match store {
                    ReadAccess::Ordinary(store) => store.reconcile(&handle),
                    ReadAccess::Candidate(store) => store.reconcile(&handle),
                }
                .map_err(DraftEditorCandidatePublicationCommandErrorV1::Reconciliation)?
                {
                    ReconciliationResolution::ExactNew { .. } => true,
                    ReconciliationResolution::ExactOld => false,
                    ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision => {
                        return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
                    }
                }
            }
        };
        let revision = self
            .revision_with_access(store)
            .map_err(SyndicReadError::Read)?;
        let result = self.disposal_outcome_with_access(store, prepared, committed);
        disposal::confirm_revision(self, store, revision)?;
        result
    }

    fn disposal_outcome_with_access(
        &self,
        store: ReadAccess<'_>,
        prepared: &PreparedDraftEditorCandidateSessionDisposeV1,
        committed: bool,
    ) -> Result<
        DraftEditorCandidateSessionDisposeOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let request = prepared.request;
        let limit = point_limit();
        if let Some(record) = self.point_with_access::<DraftEditorCandidateSessionsFamily>(
            store,
            disposal_key(request),
            limit,
        )? {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            };
            let receipt = receipt
                .disposal()
                .cloned()
                .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
            if !validate_disposal_receipt_with_access(self, store, &receipt)? {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            }
            if receipt.request_bytes() != prepared.canonical_request {
                return Ok(
                    DraftEditorCandidateSessionDisposeOutcomeV1::OccupiedIdentityCollision(
                        DraftEditorCandidateSessionDisposeCollisionProofV1::new(request, receipt),
                    ),
                );
            }
            return if committed && prepared.initially_absent {
                Ok(DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(
                    receipt.after_head().clone(),
                ))
            } else {
                Ok(DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(
                    receipt,
                ))
            };
        }
        let head =
            match disposal::read_session(self, store, request.draft_id(), request.session_id())? {
                DraftEditorCandidateSessionReadOutcomeV1::Active(h)
                | DraftEditorCandidateSessionReadOutcomeV1::Disposed(h) => h,
                _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
            };
        if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed {
            return Ok(DraftEditorCandidateSessionDisposeOutcomeV1::AlreadyDisposed(head));
        }
        if !disposal_request_matches_head(request, &head) {
            return Ok(DraftEditorCandidateSessionDisposeOutcomeV1::DirtyConflict(
                head,
            ));
        }
        Err(if committed {
            DraftEditorCandidatePublicationCommandErrorV1::Invariant
        } else {
            DraftEditorCandidatePublicationCommandErrorV1::NotCommitted
        })
    }
}
