use super::*;

impl SyndicComposerHost {
    pub const fn mutation_status(&self) -> Option<ComposerHostMutationStatus> {
        match self.pending_mutation {
            Some(ComposerHostPendingMutation::Staged(_)) => {
                Some(ComposerHostMutationStatus::Staged)
            }
            Some(ComposerHostPendingMutation::Admitted(_)) => {
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
        request: ComposerHostMutationRequest,
    ) -> Result<(), ComposerHostError> {
        if let Some(pending) = &self.pending_mutation {
            return Err(match pending {
                ComposerHostPendingMutation::Unavailable(_) => {
                    ComposerHostError::MutationUnavailable
                }
                ComposerHostPendingMutation::Staged(_)
                | ComposerHostPendingMutation::Admitted(_) => ComposerHostError::MutationPending,
            });
        }
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != request.binding {
            return Err(ComposerHostError::OldBinding);
        }
        validate_store(active.binding, store)?;
        validate_request_key(active.binding, &request)?;
        if request.proposal.kind() != MutationKind::Edit {
            return Err(ComposerHostError::MutationUnavailable);
        }
        let identity = ComposerHostMutationIdentity::new(&request);
        if let Some(last) = &self.last_mutation_identity {
            match identity.operation().cmp(&last.operation()) {
                Ordering::Less => return Err(ComposerHostError::StaleRequestIdentity),
                Ordering::Equal if &identity != last => {
                    return Err(ComposerHostError::MutationIdentityCollision);
                }
                Ordering::Equal | Ordering::Greater => {}
            }
        }
        let source = candidate_head(&self.storage, store, active.storage_candidate)?;
        let (replacements, positions, targets, successors) =
            translate_request(&self.storage, store, source.newest_root(), &request)?;
        let intent = ComposerHostRetainedMutationIntent {
            binding: active.binding,
            operation_id: request.operation_id,
            proposal: request.proposal,
            replacements: replacements.clone().into_boxed_slice(),
            positions,
            targets: targets.into_boxed_slice(),
        };
        let header = DraftPieceEditHeaderV1::new(
            source.draft_id(),
            source.session_id(),
            source.newest_candidate_generation(),
            source.newest_root(),
            source.newest_history(),
            request.operation_id,
            canonical_position(request.predecessor_positions().caret())?,
            canonical_position(request.predecessor_positions().selection_anchor())?,
            canonical_position(positions.caret())?,
            canonical_position(positions.selection_anchor())?,
            u64::try_from(replacements.len()).map_err(|_| ComposerHostError::MutationMalformed)?,
            canonical_draft_piece_fragment_chain_v1(&replacements),
        );
        let prepared = self
            .storage
            .prepare_draft_piece_edit(store, header, &source)?;
        let mut preceding = canonical_empty_draft_piece_fragment_chain_v1();
        let mut staged = Vec::with_capacity(replacements.len());
        for (index, replacement) in replacements.into_iter().enumerate() {
            let ordinal = u64::try_from(index)
                .ok()
                .and_then(|value| value.checked_add(1))
                .ok_or(ComposerHostError::MutationMalformed)?;
            let fragment = self.storage.prepare_draft_piece_fragment(
                &prepared,
                ordinal,
                preceding,
                replacement,
            )?;
            preceding = fragment.chain_digest();
            staged.push(fragment);
        }
        self.pending_mutation = Some(ComposerHostPendingMutation::Staged(
            ComposerHostMutationTransaction {
                binding: active.binding,
                prepared,
                fragments: staged.into_boxed_slice(),
                positions,
                successors: successors.into_boxed_slice(),
                identity,
                intent,
            },
        ));
        Ok(())
    }

    pub fn execute_mutation(
        &mut self,
        store: &HomeStore,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let pending = match self
            .pending_mutation
            .as_ref()
            .ok_or(ComposerHostError::MutationNotPending)?
        {
            ComposerHostPendingMutation::Staged(pending)
            | ComposerHostPendingMutation::Admitted(pending) => pending,
            ComposerHostPendingMutation::Unavailable(_) => {
                return Err(ComposerHostError::MutationUnavailable);
            }
        };
        self.validate_mutation_store(pending.binding, store)?;
        if cancellation.is_cancelled() {
            return match self.pending_mutation {
                Some(ComposerHostPendingMutation::Staged(_)) => {
                    self.pending_mutation = None;
                    Ok(ComposerHostMutationOutcome::Cancelled)
                }
                Some(ComposerHostPendingMutation::Admitted(_)) => {
                    let prepared = pending.prepared.clone();
                    let fragments = pending.fragments.clone();
                    let positions = pending.positions;
                    self.cancel_mutation(store, &prepared, &fragments, positions)
                }
                Some(ComposerHostPendingMutation::Unavailable(_)) => {
                    Err(ComposerHostError::MutationUnavailable)
                }
                None => Err(ComposerHostError::MutationNotPending),
            };
        }
        let prepared = pending.prepared.clone();
        let fragments = pending.fragments.clone();
        let position_facts = pending.positions;
        if !self.synchronize_mutation_candidate(store)? {
            self.retain_conflicted_mutation()?;
            return Ok(ComposerHostMutationOutcome::Conflict);
        }
        let begin = self
            .storage
            .begin_draft_piece_edit(self.storage.revision(store)?, prepared.clone());
        match self.run_mutation_command(store, &prepared, &fragments, begin, Some(cancellation))? {
            MutationCommandResult::Pending => {}
            MutationCommandResult::Terminal(outcome) => {
                return self.finish_mutation(store, outcome, position_facts);
            }
            MutationCommandResult::CancelledBeforeAdmission => {
                self.pending_mutation = None;
                return Ok(ComposerHostMutationOutcome::Cancelled);
            }
        }
        for fragment in fragments.iter().cloned() {
            if cancellation.is_cancelled() {
                return self.cancel_mutation(store, &prepared, &fragments, position_facts);
            }
            let stage = self.storage.stage_draft_piece_fragment(
                self.storage.revision(store)?,
                prepared.clone(),
                fragment,
            );
            match self.run_mutation_command(
                store,
                &prepared,
                &fragments,
                stage,
                Some(cancellation),
            )? {
                MutationCommandResult::Pending => {}
                MutationCommandResult::Terminal(outcome) => {
                    return self.finish_mutation(store, outcome, position_facts);
                }
                MutationCommandResult::CancelledBeforeAdmission => {
                    return self.cancel_mutation(store, &prepared, &fragments, position_facts);
                }
            }
        }
        let transition_limit = self.mutation_transition_limit();
        for _ in 0..transition_limit {
            if cancellation.is_cancelled() {
                return self.cancel_mutation(store, &prepared, &fragments, position_facts);
            }
            match self.storage.prepare_draft_piece_build_advance(
                store,
                prepared.header().draft_id(),
                prepared.header().session_id(),
                prepared.header().operation_id(),
            ) {
                Ok(Some(advance)) => {
                    let contribution = self
                        .storage
                        .advance_draft_piece_edit(self.storage.revision(store)?, advance);
                    match self.run_mutation_command(
                        store,
                        &prepared,
                        &fragments,
                        contribution,
                        Some(cancellation),
                    )? {
                        MutationCommandResult::Pending => {}
                        MutationCommandResult::Terminal(outcome) => {
                            return self.finish_mutation(store, outcome, position_facts);
                        }
                        MutationCommandResult::CancelledBeforeAdmission => {
                            return self.cancel_mutation(
                                store,
                                &prepared,
                                &fragments,
                                position_facts,
                            );
                        }
                    }
                }
                Ok(None) => {
                    let contribution = self
                        .storage
                        .settle_draft_piece_edit(self.storage.revision(store)?, prepared.clone());
                    match self.run_mutation_command(
                        store,
                        &prepared,
                        &fragments,
                        contribution,
                        Some(cancellation),
                    )? {
                        MutationCommandResult::Pending => {}
                        MutationCommandResult::Terminal(outcome) => {
                            return self.finish_mutation(store, outcome, position_facts);
                        }
                        MutationCommandResult::CancelledBeforeAdmission => {
                            return self.cancel_mutation(
                                store,
                                &prepared,
                                &fragments,
                                position_facts,
                            );
                        }
                    }
                }
                Err(DraftPiecePrepareErrorV1::Rejected(reason)) => {
                    let contribution = self.storage.reject_draft_piece_edit(
                        self.storage.revision(store)?,
                        prepared.clone(),
                        reason,
                    );
                    match self.run_mutation_command(
                        store,
                        &prepared,
                        &fragments,
                        contribution,
                        None,
                    )? {
                        MutationCommandResult::Pending
                        | MutationCommandResult::CancelledBeforeAdmission => {}
                        MutationCommandResult::Terminal(outcome) => {
                            return self.finish_mutation(store, outcome, position_facts);
                        }
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(ComposerHostError::MutationWorkPending)
    }

    fn cancel_mutation(
        &mut self,
        store: &HomeStore,
        prepared: &PreparedDraftPieceEditV1,
        fragments: &[DraftPieceBuildFragmentV1],
        positions: MutationPositions,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let contribution = self
            .storage
            .cancel_draft_piece_edit(self.storage.revision(store)?, prepared.clone());
        match self.run_mutation_command(store, prepared, fragments, contribution, None)? {
            MutationCommandResult::Terminal(outcome) => {
                self.finish_mutation(store, outcome, positions)
            }
            MutationCommandResult::Pending | MutationCommandResult::CancelledBeforeAdmission => {
                Err(ComposerHostError::MutationMalformed)
            }
        }
    }

    fn run_mutation_command(
        &mut self,
        store: &HomeStore,
        prepared: &PreparedDraftPieceEditV1,
        fragments: &[DraftPieceBuildFragmentV1],
        contribution: beryl_home_store::MutationContribution,
        cancellation: Option<&CommandCancellation>,
    ) -> Result<MutationCommandResult, ComposerHostError> {
        let mut command = HomeCommand::new(store.home_revision()?);
        if let Some(cancellation) = cancellation {
            command = command.with_cancellation(cancellation.clone());
        }
        command.add(contribution)?;
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.mutation_before_execute_fault.take() {
            fault(store, self.storage);
        }
        let outcome = store.execute(command);
        if matches!(
            &outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::CancelledBeforeAdmission
            }
        ) {
            return Ok(MutationCommandResult::CancelledBeforeAdmission);
        }
        match self.storage.reconcile_draft_piece_command_outcome(
            store,
            prepared,
            outcome,
            |ordinal| {
                usize::try_from(ordinal.saturating_sub(1))
                    .ok()
                    .and_then(|start| fragments.get(start..))
                    .unwrap_or_default()
                    .to_vec()
            },
        )? {
            syndic_storage::DraftPieceReconciledCommandV1::Pending(
                DraftPieceOperationStatusV1::Absent,
            ) => Ok(MutationCommandResult::Pending),
            syndic_storage::DraftPieceReconciledCommandV1::Pending(
                DraftPieceOperationStatusV1::Open(_) | DraftPieceOperationStatusV1::Complete(_),
            ) => {
                self.mark_mutation_admitted()?;
                Ok(MutationCommandResult::Pending)
            }
            syndic_storage::DraftPieceReconciledCommandV1::Pending(
                DraftPieceOperationStatusV1::Settled(_) | DraftPieceOperationStatusV1::Collision(_),
            ) => Err(ComposerHostError::MutationMalformed),
            syndic_storage::DraftPieceReconciledCommandV1::Terminal(outcome) => {
                self.record_terminal_mutation_identity()?;
                Ok(MutationCommandResult::Terminal(outcome))
            }
        }
    }

    fn mark_mutation_admitted(&mut self) -> Result<(), ComposerHostError> {
        let pending = self
            .pending_mutation
            .take()
            .ok_or(ComposerHostError::MutationNotPending)?;
        self.pending_mutation = Some(match pending {
            ComposerHostPendingMutation::Staged(pending) => {
                let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
                if active.binding != pending.binding {
                    return Err(ComposerHostError::OldBinding);
                }
                self.last_mutation_identity = Some(pending.identity.clone());
                ComposerHostPendingMutation::Admitted(pending)
            }
            ComposerHostPendingMutation::Admitted(pending) => {
                ComposerHostPendingMutation::Admitted(pending)
            }
            ComposerHostPendingMutation::Unavailable(intent) => {
                self.pending_mutation = Some(ComposerHostPendingMutation::Unavailable(intent));
                return Err(ComposerHostError::MutationUnavailable);
            }
        });
        Ok(())
    }

    fn record_terminal_mutation_identity(&mut self) -> Result<(), ComposerHostError> {
        let pending = self
            .pending_mutation
            .as_ref()
            .ok_or(ComposerHostError::MutationNotPending)?;
        let pending = match pending {
            ComposerHostPendingMutation::Staged(pending)
            | ComposerHostPendingMutation::Admitted(pending) => pending,
            ComposerHostPendingMutation::Unavailable(_) => {
                return Err(ComposerHostError::MutationUnavailable);
            }
        };
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != pending.binding {
            return Err(ComposerHostError::OldBinding);
        }
        self.last_mutation_identity = Some(pending.identity.clone());
        Ok(())
    }

    fn retain_conflicted_mutation(&mut self) -> Result<(), ComposerHostError> {
        let pending = self
            .pending_mutation
            .take()
            .ok_or(ComposerHostError::MutationNotPending)?;
        let intent = match pending {
            ComposerHostPendingMutation::Staged(pending)
            | ComposerHostPendingMutation::Admitted(pending) => pending.intent,
            ComposerHostPendingMutation::Unavailable(intent) => intent,
        };
        self.pending_mutation = Some(ComposerHostPendingMutation::Unavailable(intent));
        Ok(())
    }

    const fn mutation_transition_limit(&self) -> usize {
        #[cfg(feature = "test-faults")]
        {
            self.mutation_transition_limit
        }
        #[cfg(not(feature = "test-faults"))]
        {
            COMPOSER_HOST_MAX_MUTATION_TRANSITIONS
        }
    }

    fn validate_mutation_store(
        &self,
        binding: ComposerHostBinding,
        store: &HomeStore,
    ) -> Result<(), ComposerHostError> {
        if self.binding() != Some(binding) {
            return Err(ComposerHostError::OldBinding);
        }
        validate_store(binding, store)
    }
}
