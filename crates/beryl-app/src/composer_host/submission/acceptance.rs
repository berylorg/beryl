use beryl_home_store::{
    CommandCancellation, CommandOutcome, FreeSpaceOutcome, HomeStore, ReconciliationHandle,
    ReconciliationResolution,
};
use beryl_state::AssetState;
use syndic_storage::FirstAcceptance;

use super::*;

impl SyndicComposerHost {
    pub(super) fn advance_submission_acceptance(
        &mut self,
        store: &HomeStore,
        assets: AssetState,
        acceptance: FirstAcceptance,
        reconciliation: Option<ReconciliationHandle>,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
        if let Some(handle) = reconciliation {
            return match store.reconcile(&handle).map_err(ComposerHostError::from)? {
                ReconciliationResolution::ExactOld => {
                    self.submission.pending = None;
                    Ok(ComposerHostSubmissionAdvance::NotCommitted)
                }
                ReconciliationResolution::ExactNew { .. }
                | ReconciliationResolution::ExactSuccessor { .. } => {
                    let kind = submission_acceptance_kind(&acceptance);
                    self.finish_submission_success(kind);
                    Ok(ComposerHostSubmissionAdvance::ExactSuccess(kind))
                }
                ReconciliationResolution::Collision => {
                    if let Some(active) = self.active.as_mut() {
                        active.unavailable = true;
                    }
                    self.submission.pending = None;
                    Ok(ComposerHostSubmissionAdvance::Collision)
                }
            };
        }
        if cancellation.is_cancelled() {
            return self.cancel_submission();
        }
        #[cfg(feature = "test-faults")]
        self.inject_submission_transition_fault(
            ComposerHostSubmissionFaultPoint::AcceptanceBeforeAttempt,
        )?;
        let command = match crate::input_admission::first_acceptance_command(
            store,
            &self.storage,
            &assets,
            acceptance.clone(),
        ) {
            Ok(command) => command,
            Err(crate::input_admission::InputAdmissionBuildError::FirstAcceptanceCollision)
                if self
                    .storage
                    .input_gate(
                        store,
                        acceptance.thread_id(),
                        super::submission_point_limit(),
                    )?
                    .is_some_and(|gate| {
                        matches!(
                            gate.state(),
                            syndic_storage::InputGateState::RepairRequired(_)
                        )
                    }) =>
            {
                self.submission.pending = None;
                return Ok(ComposerHostSubmissionAdvance::SubmissionUnavailable(
                    crate::cas_projection::SubmissionUnavailable::RepairRequired,
                ));
            }
            Err(error) => return Err(error.into()),
        };
        let command = match command {
            crate::input_admission::FirstAcceptanceCommand::AlreadyAccepted(kind) => {
                self.finish_submission_success(kind);
                return Ok(ComposerHostSubmissionAdvance::ExactSuccess(kind));
            }
            crate::input_admission::FirstAcceptanceCommand::Execute(command) => command,
        };
        if matches!(
            acceptance.expected_gate_state(),
            syndic_storage::InputGateState::RepairRequired(_)
        ) {
            self.submission.pending = None;
            return Ok(ComposerHostSubmissionAdvance::SubmissionUnavailable(
                crate::cas_projection::SubmissionUnavailable::RepairRequired,
            ));
        }
        let command = match self
            .submission
            .pending
            .as_ref()
            .unwrap()
            .request
            .execution_wake
            .guard_command(store, acceptance.thread_id(), command)
        {
            Ok(command) => command,
            Err(reason) => {
                self.submission.pending = None;
                return Ok(ComposerHostSubmissionAdvance::SubmissionUnavailable(reason));
            }
        };
        if matches!(
            acceptance.expected_gate_state(),
            syndic_storage::InputGateState::Idle
        ) {
            let free_space = store.query_free_space(
                self.submission
                    .pending
                    .as_ref()
                    .unwrap()
                    .request
                    .turn_start_admission_requirement(),
            );
            #[cfg(feature = "test-faults")]
            self.inject_submission_cancellation_fault(
                ComposerHostSubmissionFaultPoint::CancellationAfterFreeSpace,
                cancellation,
            );
            if !matches!(free_space, FreeSpaceOutcome::Sufficient { .. }) {
                self.pending_submission_mut().stage = PendingSubmissionStage::Accepting {
                    acceptance,
                    reconciliation: None,
                };
                return Ok(ComposerHostSubmissionAdvance::DirectAdmissionDenied(
                    free_space,
                ));
            }
        }
        if cancellation.is_cancelled() {
            return self.cancel_submission();
        }
        let _admission = self
            .submission
            .pending
            .as_ref()
            .unwrap()
            .execution
            .as_ref()
            .map(crate::cas_projection::LiveExecutionCandidate::reserve)
            .transpose()?;
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.submission_before_execute_fault.take() {
            fault(store, self.storage.clone());
        }
        #[cfg(feature = "test-faults")]
        self.inject_submission_cancellation_fault(
            ComposerHostSubmissionFaultPoint::CancellationBeforeFinalCommand,
            cancellation,
        );
        let outcome = store.execute(command.with_cancellation(cancellation.clone()));
        match outcome {
            CommandOutcome::NotCommitted { .. } if cancellation.is_cancelled() => {
                self.cancel_submission()
            }
            CommandOutcome::NotCommitted { .. } => {
                let wake = self
                    .submission
                    .pending
                    .as_ref()
                    .unwrap()
                    .request
                    .execution_wake
                    .clone();
                self.submission.pending = None;
                if self
                    .storage
                    .input_gate(
                        store,
                        acceptance.thread_id(),
                        super::submission_point_limit(),
                    )?
                    .is_some_and(|gate| {
                        matches!(
                            gate.state(),
                            syndic_storage::InputGateState::RepairRequired(_)
                        )
                    })
                {
                    Ok(ComposerHostSubmissionAdvance::SubmissionUnavailable(
                        crate::cas_projection::SubmissionUnavailable::RepairRequired,
                    ))
                } else if let Some(reason) = wake.submission_unavailable(acceptance.thread_id()) {
                    Ok(ComposerHostSubmissionAdvance::SubmissionUnavailable(reason))
                } else {
                    Ok(ComposerHostSubmissionAdvance::NotCommitted)
                }
            }
            CommandOutcome::Committed { .. } => {
                let kind = submission_acceptance_kind(&acceptance);
                self.finish_submission_success(kind);
                Ok(ComposerHostSubmissionAdvance::ExactSuccess(kind))
            }
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                self.pending_submission_mut().stage = PendingSubmissionStage::Accepting {
                    acceptance,
                    reconciliation: Some(reconciliation.install_and_handle()),
                };
                #[cfg(feature = "test-faults")]
                if let Some(fault) = self.submission_after_indeterminate_fault.take() {
                    fault(store);
                }
                #[cfg(feature = "test-faults")]
                self.inject_submission_transition_fault(
                    ComposerHostSubmissionFaultPoint::AcceptanceAfterAttempt,
                )?;
                Ok(ComposerHostSubmissionAdvance::ReconciliationPending)
            }
        }
    }
}

fn submission_acceptance_kind(acceptance: &FirstAcceptance) -> syndic_storage::FirstAcceptanceKind {
    if matches!(
        acceptance.expected_gate_state(),
        syndic_storage::InputGateState::Idle
    ) {
        syndic_storage::FirstAcceptanceKind::Idle {
            user_item_id: acceptance.idle_user_item_id(),
        }
    } else {
        syndic_storage::FirstAcceptanceKind::Accepted
    }
}
