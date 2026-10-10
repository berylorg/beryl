use super::*;
use crate::main_window::{
    MainWindowComposerClaimAdvance, MainWindowComposerClaimPreparedPresentation,
    MainWindowComposerClaimPublication, MainWindowComposerClaimWidgetWork,
};

impl MainWindowConversationComposerMount {
    pub(in crate::main_window) fn presentation_content_height(
        &self,
        cx: &App,
    ) -> Option<gpui::Pixels> {
        if let Some(layout) = self
            .contribution()
            .and_then(|selected| selected.read(cx).protected_resident_layout(cx))
        {
            return Some(layout.viewport_extent);
        }
        let pending = self.pending_presentation.as_ref().filter(|pending| {
            pending
                .contribution
                .read(cx)
                .matches_pending_target(pending.receipt)
        });
        pending
            .and_then(|pending| {
                pending
                    .contribution
                    .read(cx)
                    .gpui_input()
                    .read(cx)
                    .surface()
                    .map(|surface| surface.content_height())
            })
            .or_else(|| {
                self.contribution().and_then(|selected| {
                    selected
                        .read(cx)
                        .gpui_input()
                        .read(cx)
                        .surface()
                        .map(|surface| surface.content_height())
                })
            })
    }

    #[cfg(test)]
    pub(crate) fn test_claim_pending_state(&self, cx: &App) -> String {
        self.pending_presentation.as_ref().map_or_else(|| "absent".into(), |pending| {
            let composer = pending.contribution.read(cx);
            let input = composer.gpui_input();
            let input = input.read(cx);
            let realization = input.realization_diagnostics();
            format!("target={:?}, generation={}, claim={:?}, pending={}, ready={}, interactive={}, quiescent={}, enabled={}, surface={}, flight={}, prior_released={}, realizer={}, render_child={}, realization={:?}, rejection={:?}, error={:?}", pending.receipt.target_thread(), pending.receipt.generation(), composer.selection_identity().claim(), composer.is_pending_target(), composer.pending_surface_ready(cx), input.is_surface_current_and_interactive(), input.is_quiescent(), input.is_enabled(), input.surface().is_some(), composer.test_has_active_flight(), self.contribution.as_ref().is_some_and(|selected| selected.read(cx).test_widget_released()), self.contribution.as_ref().is_some_and(|selected| selected.read(cx).test_has_pending_realizer()), self.contribution.as_ref().is_some_and(|selected| selected.read(cx).test_has_pending_render_child(cx)), realization.current, realization.last_response_rejection, composer.last_error())
        })
    }

    #[cfg(test)]
    fn test_claim_promotion_predicates(
        &self,
        publication: &MainWindowComposerClaimPublication,
        cx: &App,
    ) -> String {
        let receipt = publication.receipt();
        let pending = self.pending_presentation.as_ref();
        format!(
            "receipt_match={}, route_match={}, selection_match={}, owner_ready={}, surface_ready={}, mount_service_match={}, expected_claim={:?}; {}",
            pending.is_some_and(|pending| pending.receipt == receipt),
            pending.is_some_and(|pending| pending
                .contribution
                .read(cx)
                .matches_pending_target(receipt)),
            pending.is_some_and(|pending| pending.contribution.read(cx).selection_identity()
                == publication.selection()),
            pending.is_some_and(|pending| pending
                .contribution
                .read(cx)
                .claim_pending_promotion_ready(publication, cx)),
            pending.is_some_and(|pending| pending.contribution.read(cx).pending_surface_ready(cx)),
            self.bound_service()
                .is_ok_and(|service| publication.matches_service(service)),
            publication.selection().claim(),
            self.test_claim_pending_state(cx),
        )
    }

    pub(in crate::main_window) fn claim_publication_service(
        &self,
    ) -> Result<Arc<MainWindowConversationComposerService>, String> {
        Ok(self.bound_service()?.clone())
    }

    pub(in crate::main_window) fn install_claim_pending_presentation(
        &mut self,
        prepared: MainWindowComposerClaimPreparedPresentation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.window_close.is_some() || self.pending_presentation.is_some() {
            return Err("composer claim presentation is unavailable".to_owned());
        }
        let selection = prepared.selection();
        let receipt = prepared.receipt();
        let service = self.bound_service()?.clone();
        if !prepared.matches_service(&service) {
            return Err("composer claim presentation service is stale".to_owned());
        }
        let config = self.configure_selection(selection)?;
        if config.selection() != selection {
            return Err("composer claim presentation configuration is stale".to_owned());
        }
        let target_bound = config.residency_bound()?;
        let bound = if let Some(selected) = &self.contribution {
            if selected.read(cx).selection_identity() != receipt.expected_prior() {
                return Err("composer claim prior presentation is stale".to_owned());
            }
            selected
                .read(cx)
                .residency_bound()
                .checked_add(target_bound)
                .ok_or_else(|| "composer claim residency overflowed".to_owned())?
        } else {
            self.resident_suspended_native_lineage_release(receipt.expected_prior())?;
            target_bound
        };
        let pending = cx.new(|composer_cx| {
            MainWindowConversationComposer::new_claim_pending(
                config,
                service,
                prepared,
                MainWindowConversationComposer::production_clipboard_writer(),
                window,
                composer_cx,
            )
            .expect("validated pending composer contribution")
        });
        pending.update(cx, |composer, _| composer.enable_checked_clipboard_writer());
        self.attach_resident_pending_presentation(receipt, pending, bound, cx)?;
        self.admit_claim_target(receipt, selection, cx)
    }

    fn admit_claim_target(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        selection: MainWindowComposerSelectionIdentity,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let pending = self
            .pending_presentation
            .as_ref()
            .filter(|pending| pending.receipt == receipt)
            .ok_or_else(|| "composer claim target presentation is missing".to_owned())?;
        let entity = pending.contribution.clone();
        if entity.read(cx).selection_identity() != selection
            || !entity.read(cx).matches_pending_target(receipt)
        {
            return Err("composer claim target presentation is stale".to_owned());
        }
        if let Some(error) = entity.read(cx).last_error() {
            return Err(error.to_owned());
        }
        let usage = entity.read(cx).residency_usage(cx)?;
        let usage = if let Some(selected) = &self.contribution {
            selected.read(cx).residency_usage(cx)?.checked_add(usage)
        } else {
            Some(usage)
        };
        if usage
            .and_then(|usage| usage.admit(pending.residency_bound))
            .is_none()
        {
            return Err("composer claim residency exceeded its bound".to_owned());
        }
        Ok(entity.update(cx, |composer, composer_cx| {
            composer.admit_pending_surface(composer_cx)
        }))
    }

    pub(in crate::main_window) fn admit_claim_pending_presentation(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        selection: MainWindowComposerSelectionIdentity,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.admit_claim_target(receipt, selection, cx)
    }

    pub(in crate::main_window) fn fence_claim_publication(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.window_close.is_some()
            || !self.pending_presentation.as_ref().is_some_and(|pending| {
                pending.receipt == receipt
                    && pending.contribution.read(cx).pending_surface_ready(cx)
            })
        {
            return Ok(false);
        }
        self.suspend_autosave()?;
        if self
            .resident_suspended_native_lineage_release(expected)?
            .is_some()
        {
            return Ok(true);
        }
        if let Some(selected) = &self.contribution {
            selected.update(cx, |composer, composer_cx| {
                let previous = composer.selection_identity();
                if previous == expected {
                    return Ok(());
                }
                if previous.window_id() != expected.window_id()
                    || previous.claim() != expected.claim()
                {
                    return Err("composer claim prior selection changed".to_owned());
                }
                composer.synchronize_lifecycle_selection(previous, expected, composer_cx)
            })?;
        }
        self.fence_contribution(expected, window, cx)
    }

    pub(in crate::main_window) fn accept_claim_publication_advance(
        &mut self,
        source: &MainWindowComposerClaimAdvance,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if let Some(selected) = &self.contribution {
            selected.update(cx, |composer, composer_cx| {
                let previous = composer.selection_identity();
                if previous == source.selected {
                    Ok(())
                } else {
                    composer.synchronize_lifecycle_selection(previous, source.selected, composer_cx)
                }
            })?;
        } else {
            self.resident_suspended_native_lineage_release(source.selected)?;
            self.synchronize_resident_suspended_selection(source.selected)?;
        }
        match source.advance {
            MainWindowComposerPublishAdvance::WidgetReleaseRequired(expected)
                if expected == source.selected =>
            {
                self.admit_claim_target(source.receipt, source.pending, cx)
            }
            MainWindowComposerPublishAdvance::PriorFlushFailed => {
                Err("composer claim prior flush failed".to_owned())
            }
            _ => Ok(false),
        }
    }

    pub(in crate::main_window) fn release_claim_publication_widget(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Option<MainWindowComposerClaimWidgetWork>, String> {
        if !self.pending_presentation.as_ref().is_some_and(|pending| {
            pending.receipt == receipt && pending.contribution.read(cx).pending_surface_ready(cx)
        }) {
            return Err("composer claim target is not ready".to_owned());
        }
        if let Some(release) = self.resident_suspended_native_lineage_release(expected)? {
            return Ok(Some(MainWindowComposerClaimWidgetWork::Released(release)));
        }
        let selected = self
            .contribution
            .as_ref()
            .filter(|selected| selected.read(cx).selection_identity() == expected)
            .cloned()
            .ok_or_else(|| "composer claim release presentation is stale".to_owned())?;
        if !selected.update(cx, |composer, composer_cx| {
            composer.begin_widget_release_fence(window, composer_cx)
        })? {
            return Ok(None);
        }
        selected
            .update(cx, |composer, composer_cx| {
                composer.capture_claim_widget_release(window, composer_cx)
            })
            .map(Some)
    }

    pub(in crate::main_window) fn accept_claim_widget_release(
        &mut self,
        release: MainWindowComposerWidgetRelease,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self
            .resident_suspended_native_lineage_release(release.selection())?
            .is_some()
        {
            return Ok(());
        }
        let selected = self
            .contribution
            .as_ref()
            .ok_or_else(|| "composer claim release presentation is missing".to_owned())?;
        selected.update(cx, |composer, _| {
            composer.accept_claim_widget_release(release)
        })
    }

    pub(in crate::main_window) fn promote_claim_publication(
        &mut self,
        publication: &MainWindowComposerClaimPublication,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.validate_claim_promotion(publication, cx)?;
        let receipt = publication.receipt();
        let pending = self
            .pending_presentation
            .as_ref()
            .unwrap()
            .contribution
            .clone();
        self.detach_pending_presentation(receipt, cx)?;
        pending.update(cx, |composer, composer_cx| {
            composer.promote_claim_pending(publication, window, composer_cx)
        })?;
        self.clear_native_lineage_mount_state();
        self.native_lineage_widget_release = None;
        self.contribution = Some(pending);
        self.subscribe_to_contribution(window, cx)?;
        cx.notify();
        Ok(())
    }

    pub(in crate::main_window) fn validate_claim_promotion(
        &self,
        publication: &MainWindowComposerClaimPublication,
        cx: &App,
    ) -> Result<(), String> {
        if self.window_close.is_some() {
            return Err("composer claim publication is closing".to_owned());
        }
        if !publication.matches_service(self.bound_service()?) {
            return Err("composer claim publication service is stale".to_owned());
        }
        let receipt = publication.receipt();
        self.pending_presentation
            .as_ref()
            .filter(|pending| {
                pending.receipt == receipt
                    && pending
                        .contribution
                        .read(cx)
                        .claim_pending_promotion_ready(publication, cx)
            })
            .ok_or_else(|| {
                #[cfg(test)]
                eprintln!(
                    "exact composer claim promotion refused: {}",
                    self.test_claim_promotion_predicates(publication, cx)
                );
                "composer claim publication target is stale".to_owned()
            })?
            .contribution
            .read(cx);
        if self
            .contribution
            .as_ref()
            .is_some_and(|selected| !selected.read(cx).claim_realizer_detach_ready(receipt))
        {
            return Err("composer claim publication prior realizer is stale".to_owned());
        }
        Ok(())
    }

    pub(in crate::main_window) fn detach_retired_claim_presentation(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.detach_pending_presentation(receipt, cx)?;
        if let Some(selected) = &self.contribution {
            selected.update(cx, |composer, composer_cx| {
                composer.resume_retired_claim_prior(window, composer_cx)
            })?;
        }
        Ok(())
    }

    pub(in crate::main_window) fn restore_claim_prior_selection(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.window_close.is_some() || self.pending_presentation.is_some() {
            return Err("composer claim retirement is not settled".to_owned());
        }
        if let Some(selected) = &self.contribution {
            selected.update(cx, |composer, composer_cx| {
                composer.restore_claim_prior_selection(selection, composer_cx)
            })
        } else {
            self.synchronize_resident_suspended_selection(selection)
        }
    }
}
