use super::*;

impl MainWindowCreation {
    #[inline(never)]
    pub(super) fn request(
        &mut self,
        target: RememberedTarget,
        reservation: RuntimeBackedWindowMainWindowReservation,
    ) -> CreationStep {
        if self.cancellation.is_cancelled() {
            self.error = Some("New Window creation was cancelled.".to_owned());
            return CreationStep::Continue;
        }
        match (self.services.request_source)(self.window_id, target) {
            Ok(request) if request.window_id() == self.window_id && request.target() == target => {
                self.state = CreationState::Acquire {
                    request,
                    reservation,
                };
            }
            Ok(_) => {
                self.error = Some("New Window request changed its captured target.".to_owned())
            }
            Err(error) => self.error = Some(error),
        }

        CreationStep::Continue
    }

    #[inline(never)]
    pub(super) fn acquire(
        &mut self,
        request: RuntimeBackedWindowAcquisitionRequest,
        reservation: RuntimeBackedWindowMainWindowReservation,
    ) -> CreationStep {
        match self
            .services
            .acquisition
            .acquire(request.clone(), self.cancellation.clone())
        {
            RuntimeBackedWindowAcquisitionOutcome::NotCommitted { evidence, .. } => {
                self.error = Some(format!(
                    "New Window acquisition did not commit: {evidence:?}"
                ));
            }
            RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. }
            | RuntimeBackedWindowAcquisitionOutcome::ExactCommitted { acquisition } => {
                self.state = CreationState::Acquired {
                    acquisition,
                    reservation,
                };
            }
            RuntimeBackedWindowAcquisitionOutcome::Indeterminate { reconciliation, .. } => {
                self.state = CreationState::AcquisitionPending {
                    reconciliation,
                    reservation,
                };
                return CreationStep::Pending;
            }
            RuntimeBackedWindowAcquisitionOutcome::RepairIndeterminate {
                reconciliation, ..
            } => {
                self.state = CreationState::RepairPending {
                    request,
                    reconciliation,
                    reservation,
                };
                return CreationStep::Pending;
            }
        }

        CreationStep::Continue
    }

    #[inline(never)]
    pub(super) fn reconcile_acquisition(
        &mut self,
        reconciliation: RuntimeBackedWindowAcquisitionReconciliation,
        reservation: RuntimeBackedWindowMainWindowReservation,
    ) -> CreationStep {
        match reconciliation.reconcile(&self.services.store) {
            RuntimeBackedWindowAcquisitionReconciliationOutcome::Pending {
                reconciliation, ..
            } => {
                self.state = CreationState::AcquisitionPending {
                    reconciliation,
                    reservation,
                };
                return CreationStep::Pending;
            }
            RuntimeBackedWindowAcquisitionReconciliationOutcome::ExactNew {
                acquisition, ..
            } => {
                self.state = CreationState::Acquired {
                    acquisition,
                    reservation,
                };
            }
            RuntimeBackedWindowAcquisitionReconciliationOutcome::ExactOld { .. }
            | RuntimeBackedWindowAcquisitionReconciliationOutcome::Collision { .. } => {
                self.error = Some("New Window acquisition was not established.".to_owned());
            }
        }
        CreationStep::Continue
    }

    #[inline(never)]
    pub(super) fn reconcile_repair(
        &mut self,
        request: RuntimeBackedWindowAcquisitionRequest,
        reconciliation: RuntimeBackedWindowAcquisitionRepairReconciliation,
        reservation: RuntimeBackedWindowMainWindowReservation,
    ) -> CreationStep {
        match reconciliation.reconcile(&self.services.store) {
            RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::Pending {
                reconciliation,
                ..
            } => {
                self.state = CreationState::RepairPending {
                    request,
                    reconciliation,
                    reservation,
                };
                return CreationStep::Pending;
            }
            RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::ExactOld { .. }
            | RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::ExactNew { .. } => {
                self.state = CreationState::Acquire {
                    request,
                    reservation,
                };
            }
            RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::Collision { .. } => {
                self.error = Some("New Window catalog repair collided.".to_owned());
            }
        }
        CreationStep::Continue
    }
}
