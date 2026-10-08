use super::*;

pub struct AdmissionFacts {
    pub(super) window_id: WindowId,
    pub(super) runtime_id: RuntimeId,
    pub(super) root_id: RootId,
    pub(super) onboarding: Option<OnboardingFacts>,
}

impl AdmissionFacts {
    pub fn window_id(&self) -> WindowId {
        self.window_id
    }
    pub fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }
    pub fn root_id(&self) -> RootId {
        self.root_id
    }
    pub fn onboarding(&self) -> Option<&OnboardingFacts> {
        self.onboarding.as_ref()
    }
}

#[must_use]
pub enum RuntimeAdmissionOutcome {
    Existing {
        runtime_id: RuntimeId,
        root_id: Option<RootId>,
    },
    NotCommitted {
        error: AdmissionError,
    },
    Committed {
        admission: CommittedAdmission,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate {
        failure: CommandError,
        reconciliation: AdmissionReconciliation,
    },
}

#[must_use]
pub struct CommittedAdmission {
    pub(super) facts: AdmissionFacts,
    pub(super) lease: WindowSelectionLease,
    pub(super) store: Arc<HomeServiceReference>,
    pub(super) generation: HomeGenerationIdentity,
    pub(super) lifetime: Arc<AdmissionLifetime>,
}

impl CommittedAdmission {
    pub fn facts(&self) -> &AdmissionFacts {
        &self.facts
    }
    pub fn validate_publication(&self) -> Result<(), AdmissionError> {
        let state = self
            .lifetime
            .state
            .try_lock()
            .map_err(|_| AdmissionError::Busy)?;
        if state.retired {
            return Err(AdmissionError::Busy);
        }
        if self.store.generation_identity().map_err(preparation)? != self.generation {
            return Err(invalid(
                "committed admission belongs to a retired home generation",
            ));
        }
        self.lease.validate_publication().map_err(preparation)
    }
}

#[must_use]
pub struct AdmissionReconciliation {
    pub(super) admission: CommittedAdmission,
    pub(super) handle: ReconciliationHandle,
}

#[must_use]
pub enum AdmissionReconciliationOutcome {
    NotCommitted,
    Committed {
        admission: CommittedAdmission,
        receipt: CommitReceipt,
    },
    Pending {
        failure: ReconciliationFailure,
        reconciliation: AdmissionReconciliation,
    },
    Unavailable {
        custody: UnavailableAdmission,
    },
}

#[must_use]
pub struct UnavailableAdmission {
    pub(super) admission: CommittedAdmission,
    pub(super) _handle: ReconciliationHandle,
}

impl UnavailableAdmission {
    pub fn facts(&self) -> &AdmissionFacts {
        self.admission.facts()
    }
}

impl AdmissionReconciliation {
    pub fn retry(self, store: &HomeStore) -> AdmissionReconciliationOutcome {
        let result = store.retry_reconciliation(&self.handle);
        self.classify(result)
    }

    pub fn retry_candidate(
        self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
    ) -> AdmissionReconciliationOutcome {
        let result = access.retry_reconciliation(&self.handle);
        self.classify(result)
    }
    pub fn reconcile(self, store: &HomeStore) -> AdmissionReconciliationOutcome {
        let result = store.reconcile(&self.handle);
        self.classify(result)
    }

    pub fn reconcile_candidate(
        self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
    ) -> AdmissionReconciliationOutcome {
        let result = access.reconcile(&self.handle);
        self.classify(result)
    }

    fn classify(
        self,
        result: Result<ReconciliationResolution, ReconciliationFailure>,
    ) -> AdmissionReconciliationOutcome {
        match result {
            Ok(ReconciliationResolution::ExactOld) => AdmissionReconciliationOutcome::NotCommitted,
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                AdmissionReconciliationOutcome::Committed {
                    admission: self.admission,
                    receipt,
                }
            }
            Ok(
                ReconciliationResolution::ExactSuccessor { .. }
                | ReconciliationResolution::Collision,
            ) => AdmissionReconciliationOutcome::Unavailable {
                custody: UnavailableAdmission {
                    admission: self.admission,
                    _handle: self.handle,
                },
            },
            Err(failure) => AdmissionReconciliationOutcome::Pending {
                failure,
                reconciliation: self,
            },
        }
    }
}
