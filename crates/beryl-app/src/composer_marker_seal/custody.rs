use super::{access::MarkerAccess, *};
use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization, HomeCommand,
    ReconciliationFailure, ReconciliationHandle, ReconciliationResolution,
};

pub(super) enum SealCommandSource {
    Begin(Box<syndic_storage::PreparedDraftMarkerSealBeginV1>),
    Page(Box<syndic_storage::PreparedDraftMarkerSealAdvanceV1>),
    AssetSeal(beryl_model::SealedAssetReferenceSetProof),
    Terminal(syndic_storage::DraftMarkerSealCustodyReleaseV1),
}

pub(super) struct SealCommandAttempt {
    pub(super) source: SealCommandSource,
    pub(super) outcome: RetainedSealOutcome,
}

#[derive(Default)]
pub(super) struct SealCommandCustody {
    pub(super) original: Option<Box<SealCommandAttempt>>,
    pub(super) recovery: Option<Box<SealCommandAttempt>>,
}

pub(super) enum RetainedSealOutcome {
    NotCommitted(CommandError),
    Committed {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate {
        failure: CommandError,
        handle: ReconciliationHandle,
        resolution: Option<Result<ReconciliationResolution, ReconciliationFailure>>,
    },
}

impl RetainedSealOutcome {
    pub(super) fn new(outcome: CommandOutcome) -> Self {
        match outcome {
            CommandOutcome::NotCommitted { evidence } => Self::NotCommitted(evidence),
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => Self::Committed {
                receipt,
                later_failure,
                local_finalization,
            },
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => Self::Indeterminate {
                failure,
                handle: reconciliation.install_and_handle(),
                resolution: None,
            },
        }
    }
    pub(super) fn known(&self) -> Option<DurableCommandResult> {
        match self {
            Self::NotCommitted(_)
            | Self::Indeterminate {
                resolution: Some(Ok(ReconciliationResolution::ExactOld)),
                ..
            } => Some(DurableCommandResult::ExactOld),
            Self::Committed { .. }
            | Self::Indeterminate {
                resolution: Some(Ok(ReconciliationResolution::ExactNew { .. })),
                ..
            } => Some(DurableCommandResult::ExactNew),
            _ => None,
        }
    }
    pub(super) fn settle(
        &mut self,
        access: MarkerAccess<'_>,
    ) -> Result<DurableCommandResult, DraftMarkerSealServiceError> {
        if let Self::Indeterminate {
            handle, resolution, ..
        } = self
        {
            let result = match (&*resolution, access) {
                (None, MarkerAccess::Ordinary(store)) => store.reconcile(handle),
                (None, MarkerAccess::Candidate(candidate)) => candidate.reconcile(handle),
                (Some(Err(_)), MarkerAccess::Candidate(candidate)) => {
                    candidate.retry_reconciliation(handle)
                }
                (Some(Err(error)), _) => {
                    return Err(DraftMarkerSealServiceError::Reconciliation(error.clone()));
                }
                (Some(Ok(_)), _) => {
                    return self
                        .known()
                        .ok_or(DraftMarkerSealServiceError::ReconciliationCollision);
                }
            };
            *resolution = Some(result);
            if let Some(Err(error)) = resolution {
                return Err(DraftMarkerSealServiceError::Reconciliation(error.clone()));
            }
        }
        self.known()
            .ok_or(DraftMarkerSealServiceError::ReconciliationCollision)
    }
}

impl SealCommandCustody {
    pub(super) fn require_settled(
        &mut self,
        access: MarkerAccess<'_>,
    ) -> Result<(), DraftMarkerSealServiceError> {
        if let Some(original) = &mut self.original {
            original.outcome.settle(access)?;
        }
        if let Some(recovery) = &mut self.recovery {
            recovery.outcome.settle(access)?;
        }
        Ok(())
    }
    pub(super) fn record(
        &mut self,
        access: MarkerAccess<'_>,
        source: SealCommandSource,
        outcome: CommandOutcome,
    ) -> &mut RetainedSealOutcome {
        let target = match access {
            MarkerAccess::Ordinary(_) => &mut self.original,
            MarkerAccess::Candidate(_) => &mut self.recovery,
        };
        *target = Some(Box::new(SealCommandAttempt {
            source,
            outcome: RetainedSealOutcome::new(outcome),
        }));
        &mut target.as_mut().unwrap().outcome
    }
}

pub(super) fn execute_retained_command(
    access: MarkerAccess<'_>,
    command: HomeCommand,
    command_fault: CommandFault,
    reconcile_fault: ReconcileFault,
    storage: &SyndicStorage,
    request: DraftMarkerSealFlightRequest,
    custody: &Mutex<SealCommandCustody>,
    source: SealCommandSource,
) -> Result<DurableCommandResult, DraftMarkerSealServiceError> {
    let mut custody = lock_state(custody);
    custody.require_settled(access)?;
    let outcome = access.execute(command, command_fault);
    let retained = custody.record(access, source, outcome);
    if matches!(retained, RetainedSealOutcome::Indeterminate { .. }) {
        if let MarkerAccess::Ordinary(store) = access {
            reconcile_fault.run(store, storage, request);
        }
    }
    retained.settle(access)
}
