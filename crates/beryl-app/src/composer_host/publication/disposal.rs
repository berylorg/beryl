use beryl_home_store::{CommandError, CommandOutcome, HomeCommand, ReconciliationResolution};
use syndic_storage::{
    DraftEditorCandidateSessionDisposeOutcomeV1, DraftEditorCandidateSessionDisposeRequestV1,
};

use super::*;

impl SyndicComposerHost {
    pub(in crate::composer_host) fn capture_clean_disposal(
        &mut self,
        store: &HomeStore,
        operation_id: DraftPieceOperationIdV1,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostDisposalTicket, ComposerHostError> {
        if self.publication.lane.is_some() {
            return Err(ComposerHostError::PublicationPending);
        }
        if self.is_dirty() || self.live_operation_pending() {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        validate_store(active.binding, store)?;
        if active.unavailable {
            return Err(ComposerHostError::PublicationUnavailable);
        }
        let request = DraftEditorCandidateSessionDisposeRequestV1::new(
            active.storage_candidate.draft_id(),
            active.storage_candidate.session_id(),
            operation_id,
            active.storage_candidate.session_generation(),
            active.published_pair,
        );
        let prepared = self
            .storage
            .prepare_dispose_draft_editor_candidate_session(store, request)?;
        let lane_generation = next(self.publication.lane_generation)?;
        let ticket = ComposerHostDisposalTicket {
            host_generation: active.binding.host_generation(),
            lane_generation,
        };
        self.publication.lane_generation = lane_generation;
        self.publication.lane = Some(Box::new(ComposerHostPublicationLane::Disposal(
            PendingDisposal {
                ticket,
                binding: active.binding,
                prepared,
                cancellation: cancellation.clone(),
                reconciliation: None,
                terminal: None,
            },
        )));
        Ok(ticket)
    }

    pub(in crate::composer_host) fn execute_clean_disposal(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostDisposalTicket,
    ) -> Result<ComposerHostDisposalCompletion, ComposerHostError> {
        let (binding, prepared, cancellation) = {
            let pending = self.pending_disposal(ticket)?;
            validate_store(pending.binding, store)?;
            if pending.reconciliation.is_some() || pending.terminal.is_some() {
                return Err(ComposerHostError::PublicationPending);
            }
            (
                pending.binding,
                pending.prepared.clone(),
                pending.cancellation.clone(),
            )
        };
        let mut command = HomeCommand::new(store.home_revision()?).with_cancellation(cancellation);
        command.add(self.storage.dispose_draft_editor_candidate_session(
            self.storage.revision(store)?,
            prepared.clone(),
        ))?;
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.publication_before_execute_fault.take() {
            fault(store, self.storage);
        }
        let outcome = store.execute(command);
        match outcome {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                self.pending_disposal_mut(ticket)?.reconciliation =
                    Some(reconciliation.install_and_handle());
                Ok(ComposerHostDisposalCompletion::ReconciliationPending)
            }
            outcome => self.settle_disposal(store, ticket, binding, prepared, outcome),
        }
    }

    pub(in crate::composer_host) fn reconcile_clean_disposal(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostDisposalTicket,
    ) -> Result<ComposerHostDisposalCompletion, ComposerHostError> {
        let (binding, prepared, handle) = {
            let pending = self.pending_disposal(ticket)?;
            validate_store(pending.binding, store)?;
            (
                pending.binding,
                pending.prepared.clone(),
                pending
                    .reconciliation
                    .clone()
                    .ok_or(ComposerHostError::PublicationPending)?,
            )
        };
        let outcome = match store.reconcile(&handle)? {
            ReconciliationResolution::ExactOld => CommandOutcome::NotCommitted {
                evidence: CommandError::ReentrantWriter,
            },
            ReconciliationResolution::ExactNew { receipt } => CommandOutcome::Committed {
                receipt,
                later_failure: None,
            },
            ReconciliationResolution::Collision => {
                self.make_disposal_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::ReconciliationCollision,
                )?;
                return Ok(ComposerHostDisposalCompletion::ReconciliationCollision);
            }
        };
        self.settle_disposal(store, ticket, binding, prepared, outcome)
    }

    fn settle_disposal(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostDisposalTicket,
        binding: ComposerHostBinding,
        prepared: PreparedDraftEditorCandidateSessionDisposeV1,
        command_outcome: CommandOutcome,
    ) -> Result<ComposerHostDisposalCompletion, ComposerHostError> {
        let cancelled = matches!(
            &command_outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::CancelledBeforeAdmission
            }
        );
        let outcome = self
            .storage
            .reconcile_draft_editor_candidate_session_disposal(store, &prepared, command_outcome);
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(syndic_storage::DraftEditorCandidatePublicationCommandErrorV1::NotCommitted) => {
                self.publication.lane = None;
                return Ok(if cancelled {
                    ComposerHostDisposalCompletion::CancelledBeforeAdmission
                } else {
                    ComposerHostDisposalCompletion::NotCommitted
                });
            }
            Err(error) => {
                self.make_disposal_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::ReconciliationCollision,
                )?;
                return Err(error.into());
            }
        };
        let completion = match outcome {
            DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_) => {
                ComposerHostDisposalCompletion::Disposed
            }
            DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(_) => {
                ComposerHostDisposalCompletion::ExactReplay
            }
            DraftEditorCandidateSessionDisposeOutcomeV1::AlreadyDisposed(_) => {
                ComposerHostDisposalCompletion::AlreadyDisposed
            }
            DraftEditorCandidateSessionDisposeOutcomeV1::DirtyConflict(_) => {
                self.make_disposal_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::DisposalDirtyConflict,
                )?;
                return Ok(ComposerHostDisposalCompletion::DirtyConflict);
            }
            DraftEditorCandidateSessionDisposeOutcomeV1::OccupiedIdentityCollision(_) => {
                self.make_disposal_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::IdentityCollision,
                )?;
                return Ok(ComposerHostDisposalCompletion::OccupiedIdentityCollision);
            }
        };
        if let Some(active) = self.active.as_mut()
            && same_session(active.binding, binding)
        {
            active.session_disposed = true;
        }
        self.publication.lane = None;
        Ok(completion)
    }

    fn pending_disposal(
        &self,
        ticket: ComposerHostDisposalTicket,
    ) -> Result<&PendingDisposal, ComposerHostError> {
        match self.publication.lane.as_deref() {
            Some(ComposerHostPublicationLane::Disposal(pending))
                if pending.ticket == ticket
                    && ticket.host_generation == pending.binding.host_generation() =>
            {
                Ok(pending)
            }
            Some(_) => Err(ComposerHostError::StalePublicationGeneration),
            None => Err(ComposerHostError::PublicationNotPending),
        }
    }

    fn pending_disposal_mut(
        &mut self,
        ticket: ComposerHostDisposalTicket,
    ) -> Result<&mut PendingDisposal, ComposerHostError> {
        match self.publication.lane.as_deref_mut() {
            Some(ComposerHostPublicationLane::Disposal(pending)) if pending.ticket == ticket => {
                Ok(pending)
            }
            Some(_) => Err(ComposerHostError::StalePublicationGeneration),
            None => Err(ComposerHostError::PublicationNotPending),
        }
    }

    fn make_disposal_terminal(
        &mut self,
        ticket: ComposerHostDisposalTicket,
        reason: ComposerHostPublicationUnavailable,
    ) -> Result<(), ComposerHostError> {
        let binding = {
            let pending = self.pending_disposal_mut(ticket)?;
            pending.terminal = Some(reason);
            pending.binding
        };
        self.mark_active_session_unavailable(binding);
        Ok(())
    }
}
