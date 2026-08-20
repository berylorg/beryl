use super::*;

impl SyndicStorage {
    pub fn prepare_draft_mutation_staging_terminal(
        &self,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
        evidence: DraftMutationStagingTerminalEvidenceV1,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        if matches!(
            evidence,
            DraftMutationStagingTerminalEvidenceV1::Error {
                error: DraftMutationStagingErrorEvidenceV1::OccupiedIdentity { .. },
                ..
            }
        ) {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        self.prepare_draft_mutation_staging_terminal_validated(head, session, evidence, None)
    }

    fn prepare_draft_mutation_staging_terminal_validated(
        &self,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
        evidence: DraftMutationStagingTerminalEvidenceV1,
        rejected_request: Option<(DraftMutationStagingTerminalAnchorV1, DraftPieceDigestV1)>,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        if !matches!(
            head.lifecycle(),
            DraftMutationStagingLifecycleV1::Receiving
                | DraftMutationStagingLifecycleV1::Finished(_)
        ) || !staging_session_matches_head(session, head)
            || !terminal_evidence_matches(
                head.begin(),
                Some(head),
                session,
                evidence,
                rejected_request,
            )
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        if let DraftMutationStagingTerminalEvidenceV1::Cancelled {
            source_lifecycle,
            writer_admitted,
            ..
        } = evidence
        {
            if !writer_admitted || source_lifecycle != head.lifecycle() {
                return Err(DraftMutationStagingErrorV1::Invalid);
            }
        }
        let expected = staging_custody(head.begin(), head.begin_digest(), head.receipt());
        if session.active_operation() != Some(&expected) {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let target_session = session
            .clear_active_operation(&expected)
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        let transition = head
            .receipt()
            .transition_ordinal()
            .checked_add(1)
            .ok_or(DraftMutationStagingErrorV1::Overflow)?;
        let key =
            DraftMutationStagingProgressReceiptKeyV1::new(head.identity(), transition).unwrap();
        let provisional_receipt = DraftMutationStagingProgressReceiptReferenceV1::new(
            head.identity(),
            transition,
            DraftPieceDigestV1::from_bytes([0; 32]),
        )
        .unwrap();
        let lifecycle = terminal_lifecycle(evidence);
        let mut target = DraftMutationStagingHeadV1::from_parts(
            head.identity(),
            head.begin(),
            head.begin_digest(),
            head.source(),
            head.proposal(),
            provisional_receipt,
            lifecycle,
            DraftPieceDigestV1::from_bytes([0; 32]),
        );
        let digest = head_digest(target.clone())?;
        let receipt = make_receipt(
            key,
            Some(head.receipt()),
            DraftMutationStagingCommandKindV1::Terminal,
            None,
            None,
            head.source(),
            head.source(),
            head.proposal(),
            head.proposal(),
            Some(head.digest()),
            digest,
            Some(head.lifecycle()),
            lifecycle,
            DraftMutationStagingCustodyTagV1::Staging,
            DraftMutationStagingCustodyTagV1::None,
            None,
            Some(evidence),
        )?;
        let receipt_ref = DraftMutationStagingProgressReceiptReferenceV1::new(
            head.identity(),
            transition,
            receipt.digest(),
        )
        .unwrap();
        target = DraftMutationStagingHeadV1::from_parts(
            head.identity(),
            head.begin(),
            head.begin_digest(),
            head.source(),
            head.proposal(),
            receipt_ref,
            lifecycle,
            digest,
        );
        Ok(PreparedDraftMutationStagingCommandV1 {
            source_head: Some(head.clone()),
            target_head: target,
            source_session: session.clone(),
            target_session: Some(target_session),
            page: None,
            receipt,
        })
    }

    pub fn prepare_draft_mutation_staging_rejected_page(
        &self,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
        requested: &PreparedDraftMutationStagingCommandV1,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        let page = requested
            .page
            .as_ref()
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        if requested.source_head.as_ref() != Some(head)
            || requested.source_session != *session
            || page.key().identity() != head.identity()
            || !draft_mutation_staging_page_is_locally_exact(page)
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let anchor = DraftMutationStagingTerminalAnchorV1::Page(page.key());
        let evidence = DraftMutationStagingTerminalEvidenceV1::Rejected {
            reason: DraftMutationStagingRejectedReasonV1::InvalidPage,
            anchor,
            digest: page.digest(),
            candidate_generation: session.newest_candidate_generation(),
            root: session.newest_root(),
            history: session.newest_history(),
            session_revision: session.session_generation(),
        };
        self.prepare_draft_mutation_staging_terminal_validated(
            head,
            session,
            evidence,
            Some((anchor, page.digest())),
        )
    }

    pub fn prepare_draft_mutation_staging_operational_error(
        &self,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
        reason: DraftMutationStagingErrorReasonV1,
        anchor: DraftMutationStagingTerminalAnchorV1,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        let evidence = DraftMutationStagingTerminalEvidenceV1::Error {
            error: DraftMutationStagingErrorEvidenceV1::Operational { reason, anchor },
            candidate_generation: session.newest_candidate_generation(),
            root: session.newest_root(),
            history: session.newest_history(),
            session_revision: session.session_generation(),
        };
        self.prepare_draft_mutation_staging_terminal_validated(head, session, evidence, None)
    }

    pub fn prepare_draft_mutation_staging_occupied_page_error(
        &self,
        store: &beryl_home_store::HomeStore,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
        requested: &PreparedDraftMutationStagingCommandV1,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        let requested_page = requested
            .page
            .as_ref()
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        if requested.source_head.as_ref() != Some(head)
            || requested.source_session != *session
            || !draft_mutation_staging_page_is_locally_exact(requested_page)
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let stored_page = self
            .draft_mutation_staging_page(store, requested_page.key())?
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        let stored_bytes = canonical_staging_page_bytes(&stored_page)
            .map_err(|_| DraftMutationStagingErrorV1::Invariant)?;
        let requested_bytes = canonical_staging_page_bytes(requested_page)
            .map_err(|_| DraftMutationStagingErrorV1::Invariant)?;
        let (first_difference, stored, requested_byte) =
            canonical_difference(&stored_bytes, &requested_bytes)
                .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        let evidence = DraftMutationStagingTerminalEvidenceV1::Error {
            error: DraftMutationStagingErrorEvidenceV1::OccupiedIdentity {
                key: DraftMutationStagingOccupiedKeyV1::Page(requested_page.key()),
                stored_digest: stored_page.digest(),
                requested_digest: requested_page.digest(),
                first_difference,
                stored,
                requested: requested_byte,
            },
            candidate_generation: session.newest_candidate_generation(),
            root: session.newest_root(),
            history: session.newest_history(),
            session_revision: session.session_generation(),
        };
        self.prepare_draft_mutation_staging_terminal_validated(head, session, evidence, None)
    }

    pub fn prepare_draft_mutation_staging_transfer(
        &self,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
    ) -> Result<PreparedDraftMutationTransferV1, DraftMutationStagingErrorV1> {
        let DraftMutationStagingLifecycleV1::Finished(finish) = head.lifecycle() else {
            return Err(DraftMutationStagingErrorV1::Invalid);
        };
        if !draft_mutation_staging_head_is_locally_exact(head)
            || !staging_session_matches_head(session, head)
            || finish.proposal().item_total() == 0
            || finish.intended_selection_head() != finish.intended_caret()
            || head.begin().predecessor_selection_head() != head.begin().predecessor_caret()
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let expected = staging_custody(head.begin(), head.begin_digest(), head.receipt());
        if session.active_operation() != Some(&expected) {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let header = DraftPieceEditHeaderV1::new(
            head.identity().draft_id(),
            head.identity().session_id(),
            head.begin().predecessor_candidate_generation(),
            head.begin().predecessor_root(),
            head.begin().predecessor_history(),
            head.identity().operation_id().as_piece_operation(),
            head.begin().predecessor_caret(),
            head.begin().predecessor_selection_anchor(),
            finish.intended_caret(),
            finish.intended_selection_anchor(),
            finish.proposal().item_total(),
            finish.proposal_fragment_chain(),
        );
        let (prepared_edit, build, build_receipt, target_session) =
            super::mutation::initial_build_for_staging(header, session, expected)
                .map_err(|_| DraftMutationStagingErrorV1::Invalid)?;
        let build_endpoint = build.progress_receipt();
        let transition = head
            .receipt()
            .transition_ordinal()
            .checked_add(1)
            .ok_or(DraftMutationStagingErrorV1::Overflow)?;
        let key =
            DraftMutationStagingProgressReceiptKeyV1::new(head.identity(), transition).unwrap();
        let provisional_receipt = DraftMutationStagingProgressReceiptReferenceV1::new(
            head.identity(),
            transition,
            DraftPieceDigestV1::from_bytes([0; 32]),
        )
        .unwrap();
        let lifecycle = DraftMutationStagingLifecycleV1::Building(build_endpoint);
        let mut target_head = DraftMutationStagingHeadV1::from_parts(
            head.identity(),
            head.begin(),
            head.begin_digest(),
            head.source(),
            head.proposal(),
            provisional_receipt,
            lifecycle,
            DraftPieceDigestV1::from_bytes([0; 32]),
        );
        let digest = head_digest(target_head.clone())?;
        let receipt = make_receipt(
            key,
            Some(head.receipt()),
            DraftMutationStagingCommandKindV1::Transfer,
            None,
            Some(finish_digest(finish)),
            head.source(),
            head.source(),
            head.proposal(),
            head.proposal(),
            Some(head.digest()),
            digest,
            Some(head.lifecycle()),
            lifecycle,
            DraftMutationStagingCustodyTagV1::Staging,
            DraftMutationStagingCustodyTagV1::Building,
            Some(build_endpoint),
            None,
        )?;
        let receipt_ref = DraftMutationStagingProgressReceiptReferenceV1::new(
            head.identity(),
            transition,
            receipt.digest(),
        )
        .unwrap();
        target_head = DraftMutationStagingHeadV1::from_parts(
            head.identity(),
            head.begin(),
            head.begin_digest(),
            head.source(),
            head.proposal(),
            receipt_ref,
            lifecycle,
            digest,
        );
        Ok(PreparedDraftMutationTransferV1 {
            source_head: head.clone(),
            target_head,
            receipt,
            source_session: session.clone(),
            target_session,
            prepared_edit,
            build,
            build_receipt,
        })
    }
}
