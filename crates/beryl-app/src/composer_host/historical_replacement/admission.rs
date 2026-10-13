use super::*;

impl SyndicComposerHost {
    pub fn begin_historical_replacement(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        target: ComposerHostHistoricalTarget,
        operation: DraftPieceOperationIdV1,
    ) -> Result<ComposerHostHistoricalTicket, ComposerHostError> {
        self.historical_admission(store, binding)?;
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if self.is_dirty()
            || active.thread_id != target.thread_id
            || active.submission_intent != Some(DraftSubmissionIntent::Ordinary)
            || binding.root().summary().logical_utf8_bytes() != 0
            || binding.root().summary().marker_count() != 0
        {
            return Err(DraftReplacementErrorV1::Ineligible.into());
        }
        let thread = self
            .storage
            .thread(store, target.thread_id, point_limit())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let item = self
            .storage
            .canonical_item(store, target.item_id, point_limit())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        if item.turn_id() != target.turn_id || item.kind() != CanonicalItemKind::UserInput {
            return Err(DraftReplacementErrorV1::Ineligible.into());
        }
        let content = item
            .presentation()
            .content()
            .ok_or(DraftReplacementErrorV1::Ineligible)?;
        let assets = item.presentation().asset_reference_set();
        let gate = self
            .storage
            .input_gate(store, target.thread_id, point_limit())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let intent = ReplacementEditIntent::new(
            target.turn_id,
            SelectedPathProof::new(
                thread.committed_tail(),
                thread.revision(),
                thread.selected_path_digest(),
            ),
            target.transcript,
        );
        let pieces = self.storage.content_pieces(
            store,
            content.id(),
            None,
            beryl_home_store::CursorReadLimits::new(1, 4_096).unwrap(),
        )?;
        let position = DraftCompositePositionV1::new(
            0,
            match pieces.records().first() {
                Some(ContentPieceRecord::ImageMarker {
                    logical_offset: 0, ..
                }) => DraftCompositeGapWitnessV1::BeforeAll,
                _ => DraftCompositeGapWitnessV1::Unambiguous,
            },
        );
        let empty = DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::Unambiguous);
        let import = self.storage.prepare_replacement_draft_import(
            store,
            DraftReplacementImportRequestV1::new(
                active.durable_selector,
                active.storage_candidate,
                operation,
                intent,
                target.item_id,
                item.revision(),
                content,
                gate.revision(),
                assets,
                empty,
                empty,
                position,
                position,
            ),
        )?;
        self.install_historical_operation(binding, Some(import), position, HistoricalStage::Begin)
    }

    pub fn begin_historical_replacement_cancellation(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        operation: DraftPieceOperationIdV1,
        at: SyndicTimestamp,
    ) -> Result<ComposerHostHistoricalTicket, ComposerHostError> {
        self.historical_admission(store, binding)?;
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        let Some(DraftSubmissionIntent::Replacement(intent)) = active.submission_intent else {
            return Err(DraftReplacementErrorV1::Ineligible.into());
        };
        let prepared = self.storage.prepare_replacement_intent_cancellation(
            store,
            DraftReplacementCancellationRequestV1::new(
                active.durable_selector,
                active.storage_candidate.session_id(),
                active.storage_candidate.session_generation(),
                operation,
                intent,
                at,
            ),
        )?;
        self.install_historical_operation(
            binding,
            None,
            DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::Unambiguous),
            HistoricalStage::Cancel {
                prepared,
                executed: false,
            },
        )
    }

    fn install_historical_operation(
        &mut self,
        binding: ComposerHostBinding,
        import: Option<PreparedDraftReplacementImportV1>,
        position: DraftCompositePositionV1,
        stage: HistoricalStage,
    ) -> Result<ComposerHostHistoricalTicket, ComposerHostError> {
        if self
            .historical_preflush
            .as_ref()
            .is_some_and(|preflush| preflush.successor != Some(binding))
        {
            return Err(ComposerHostError::OldBinding);
        }
        let generation = self
            .historical_generation
            .checked_add(1)
            .ok_or(ComposerHostError::GenerationExhausted)?;
        let ticket = ComposerHostHistoricalTicket {
            binding,
            generation,
        };
        self.historical_generation = generation;
        let prior_publication_binding = self
            .historical_preflush
            .take()
            .filter(|preflush| preflush.successor == Some(binding))
            .map_or(binding, |preflush| preflush.prior);
        self.historical_replacement = Some(Box::new(PendingHistoricalReplacement {
            prior_publication_binding,
            ticket,
            import,
            position,
            stage,
            cancelled: false,
            assets: None,
            at: None,
        }));
        Ok(ticket)
    }
}
