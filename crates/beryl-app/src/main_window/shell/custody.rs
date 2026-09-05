use super::*;

pub struct MainWindowShellUnpublished {
    pub(super) acquisition: RuntimeBackedWindowAcquisition,
    pub(super) reservation: RuntimeBackedWindowMainWindowReservation,
    pub(super) initial_composer: Option<Box<InitialComposerCandidate>>,
}

impl MainWindowShellUnpublished {
    pub(in crate::main_window) fn from_retired_initial_composer(
        acquisition: RuntimeBackedWindowAcquisition,
        reservation: RuntimeBackedWindowMainWindowReservation,
    ) -> Self {
        Self {
            acquisition,
            reservation,
            initial_composer: None,
        }
    }

    #[must_use]
    pub const fn window_id(&self) -> beryl_model::WindowId {
        self.acquisition.window_id()
    }

    #[must_use]
    pub fn prepare_abandonment(
        mut self,
        service: &RuntimeBackedWindowAcquisitionService,
        cancellation: CommandCancellation,
    ) -> MainWindowShellAbandonmentPreparationOutcome {
        if let Some(candidate) = self.initial_composer.as_mut() {
            let error = match candidate.drive_retirement(cancellation.clone()) {
                Ok(true) => None,
                Ok(false) => Some("initial composer retirement remains pending".to_owned()),
                Err(error) => Some(error),
            };
            if let Some(error) = error {
                return MainWindowShellAbandonmentPreparationOutcome::InitialComposerPending {
                    unpublished: self,
                    error,
                };
            }
            self.initial_composer = None;
        }
        let Self {
            acquisition,
            reservation,
            initial_composer: _,
        } = self;
        match service.prepare_abandonment(acquisition, cancellation) {
            RuntimeBackedWindowAbandonmentPreparationOutcome::NotCommitted {
                acquisition,
                evidence,
            } => MainWindowShellAbandonmentPreparationOutcome::NotCommitted {
                unpublished: Self {
                    acquisition,
                    reservation,
                    initial_composer: None,
                },
                evidence,
            },
            RuntimeBackedWindowAbandonmentPreparationOutcome::ExactAcquired { abandonment } => {
                MainWindowShellAbandonmentPreparationOutcome::ExactAcquired {
                    abandonment: MainWindowShellAbandonment {
                        abandonment,
                        reservation,
                    },
                }
            }
            RuntimeBackedWindowAbandonmentPreparationOutcome::Collision { window_id } => {
                MainWindowShellAbandonmentPreparationOutcome::Collision { window_id }
            }
        }
    }
}

pub enum MainWindowShellAbandonmentPreparationOutcome {
    InitialComposerPending {
        unpublished: MainWindowShellUnpublished,
        error: String,
    },
    NotCommitted {
        unpublished: MainWindowShellUnpublished,
        evidence: RuntimeBackedWindowAbandonmentNotCommitted,
    },
    ExactAcquired {
        abandonment: MainWindowShellAbandonment,
    },
    Collision {
        window_id: beryl_model::WindowId,
    },
}

pub struct MainWindowShellAbandonment {
    abandonment: RuntimeBackedWindowAbandonment,
    pub(super) reservation: RuntimeBackedWindowMainWindowReservation,
}

impl MainWindowShellAbandonment {
    #[must_use]
    pub const fn window_id(&self) -> beryl_model::WindowId {
        self.abandonment.window_id()
    }

    #[must_use]
    pub fn audit_seed(&self) -> RuntimeBackedWindowAbandonmentAuditSeed {
        self.abandonment.audit_seed()
    }

    #[must_use]
    pub fn abandon(
        self,
        service: &RuntimeBackedWindowAcquisitionService,
        cancellation: CommandCancellation,
    ) -> MainWindowShellAbandonmentOutcome {
        let Self {
            abandonment,
            reservation,
        } = self;
        match service.abandon(abandonment, cancellation) {
            RuntimeBackedWindowAbandonmentOutcome::NotCommitted {
                abandonment,
                evidence,
            } => MainWindowShellAbandonmentOutcome::NotCommitted {
                abandonment: Self {
                    abandonment,
                    reservation,
                },
                evidence,
            },
            RuntimeBackedWindowAbandonmentOutcome::Committed {
                window_id,
                receipt,
                later_failure,
                local_finalization,
            } => MainWindowShellAbandonmentOutcome::Committed {
                window_id,
                receipt,
                later_failure,
                local_finalization,
            },
            RuntimeBackedWindowAbandonmentOutcome::Indeterminate {
                window_id,
                failure,
                reconciliation,
            } => MainWindowShellAbandonmentOutcome::Indeterminate {
                window_id,
                failure,
                reconciliation: MainWindowShellAbandonmentReconciliation {
                    reconciliation,
                    reservation,
                },
            },
        }
    }
}

pub enum MainWindowShellAbandonmentOutcome {
    NotCommitted {
        abandonment: MainWindowShellAbandonment,
        evidence: RuntimeBackedWindowAbandonmentNotCommitted,
    },
    Committed {
        window_id: beryl_model::WindowId,
        receipt: beryl_home_store::CommitReceipt,
        later_failure: Option<beryl_home_store::CommandError>,
        local_finalization: Option<beryl_home_store::CommittedLocalFinalization>,
    },
    Indeterminate {
        window_id: beryl_model::WindowId,
        failure: beryl_home_store::CommandError,
        reconciliation: MainWindowShellAbandonmentReconciliation,
    },
}

pub struct MainWindowShellAbandonmentReconciliation {
    reconciliation: RuntimeBackedWindowAbandonmentReconciliation,
    pub(super) reservation: RuntimeBackedWindowMainWindowReservation,
}

impl MainWindowShellAbandonmentReconciliation {
    #[must_use]
    pub const fn window_id(&self) -> beryl_model::WindowId {
        self.reconciliation.window_id()
    }

    #[must_use]
    pub fn reconcile(
        self,
        store: &beryl_home_store::HomeStore,
    ) -> MainWindowShellAbandonmentReconciliationOutcome {
        let Self {
            reconciliation,
            reservation,
        } = self;
        match reconciliation.reconcile(store) {
            RuntimeBackedWindowAbandonmentReconciliationOutcome::Pending {
                failure,
                reconciliation,
            } => MainWindowShellAbandonmentReconciliationOutcome::Pending {
                failure,
                reconciliation: Self {
                    reconciliation,
                    reservation,
                },
            },
            RuntimeBackedWindowAbandonmentReconciliationOutcome::ExactAcquired { abandonment } => {
                MainWindowShellAbandonmentReconciliationOutcome::ExactAcquired {
                    abandonment: MainWindowShellAbandonment {
                        abandonment,
                        reservation,
                    },
                }
            }
            RuntimeBackedWindowAbandonmentReconciliationOutcome::ExactAbandoned {
                window_id,
                receipt,
            } => MainWindowShellAbandonmentReconciliationOutcome::ExactAbandoned {
                window_id,
                receipt,
            },
            RuntimeBackedWindowAbandonmentReconciliationOutcome::Collision { window_id } => {
                MainWindowShellAbandonmentReconciliationOutcome::Collision { window_id }
            }
        }
    }
}

pub enum MainWindowShellAbandonmentReconciliationOutcome {
    Pending {
        failure: beryl_home_store::ReconciliationFailure,
        reconciliation: MainWindowShellAbandonmentReconciliation,
    },
    ExactAcquired {
        abandonment: MainWindowShellAbandonment,
    },
    ExactAbandoned {
        window_id: beryl_model::WindowId,
        receipt: beryl_home_store::CommitReceipt,
    },
    Collision {
        window_id: beryl_model::WindowId,
    },
}
