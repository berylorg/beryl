use super::*;

impl SyndicStorage {
    pub fn prepare_draft_mutation_staging_finish(
        &self,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
        finish: DraftMutationFinishInputV1,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        if head.lifecycle() != DraftMutationStagingLifecycleV1::Receiving
            || finish.source() != head.source()
            || finish.proposal() != head.proposal()
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
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
        let lifecycle = DraftMutationStagingLifecycleV1::Finished(finish);
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
        let final_digest = head_digest(target.clone())?;
        let finish_digest = finish_digest(finish);
        let receipt = make_receipt(
            key,
            Some(head.receipt()),
            DraftMutationStagingCommandKindV1::Finish,
            None,
            Some(finish_digest),
            head.source(),
            head.source(),
            head.proposal(),
            head.proposal(),
            Some(head.digest()),
            final_digest,
            Some(head.lifecycle()),
            lifecycle,
            DraftMutationStagingCustodyTagV1::Staging,
            DraftMutationStagingCustodyTagV1::Staging,
            None,
            None,
        )?;
        let target_ref = DraftMutationStagingProgressReceiptReferenceV1::new(
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
            target_ref,
            lifecycle,
            final_digest,
        );
        let expected = staging_custody(head.begin(), head.begin_digest(), head.receipt());
        let next = staging_custody(head.begin(), head.begin_digest(), target_ref);
        let target_session = session
            .advance_active_operation(&expected, next)
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        Ok(PreparedDraftMutationStagingCommandV1 {
            source_head: Some(head.clone()),
            target_head: target,
            source_session: session.clone(),
            target_session: Some(target_session),
            page: None,
            receipt,
        })
    }

    pub fn prepare_draft_mutation_terminal_before_begin(
        &self,
        begin: DraftMutationBeginV1,
        session: &DraftEditorCandidateSessionV1,
        evidence: DraftMutationStagingTerminalEvidenceV1,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        let identity = begin.identity();
        let is_conflict = matches!(
            evidence,
            DraftMutationStagingTerminalEvidenceV1::Conflict { .. }
        );
        if identity.draft_id() != session.draft_id()
            || identity.session_id() != session.session_id()
            || session.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
            || session.active_operation().is_some()
            || !session.is_coherent()
            || !terminal_evidence_matches(begin, None, session, evidence, None)
            || (!is_conflict
                && (begin.session_generation() != session.session_generation()
                    || begin.predecessor_candidate_generation()
                        != session.newest_candidate_generation()
                    || begin.predecessor_root() != session.newest_root()
                    || begin.predecessor_history() != session.newest_history()
                    || begin.predecessor_extent() != session.logical_extent()))
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let lifecycle = terminal_lifecycle(evidence);
        let source = DraftMutationStagingLaneFrontierV1::new(
            begin.source_initial_cursor(),
            1,
            0,
            0,
            empty_lane_identity(DraftMutationStagingLaneV1::Source),
        )
        .unwrap();
        let proposal = DraftMutationStagingLaneFrontierV1::new(
            begin.proposal_initial_cursor(),
            1,
            0,
            0,
            empty_lane_identity(DraftMutationStagingLaneV1::Proposal),
        )
        .unwrap();
        let begin_digest = begin_digest(begin);
        let key = DraftMutationStagingProgressReceiptKeyV1::new(identity, 1).unwrap();
        let provisional_receipt = DraftMutationStagingProgressReceiptReferenceV1::new(
            identity,
            1,
            DraftPieceDigestV1::from_bytes([0; 32]),
        )
        .unwrap();
        let mut target = DraftMutationStagingHeadV1::from_parts(
            identity,
            begin,
            begin_digest,
            source,
            proposal,
            provisional_receipt,
            lifecycle,
            DraftPieceDigestV1::from_bytes([0; 32]),
        );
        let digest = head_digest(target.clone())?;
        let receipt = make_receipt(
            key,
            None,
            DraftMutationStagingCommandKindV1::Terminal,
            None,
            None,
            source,
            source,
            proposal,
            proposal,
            None,
            digest,
            None,
            lifecycle,
            DraftMutationStagingCustodyTagV1::None,
            DraftMutationStagingCustodyTagV1::None,
            None,
            Some(evidence),
        )?;
        let receipt_ref =
            DraftMutationStagingProgressReceiptReferenceV1::new(identity, 1, receipt.digest())
                .unwrap();
        target = DraftMutationStagingHeadV1::from_parts(
            identity,
            begin,
            begin_digest,
            source,
            proposal,
            receipt_ref,
            lifecycle,
            digest,
        );
        Ok(PreparedDraftMutationStagingCommandV1 {
            source_head: None,
            target_head: target,
            source_session: session.clone(),
            target_session: None,
            page: None,
            receipt,
        })
    }
}
