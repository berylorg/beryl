use beryl_home_store::{CommandError, CommandOutcome, HomeCommand, ReconciliationResolution};
use beryl_state::{
    AssetOwner, AssetOwnerHeadAssertion, AssetOwnerHeadUpdate, UpdateAssetOwnerHeads,
    ValidateAssetOwnerHeads,
};
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidatePublicationOutcomeV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftPieceTextDemandV1, DraftRootHistoryPairV1,
};

use super::*;

impl SyndicComposerHost {
    pub(in crate::composer_host) fn execute_publication_lane(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostPublicationCompletion, ComposerHostError> {
        let (binding, prepared, assets, cancellation) = {
            let pending = self.pending_publication(ticket)?;
            validate_store(pending.intent.binding, store)?;
            let PublicationStage::Ready(prepared) = &pending.stage else {
                return Err(ComposerHostError::PublicationPending);
            };
            (
                pending.intent.binding,
                prepared.clone(),
                pending.intent.assets,
                pending.intent.cancellation.clone(),
            )
        };

        let mut command = HomeCommand::new(store.home_revision()?).with_cancellation(cancellation);
        command.add(self.storage.publish_draft_editor_candidate(
            self.storage.revision(store)?,
            prepared.syndic.clone(),
        ))?;
        add_asset_participant(&mut command, store, assets, prepared.asset)?;

        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.publication_before_execute_fault.take() {
            fault(store, self.storage);
        }
        let outcome = store.execute(command);
        match outcome {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                self.pending_publication_mut(ticket)?.stage = PublicationStage::Reconciling {
                    prepared,
                    handle: reconciliation.install_and_handle(),
                };
                Ok(ComposerHostPublicationCompletion::ReconciliationPending)
            }
            outcome => self.settle_publication(store, ticket, binding, prepared, outcome),
        }
    }

    pub(in crate::composer_host) fn reconcile_publication_lane(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostPublicationCompletion, ComposerHostError> {
        let (binding, prepared, handle) = {
            let pending = self.pending_publication(ticket)?;
            validate_store(pending.intent.binding, store)?;
            let PublicationStage::Reconciling { prepared, handle } = &pending.stage else {
                return Err(ComposerHostError::PublicationPending);
            };
            (pending.intent.binding, prepared.clone(), handle.clone())
        };
        let outcome = match store.reconcile(&handle)? {
            ReconciliationResolution::ExactOld => CommandOutcome::NotCommitted {
                evidence: CommandError::ReentrantWriter,
            },
            ReconciliationResolution::ExactNew { receipt } => CommandOutcome::Committed {
                receipt,
                later_failure: None,
            },
            ReconciliationResolution::ExactSuccessor { .. }
            | ReconciliationResolution::Collision => {
                self.make_publication_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::ReconciliationCollision,
                )?;
                return Ok(ComposerHostPublicationCompletion::ReconciliationCollision);
            }
        };
        self.settle_publication(store, ticket, binding, prepared, outcome)
    }

    fn settle_publication(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
        binding: ComposerHostBinding,
        prepared: PreparedPublication,
        command_outcome: CommandOutcome,
    ) -> Result<ComposerHostPublicationCompletion, ComposerHostError> {
        let cancelled = matches!(
            &command_outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::CancelledBeforeAdmission
            }
        );
        let outcome = self.storage.reconcile_draft_editor_candidate_publication(
            store,
            &prepared.syndic,
            command_outcome,
        );
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(syndic_storage::DraftEditorCandidatePublicationCommandErrorV1::NotCommitted) => {
                self.publication.lane = None;
                return Ok(if cancelled {
                    ComposerHostPublicationCompletion::CancelledBeforeAdmission
                } else {
                    ComposerHostPublicationCompletion::NotCommitted
                });
            }
            Err(error) => {
                self.make_publication_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::ReconciliationCollision,
                )?;
                return Err(error.into());
            }
        };
        self.finish_publication(store, ticket, binding, outcome)
    }

    fn finish_publication(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
        binding: ComposerHostBinding,
        outcome: DraftEditorCandidatePublicationOutcomeV1,
    ) -> Result<ComposerHostPublicationCompletion, ComposerHostError> {
        let (thread, draft, session, captured_generation) = {
            let pending = self.pending_publication(ticket)?;
            (
                pending.intent.selector.thread_id(),
                pending.intent.selector.draft_id(),
                pending.intent.candidate.session_id(),
                pending.intent.candidate.candidate_generation(),
            )
        };
        let (completion, outcome_selector, outcome_generation, outcome_pair) = match outcome {
            DraftEditorCandidatePublicationOutcomeV1::Published(selector, pair) => (
                ComposerHostPublicationCompletion::Published,
                Some(selector),
                captured_generation,
                pair,
            ),
            DraftEditorCandidatePublicationOutcomeV1::ExactReplay(receipt) => (
                ComposerHostPublicationCompletion::ExactReplay,
                Some(receipt.successor_selector()),
                captured_generation,
                receipt.published_pair(),
            ),
            DraftEditorCandidatePublicationOutcomeV1::Superseded(generation, pair) => (
                ComposerHostPublicationCompletion::Superseded,
                None,
                generation,
                pair,
            ),
            DraftEditorCandidatePublicationOutcomeV1::DurableBaseConflict(_) => {
                self.make_publication_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::DurableBaseConflict,
                )?;
                return Ok(ComposerHostPublicationCompletion::DurableBaseConflict);
            }
            DraftEditorCandidatePublicationOutcomeV1::SessionDisposed => {
                self.make_publication_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::SessionDisposed,
                )?;
                return Ok(ComposerHostPublicationCompletion::SessionDisposed);
            }
            DraftEditorCandidatePublicationOutcomeV1::OccupiedIdentityCollision(_) => {
                self.make_publication_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::IdentityCollision,
                )?;
                return Ok(ComposerHostPublicationCompletion::OccupiedIdentityCollision);
            }
        };
        if outcome_selector.is_some_and(|selector| {
            selector.root() != outcome_pair.root() || selector.history() != outcome_pair.history()
        }) {
            self.make_publication_terminal(
                ticket,
                ComposerHostPublicationUnavailable::ReconciliationCollision,
            )?;
            return Err(ComposerHostError::PublicationAssetMismatch);
        }
        let revision = self.storage.revision(store)?;
        let current = self
            .storage
            .current_draft(store, thread, publication_point_limit())?
            .ok_or(ComposerHostError::MissingCurrentDraft)?;
        let selector = current_selector(&current);
        let current_pair = DraftRootHistoryPairV1::new(selector.root(), selector.history());
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.publication.convergence_read_fault.take() {
            fault(store, self.storage);
        }
        let session_outcome = self
            .storage
            .draft_editor_candidate_session(store, draft, session)?;
        if self.storage.revision(store)? != revision {
            return Err(ComposerHostError::PublicationPending);
        }
        let head = match session_outcome {
            DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
            DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => {
                let session_is_current = selector.selector_revision()
                    == head.published_selector_revision()
                    && current_pair
                        == DraftRootHistoryPairV1::new(
                            head.published_root(),
                            head.published_history(),
                        );
                let reason = if session_is_current {
                    ComposerHostPublicationUnavailable::SessionDisposed
                } else {
                    ComposerHostPublicationUnavailable::DurableBaseConflict
                };
                self.make_publication_terminal(ticket, reason)?;
                return Ok(if session_is_current {
                    ComposerHostPublicationCompletion::SessionDisposed
                } else {
                    ComposerHostPublicationCompletion::DurableBaseConflict
                });
            }
            DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange => {
                return Err(ComposerHostError::PublicationPending);
            }
            DraftEditorCandidateSessionReadOutcomeV1::Absent
            | DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure => {
                self.make_publication_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::ReconciliationCollision,
                )?;
                return Err(
                    syndic_storage::DraftEditorCandidatePublicationCommandErrorV1::Invariant.into(),
                );
            }
        };
        let pair = DraftRootHistoryPairV1::new(head.published_root(), head.published_history());
        let generation = head.published_candidate_generation();
        let session_is_current = selector.selector_revision() == head.published_selector_revision()
            && pair == current_pair;
        let exact_outcome = outcome_selector == Some(selector)
            && pair == outcome_pair
            && generation == outcome_generation;
        let same_session_descendant = session_is_current && generation > outcome_generation;
        let exact_supersession = outcome_selector.is_none()
            && session_is_current
            && generation == outcome_generation
            && pair == outcome_pair;
        if !exact_outcome && !same_session_descendant && !exact_supersession {
            if !session_is_current {
                self.make_publication_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::DurableBaseConflict,
                )?;
                return Ok(ComposerHostPublicationCompletion::DurableBaseConflict);
            }
            self.make_publication_terminal(
                ticket,
                ComposerHostPublicationUnavailable::ReconciliationCollision,
            )?;
            return Err(ComposerHostError::PublicationAssetMismatch);
        }
        let storage_candidate = DraftEditorCandidateActivationBindingV1::from_head(&head);
        if let Some(active) = self.active.as_mut()
            && same_session(active.binding, binding)
        {
            active.binding = ComposerHostBinding::new(
                active.binding.home_id(),
                active.binding.home_generation(),
                active.binding.host_generation(),
                storage_candidate,
                active.binding.presentation_generation(),
            );
            active.durable_selector = selector;
            active.published_candidate_generation = generation;
            active.published_pair = pair;
            active.storage_candidate = storage_candidate;
            active.session_disposed = false;
        }
        self.publication.lane = None;
        Ok(completion)
    }
}

fn add_asset_participant(
    command: &mut HomeCommand,
    store: &HomeStore,
    assets: AssetState,
    plan: PublicationAssetPlan,
) -> Result<(), ComposerHostError> {
    let revision = assets.revision(store)?;
    match plan {
        PublicationAssetPlan::Replace {
            draft,
            expected,
            proof,
        } => {
            command.add(assets.update_owner_heads(
                revision,
                UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                    AssetOwner::CurrentDraft(draft),
                    expected,
                    Some(proof),
                )]))?,
            ))?;
        }
        PublicationAssetPlan::Remove { draft, expected } => {
            command.add(assets.update_owner_heads(
                revision,
                UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                    AssetOwner::CurrentDraft(draft),
                    Some(expected),
                    None,
                )]))?,
            ))?;
        }
        PublicationAssetPlan::Validate { draft, expected } => {
            command.add_validation(assets.validate_owner_heads(
                revision,
                ValidateAssetOwnerHeads::new(Box::from([AssetOwnerHeadAssertion::new(
                    AssetOwner::CurrentDraft(draft),
                    expected,
                )]))?,
            ))?;
        }
    }
    Ok(())
}
