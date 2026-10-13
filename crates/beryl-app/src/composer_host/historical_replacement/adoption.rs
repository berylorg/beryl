use super::*;

impl SyndicComposerHost {
    pub(super) fn adopt_historical_start(
        &mut self,
        store: &HomeStore,
        previous: ComposerHostBinding,
        prepared: &PreparedDraftReplacementStartV1,
        selector: DraftEditorCurrentSelectorV1,
        position: DraftCompositePositionV1,
    ) -> Result<ComposerHostHistoricalCompletion, ComposerHostError> {
        super::super::request::validate_store(previous, store)?;
        let DraftPieceSettlementClosureV1::SealedImport(selection) =
            prepared.settlement().closure()
        else {
            return Err(DraftReplacementErrorV1::Conflict.into());
        };
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != previous
            || active.durable_selector != selection.prior_selector()
            || selection.successor_selector() != selector
            || selection.after_session().session_id() != active.storage_candidate.session_id()
            || selection.after_session().draft_id() != active.storage_candidate.draft_id()
        {
            return Err(ComposerHostError::OldBinding);
        }
        let head = selection.after_session();
        let candidate = DraftEditorCandidateActivationBindingV1::from_head(head);
        let binding = ComposerHostBinding::new(
            previous.home_id(),
            previous.home_generation(),
            previous.host_generation(),
            candidate,
            previous.presentation_generation(),
        );
        let caret = self.history_position(store, binding, position)?;
        let intent = DraftSubmissionIntent::Replacement(selection.basis().intent());
        let active = self.active.as_mut().unwrap();
        active.binding = binding;
        active.storage_candidate = candidate;
        active.durable_selector = selector;
        active.submission_intent = Some(intent);
        active.published_candidate_generation = head.published_candidate_generation();
        active.published_pair =
            DraftRootHistoryPairV1::new(head.published_root(), head.published_history());
        active.initial_responses.clear();
        self.pending.clear();
        self.last_mutation_identity = None;
        self.last_history_identity = None;
        self.last_history_outcome = None;
        Ok(ComposerHostHistoricalCompletion {
            previous,
            binding,
            intent,
            caret: Some(caret),
            selection: Some(caret),
        })
    }

    pub(super) fn adopt_historical_cancellation(
        &mut self,
        previous: ComposerHostBinding,
        prepared: &PreparedDraftReplacementCancellationV1,
        selector: DraftEditorCurrentSelectorV1,
    ) -> Result<ComposerHostHistoricalCompletion, ComposerHostError> {
        let receipt = prepared.receipt();
        let active = self.active.as_mut().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != previous
            || receipt.selector() != selector
            || DraftEditorCandidateActivationBindingV1::from_head(receipt.before_head())
                != active.storage_candidate
        {
            return Err(ComposerHostError::OldBinding);
        }
        active.storage_candidate =
            DraftEditorCandidateActivationBindingV1::from_head(receipt.after_head());
        active.binding = ComposerHostBinding::new(
            previous.home_id(),
            previous.home_generation(),
            previous.host_generation(),
            active.storage_candidate,
            previous.presentation_generation(),
        );
        active.durable_selector = selector;
        active.submission_intent = Some(DraftSubmissionIntent::Ordinary);
        Ok(ComposerHostHistoricalCompletion {
            previous,
            binding: active.binding,
            intent: DraftSubmissionIntent::Ordinary,
            caret: None,
            selection: None,
        })
    }

    pub(super) fn adopt_historical_termination(
        &mut self,
        previous: ComposerHostBinding,
        prepared: &PreparedDraftReplacementImportTerminationV1,
    ) -> Result<ComposerHostHistoricalCompletion, ComposerHostError> {
        let DraftPieceSettlementClosureV1::Noncommit(noncommit) = prepared.settlement().closure()
        else {
            return Err(DraftReplacementErrorV1::Conflict.into());
        };
        let active = self.active.as_mut().ok_or(ComposerHostError::OldBinding)?;
        let head = noncommit.observed_session();
        let intent = active
            .submission_intent
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        if active.binding != previous
            || head.session_id() != active.storage_candidate.session_id()
            || head.draft_id() != active.storage_candidate.draft_id()
            || head.newest_root() != active.storage_candidate.root()
            || head.newest_history() != active.storage_candidate.history()
        {
            return Err(ComposerHostError::OldBinding);
        }
        active.storage_candidate = DraftEditorCandidateActivationBindingV1::from_head(head);
        active.binding = ComposerHostBinding::new(
            previous.home_id(),
            previous.home_generation(),
            previous.host_generation(),
            active.storage_candidate,
            previous.presentation_generation(),
        );
        Ok(ComposerHostHistoricalCompletion {
            previous,
            binding: active.binding,
            intent,
            caret: None,
            selection: None,
        })
    }
}
