use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftLogicalExtentV1, DraftMutationStagingPageItemV1,
    DraftMutationStagingTerminalEvidenceV1, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1,
};

use super::translation::{
    TranslatedWidgetPage, translate_widget_page, validate_marker_metadata_intake,
};
use super::*;

enum MutationBeginReadiness {
    Ordinary,
    #[cfg(feature = "test-faults")]
    Marker(syndic_storage::DraftMarkerLabelReadinessProofV1),
}

impl SyndicComposerHost {
    pub const fn mutation_status(&self) -> Option<ComposerHostMutationStatus> {
        match self.pending_mutation {
            Some(ComposerHostPendingMutation::Active(ref pending)) => match pending.phase {
                ComposerHostMutationPhase::Receiving => Some(ComposerHostMutationStatus::Admitted),
                ComposerHostMutationPhase::Finished => Some(ComposerHostMutationStatus::Admitted),
                ComposerHostMutationPhase::Building { .. } => {
                    Some(ComposerHostMutationStatus::Admitted)
                }
            },
            Some(ComposerHostPendingMutation::Terminal(_)) => {
                Some(ComposerHostMutationStatus::Admitted)
            }
            Some(ComposerHostPendingMutation::Unavailable(_)) => {
                Some(ComposerHostMutationStatus::Unavailable)
            }
            None => None,
        }
    }

    pub const fn retained_mutation_intent(&self) -> Option<&ComposerHostRetainedMutationIntent> {
        match &self.pending_mutation {
            Some(ComposerHostPendingMutation::Unavailable(intent)) => Some(intent),
            _ => None,
        }
    }

    pub fn begin_mutation(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        request: MutationBeginRequest,
    ) -> Result<(), ComposerHostError> {
        self.begin_mutation_with_readiness(
            store,
            binding,
            request,
            MutationBeginReadiness::Ordinary,
        )
    }

    #[cfg(feature = "test-faults")]
    pub fn test_begin_marker_mutation(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        request: MutationBeginRequest,
        readiness: syndic_storage::DraftMarkerLabelReadinessProofV1,
    ) -> Result<(), ComposerHostError> {
        self.begin_mutation_with_readiness(
            store,
            binding,
            request,
            MutationBeginReadiness::Marker(readiness),
        )
    }

    fn begin_mutation_with_readiness(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        request: MutationBeginRequest,
        readiness: MutationBeginReadiness,
    ) -> Result<(), ComposerHostError> {
        if self.lifecycle.freezes_admission() {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        if self.submission_pending() {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        if self.pending_history.is_some() {
            return Err(ComposerHostError::HistoryPending);
        }
        if self.pending_mutation.is_some() {
            return Err(match self.pending_mutation {
                Some(ComposerHostPendingMutation::Unavailable(_)) => {
                    ComposerHostError::MutationUnavailable
                }
                _ => ComposerHostError::MutationPending,
            });
        }
        self.reserve_settlement_custody()?;
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != binding {
            return Err(ComposerHostError::OldBinding);
        }
        if active.unavailable || active.session_disposed {
            return Err(ComposerHostError::HistoryUnavailable);
        }
        validate_store(binding, store)?;
        validate_begin_key(binding, request)?;
        if request.proposal().kind() != MutationKind::Edit {
            return Err(ComposerHostError::MutationUnavailable);
        }
        let identity = ComposerHostMutationIdentity::new(request);
        let session = match self.storage.draft_editor_candidate_session(
            store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
            _ => return Err(ComposerHostError::MutationMalformed),
        };
        let proposal = request.proposal();
        let predecessor = proposal.predecessor();
        let storage_identity = DraftMutationStagingIdentityV1::new(
            session.draft_id(),
            session.session_id(),
            operation_id(proposal.key().operation().get()),
        );
        let candidate = binding.candidate();
        let storage_begin = DraftMutationBeginV1::new(
            storage_identity,
            candidate.session_generation(),
            candidate.candidate_generation(),
            candidate.root(),
            candidate.history(),
            candidate.logical_extent(),
            canonical_position(predecessor.caret())?,
            canonical_position(predecessor.selection_anchor())?,
            canonical_position(predecessor.selection_head())?,
            canonical_position(proposal.replacement().start())?,
            canonical_position(proposal.replacement().end())?,
            request.source_cursor().get(),
            request.proposal_cursor().get(),
        );
        if DraftEditorCandidateActivationBindingV1::from_head(&session) != candidate {
            let evidence = DraftMutationStagingTerminalEvidenceV1::Conflict {
                expected_generation: candidate.candidate_generation(),
                expected_root: candidate.root(),
                expected_history: candidate.history(),
                observed_generation: session.newest_candidate_generation(),
                observed_root: session.newest_root(),
                observed_history: session.newest_history(),
                session_revision: session.session_generation(),
            };
            let prepared = self.storage.prepare_draft_mutation_terminal_before_begin(
                storage_begin,
                &session,
                evidence,
            )?;
            if !matches!(
                self.run_staging_command(store, &prepared, None)?,
                StagingCommandResult::Target | StagingCommandResult::Terminal
            ) {
                return Err(ComposerHostError::MutationWorkPending);
            }
            self.last_mutation_identity = Some(Box::new((binding, identity)));
            self.pending_mutation = Some(ComposerHostPendingMutation::Terminal(Box::new(
                ComposerHostTerminalMutation {
                    binding,
                    key: proposal.key(),
                    outcome: ComposerHostMutationOutcome::Conflict,
                    detached: false,
                },
            )));
            return Ok(());
        }
        if let Some((last_binding, last)) = self.last_mutation_identity.as_deref()
            && *last_binding == binding
        {
            match identity.operation().cmp(&last.operation()) {
                Ordering::Less => return Err(ComposerHostError::StaleRequestIdentity),
                Ordering::Equal if last != &identity => {
                    return Err(ComposerHostError::MutationIdentityCollision);
                }
                Ordering::Equal | Ordering::Greater => {}
            }
        }
        let prepared = match readiness {
            MutationBeginReadiness::Ordinary => self
                .storage
                .prepare_draft_mutation_staging_begin(storage_begin, &session)?,
            #[cfg(feature = "test-faults")]
            MutationBeginReadiness::Marker(readiness) => self
                .storage
                .prepare_draft_mutation_staging_marker_begin(storage_begin, &session, readiness)?,
        };
        let command_result = self.run_staging_command(store, &prepared, None)?;
        if !matches!(command_result, StagingCommandResult::Target) {
            return Err(ComposerHostError::MutationMalformed);
        }
        let target_session = prepared
            .target_session()
            .cloned()
            .ok_or(ComposerHostError::MutationMalformed)?;
        let head = prepared.target_head().clone();
        self.last_mutation_identity = Some(Box::new((binding, identity)));
        self.pending_mutation = Some(ComposerHostPendingMutation::Active(Box::new(
            ComposerHostMutationCoordinator {
                binding,
                begin: request,
                identity: storage_identity,
                session: target_session,
                head,
                source: WidgetLaneFrontier::initial(request.source_cursor()),
                proposal: WidgetLaneFrontier::initial(request.proposal_cursor()),
                phase: ComposerHostMutationPhase::Receiving,
                fragment_count: 0,
                fragment_chain: canonical_empty_draft_piece_fragment_chain_v1(),
                proposal_envelope_applied: false,
                last_proposal_range: None,
                remaining_proposal_range: request.proposal().replacement(),
                in_flight_page: None,
                finish_input: None,
                intended: None,
                detached: false,
            },
        )));
        Ok(())
    }

    pub fn stage_mutation_page(
        &mut self,
        store: &HomeStore,
        request: MutationPageRequest,
        marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
    ) -> Result<MutationPageAcceptance, ComposerHostError> {
        let mut pending = self.take_active_mutation()?;
        let result = (|| {
            self.validate_mutation_store(pending.binding, store)?;
            if !matches!(pending.phase, ComposerHostMutationPhase::Receiving)
                || request.page().key().key() != pending.begin.proposal().key()
            {
                return Err(ComposerHostError::MutationMalformed);
            }
            if let Some(in_flight) = &pending.in_flight_page {
                let ComposerHostInFlightPageKind::Widget {
                    request: retained, ..
                } = &in_flight.kind
                else {
                    return Err(ComposerHostError::MutationPending);
                };
                if *retained != request {
                    return Err(ComposerHostError::MutationPending);
                }
                return self
                    .drive_in_flight_page(store, &mut pending)?
                    .ok_or(ComposerHostError::MutationMalformed);
            }
            validate_marker_metadata_intake(request.page(), &marker_metadata)?;
            let lane = request.page().key().lane();
            let frontier = pending.lane(lane);
            let (next_frontier, acceptance) = match frontier.prevalidate(request.page())? {
                WidgetPageDisposition::Replay => return Ok(MutationPageAcceptance::Replay),
                WidgetPageDisposition::Accepted {
                    frontier,
                    acceptance,
                } => (frontier, acceptance),
            };
            let translated = translate_widget_page(
                &self.storage,
                store,
                &pending,
                request.page(),
                &marker_metadata,
            )?;
            pending.in_flight_page = Some(self.prepare_translated_page(
                &pending,
                request,
                next_frontier,
                acceptance,
                translated,
            )?);
            self.drive_in_flight_page(store, &mut pending)?
                .ok_or(ComposerHostError::MutationMalformed)
        })();
        self.restore_active_mutation(pending);
        result
    }

    pub fn finish_mutation_input(
        &mut self,
        store: &HomeStore,
        finish: MutationFinishInput,
    ) -> Result<(), ComposerHostError> {
        let mut pending = self.take_active_mutation()?;
        let result = (|| {
            self.validate_mutation_store(pending.binding, store)?;
            if !matches!(pending.phase, ComposerHostMutationPhase::Receiving)
                || finish.key() != pending.begin.proposal().key()
                || !pending.source.matches_finish(finish.source())
                || !pending.proposal.matches_finish(finish.proposal())
            {
                return Err(ComposerHostError::MutationMalformed);
            }
            let finish = pending.retain_finish_input(finish)?;
            if let Some(in_flight) = &pending.in_flight_page {
                if !matches!(
                    &in_flight.kind,
                    ComposerHostInFlightPageKind::Internal { finish: retained } if *retained == finish
                ) {
                    return Err(ComposerHostError::MutationMalformed);
                }
                if self.drive_in_flight_page(store, &mut pending)?.is_some() {
                    return Err(ComposerHostError::MutationMalformed);
                }
            }
            if pending.fragment_count == 0 {
                let replacement = syndic_storage::DraftPieceReplacementV1::new(
                    canonical_position(pending.begin.proposal().replacement().start())?,
                    canonical_position(pending.begin.proposal().replacement().end())?,
                    Vec::new(),
                );
                let chain =
                    canonical_draft_piece_fragment_chain_v1(std::slice::from_ref(&replacement));
                let translated = TranslatedWidgetPage::proposal(
                    vec![DraftMutationStagingPageItemV1::Proposal(replacement)],
                    1,
                    chain,
                    true,
                    Some((
                        canonical_position(pending.begin.proposal().replacement().start())?,
                        canonical_position(pending.begin.proposal().replacement().end())?,
                    )),
                    pending.remaining_proposal_range,
                );
                self.stage_internal_translated_page(store, &mut pending, finish, translated)?;
            }
            let intended_extent = DraftLogicalExtentV1::new(
                finish.intended_extent().byte_len(),
                finish.intended_extent().line_count(),
            );
            let intended = finish.intended();
            let storage_finish = DraftMutationFinishInputV1::new(
                pending.head.source(),
                pending.head.proposal(),
                intended_extent,
                canonical_position(intended.caret())?,
                canonical_position(intended.selection_anchor())?,
                canonical_position(intended.selection_head())?,
                pending.fragment_chain,
            );
            let prepared = self.storage.prepare_draft_mutation_staging_finish(
                &pending.head,
                &pending.session,
                storage_finish,
            )?;
            if !matches!(
                self.run_staging_command(store, &prepared, None)?,
                StagingCommandResult::Target
            ) {
                return Err(ComposerHostError::MutationMalformed);
            }
            pending.head = prepared.target_head().clone();
            pending.session = prepared
                .target_session()
                .cloned()
                .ok_or(ComposerHostError::MutationMalformed)?;
            pending.intended = Some(intended);
            pending.finish_input = None;
            pending.phase = ComposerHostMutationPhase::Finished;
            Ok(())
        })();
        self.restore_active_mutation(pending);
        result
    }

    pub fn execute_mutation(
        &mut self,
        store: &HomeStore,
        request: MutationCommitRequest,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        if self
            .detached_mutations
            .iter()
            .any(|pending| pending.key() == request.key())
        {
            return self.execute_detached_mutation(store, request, cancellation);
        }
        self.execute_current_mutation(store, request, cancellation)
    }

    fn execute_current_mutation(
        &mut self,
        store: &HomeStore,
        request: MutationCommitRequest,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        if matches!(
            self.pending_mutation,
            Some(ComposerHostPendingMutation::Terminal(_))
        ) {
            let Some(ComposerHostPendingMutation::Terminal(terminal)) =
                self.pending_mutation.take()
            else {
                unreachable!();
            };
            if (!terminal.detached
                && terminal.binding != self.binding().ok_or(ComposerHostError::OldBinding)?)
                || request.key() != terminal.key
            {
                self.pending_mutation = Some(ComposerHostPendingMutation::Terminal(terminal));
                return Err(ComposerHostError::RequestMismatch);
            }
            return Ok(terminal.outcome);
        }
        let mut pending = self.take_active_mutation()?;
        let result = self.execute_active_mutation(store, request, cancellation, &mut pending);
        match &result {
            Ok(ComposerHostMutationOutcome::Committed { .. })
            | Ok(ComposerHostMutationOutcome::Rejected)
            | Ok(ComposerHostMutationOutcome::Cancelled)
            | Ok(ComposerHostMutationOutcome::Error) => {}
            Ok(ComposerHostMutationOutcome::Conflict)
            | Err(ComposerHostError::MutationUnavailable) => {
                self.pending_mutation = Some(ComposerHostPendingMutation::Unavailable(Box::new(
                    pending.intent(),
                )));
            }
            Err(_) => self.restore_active_mutation(pending),
        }
        result
    }

    fn execute_detached_mutation(
        &mut self,
        store: &HomeStore,
        request: MutationCommitRequest,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let position = self
            .detached_mutations
            .iter()
            .position(|pending| pending.key() == request.key())
            .ok_or(ComposerHostError::MutationNotPending)?;
        let pending = self.detached_mutations.remove(position);
        match pending {
            ComposerHostPendingMutation::Terminal(terminal) => {
                if request.key() != terminal.key {
                    self.detached_mutations
                        .push(ComposerHostPendingMutation::Terminal(terminal));
                    return Err(ComposerHostError::RequestMismatch);
                }
                Ok(ComposerHostMutationOutcome::Conflict)
            }
            ComposerHostPendingMutation::Unavailable(_) => {
                Ok(ComposerHostMutationOutcome::Conflict)
            }
            ComposerHostPendingMutation::Active(mut pending) => {
                if request.key() != pending.begin.proposal().key() {
                    self.detached_mutations
                        .push(ComposerHostPendingMutation::Active(pending));
                    return Err(ComposerHostError::RequestMismatch);
                }
                let result =
                    self.execute_active_mutation(store, request, cancellation, &mut pending);
                match result {
                    Ok(_) | Err(ComposerHostError::MutationUnavailable) => {
                        Ok(ComposerHostMutationOutcome::Conflict)
                    }
                    Err(error) => {
                        self.detached_mutations
                            .push(ComposerHostPendingMutation::Active(pending));
                        Err(error)
                    }
                }
            }
        }
    }

    fn take_active_mutation(
        &mut self,
    ) -> Result<Box<ComposerHostMutationCoordinator>, ComposerHostError> {
        match self
            .pending_mutation
            .take()
            .ok_or(ComposerHostError::MutationNotPending)?
        {
            ComposerHostPendingMutation::Active(pending) => Ok(pending),
            ComposerHostPendingMutation::Terminal(terminal) => {
                self.pending_mutation = Some(ComposerHostPendingMutation::Terminal(terminal));
                Err(ComposerHostError::MutationUnavailable)
            }
            ComposerHostPendingMutation::Unavailable(intent) => {
                self.pending_mutation = Some(ComposerHostPendingMutation::Unavailable(intent));
                Err(ComposerHostError::MutationUnavailable)
            }
        }
    }

    fn restore_active_mutation(&mut self, pending: Box<ComposerHostMutationCoordinator>) {
        if self.pending_mutation.is_none() {
            self.pending_mutation = Some(ComposerHostPendingMutation::Active(pending));
        }
    }

    pub(super) fn validate_mutation_store(
        &self,
        binding: ComposerHostBinding,
        store: &HomeStore,
    ) -> Result<(), ComposerHostError> {
        validate_store(binding, store)
    }

    pub(super) const fn mutation_transition_limit(&self) -> usize {
        #[cfg(feature = "test-faults")]
        {
            self.mutation_transition_limit
        }
        #[cfg(not(feature = "test-faults"))]
        {
            COMPOSER_HOST_MAX_MUTATION_TRANSITIONS
        }
    }
}

fn validate_begin_key(
    binding: ComposerHostBinding,
    request: MutationBeginRequest,
) -> Result<(), ComposerHostError> {
    let key = request.proposal().key();
    if key.binding() != BindingId::new(binding.host_generation().get())
        || key.base_revision() != SourceRevision::new(binding.candidate().candidate_generation())
        || request.proposal().predecessor().caret()
            != request.proposal().predecessor().selection_head()
    {
        return Err(ComposerHostError::MutationMalformed);
    }
    Ok(())
}

fn operation_id(operation: u64) -> DraftMutationOperationIdV1 {
    let mut bytes = [0; 16];
    bytes[8..].copy_from_slice(&operation.to_be_bytes());
    DraftMutationOperationIdV1::from_bytes(bytes)
}

pub(super) fn active_session(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    binding: ComposerHostBinding,
) -> Result<DraftEditorCandidateSessionV1, ComposerHostError> {
    match storage.draft_editor_candidate_session(
        store,
        binding.candidate().draft_id(),
        binding.candidate().session_id(),
    )? {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => Ok(session),
        _ => Err(ComposerHostError::MutationMalformed),
    }
}
