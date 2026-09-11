use super::*;

impl MainWindowCreation {
    #[inline(never)]
    pub(super) fn prepare_abandonment(
        &mut self,
        unpublished: MainWindowShellUnpublished,
    ) -> CreationStep {
        match unpublished
            .prepare_abandonment(&self.services.acquisition, CommandCancellation::new())
        {
            MainWindowShellAbandonmentPreparationOutcome::ExactAcquired { abandonment } => {
                self.state = CreationState::Abandonment(abandonment);
            }
            MainWindowShellAbandonmentPreparationOutcome::InitialComposerPending {
                unpublished,
                error,
            } => {
                self.error = Some(error);
                self.state = CreationState::Unpublished(unpublished);
                return CreationStep::Pending;
            }
            MainWindowShellAbandonmentPreparationOutcome::NotCommitted {
                unpublished,
                evidence,
            } => {
                self.error = Some(format!(
                    "New Window abandonment did not prepare: {evidence:?}"
                ));
                self.state = CreationState::Unpublished(unpublished);
                return CreationStep::Pending;
            }
            MainWindowShellAbandonmentPreparationOutcome::Collision { .. } => {
                self.error = Some("New Window abandonment collided.".to_owned());
            }
        }

        CreationStep::Continue
    }

    #[inline(never)]
    pub(super) fn abandon_acquisition(
        &mut self,
        abandonment: MainWindowShellAbandonment,
    ) -> CreationStep {
        match abandonment.abandon(&self.services.acquisition, CommandCancellation::new()) {
            MainWindowShellAbandonmentOutcome::NotCommitted {
                abandonment,
                evidence,
            } => {
                self.error = Some(format!(
                    "New Window abandonment did not commit: {evidence:?}"
                ));
                self.state = CreationState::Abandonment(abandonment);
                return CreationStep::Pending;
            }
            MainWindowShellAbandonmentOutcome::Committed { .. } => {}
            MainWindowShellAbandonmentOutcome::Indeterminate { reconciliation, .. } => {
                self.state = CreationState::AbandonmentPending(reconciliation);
                return CreationStep::Pending;
            }
        }

        CreationStep::Continue
    }

    #[inline(never)]
    pub(super) fn reconcile_abandonment(
        &mut self,
        reconciliation: MainWindowShellAbandonmentReconciliation,
    ) -> CreationStep {
        match reconciliation.reconcile(&self.services.store) {
            MainWindowShellAbandonmentReconciliationOutcome::Pending { reconciliation, .. } => {
                self.state = CreationState::AbandonmentPending(reconciliation);
                return CreationStep::Pending;
            }
            MainWindowShellAbandonmentReconciliationOutcome::ExactAcquired { abandonment } => {
                self.state = CreationState::Abandonment(abandonment);
            }
            MainWindowShellAbandonmentReconciliationOutcome::ExactAbandoned { .. } => {}
            MainWindowShellAbandonmentReconciliationOutcome::Collision { .. } => {
                self.error = Some("New Window abandonment reconciliation collided.".to_owned());
            }
        }

        CreationStep::Continue
    }
}
