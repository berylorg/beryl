use super::*;

impl SyndicStorage {
    pub fn prepare_draft_mutation_staging_begin(
        &self,
        begin: DraftMutationBeginV1,
        session: &DraftEditorCandidateSessionV1,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        let identity = begin.identity();
        if identity.draft_id() != session.draft_id()
            || identity.session_id() != session.session_id()
            || begin.session_generation() != session.session_generation()
            || begin.predecessor_candidate_generation() != session.newest_candidate_generation()
            || begin.predecessor_root() != session.newest_root()
            || begin.predecessor_history() != session.newest_history()
            || begin.predecessor_extent() != session.logical_extent()
            || session.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
            || session.active_operation().is_some()
            || !session.is_coherent()
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
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
        let receipt_key = DraftMutationStagingProgressReceiptKeyV1::new(identity, 1).unwrap();
        let placeholder_ref = DraftMutationStagingProgressReceiptReferenceV1::new(
            identity,
            1,
            DraftPieceDigestV1::from_bytes([0; 32]),
        )
        .unwrap();
        let mut target_head = DraftMutationStagingHeadV1::from_parts(
            identity,
            begin,
            begin_digest,
            source,
            proposal,
            placeholder_ref,
            DraftMutationStagingLifecycleV1::Receiving,
            DraftPieceDigestV1::from_bytes([0; 32]),
        );
        let target_head_digest = head_digest(target_head.clone())?;
        let receipt = make_receipt(
            receipt_key,
            None,
            DraftMutationStagingCommandKindV1::Begin,
            None,
            None,
            source,
            source,
            proposal,
            proposal,
            None,
            target_head_digest,
            None,
            DraftMutationStagingLifecycleV1::Receiving,
            DraftMutationStagingCustodyTagV1::None,
            DraftMutationStagingCustodyTagV1::Staging,
            None,
            None,
        )?;
        let receipt_ref =
            DraftMutationStagingProgressReceiptReferenceV1::new(identity, 1, receipt.digest())
                .unwrap();
        target_head = DraftMutationStagingHeadV1::from_parts(
            identity,
            begin,
            begin_digest,
            source,
            proposal,
            receipt_ref,
            DraftMutationStagingLifecycleV1::Receiving,
            target_head_digest,
        );
        let target_session = session
            .with_active_operation(staging_custody(begin, begin_digest, receipt_ref))
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        Ok(PreparedDraftMutationStagingCommandV1 {
            source_head: None,
            target_head,
            source_session: session.clone(),
            target_session: Some(target_session),
            receipt,
        })
    }

    pub(super) fn prepare_draft_mutation_staging_page_step(
        &self,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
        lane: DraftMutationStagingLaneV1,
        input_cursor: u64,
        successor_cursor: u64,
        item_ceiling: u16,
        byte_ceiling: u32,
        items: Box<[DraftMutationStagingPageItemV1]>,
    ) -> Result<
        (
            DraftMutationStagingHeadV1,
            DraftEditorCandidateSessionV1,
            PreparedDraftMutationStagingBatchTargetV1,
            usize,
        ),
        DraftMutationStagingErrorV1,
    > {
        if head.lifecycle() != DraftMutationStagingLifecycleV1::Receiving
            || items.is_empty()
            || items.len() > DRAFT_PIECE_PAGE_MAX_RECORDS
            || item_ceiling == 0
            || usize::from(item_ceiling) > DRAFT_PIECE_PAGE_MAX_RECORDS
            || items.len() > usize::from(item_ceiling)
            || byte_ceiling == 0
            || usize::try_from(byte_ceiling)
                .ok()
                .is_none_or(|limit| limit > DRAFT_PIECE_PAGE_MAX_BYTES)
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        if items.iter().any(|item| {
            !matches!(
                (lane, item),
                (
                    DraftMutationStagingLaneV1::Source,
                    DraftMutationStagingPageItemV1::SourcePosition(_)
                ) | (
                    DraftMutationStagingLaneV1::Proposal,
                    DraftMutationStagingPageItemV1::Proposal(_)
                )
            )
        }) {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        for item in items.iter() {
            if let DraftMutationStagingPageItemV1::Proposal(replacement) = item {
                validate_fragment(replacement).map_err(|_| DraftMutationStagingErrorV1::Invalid)?;
            }
        }
        let before = match lane {
            DraftMutationStagingLaneV1::Source => head.source(),
            DraftMutationStagingLaneV1::Proposal => head.proposal(),
        };
        if input_cursor != before.next_cursor() || successor_cursor <= input_cursor {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let item_bytes = canonical_staging_items_bytes(&items)
            .map_err(|_| DraftMutationStagingErrorV1::Invalid)?;
        if item_bytes.len() > usize::try_from(byte_ceiling).unwrap() {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let item_total = before
            .item_total()
            .checked_add(
                u64::try_from(items.len()).map_err(|_| DraftMutationStagingErrorV1::Overflow)?,
            )
            .ok_or(DraftMutationStagingErrorV1::Overflow)?;
        let byte_total = before
            .canonical_byte_total()
            .checked_add(
                u64::try_from(item_bytes.len())
                    .map_err(|_| DraftMutationStagingErrorV1::Overflow)?,
            )
            .ok_or(DraftMutationStagingErrorV1::Overflow)?;
        let key = DraftMutationStagingPageKeyV1::new(head.identity(), lane, before.next_ordinal())
            .unwrap();
        let transition = head
            .receipt()
            .transition_ordinal()
            .checked_add(1)
            .ok_or(DraftMutationStagingErrorV1::Overflow)?;
        let digest = page_digest(
            key,
            transition,
            before.next_cursor(),
            successor_cursor,
            item_ceiling,
            byte_ceiling,
            before.cumulative_identity(),
            item_total,
            byte_total,
            &item_bytes,
        );
        let cumulative = cumulative_identity(before.cumulative_identity(), digest);
        let page = DraftMutationStagingPageV1::from_parts(
            key,
            transition,
            before.next_cursor(),
            successor_cursor,
            item_ceiling,
            byte_ceiling,
            before.cumulative_identity(),
            cumulative,
            item_total,
            byte_total,
            items,
            digest,
        );
        let encoded_page_bytes = canonical_staging_page_bytes(&page)
            .map_err(|_| DraftMutationStagingErrorV1::Invalid)?
            .len();
        if encoded_page_bytes > DRAFT_PIECE_PAGE_MAX_BYTES {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let after = DraftMutationStagingLaneFrontierV1::new(
            successor_cursor,
            before
                .next_ordinal()
                .checked_add(1)
                .ok_or(DraftMutationStagingErrorV1::Overflow)?,
            item_total,
            byte_total,
            cumulative,
        )
        .unwrap();
        let key_receipt =
            DraftMutationStagingProgressReceiptKeyV1::new(head.identity(), transition).unwrap();
        let placeholder_ref = DraftMutationStagingProgressReceiptReferenceV1::new(
            head.identity(),
            transition,
            DraftPieceDigestV1::from_bytes([0; 32]),
        )
        .unwrap();
        let (source, proposal) = match lane {
            DraftMutationStagingLaneV1::Source => (after, head.proposal()),
            DraftMutationStagingLaneV1::Proposal => (head.source(), after),
        };
        let mut target = DraftMutationStagingHeadV1::from_parts(
            head.identity(),
            head.begin(),
            head.begin_digest(),
            source,
            proposal,
            placeholder_ref,
            DraftMutationStagingLifecycleV1::Receiving,
            DraftPieceDigestV1::from_bytes([0; 32]),
        );
        let final_digest = head_digest(target.clone())?;
        let command = match lane {
            DraftMutationStagingLaneV1::Source => DraftMutationStagingCommandKindV1::SourcePage,
            DraftMutationStagingLaneV1::Proposal => DraftMutationStagingCommandKindV1::ProposalPage,
        };
        let receipt = make_receipt(
            key_receipt,
            Some(head.receipt()),
            command,
            Some((page.key(), page.digest())),
            None,
            head.source(),
            source,
            head.proposal(),
            proposal,
            Some(head.digest()),
            final_digest,
            Some(head.lifecycle()),
            target.lifecycle(),
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
            source,
            proposal,
            target_ref,
            DraftMutationStagingLifecycleV1::Receiving,
            final_digest,
        );
        let expected = staging_custody(head.begin(), head.begin_digest(), head.receipt());
        let next_custody = staging_custody(head.begin(), head.begin_digest(), target_ref);
        let target_session = session
            .advance_active_operation(&expected, next_custody)
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        Ok((
            target,
            target_session,
            PreparedDraftMutationStagingBatchTargetV1 { page, receipt },
            encoded_page_bytes,
        ))
    }
}
