use syndic_storage::{
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionCommittedUnavailableReasonV1,
    DraftMarkerAdmissionOperationIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionTerminalFlightV1, DraftMarkerAdmissionTerminalOutcomeV1,
};

use super::*;

pub(super) struct ComposerHostMutationCleanup {
    owner: DraftMarkerAdmissionOwnerV1,
    outcome: ComposerHostMutationOutcome,
    serial: u64,
    flight: Option<DraftMarkerAdmissionTerminalFlightV1>,
    failure: Option<std::sync::Arc<ComposerHostMutationAdmissionFailure>>,
    complete: bool,
    unavailable: bool,
    inert: Option<syndic_storage::StagedDraftPieceInertCleanupV1>,
}

impl ComposerHostMutationCleanup {
    pub(super) fn new(
        identity: DraftMutationStagingIdentityV1,
        outcome: ComposerHostMutationOutcome,
    ) -> Self {
        Self {
            owner: DraftMarkerAdmissionOwnerV1::new(
                identity.draft_id(),
                identity.session_id(),
                DraftMarkerAdmissionOperationIdV1::from_bytes(*identity.operation_id().as_bytes()),
            ),
            outcome,
            serial: 1,
            flight: None,
            failure: None,
            complete: false,
            unavailable: false,
            inert: None,
        }
    }

    pub(super) fn from_inert(
        inert: syndic_storage::StagedDraftPieceInertCleanupV1,
        outcome: ComposerHostMutationOutcome,
    ) -> Self {
        Self {
            owner: inert.owner(),
            outcome,
            serial: 1,
            flight: None,
            failure: None,
            complete: false,
            unavailable: false,
            inert: Some(inert),
        }
    }
}

impl SyndicComposerHost {
    pub(super) fn drive_mutation_cleanup(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let cleanup = pending
            .cleanup
            .as_mut()
            .ok_or(ComposerHostError::MutationMalformed)?;
        for _ in 0..self.mutation_transition_limit().min(16) {
            if cleanup.unavailable {
                return Err(ComposerHostError::MutationAdmission(
                    cleanup
                        .failure
                        .clone()
                        .ok_or(ComposerHostError::MutationMalformed)?,
                ));
            }
            if cleanup.complete {
                cleanup.inert = None;
                if let Some(failure) = cleanup.failure.clone() {
                    return Err(ComposerHostError::MutationAdmission(failure));
                }
                return Ok(cleanup.outcome.clone());
            }
            let mut bytes = *b"cleanup\0\0\0\0\0\0\0\0\0";
            bytes[8..].copy_from_slice(&cleanup.serial.to_be_bytes());
            let outcome = match cleanup.flight.take() {
                Some(flight) => self
                    .storage
                    .resolve_draft_marker_admission_terminal(store, flight),
                None => self.storage.advance_draft_marker_admission_cleanup(
                    store,
                    cleanup.owner,
                    DraftMarkerAdmissionCommandIdV1::from_bytes(bytes),
                ),
            };
            match outcome {
                DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => cleanup.complete = true,
                DraftMarkerAdmissionTerminalOutcomeV1::Advanced {
                    receipt,
                    later_failure,
                } => {
                    cleanup.serial = cleanup
                        .serial
                        .checked_add(1)
                        .ok_or(ComposerHostError::MutationMalformed)?;
                    if let Some(error) = later_failure
                        && cleanup.failure.is_none()
                    {
                        cleanup.failure = Some(std::sync::Arc::new(
                            ComposerHostMutationAdmissionFailure::CommittedUnavailable {
                                receipt,
                                later_failure: Some(error),
                                reason:
                                    DraftMarkerAdmissionCommittedUnavailableReasonV1::LaterFailure,
                            },
                        ));
                    }
                }
                DraftMarkerAdmissionTerminalOutcomeV1::Replayed => {
                    cleanup.serial = cleanup
                        .serial
                        .checked_add(1)
                        .ok_or(ComposerHostError::MutationMalformed)?;
                }
                DraftMarkerAdmissionTerminalOutcomeV1::Retryable => {
                    return Err(ComposerHostError::MutationWorkPending);
                }
                DraftMarkerAdmissionTerminalOutcomeV1::ReconciliationPending(flight) => {
                    cleanup.flight = Some(flight);
                    return Err(ComposerHostError::MutationWorkPending);
                }
                outcome @ (DraftMarkerAdmissionTerminalOutcomeV1::ReleasedTransient
                | DraftMarkerAdmissionTerminalOutcomeV1::Refused(_)
                | DraftMarkerAdmissionTerminalOutcomeV1::Collision) => {
                    let refusal = match outcome {
                        DraftMarkerAdmissionTerminalOutcomeV1::Refused(refusal) => Some(refusal),
                        _ => None,
                    };
                    cleanup.unavailable = true;
                    let failure = cleanup
                        .failure
                        .get_or_insert_with(|| {
                            std::sync::Arc::new(
                                ComposerHostMutationAdmissionFailure::TerminalCleanup {
                                    outcome: cleanup.outcome.clone(),
                                    refusal,
                                },
                            )
                        })
                        .clone();
                    return Err(ComposerHostError::MutationAdmission(failure));
                }
            }
        }
        Err(ComposerHostError::MutationWorkPending)
    }
}
