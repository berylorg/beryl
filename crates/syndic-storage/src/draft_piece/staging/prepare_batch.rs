use super::*;

impl SyndicStorage {
    pub fn prepare_draft_mutation_staging_page_batch(
        &self,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
        inputs: Box<[DraftMutationStagingPageInputV1]>,
    ) -> Result<PreparedDraftMutationStagingBatchV1, DraftMutationStagingErrorV1> {
        if inputs.is_empty() || inputs.len() > DRAFT_MUTATION_STAGING_BATCH_MAX_PAGES {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        if !draft_mutation_staging_head_is_locally_exact(head)
            || !staging_session_matches_head(session, head)
            || session.active_operation()
                != Some(&staging_custody(
                    head.begin(),
                    head.begin_digest(),
                    head.receipt(),
                ))
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let lane = inputs[0].lane();
        if inputs.iter().any(|input| input.lane() != lane) {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }

        let source_head = head.clone();
        let source_session = session.clone();
        let mut derived_head = source_head.clone();
        let mut derived_session = source_session.clone();
        let mut item_count = 0usize;
        let mut encoded_page_bytes = 0usize;
        let mut targets = Vec::with_capacity(inputs.len());
        for input in inputs.into_vec() {
            item_count = item_count
                .checked_add(input.items().len())
                .ok_or(DraftMutationStagingErrorV1::Overflow)?;
            if item_count > DRAFT_MUTATION_STAGING_BATCH_MAX_ITEMS {
                return Err(DraftMutationStagingErrorV1::Invalid);
            }
            let lane = input.lane();
            let input_cursor = input.input_cursor();
            let successor_cursor = input.successor_cursor();
            let item_ceiling = input.item_ceiling();
            let byte_ceiling = input.byte_ceiling();
            let (target_head, target_session, target, page_bytes) = self
                .prepare_draft_mutation_staging_page_step(
                    &derived_head,
                    &derived_session,
                    lane,
                    input_cursor,
                    successor_cursor,
                    item_ceiling,
                    byte_ceiling,
                    input.into_items(),
                )?;
            encoded_page_bytes = encoded_page_bytes
                .checked_add(page_bytes)
                .ok_or(DraftMutationStagingErrorV1::Overflow)?;
            if encoded_page_bytes > DRAFT_MUTATION_STAGING_BATCH_MAX_BYTES {
                return Err(DraftMutationStagingErrorV1::Invalid);
            }
            derived_head = target_head;
            derived_session = target_session;
            targets.push(target);
        }

        Ok(PreparedDraftMutationStagingBatchV1 {
            source_head,
            target_head: derived_head,
            source_session,
            target_session: derived_session,
            targets: targets.into_boxed_slice(),
            item_count,
            encoded_page_bytes,
        })
    }

    pub fn draft_mutation_staging_page_batch(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMutationStagingBatchV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, StagingBatchMutation { prepared })
    }
}
