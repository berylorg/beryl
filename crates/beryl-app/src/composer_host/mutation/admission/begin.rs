use syndic_storage::canonical_empty_draft_piece_fragment_chain_v1;

use super::*;

impl SyndicComposerHost {
    pub(super) fn begin_mutation_evidence(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        assets: &AssetState,
        begin: MutationBeginRequest,
        pass: MutationPass,
    ) -> Result<ComposerHostMutationEvidenceOutcome, ComposerHostError> {
        if self.lifecycle.freezes_admission() || self.submission_pending() {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        if self.pending_history.is_some() {
            return Err(ComposerHostError::HistoryPending);
        }
        if self.pending_mutation.is_some() {
            return Err(ComposerHostError::MutationPending);
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
        super::super::execution::validate_begin_key(binding, begin)?;
        let key = begin.proposal().key();
        if pass.key() != key
            || pass.kind() != MutationPassKind::Evidence
            || begin.producer() != Some(pass.producer())
            || begin.proposal().kind() != MutationKind::Edit
        {
            return Err(ComposerHostError::MutationMalformed);
        }
        let identity = ComposerHostMutationIdentity::new(begin);
        if let Some((last_binding, last)) = self.last_mutation_identity.as_deref()
            && *last_binding == binding
        {
            match identity.operation().cmp(&last.operation()) {
                Ordering::Less => return Err(ComposerHostError::StaleRequestIdentity),
                Ordering::Equal if *last != identity => {
                    return Err(ComposerHostError::MutationIdentityCollision);
                }
                _ => {}
            }
        }
        let session = super::super::execution::active_session(&self.storage, store, binding)?;
        let candidate = binding.candidate();
        if DraftEditorCandidateActivationBindingV1::from_head(&session) != candidate {
            return Ok(ComposerHostMutationEvidenceOutcome::Refused {
                key,
                failure: Arc::new(ComposerHostMutationAdmissionFailure::Conflict),
            });
        }
        let proposal = begin.proposal();
        let predecessor = proposal.predecessor();
        let storage_identity = DraftMutationStagingIdentityV1::new(
            candidate.draft_id(),
            candidate.session_id(),
            super::super::execution::operation_id(key.operation().get()),
        );
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
            begin.source_cursor().get(),
            begin.proposal_cursor().get(),
        );
        self.last_mutation_identity = Some(Box::new((binding, identity)));
        self.pending_mutation = Some(ComposerHostPendingMutation::Admission(Box::new(
            ComposerHostMutationAdmission {
                binding,
                begin,
                pass,
                storage_begin,
                session,
                assets: assets.clone(),
                owner: DraftMarkerAdmissionOwnerV1::new(
                    candidate.draft_id(),
                    candidate.session_id(),
                    syndic_storage::DraftMarkerAdmissionOperationIdV1::from_bytes(
                        *storage_identity.operation_id().as_bytes(),
                    ),
                ),
                source: WidgetLaneFrontier::initial(begin.source_cursor()),
                proposal: WidgetLaneFrontier::initial(begin.proposal_cursor()),
                page: None,
                finish: None,
                eof: None,
                eof_accepted: false,
                next_ordinal: NonZeroU64::MIN,
                next_command: 1,
                assignment_command: None,
                assignment_flight: None,
                assignment_error: None,
                prepared_begin: None,
                begin_attempted: false,
                durable_progress: false,
                failure: None,
                terminal_command: None,
                terminal_flight: None,
                terminal_cleanup: false,
                cancelling_staging: None,
            },
        )));
        Ok(ComposerHostMutationEvidenceOutcome::Started(pass))
    }

    pub(super) fn admit_evidenced_mutation(
        &mut self,
        store: &HomeStore,
        admission: &mut ComposerHostMutationAdmission,
    ) -> Result<ComposerHostMutationEvidenceOutcome, ComposerHostError> {
        let prepared = admission
            .prepared_begin
            .as_ref()
            .ok_or(ComposerHostError::MutationMalformed)?
            .clone();
        let key = admission.pass.key();
        if admission.begin_attempted {
            match self
                .storage
                .reconcile_draft_mutation_staging_command(store, &prepared)?
            {
                syndic_storage::DraftMutationStagingReconcileV1::TargetSelected => {
                    self.adopt_evidenced_begin(admission, &prepared)?;
                    return Ok(ComposerHostMutationEvidenceOutcome::Began(key));
                }
                syndic_storage::DraftMutationStagingReconcileV1::SourceSelected => {}
                syndic_storage::DraftMutationStagingReconcileV1::Terminal(_) => {
                    return Err(ComposerHostError::MutationUnavailable);
                }
            }
        }
        admission.begin_attempted = true;
        match self.run_staging_command(store, &prepared, None)? {
            StagingCommandResult::Target => {
                self.adopt_evidenced_begin(admission, &prepared)?;
                Ok(ComposerHostMutationEvidenceOutcome::Began(key))
            }
            StagingCommandResult::Source => Ok(ComposerHostMutationEvidenceOutcome::Pending(key)),
            StagingCommandResult::Terminal => Err(ComposerHostError::MutationUnavailable),
        }
    }

    pub(super) fn adopt_evidenced_begin(
        &mut self,
        admission: &ComposerHostMutationAdmission,
        prepared: &PreparedDraftMutationStagingCommandV1,
    ) -> Result<(), ComposerHostError> {
        self.pending_mutation = Some(ComposerHostPendingMutation::Active(
            Self::evidenced_mutation_coordinator(admission, prepared)?,
        ));
        Ok(())
    }

    pub(super) fn evidenced_mutation_coordinator(
        admission: &ComposerHostMutationAdmission,
        prepared: &PreparedDraftMutationStagingCommandV1,
    ) -> Result<Box<ComposerHostMutationCoordinator>, ComposerHostError> {
        let session = prepared
            .target_session()
            .cloned()
            .ok_or(ComposerHostError::MutationMalformed)?;
        let finish = admission
            .finish
            .ok_or(ComposerHostError::MutationMalformed)?;
        Ok(Box::new(ComposerHostMutationCoordinator {
            binding: admission.binding,
            begin: admission.begin,
            storage_begin: admission.storage_begin,
            evidence: Some(finish),
            cleanup: None,
            build_flight: None,
            build_completion: None,
            build_noncommit: None,
            build_diagnostics: Default::default(),
            unavailable_intent: None,
            identity: admission.storage_begin.identity(),
            session,
            head: prepared.target_head().clone(),
            source: WidgetLaneFrontier::initial(admission.begin.source_cursor()),
            proposal: WidgetLaneFrontier::initial(admission.begin.proposal_cursor()),
            phase: ComposerHostMutationPhase::Receiving,
            fragment_count: 0,
            fragment_chain: canonical_empty_draft_piece_fragment_chain_v1(),
            proposal_envelope_applied: false,
            last_proposal_range: None,
            remaining_proposal_range: admission.begin.proposal().replacement(),
            in_flight_page: None,
            finish_input: None,
            intended: None,
            detached: false,
        }))
    }
}
