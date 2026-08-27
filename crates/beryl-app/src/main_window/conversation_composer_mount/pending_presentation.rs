use std::time::Duration;

use super::*;

pub(super) struct MainWindowConversationComposerPendingPresentation {
    pub(super) receipt: MainWindowComposerActivationReceipt,
    pub(super) contribution: Entity<MainWindowConversationComposer>,
    pub(super) residency_bound: MainWindowComposerResidencyBound,
    _realizer_token: MainWindowConversationComposerPendingRealizerToken,
}

impl MainWindowConversationComposerMount {
    pub(super) fn ensure_pending_composer(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let selection = self
            .service
            .pending_identity(receipt)
            .ok_or_else(|| "pending composer activation receipt is stale".to_owned())?;
        if self.pending_presentation.is_none() {
            let config = match (self.configurator)(selection) {
                Ok(config) => config,
                Err(error) => {
                    self.retire_failed_pending(receipt, cx)?;
                    return Err(error);
                }
            };
            let target_residency_bound = config.residency_bound()?;
            let service = self.service.clone();
            let selected_residency_bound = self
                .contribution
                .as_ref()
                .ok_or_else(|| "selected composer contribution is missing".to_owned())?
                .read(cx)
                .residency_bound();
            let residency_bound = selected_residency_bound
                .checked_add(target_residency_bound)
                .ok_or_else(|| "combined composer residency bound overflowed".to_owned())?;
            let (prepared_selection, activation_seeds) =
                match MainWindowConversationComposer::prepare_pending_activation(&service, receipt)
                {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        self.retire_failed_pending(receipt, cx)?;
                        return Err(error);
                    }
                };
            if prepared_selection != selection {
                self.retire_failed_pending(receipt, cx)?;
                return Err("pending conversation composer selection is stale".to_owned());
            }
            let pending = cx.new(|composer_cx| {
                MainWindowConversationComposer::new_pending(
                    config,
                    service,
                    receipt,
                    activation_seeds,
                    MainWindowConversationComposer::production_clipboard_writer(),
                    window,
                    composer_cx,
                )
                .expect("validated pending composer contribution")
            });
            self.attach_pending_presentation(receipt, pending, residency_bound, cx)?;
        }
        let pending = self.pending_presentation.as_ref().unwrap();
        if pending.receipt != receipt
            || pending.contribution.read(cx).selection_identity() != selection
            || !pending.contribution.read(cx).is_pending_target()
        {
            return Err("pending composer contribution identity is stale".to_owned());
        }
        if let Some(error) = pending.contribution.read(cx).last_error() {
            let error = error.to_owned();
            self.retire_failed_pending(receipt, cx)?;
            return Err(error);
        }
        if self.activation_residency(cx)?.is_none() {
            self.retire_failed_pending(receipt, cx)?;
            return Err("combined composer residency exceeded its activation bound".to_owned());
        }
        let pending = pending.contribution.clone();
        Ok(pending.update(cx, |pending, pending_cx| {
            pending.admit_pending_surface(pending_cx)
        }))
    }

    pub(super) fn attach_pending_presentation(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        pending: Entity<MainWindowConversationComposer>,
        residency_bound: MainWindowComposerResidencyBound,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.pending_presentation.is_some() {
            return Err("pending composer presentation is already attached".to_owned());
        }
        let selected = self
            .contribution
            .as_ref()
            .ok_or_else(|| "selected composer contribution is missing".to_owned())?;
        let realizer_token = selected.update(cx, |composer, composer_cx| {
            composer.attach_pending_realizer(receipt, &pending, composer_cx)
        })?;
        self.pending_presentation = Some(MainWindowConversationComposerPendingPresentation {
            receipt,
            contribution: pending,
            residency_bound,
            _realizer_token: realizer_token,
        });
        Ok(())
    }

    pub(super) fn detach_pending_presentation(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        cx: &mut Context<Self>,
    ) -> Result<Option<Entity<MainWindowConversationComposer>>, String> {
        if !self
            .pending_presentation
            .as_ref()
            .is_some_and(|pending| pending.receipt == receipt)
        {
            return Ok(None);
        }
        let selected = self
            .contribution
            .as_ref()
            .ok_or_else(|| "selected composer contribution is missing".to_owned())?;
        selected.update(cx, |composer, composer_cx| {
            composer.detach_pending_realizer(receipt, composer_cx)
        })?;
        Ok(self
            .pending_presentation
            .take()
            .map(|pending| pending.contribution))
    }

    pub(super) fn clear_pending_presentation(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<Option<Entity<MainWindowConversationComposer>>, String> {
        let Some(receipt) = self
            .pending_presentation
            .as_ref()
            .map(|pending| pending.receipt)
        else {
            return Ok(None);
        };
        self.detach_pending_presentation(receipt, cx)
    }

    pub(super) fn activation_residency(
        &self,
        cx: &App,
    ) -> Result<Option<MainWindowComposerActivationResidency>, String> {
        let Some(presentation) = self.pending_presentation.as_ref() else {
            return Ok(None);
        };
        let selected = self
            .contribution
            .as_ref()
            .ok_or_else(|| "selected composer contribution is missing".to_owned())?
            .read(cx)
            .residency_usage(cx)?;
        let pending = presentation.contribution.read(cx).residency_usage(cx)?;
        Ok(selected
            .checked_add(pending)
            .and_then(|usage| usage.admit(presentation.residency_bound)))
    }

    pub(super) fn retire_failed_pending(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.detach_pending_presentation(receipt, cx)?;
        if self.service.pending_receipt() != Some(receipt) {
            return Ok(());
        }
        match self.service.release_failed_pending(receipt)? {
            MainWindowComposerRetirementAdvance::Retired => return Ok(()),
            MainWindowComposerRetirementAdvance::Pending => {}
            MainWindowComposerRetirementAdvance::DepartedFreshBoundary => {
                return Err("failed composer target departed fresh state".to_owned());
            }
        }
        let service = self.service.clone();
        let executor = cx.background_executor().clone();
        let retirement_executor = executor.clone();
        executor
            .spawn(async move {
                let mut delay = Duration::from_millis(1);
                loop {
                    retirement_executor.timer(delay).await;
                    if service.pending_receipt() != Some(receipt) {
                        return;
                    }
                    match service.release_failed_pending(receipt) {
                        Ok(MainWindowComposerRetirementAdvance::Retired)
                        | Ok(MainWindowComposerRetirementAdvance::DepartedFreshBoundary)
                        | Err(_) => return,
                        Ok(MainWindowComposerRetirementAdvance::Pending) => {
                            delay = delay.saturating_mul(2).min(Duration::from_millis(100));
                        }
                    }
                }
            })
            .detach();
        Ok(())
    }
}
