use syndic_storage::{
    DraftMarkerAdmissionTerminalOutcomeV1 as TerminalOutcome,
    DraftMarkerLabelAssignmentOutcomeV1 as AssignmentOutcome,
    DraftMarkerLabelReadinessPageSubmissionOutcomeV1 as PageOutcome,
};

use super::*;

impl SyndicComposerHost {
    pub(in crate::composer_host::mutation) fn cancel_mutation_admission(
        &mut self,
        store: &HomeStore,
        admission: &mut ComposerHostMutationAdmission,
    ) -> Result<ComposerHostMutationEvidenceOutcome, ComposerHostError> {
        let key = admission.pass.key();
        let page_flight = admission
            .page
            .as_mut()
            .and_then(|page| page.readiness.as_mut())
            .or(admission.eof.as_mut())
            .and_then(|page| page.flight.take());
        if let Some(flight) = page_flight {
            match self
                .storage
                .submit_draft_marker_label_readiness_page(store, flight)
            {
                PageOutcome::ReconciliationPending(flight) => {
                    let page = admission
                        .page
                        .as_mut()
                        .and_then(|page| page.readiness.as_mut())
                        .or(admission.eof.as_mut())
                        .ok_or(ComposerHostError::MutationMalformed)?;
                    page.flight = Some(flight);
                    return Ok(ComposerHostMutationEvidenceOutcome::Pending(key));
                }
                PageOutcome::Advanced { .. } | PageOutcome::Replayed => {
                    admission.durable_progress = true
                }
                PageOutcome::CommittedUnavailable {
                    receipt,
                    later_failure,
                    reason,
                } => {
                    admission.durable_progress = true;
                    admission.fail(ComposerHostMutationAdmissionFailure::CommittedUnavailable {
                        receipt,
                        later_failure,
                        reason,
                    });
                }
                PageOutcome::StorageError(error) => {
                    admission.fail(ComposerHostMutationAdmissionFailure::Storage(error))
                }
                PageOutcome::Retryable => {}
                PageOutcome::Refused(_) | PageOutcome::Collision => {
                    return Ok(admission.unavailable());
                }
            }
        }
        if let Some(flight) = admission.assignment_flight.take() {
            if flight.committed_receipt().is_some() {
                admission.durable_progress = true;
                drop(flight);
            } else {
                match self
                    .storage
                    .submit_draft_marker_label_assignment(store, flight)
                {
                    AssignmentOutcome::ReconciliationPending(flight) => {
                        admission.assignment_flight = Some(flight);
                        return Ok(ComposerHostMutationEvidenceOutcome::Pending(key));
                    }
                    AssignmentOutcome::CommittedReadinessPending { flight, .. } => {
                        admission.durable_progress = true;
                        drop(flight);
                    }
                    AssignmentOutcome::Advanced { .. } | AssignmentOutcome::Ready { .. } => {
                        admission.durable_progress = true
                    }
                    AssignmentOutcome::CommittedUnavailable {
                        receipt,
                        later_failure,
                        reason,
                    } => {
                        admission.durable_progress = true;
                        admission.fail(
                            ComposerHostMutationAdmissionFailure::CommittedUnavailable {
                                receipt,
                                later_failure,
                                reason,
                            },
                        );
                    }
                    AssignmentOutcome::StorageError(error) => {
                        admission.fail(ComposerHostMutationAdmissionFailure::Storage(error))
                    }
                    AssignmentOutcome::Retryable => {}
                    AssignmentOutcome::Refused(_) | AssignmentOutcome::Collision => {
                        return Ok(admission.unavailable());
                    }
                }
            }
        }
        if admission.begin_attempted && admission.cancelling_staging.is_none() {
            let prepared = admission
                .prepared_begin
                .as_ref()
                .ok_or(ComposerHostError::MutationMalformed)?
                .clone();
            match self
                .storage
                .reconcile_draft_mutation_staging_command(store, &prepared)?
            {
                syndic_storage::DraftMutationStagingReconcileV1::TargetSelected => {
                    admission.cancelling_staging =
                        Some(Self::evidenced_mutation_coordinator(admission, &prepared)?);
                }
                syndic_storage::DraftMutationStagingReconcileV1::SourceSelected => {
                    admission.begin_attempted = false;
                }
                syndic_storage::DraftMutationStagingReconcileV1::Terminal(_) => {
                    admission.begin_attempted = false;
                    admission.terminal_cleanup = true;
                }
            }
        }
        if let Some(pending) = admission.cancelling_staging.as_mut() {
            match self.cancel_staging_mutation(store, pending) {
                Ok(_) => {
                    admission.cancelling_staging = None;
                    admission.begin_attempted = false;
                    admission.terminal_cleanup = true;
                }
                Err(ComposerHostError::MutationWorkPending) => {
                    return Ok(ComposerHostMutationEvidenceOutcome::Pending(key));
                }
                Err(error) => return Err(error),
            }
        }
        admission.prepared_begin = None;
        admission.page = None;
        admission.eof = None;
        if !admission.durable_progress {
            return terminal_outcome(admission);
        }
        let command = match admission.terminal_command {
            Some(command) => command,
            None => {
                let command = admission.command()?;
                admission.terminal_command = Some(command);
                command
            }
        };
        let outcome = match admission.terminal_flight.take() {
            Some(flight) => self
                .storage
                .resolve_draft_marker_admission_terminal(store, flight),
            None if admission.terminal_cleanup => self
                .storage
                .advance_draft_marker_admission_cleanup(store, admission.owner, command),
            None => self
                .storage
                .cancel_draft_marker_admission(store, admission.owner, command),
        };
        match outcome {
            TerminalOutcome::ReleasedTransient | TerminalOutcome::RetainedClosure => {
                terminal_outcome(admission)
            }
            TerminalOutcome::Advanced {
                receipt,
                later_failure,
            } => {
                admission.terminal_cleanup = true;
                admission.terminal_command = None;
                if let Some(error) = later_failure {
                    admission.fail(ComposerHostMutationAdmissionFailure::CommittedUnavailable {
                        receipt,
                        later_failure: Some(error),
                        reason: DraftMarkerAdmissionCommittedUnavailableReasonV1::LaterFailure,
                    });
                }
                Ok(ComposerHostMutationEvidenceOutcome::Pending(key))
            }
            TerminalOutcome::Replayed => {
                admission.terminal_cleanup = true;
                admission.terminal_command = None;
                Ok(ComposerHostMutationEvidenceOutcome::Pending(key))
            }
            TerminalOutcome::Retryable => Ok(ComposerHostMutationEvidenceOutcome::Pending(key)),
            TerminalOutcome::ReconciliationPending(flight) => {
                admission.terminal_flight = Some(flight);
                Ok(ComposerHostMutationEvidenceOutcome::Pending(key))
            }
            TerminalOutcome::Refused(_) | TerminalOutcome::Collision => Ok(admission.unavailable()),
        }
    }

    pub(in crate::composer_host) fn cancel_pending_mutation_evidence(
        &mut self,
        store: &HomeStore,
        key: MutationKey,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let pending = self
            .pending_mutation
            .take()
            .ok_or(ComposerHostError::MutationNotPending)?;
        let ComposerHostPendingMutation::Admission(mut admission) = pending else {
            self.pending_mutation = Some(pending);
            return Err(ComposerHostError::MutationNotPending);
        };
        if admission.pass.key() != key {
            self.pending_mutation = Some(ComposerHostPendingMutation::Admission(admission));
            return Err(ComposerHostError::RequestMismatch);
        }
        admission.fail(ComposerHostMutationAdmissionFailure::Cancelled);
        let outcome = self.cancel_mutation_admission(store, &mut admission);
        if let Ok(ComposerHostMutationEvidenceOutcome::Refused { failure, .. }) = outcome {
            return if matches!(
                failure.as_ref(),
                ComposerHostMutationAdmissionFailure::Cancelled
            ) {
                Ok(ComposerHostMutationOutcome::Cancelled)
            } else {
                Err(ComposerHostError::MutationAdmission(failure))
            };
        }
        self.pending_mutation = Some(ComposerHostPendingMutation::Admission(admission));
        match outcome {
            Ok(ComposerHostMutationEvidenceOutcome::Pending(_)) => {
                Err(ComposerHostError::MutationWorkPending)
            }
            Ok(ComposerHostMutationEvidenceOutcome::Unavailable { failure, .. }) => {
                Err(ComposerHostError::MutationAdmission(failure))
            }
            Ok(_) => Err(ComposerHostError::MutationUnavailable),
            Err(error) => Err(error),
        }
    }
}

fn terminal_outcome(
    admission: &mut ComposerHostMutationAdmission,
) -> Result<ComposerHostMutationEvidenceOutcome, ComposerHostError> {
    Ok(ComposerHostMutationEvidenceOutcome::Refused {
        key: admission.pass.key(),
        failure: admission
            .failure
            .take()
            .ok_or(ComposerHostError::MutationMalformed)?,
    })
}
