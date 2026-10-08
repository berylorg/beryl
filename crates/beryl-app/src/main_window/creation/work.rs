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
    Prepared(Box<MainWindowShellPrepared>),
    Settled,
}

impl MainWindowCreation {
    pub fn advance(
        self,
        appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
    ) -> MainWindowCreationOutcome {
        *Box::new(self).advance_owned(appearance)
    }

    #[inline(never)]
    pub(in crate::main_window) fn advance_owned(
        mut self: Box<Self>,
        appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
    ) -> Box<MainWindowCreationOutcome> {
        for _ in 0..16 {
            if let Err(error) = self.services.validate_source() {
                self.error.get_or_insert(error);
                self.cancellation.cancel();
            }
            match self.advance_state(&appearance) {
                CreationStep::Continue => {}
                CreationStep::Pending => {
                    return self.pending_owned();
                }
                CreationStep::Prepared(prepared) => {
                    if let Err(error) = self.services.validate_source() {
                        self.error = Some(error);
                        self.cancellation.cancel();
                        self.retain_unpublished_preparation(prepared);
                        continue;
                    }
                    return self.prepared_owned(prepared);
                }
                CreationStep::Settled => {
                    return self.settled_owned();
                }
            }
        }
        self.pending_owned()
    }

    #[inline(never)]
    fn retain_unpublished_preparation(&mut self, prepared: Box<MainWindowShellPrepared>) {
        self.state = CreationState::Unpublished(prepared.into_unpublished());
    }

    #[inline(never)]
    fn pending_owned(self: Box<Self>) -> Box<MainWindowCreationOutcome> {
        Box::new(MainWindowCreationOutcome::Pending(*self))
    }

    #[inline(never)]
    fn prepared_owned(
        self: Box<Self>,
        prepared: Box<MainWindowShellPrepared>,
    ) -> Box<MainWindowCreationOutcome> {
        Box::new(MainWindowCreationOutcome::Prepared {
            prepared: *prepared,
            cancellation: self.cancellation,
        })
    }

    #[inline(never)]
    fn settled_owned(self: Box<Self>) -> Box<MainWindowCreationOutcome> {
        Box::new(MainWindowCreationOutcome::Settled {
            window_id: self.window_id,
            error: self.error,
        })
    }

    #[inline(never)]
    fn advance_state(
        &mut self,
        appearance: &Arc<crate::theme_runtime::AppearanceGeneration>,
    ) -> CreationStep {
        if matches!(&self.state, CreationState::Initial(_)) {
            return self.advance_initial(appearance);
        }
        self.advance_other_state(appearance)
    }

    #[inline(never)]
    fn advance_other_state(
        &mut self,
        _appearance: &Arc<crate::theme_runtime::AppearanceGeneration>,
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
            CreationState::Initial(_) => unreachable!("initial creation advances in place"),
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
