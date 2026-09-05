use std::error::Error;

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    HomeCommand, HomeStore, ReconciliationFailure, ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::{ExecutionBinding, SyndicDraftId, SyndicThreadId, WindowId, WindowPlacement};
use beryl_state::{
    RememberedTarget, WindowAbandonmentNaturalState, WindowAcquisitionAuditError,
    WindowAcquisitionCommittedFacts, WindowAcquisitionNaturalState, WindowAcquisitionThreadOrigin,
};
use syndic_storage::{
    PristineThreadAudit, PristineThreadCandidate, PristineThreadRemovalAudit, SyndicTimestamp,
};

use super::{
    AcquisitionFlight, RuntimeBackedWindowAcquisition, RuntimeBackedWindowAcquisitionDisposition,
    RuntimeBackedWindowAcquisitionNotCommitted, RuntimeBackedWindowAcquisitionService,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct AbandonmentFingerprint {
    window_id: WindowId,
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    target: RememberedTarget,
    placement: WindowPlacement,
    disposition: RuntimeBackedWindowAcquisitionDisposition,
    fallback_thread_id: SyndicThreadId,
    fallback_draft_id: SyndicDraftId,
    fallback_execution: ExecutionBinding,
    fallback_created_at: SyndicTimestamp,
}

impl From<&RuntimeBackedWindowAcquisition> for AbandonmentFingerprint {
    fn from(acquisition: &RuntimeBackedWindowAcquisition) -> Self {
        Self {
            window_id: acquisition.window_id,
            thread_id: acquisition.thread_id,
            draft_id: acquisition.draft_id,
            target: acquisition.target,
            placement: acquisition.placement.clone(),
            disposition: acquisition.disposition,
            fallback_thread_id: acquisition.fallback_thread_id,
            fallback_draft_id: acquisition.fallback_draft_id,
            fallback_execution: acquisition.fallback_execution.clone(),
            fallback_created_at: acquisition.fallback_created_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBackedWindowAbandonmentAuditSeed {
    fingerprint: AbandonmentFingerprint,
    state: WindowAcquisitionCommittedFacts,
    syndic: PristineThreadCandidate,
}

impl RuntimeBackedWindowAbandonmentAuditSeed {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.fingerprint.window_id
    }
}

#[derive(Debug)]
pub struct RuntimeBackedWindowAbandonment {
    seed: RuntimeBackedWindowAbandonmentAuditSeed,
    flight: AcquisitionFlight,
}

impl RuntimeBackedWindowAbandonment {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.seed.fingerprint.window_id
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.seed.fingerprint.thread_id
    }

    #[must_use]
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.seed.fingerprint.draft_id
    }

    #[must_use]
    pub const fn disposition(&self) -> RuntimeBackedWindowAcquisitionDisposition {
        self.seed.fingerprint.disposition
    }

    #[must_use]
    pub fn audit_seed(&self) -> RuntimeBackedWindowAbandonmentAuditSeed {
        self.seed.clone()
    }

    #[must_use]
    pub fn into_audit_seed(self) -> RuntimeBackedWindowAbandonmentAuditSeed {
        let Self { seed, flight } = self;
        drop(flight);
        seed
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeBackedWindowAbandonmentNotCommitted {
    #[error("another acquisition or abandonment already owns this window identity")]
    DuplicateWindowIdentity,
    #[error("the bounded window-operation flight capacity is full")]
    FlightCapacity,
    #[error("window abandonment was cancelled before writer admission")]
    Cancelled,
    #[error("active durable work still depends on the acquired thread")]
    ActiveDurableJob,
    #[error("window abandonment audit failed: {0}")]
    Audit(Box<dyn Error + Send + Sync>),
    #[error("window abandonment preparation failed: {0}")]
    Preparation(Box<dyn Error + Send + Sync>),
    #[error("window abandonment command construction failed: {0}")]
    CommandBuild(beryl_home_store::CommandBuildError),
    #[error("window abandonment did not commit: {0}")]
    Command(CommandError),
}

#[derive(Debug)]
pub enum RuntimeBackedWindowAbandonmentPreparationOutcome {
    NotCommitted {
        acquisition: RuntimeBackedWindowAcquisition,
        evidence: RuntimeBackedWindowAbandonmentNotCommitted,
    },
    ExactAcquired {
        abandonment: RuntimeBackedWindowAbandonment,
    },
    Collision {
        window_id: WindowId,
    },
}

#[derive(Debug)]
pub enum RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome {
    NotCommitted {
        seed: RuntimeBackedWindowAbandonmentAuditSeed,
        evidence: RuntimeBackedWindowAbandonmentNotCommitted,
    },
    ExactAcquired {
        abandonment: RuntimeBackedWindowAbandonment,
    },
    ExactAbandoned {
        window_id: WindowId,
    },
    Collision {
        window_id: WindowId,
    },
}

#[derive(Debug)]
pub enum RuntimeBackedWindowAbandonmentOutcome {
    NotCommitted {
        abandonment: RuntimeBackedWindowAbandonment,
        evidence: RuntimeBackedWindowAbandonmentNotCommitted,
    },
    Committed {
        window_id: WindowId,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate {
        window_id: WindowId,
        failure: CommandError,
        reconciliation: RuntimeBackedWindowAbandonmentReconciliation,
    },
}

#[derive(Debug)]
pub enum RuntimeBackedWindowAbandonmentReconciliationOutcome {
    Pending {
        failure: ReconciliationFailure,
        reconciliation: RuntimeBackedWindowAbandonmentReconciliation,
    },
    ExactAcquired {
        abandonment: RuntimeBackedWindowAbandonment,
    },
    ExactAbandoned {
        window_id: WindowId,
        receipt: CommitReceipt,
    },
    Collision {
        window_id: WindowId,
    },
}

#[derive(Debug)]
pub struct RuntimeBackedWindowAbandonmentReconciliation {
    abandonment: RuntimeBackedWindowAbandonment,
    handle: ReconciliationHandle,
}

impl RuntimeBackedWindowAbandonmentReconciliation {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.abandonment.window_id()
    }

    #[must_use]
    pub fn audit_seed(&self) -> RuntimeBackedWindowAbandonmentAuditSeed {
        self.abandonment.audit_seed()
    }

    #[must_use]
    pub fn reconcile(
        self,
        store: &HomeStore,
    ) -> RuntimeBackedWindowAbandonmentReconciliationOutcome {
        match store.reconcile(&self.handle) {
            Ok(ReconciliationResolution::ExactOld) => {
                RuntimeBackedWindowAbandonmentReconciliationOutcome::ExactAcquired {
                    abandonment: self.abandonment,
                }
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                RuntimeBackedWindowAbandonmentReconciliationOutcome::ExactAbandoned {
                    window_id: self.abandonment.window_id(),
                    receipt,
                }
            }
            Ok(ReconciliationResolution::ExactSuccessor { .. })
            | Ok(ReconciliationResolution::Collision) => {
                RuntimeBackedWindowAbandonmentReconciliationOutcome::Collision {
                    window_id: self.abandonment.window_id(),
                }
            }
            Err(failure) => RuntimeBackedWindowAbandonmentReconciliationOutcome::Pending {
                failure,
                reconciliation: self,
            },
        }
    }
}

impl RuntimeBackedWindowAcquisitionService {
    pub(crate) fn validate_shell_selection(
        &self,
        acquisition: &RuntimeBackedWindowAcquisition,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        let binding = selection.binding();
        if acquisition.home_id != self.store.home_id()
            || binding.home_id() != acquisition.home_id
            || self.store.health().generation() != Some(binding.home_generation())
            || selection.window_id() != acquisition.window_id
            || selection.claim().thread_id() != acquisition.thread_id
            || binding.candidate().draft_id() != acquisition.draft_id
        {
            return Err("shell selection belongs to a different acquisition or home".to_owned());
        }
        self.validate_initial_composer_claim(acquisition, selection.claim(), &self.store)
    }

    pub(crate) fn validate_initial_composer_claim(
        &self,
        acquisition: &RuntimeBackedWindowAcquisition,
        claim: beryl_state::WindowClaimSelection,
        store: &HomeStore,
    ) -> Result<(), String> {
        if !std::ptr::eq(store, self.store.as_ref())
            || acquisition.home_id != store.home_id()
            || claim.thread_id() != acquisition.thread_id
        {
            return Err(
                "initial composer belongs to a different acquisition service or home".to_owned(),
            );
        }
        for _ in 0..=beryl_state::MAX_RESTORABLE_WINDOWS {
            let before = self
                .store
                .home_revision()
                .map_err(|error| error.to_string())?;
            let state = self
                .state
                .audit_window_acquisition_for_thread_with_cancellation(
                    &self.store,
                    acquisition.window_id,
                    acquisition.thread_id,
                    &CommandCancellation::new(),
                )
                .map_err(|error| error.to_string())?;
            let WindowAcquisitionNaturalState::Committed(state) = state else {
                return Err("shell acquisition no longer owns its exact claim".to_owned());
            };
            if !state_matches_acquisition(&state, acquisition)
                || state.claim_generation() != claim.generation()
                || state.claim_revision() != claim.revision()
            {
                return Err(
                    "shell selection does not match the acquired runtime, root, and claim"
                        .to_owned(),
                );
            }
            let candidate = self
                .syndic
                .audit_pristine_thread(
                    &self.store,
                    acquisition.thread_id,
                    &acquisition.fallback_execution,
                )
                .map_err(|error| error.to_string())?;
            if !matches!(candidate, PristineThreadAudit::Exact(ref candidate) if candidate_matches_acquisition(candidate, acquisition))
            {
                return Err(
                    "shell editor does not match the acquired draft and execution binding"
                        .to_owned(),
                );
            }
            if before
                == self
                    .store
                    .home_revision()
                    .map_err(|error| error.to_string())?
            {
                return Ok(());
            }
        }
        Err("shell binding preparation exceeded its revision retry bound".to_owned())
    }

    #[must_use]
    pub fn prepare_abandonment(
        &self,
        acquisition: RuntimeBackedWindowAcquisition,
        cancellation: CommandCancellation,
    ) -> RuntimeBackedWindowAbandonmentPreparationOutcome {
        if acquisition.home_id != self.store.home_id() {
            return abandonment_preparation_not_committed(
                acquisition,
                abandonment_audit(AbandonmentInvariant(
                    "acquisition belongs to a different home",
                )),
            );
        }
        let window_id = acquisition.window_id;
        let flight = match AcquisitionFlight::acquire(self.flights.clone(), window_id) {
            Ok(flight) => flight,
            Err(evidence) => {
                return RuntimeBackedWindowAbandonmentPreparationOutcome::NotCommitted {
                    acquisition,
                    evidence: map_flight_error(evidence),
                };
            }
        };
        for _ in 0..=beryl_state::MAX_RESTORABLE_WINDOWS {
            if cancellation.is_cancelled() {
                return abandonment_preparation_not_committed(
                    acquisition,
                    RuntimeBackedWindowAbandonmentNotCommitted::Cancelled,
                );
            }
            let before = match self.store.home_revision() {
                Ok(revision) => revision,
                Err(error) => {
                    return abandonment_preparation_not_committed(
                        acquisition,
                        abandonment_audit(error),
                    );
                }
            };
            let state = match self
                .state
                .audit_window_acquisition_for_thread_with_cancellation(
                    &self.store,
                    window_id,
                    acquisition.thread_id,
                    &cancellation,
                ) {
                Ok(state) => state,
                Err(WindowAcquisitionAuditError::ConcurrentPublication) => continue,
                Err(WindowAcquisitionAuditError::Cancelled) => {
                    return abandonment_preparation_not_committed(
                        acquisition,
                        RuntimeBackedWindowAbandonmentNotCommitted::Cancelled,
                    );
                }
                Err(error) => {
                    return abandonment_preparation_not_committed(
                        acquisition,
                        abandonment_audit(error),
                    );
                }
            };
            let WindowAcquisitionNaturalState::Committed(state) = state else {
                return RuntimeBackedWindowAbandonmentPreparationOutcome::Collision { window_id };
            };
            if !state_matches_acquisition(&state, &acquisition) {
                return RuntimeBackedWindowAbandonmentPreparationOutcome::Collision { window_id };
            }
            let syndic = match self.syndic.audit_pristine_thread(
                &self.store,
                acquisition.thread_id,
                &acquisition.fallback_execution,
            ) {
                Ok(PristineThreadAudit::Exact(candidate))
                    if candidate_matches_acquisition(&candidate, &acquisition) =>
                {
                    candidate
                }
                Ok(
                    PristineThreadAudit::Exact(_)
                    | PristineThreadAudit::Missing
                    | PristineThreadAudit::Conflict,
                ) => {
                    return RuntimeBackedWindowAbandonmentPreparationOutcome::Collision {
                        window_id,
                    };
                }
                Err(error) => {
                    return abandonment_preparation_not_committed(
                        acquisition,
                        abandonment_audit(error),
                    );
                }
            };
            if cancellation.is_cancelled() {
                return abandonment_preparation_not_committed(
                    acquisition,
                    RuntimeBackedWindowAbandonmentNotCommitted::Cancelled,
                );
            }
            let after = match self.store.home_revision() {
                Ok(revision) => revision,
                Err(error) => {
                    return abandonment_preparation_not_committed(
                        acquisition,
                        abandonment_audit(error),
                    );
                }
            };
            if before != after {
                continue;
            }
            let seed = RuntimeBackedWindowAbandonmentAuditSeed {
                fingerprint: AbandonmentFingerprint::from(&acquisition),
                state,
                syndic,
            };
            return RuntimeBackedWindowAbandonmentPreparationOutcome::ExactAcquired {
                abandonment: RuntimeBackedWindowAbandonment { seed, flight },
            };
        }
        abandonment_preparation_not_committed(
            acquisition,
            abandonment_audit(AbandonmentInvariant(
                "abandonment preparation audit retry budget exhausted",
            )),
        )
    }

    #[must_use]
    pub fn reconcile_abandonment(
        &self,
        seed: RuntimeBackedWindowAbandonmentAuditSeed,
        cancellation: CommandCancellation,
    ) -> RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome {
        let window_id = seed.window_id();
        let flight = match AcquisitionFlight::acquire(self.flights.clone(), window_id) {
            Ok(flight) => flight,
            Err(evidence) => {
                return abandonment_natural_not_committed(seed, map_flight_error(evidence));
            }
        };
        for _ in 0..=beryl_state::MAX_RESTORABLE_WINDOWS {
            if cancellation.is_cancelled() {
                return abandonment_natural_not_committed(
                    seed,
                    RuntimeBackedWindowAbandonmentNotCommitted::Cancelled,
                );
            }
            let before = match self.store.home_revision() {
                Ok(revision) => revision,
                Err(error) => {
                    return abandonment_natural_not_committed(seed, abandonment_audit(error));
                }
            };
            let state = match self.state.audit_window_abandonment_with_cancellation(
                &self.store,
                &seed.state,
                &cancellation,
            ) {
                Ok(state) => state,
                Err(WindowAcquisitionAuditError::ConcurrentPublication) => continue,
                Err(WindowAcquisitionAuditError::Cancelled) => {
                    return abandonment_natural_not_committed(
                        seed,
                        RuntimeBackedWindowAbandonmentNotCommitted::Cancelled,
                    );
                }
                Err(error) => {
                    return abandonment_natural_not_committed(seed, abandonment_audit(error));
                }
            };
            let removal = match self
                .syndic
                .audit_pristine_thread_removal(&self.store, &seed.syndic)
            {
                Ok(removal) => removal,
                Err(error) => {
                    return abandonment_natural_not_committed(seed, abandonment_audit(error));
                }
            };
            let pristine = match self.syndic.audit_pristine_thread(
                &self.store,
                seed.fingerprint.thread_id,
                &seed.fingerprint.fallback_execution,
            ) {
                Ok(pristine) => pristine,
                Err(error) => {
                    return abandonment_natural_not_committed(seed, abandonment_audit(error));
                }
            };
            if cancellation.is_cancelled() {
                return abandonment_natural_not_committed(
                    seed,
                    RuntimeBackedWindowAbandonmentNotCommitted::Cancelled,
                );
            }
            let after = match self.store.home_revision() {
                Ok(revision) => revision,
                Err(error) => {
                    return abandonment_natural_not_committed(seed, abandonment_audit(error));
                }
            };
            if before != after {
                continue;
            }
            match (state, removal, pristine, seed.fingerprint.disposition) {
                (
                    WindowAbandonmentNaturalState::ExactAcquired(state),
                    PristineThreadRemovalAudit::Present,
                    PristineThreadAudit::Exact(syndic),
                    _,
                ) if candidate_matches_fingerprint(&syndic, &seed.fingerprint) => {
                    let seed = RuntimeBackedWindowAbandonmentAuditSeed {
                        state,
                        syndic,
                        ..seed
                    };
                    return RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::ExactAcquired {
                        abandonment: RuntimeBackedWindowAbandonment { seed, flight },
                    };
                }
                (
                    WindowAbandonmentNaturalState::ExactAbandoned,
                    PristineThreadRemovalAudit::Present,
                    PristineThreadAudit::Exact(_),
                    RuntimeBackedWindowAcquisitionDisposition::Reused,
                ) => {
                    return RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::ExactAbandoned {
                        window_id,
                    };
                }
                (
                    WindowAbandonmentNaturalState::ExactAbandoned,
                    PristineThreadRemovalAudit::Removed,
                    PristineThreadAudit::Missing,
                    RuntimeBackedWindowAcquisitionDisposition::Created,
                ) => {
                    return RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::ExactAbandoned {
                        window_id,
                    };
                }
                _ => {
                    return RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::Collision {
                        window_id,
                    };
                }
            }
        }
        abandonment_natural_not_committed(
            seed,
            abandonment_audit(AbandonmentInvariant(
                "abandonment natural-state audit retry budget exhausted",
            )),
        )
    }

    #[must_use]
    pub fn abandon(
        &self,
        abandonment: RuntimeBackedWindowAbandonment,
        cancellation: CommandCancellation,
    ) -> RuntimeBackedWindowAbandonmentOutcome {
        let window_id = abandonment.window_id();
        if cancellation.is_cancelled() {
            return abandonment_not_committed(
                abandonment,
                RuntimeBackedWindowAbandonmentNotCommitted::Cancelled,
            );
        }
        let home_revision = match self.store.home_revision() {
            Ok(revision) => revision,
            Err(error) => {
                return abandonment_not_committed(abandonment, abandonment_preparation(error));
            }
        };
        let durable_job_revision = match self.state.durable_jobs().revision(&self.store) {
            Ok(revision) => revision,
            Err(error) => {
                return abandonment_not_committed(abandonment, abandonment_preparation(error));
            }
        };
        let job_guard = match self
            .state
            .durable_jobs()
            .thread_reuse_guard(&self.store, abandonment.thread_id())
        {
            Ok(Some(guard)) => guard,
            Ok(None) => {
                return abandonment_not_committed(
                    abandonment,
                    RuntimeBackedWindowAbandonmentNotCommitted::ActiveDurableJob,
                );
            }
            Err(error) => {
                return abandonment_not_committed(abandonment, abandonment_preparation(error));
            }
        };
        let mut command = HomeCommand::new(home_revision).with_cancellation(cancellation.clone());
        if let Err(error) = command.add(self.state.session().abandon_window(
            abandonment.seed.state.session_domain_revision(),
            abandonment.seed.state.abandon_session_window(),
        )) {
            return abandonment_not_committed(
                abandonment,
                RuntimeBackedWindowAbandonmentNotCommitted::CommandBuild(error),
            );
        }
        let catalog = match abandonment.disposition() {
            RuntimeBackedWindowAcquisitionDisposition::Reused => {
                self.state.catalog().release_claim(
                    abandonment.seed.state.catalog_domain_revision(),
                    abandonment.seed.state.release_catalog_claim(),
                )
            }
            RuntimeBackedWindowAcquisitionDisposition::Created => {
                self.state.catalog().delete_claimed_row(
                    abandonment.seed.state.catalog_domain_revision(),
                    abandonment.seed.state.delete_catalog_claimed_row(),
                )
            }
        };
        if let Err(error) = command.add(catalog) {
            return abandonment_not_committed(
                abandonment,
                RuntimeBackedWindowAbandonmentNotCommitted::CommandBuild(error),
            );
        }
        if let Err(error) = command.add_validation(
            self.state
                .durable_jobs()
                .validate_thread_reuse_guard(durable_job_revision, job_guard),
        ) {
            return abandonment_not_committed(
                abandonment,
                RuntimeBackedWindowAbandonmentNotCommitted::CommandBuild(error),
            );
        }
        let syndic = match abandonment.disposition() {
            RuntimeBackedWindowAcquisitionDisposition::Reused => self
                .syndic
                .validate_pristine_thread(abandonment.seed.syndic.clone()),
            RuntimeBackedWindowAcquisitionDisposition::Created => {
                if let Err(error) = command.add(
                    self.syndic
                        .delete_pristine_thread(abandonment.seed.syndic.clone()),
                ) {
                    return abandonment_not_committed(
                        abandonment,
                        RuntimeBackedWindowAbandonmentNotCommitted::CommandBuild(error),
                    );
                }
                return self.execute_abandonment(command, abandonment, window_id);
            }
        };
        if let Err(error) = command.add_validation(syndic) {
            return abandonment_not_committed(
                abandonment,
                RuntimeBackedWindowAbandonmentNotCommitted::CommandBuild(error),
            );
        }
        self.execute_abandonment(command, abandonment, window_id)
    }

    fn execute_abandonment(
        &self,
        command: HomeCommand,
        abandonment: RuntimeBackedWindowAbandonment,
        window_id: WindowId,
    ) -> RuntimeBackedWindowAbandonmentOutcome {
        #[cfg(feature = "test-faults")]
        let hook = {
            self.before_execute
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        };
        #[cfg(feature = "test-faults")]
        if let Some(hook) = hook {
            hook();
        }
        match self.store.execute(command) {
            CommandOutcome::NotCommitted { evidence } => abandonment_not_committed(
                abandonment,
                RuntimeBackedWindowAbandonmentNotCommitted::Command(evidence),
            ),
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => RuntimeBackedWindowAbandonmentOutcome::Committed {
                window_id,
                receipt,
                later_failure,
                local_finalization,
            },
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => RuntimeBackedWindowAbandonmentOutcome::Indeterminate {
                window_id,
                failure,
                reconciliation: RuntimeBackedWindowAbandonmentReconciliation {
                    abandonment,
                    handle: reconciliation.install_and_handle(),
                },
            },
        }
    }
}

fn state_matches_acquisition(
    state: &WindowAcquisitionCommittedFacts,
    acquisition: &RuntimeBackedWindowAcquisition,
) -> bool {
    state.window_id() == acquisition.window_id
        && state.thread_id() == acquisition.thread_id
        && state.target() == acquisition.target
        && state.placement() == &acquisition.placement
        && state.fallback_target() == acquisition.target
        && matches!(
            (state.origin(), acquisition.disposition),
            (
                WindowAcquisitionThreadOrigin::Reused,
                RuntimeBackedWindowAcquisitionDisposition::Reused
            ) | (
                WindowAcquisitionThreadOrigin::CreatedFallback,
                RuntimeBackedWindowAcquisitionDisposition::Created
            )
        )
}

fn candidate_matches_acquisition(
    candidate: &PristineThreadCandidate,
    acquisition: &RuntimeBackedWindowAcquisition,
) -> bool {
    candidate_matches_fingerprint(candidate, &AbandonmentFingerprint::from(acquisition))
}

fn candidate_matches_fingerprint(
    candidate: &PristineThreadCandidate,
    fingerprint: &AbandonmentFingerprint,
) -> bool {
    candidate.thread_id() == fingerprint.thread_id
        && candidate.draft_id() == fingerprint.draft_id
        && candidate.execution() == &fingerprint.fallback_execution
        && (fingerprint.disposition == RuntimeBackedWindowAcquisitionDisposition::Reused
            || (candidate.thread_id() == fingerprint.fallback_thread_id
                && candidate.draft_id() == fingerprint.fallback_draft_id
                && candidate.created_at() == fingerprint.fallback_created_at))
}

fn map_flight_error(
    evidence: RuntimeBackedWindowAcquisitionNotCommitted,
) -> RuntimeBackedWindowAbandonmentNotCommitted {
    match evidence {
        RuntimeBackedWindowAcquisitionNotCommitted::DuplicateWindowIdentity => {
            RuntimeBackedWindowAbandonmentNotCommitted::DuplicateWindowIdentity
        }
        RuntimeBackedWindowAcquisitionNotCommitted::FlightCapacity => {
            RuntimeBackedWindowAbandonmentNotCommitted::FlightCapacity
        }
        _ => unreachable!("flight admission returns only duplicate or capacity evidence"),
    }
}

fn abandonment_preparation_not_committed(
    acquisition: RuntimeBackedWindowAcquisition,
    evidence: RuntimeBackedWindowAbandonmentNotCommitted,
) -> RuntimeBackedWindowAbandonmentPreparationOutcome {
    RuntimeBackedWindowAbandonmentPreparationOutcome::NotCommitted {
        acquisition,
        evidence,
    }
}

fn abandonment_natural_not_committed(
    seed: RuntimeBackedWindowAbandonmentAuditSeed,
    evidence: RuntimeBackedWindowAbandonmentNotCommitted,
) -> RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome {
    RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::NotCommitted { seed, evidence }
}

fn abandonment_not_committed(
    abandonment: RuntimeBackedWindowAbandonment,
    evidence: RuntimeBackedWindowAbandonmentNotCommitted,
) -> RuntimeBackedWindowAbandonmentOutcome {
    RuntimeBackedWindowAbandonmentOutcome::NotCommitted {
        abandonment,
        evidence,
    }
}

fn abandonment_audit(
    error: impl Error + Send + Sync + 'static,
) -> RuntimeBackedWindowAbandonmentNotCommitted {
    RuntimeBackedWindowAbandonmentNotCommitted::Audit(Box::new(error))
}

fn abandonment_preparation(
    error: impl Error + Send + Sync + 'static,
) -> RuntimeBackedWindowAbandonmentNotCommitted {
    RuntimeBackedWindowAbandonmentNotCommitted::Preparation(Box::new(error))
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct AbandonmentInvariant(&'static str);
