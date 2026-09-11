mod abandonment;
mod acquisition;
mod initial;

use super::*;

pub(super) enum CreationState {
    Request {
        target: RememberedTarget,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    Acquire {
        request: RuntimeBackedWindowAcquisitionRequest,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    AcquisitionPending {
        reconciliation: RuntimeBackedWindowAcquisitionReconciliation,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    RepairPending {
        request: RuntimeBackedWindowAcquisitionRequest,
        reconciliation: RuntimeBackedWindowAcquisitionRepairReconciliation,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    Acquired {
        acquisition: RuntimeBackedWindowAcquisition,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    Initial(MainWindowInitialComposer),
    Retiring(MainWindowInitialComposer),
    Unpublished(MainWindowShellUnpublished),
    Abandonment(MainWindowShellAbandonment),
    AbandonmentPending(MainWindowShellAbandonmentReconciliation),
    Settled,
}

enum CreationStep {
    Continue,
    Pending,
    Prepared(MainWindowShellPrepared),
    Settled,
}

impl MainWindowCreation {
    pub fn advance(
        mut self,
        appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
    ) -> MainWindowCreationOutcome {
        for _ in 0..16 {
            match self.advance_state(&appearance) {
                CreationStep::Continue => {}
                CreationStep::Pending => return MainWindowCreationOutcome::Pending(self),
                CreationStep::Prepared(prepared) => {
                    return MainWindowCreationOutcome::Prepared {
                        prepared,
                        cancellation: self.cancellation,
                    };
                }
                CreationStep::Settled => {
                    return MainWindowCreationOutcome::Settled {
                        window_id: self.window_id,
                        error: self.error,
                    };
                }
            }
        }
        MainWindowCreationOutcome::Pending(self)
    }

    #[inline(never)]
    fn advance_state(
        &mut self,
        appearance: &Arc<crate::theme_runtime::AppearanceGeneration>,
    ) -> CreationStep {
        let state = std::mem::replace(&mut self.state, CreationState::Settled);
        match state {
            CreationState::Request {
                target,
                reservation,
            } => self.request(target, reservation),
            CreationState::Acquire {
                request,
                reservation,
            } => self.acquire(request, reservation),
            CreationState::AcquisitionPending {
                reconciliation,
                reservation,
            } => self.reconcile_acquisition(reconciliation, reservation),
            CreationState::RepairPending {
                request,
                reconciliation,
                reservation,
            } => self.reconcile_repair(request, reconciliation, reservation),
            CreationState::Acquired {
                acquisition,
                reservation,
            } => self.prepare_initial(acquisition, reservation),
            CreationState::Initial(initial) => self.advance_initial(initial, appearance),
            CreationState::Retiring(initial) => self.retire_initial(initial),
            CreationState::Unpublished(unpublished) => self.prepare_abandonment(unpublished),
            CreationState::Abandonment(abandonment) => self.abandon_acquisition(abandonment),
            CreationState::AbandonmentPending(reconciliation) => {
                self.reconcile_abandonment(reconciliation)
            }
            CreationState::Settled => CreationStep::Settled,
        }
    }
}
