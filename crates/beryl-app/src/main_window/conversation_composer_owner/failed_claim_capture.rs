use super::*;
use crate::main_window::MainWindowComposerSlot;

pub(crate) struct MainWindowFailedClaimGuiAuthority {
    pub(crate) prior: MainWindowComposerSelectionIdentity,
    pub(crate) receipt: Option<MainWindowComposerActivationReceipt>,
    pub(crate) successor: Option<MainWindowComposerSelectionIdentity>,
    pub(crate) committed: bool,
    pub(crate) release: Option<MainWindowComposerWidgetRelease>,
    pub(crate) widget_disposal_requested: bool,
}

pub(crate) struct MainWindowFailedClaimCapture {
    pub(crate) selection: MainWindowComposerSelectionIdentity,
    pub(crate) prior: Option<MainWindowFailedResidentCapture>,
    protection: Option<gpui_text_input::RangeResidentProtection>,
    release: Option<MainWindowComposerWidgetRelease>,
    widget_disposal_requested: bool,
    unpublished_target: Option<UnpublishedClaimTargetWidgetCustody>,
}

struct UnpublishedClaimTargetWidgetCustody {
    receipt: MainWindowComposerActivationReceipt,
    selection: MainWindowComposerSelectionIdentity,
}

pub(crate) struct MainWindowFailedClaimResources {
    pub(crate) service: Option<Arc<MainWindowConversationComposerService>>,
    pub(crate) clipboard_writer: Option<ComposerClipboardWriter>,
}

impl MainWindowFailedClaimCapture {
    pub(crate) fn is_unpublished_target(&self) -> bool {
        self.unpublished_target.is_some()
    }

    pub(crate) fn accepted_release(&self) -> Option<MainWindowComposerWidgetRelease> {
        self.release
    }
    pub(crate) fn accept_closed_release(
        &mut self,
        release: MainWindowComposerWidgetRelease,
    ) -> Result<(), String> {
        if release.selection() != self.selection
            || self.prior.is_some()
            || self.release.is_some_and(|existing| existing != release)
        {
            return Err("original unpublished target release changed".into());
        }
        self.release = Some(release);
        self.widget_disposal_requested = true;
        Ok(())
    }
    pub(crate) fn restoration(&self) -> Option<RangeRestorationSeed> {
        self.prior.as_ref().map(|prior| prior.restoration())
    }
}

impl MainWindowConversationComposer {
    pub(crate) fn gate_failed_claim_cleanup(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.service
            .as_ref()
            .ok_or("original failed creation editor service is missing")?
            .qualify_failed_resident_home(self.selection)?;
        self.shutdown_interaction_gated = true;
        self.sync_mutation_gate(cx);
        cx.notify();
        Ok(())
    }
    pub(crate) fn release_failed_claim_widget(
        &mut self,
        capture: &MainWindowFailedClaimCapture,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<RangeTextInputRequest>, String> {
        if let Some(target) = &capture.unpublished_target {
            if self.selection != target.selection
                || self.selection != capture.selection
                || !self.matches_pending_target(target.receipt)
                || !matches!(
                    self.phase,
                    MainWindowConversationComposerPhase::RecoveryFenced
                )
                || !self.failed_editor_work_drained()
                || !self.input.read(cx).is_semantically_quiescent()
                || capture.prior.is_some()
                || capture.protection.is_some()
                || capture.release.is_some()
                || capture.widget_disposal_requested
            {
                return Err("original unpublished target widget disposal changed".into());
            }
            let requests = self.input.update(cx, |input, cx| input.dispose(window, cx));
            self.phase = MainWindowConversationComposerPhase::Releasing;
            return Ok(requests);
        }
        if self.selection != capture.selection || !self.failed_editor_drained(cx) {
            return Err("retired thread creation widget source is not drained".into());
        }
        if capture.widget_disposal_requested || capture.release.is_some() {
            return Ok(Vec::new());
        }
        let protection = capture
            .prior
            .as_ref()
            .map(|prior| prior.protection())
            .or(capture.protection)
            .ok_or("retired thread creation widget has no original protection")?;
        if !self
            .input
            .read(cx)
            .resident_protection_is_current(protection)
        {
            return Err("retired thread creation widget protection changed".into());
        }
        let failed_claim_protection = &mut self.failed_claim_protection;
        self.input
            .update(cx, |input, input_cx| {
                input
                    .release_resident_protection(protection, input_cx)
                    .map_err(|error| {
                        format!("retired widget protection release failed: {error:?}")
                    })?;
                *failed_claim_protection = None;
                Ok::<_, String>(input.dispose(window, input_cx))
            })
            .inspect(|_| {
                self.phase = MainWindowConversationComposerPhase::Releasing;
            })
    }

    pub(crate) fn accept_failed_claim_widget_release(
        &mut self,
        capture: &MainWindowFailedClaimCapture,
        release: MainWindowComposerWidgetRelease,
    ) -> Result<(), String> {
        if self.selection != capture.selection
            || release.selection() != capture.selection
            || !matches!(
                self.phase,
                MainWindowConversationComposerPhase::Releasing
                    | MainWindowConversationComposerPhase::Released(_)
            )
        {
            return Err("retired thread creation widget completion changed".into());
        }
        self.phase = MainWindowConversationComposerPhase::Released(release);
        Ok(())
    }

    pub(crate) fn capture_failed_claim_cleanup(
        &mut self,
        authority: &MainWindowFailedClaimGuiAuthority,
        service: &Arc<MainWindowConversationComposerService>,
        cx: &mut Context<Self>,
    ) -> Result<Option<MainWindowFailedClaimCapture>, String> {
        if self
            .service
            .as_ref()
            .is_none_or(|bound| !Arc::ptr_eq(bound, service))
            || (self.selection != authority.prior && Some(self.selection) != authority.successor)
            || self.is_pending_target()
                && authority
                    .receipt
                    .is_none_or(|receipt| !self.matches_pending_target(receipt))
        {
            return Err(
                "failed thread creation editor does not belong to its original operation".into(),
            );
        }
        service.qualify_failed_resident_home(self.selection)?;
        if matches!(
            self.phase,
            MainWindowConversationComposerPhase::Live
                | MainWindowConversationComposerPhase::Fencing
        ) && !self.settle_failed_view_demands(cx)?
        {
            return Ok(None);
        }
        if !authority.committed && !self.is_pending_target() {
            let ticket = self.begin_failed_resident(cx)?;
            return self.capture_failed_resident(ticket, cx).map(|capture| {
                capture.map(|prior| MainWindowFailedClaimCapture {
                    selection: self.selection,
                    protection: None,
                    release: None,
                    prior: Some(prior),
                    widget_disposal_requested: false,
                    unpublished_target: None,
                })
            });
        }
        if let MainWindowConversationComposerPhase::Released(release) = self.phase {
            if authority
                .release
                .is_some_and(|expected| expected != release)
                || release.selection() != self.selection
            {
                return Err("failed thread creation original widget release changed".into());
            }
            return Ok(Some(MainWindowFailedClaimCapture {
                selection: self.selection,
                prior: None,
                protection: None,
                release: Some(release),
                widget_disposal_requested: true,
                unpublished_target: None,
            }));
        }
        if authority.widget_disposal_requested
            && matches!(self.phase, MainWindowConversationComposerPhase::Releasing)
            && self.failed_editor_drained(cx)
        {
            return Ok(Some(MainWindowFailedClaimCapture {
                selection: self.selection,
                prior: None,
                protection: None,
                release: None,
                widget_disposal_requested: true,
                unpublished_target: None,
            }));
        }
        if !matches!(
            self.phase,
            MainWindowConversationComposerPhase::Live
                | MainWindowConversationComposerPhase::Fencing
        ) || self.recovery_snapshot.is_some()
            || self.unpublished_recovery_protection.is_some()
        {
            return Err(
                "failed thread creation widget release retains its original continuation".into(),
            );
        }
        if self.is_pending_target() {
            let receipt = authority
                .receipt
                .filter(|receipt| self.matches_pending_target(*receipt))
                .ok_or("original unpublished target receipt changed")?;
            if authority.successor != Some(self.selection) {
                return Err("original unpublished target selection changed".into());
            }
            if !self.failed_editor_work_drained()
                || !self.input.read(cx).is_semantically_quiescent()
            {
                return Ok(None);
            }
            self.input
                .update(cx, |input, cx| input.set_enabled(false, cx));
            self.phase = MainWindowConversationComposerPhase::RecoveryFenced;
            self.admitted_positions = None;
            self.scheduled = false;
            cx.notify();
            return Ok(Some(MainWindowFailedClaimCapture {
                selection: self.selection,
                prior: None,
                protection: None,
                release: None,
                widget_disposal_requested: false,
                unpublished_target: Some(UnpublishedClaimTargetWidgetCustody {
                    receipt,
                    selection: self.selection,
                }),
            }));
        }
        if !self.failed_editor_drained(cx) {
            return Ok(None);
        }
        let protection = self
            .input
            .update(cx, |input, cx| {
                if input.history_frontier() != self.selection.binding().range_history_frontier()
                    || !input.surface().is_some_and(|surface| {
                        surface.binding() == self.selection.binding().range_binding()
                    })
                {
                    return Err(gpui_text_input::RangeTextInputError::Stale);
                }
                input.set_enabled(false, cx);
                input.protect_resident(cx)
            })
            .map_err(|error| {
                format!("failed thread creation editor protection refused: {error:?}")
            })?;
        self.failed_claim_protection = Some(protection);
        self.phase = MainWindowConversationComposerPhase::RecoveryFenced;
        self.admitted_positions = None;
        self.scheduled = false;
        cx.notify();
        Ok(Some(MainWindowFailedClaimCapture {
            selection: self.selection,
            prior: None,
            protection: Some(protection),
            release: None,
            widget_disposal_requested: false,
            unpublished_target: None,
        }))
    }

    fn settle_failed_view_demands(&mut self, cx: &mut Context<Self>) -> Result<bool, String> {
        if !self.failed_editor_work_drained() {
            return Ok(false);
        }
        let binding = self.selection.binding().range_binding();
        self.input.update(cx, |input, cx| {
            settle_failed_input_view_demands(input, binding, cx)
        })
    }

    pub(crate) fn detach_failed_claim_resources(
        &mut self,
        capture: &MainWindowFailedClaimCapture,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowFailedClaimResources, String> {
        if let Some(prior) = capture.prior.as_ref() {
            let resources = self.detach_failed_resident_resources(prior, cx)?;
            return Ok(MainWindowFailedClaimResources {
                service: resources.service,
                clipboard_writer: resources.clipboard_writer,
            });
        }
        if self.selection != capture.selection
            || !self.failed_editor_drained(cx)
            || capture.unpublished_target.as_ref().is_some_and(|target| {
                self.selection != target.selection
                    || !self.matches_pending_target(target.receipt)
                    || !matches!(self.phase, MainWindowConversationComposerPhase::Releasing)
            })
            || match (capture.protection, capture.release) {
                (Some(protection), None) => !self
                    .input
                    .read(cx)
                    .resident_protection_is_current(protection),
                (None, Some(release)) => {
                    !matches!(self.phase, MainWindowConversationComposerPhase::Released(current) if current == release)
                }
                (None, None) => {
                    !(capture.widget_disposal_requested || capture.unpublished_target.is_some())
                        || !matches!(self.phase, MainWindowConversationComposerPhase::Releasing)
                }
                _ => true,
            }
        {
            return Err(
                "failed thread creation protected editor or original release changed".into(),
            );
        }
        self.last_mutation_admission_failure.take();
        Ok(MainWindowFailedClaimResources {
            service: self.service.take(),
            clipboard_writer: self.clipboard_writer.take(),
        })
    }
}

fn settle_failed_input_view_demands(
    input: &mut RangeTextInput,
    binding: gpui_text_input::RangeBinding,
    cx: &mut Context<RangeTextInput>,
) -> Result<bool, String> {
    if !input.is_semantically_quiescent() {
        return Ok(false);
    }
    let budget = input.realization_diagnostics().max_queued_requests;
    for _ in 0..budget {
        let request = input.take_request_if(|request| match request {
            RangeTextInputRequest::Page(page) => {
                let key = page.key();
                key.binding() == binding.binding()
                    && key.revision() == binding.revision()
                    && matches!(
                        key.purpose(),
                        PagePurpose::GeometryIndex | PagePurpose::GeometryTarget
                    )
            }
            RangeTextInputRequest::ObjectPage(page) => {
                let key = page.key();
                key.binding() == binding.binding()
                    && key.revision() == binding.revision()
                    && matches!(
                        key.purpose(),
                        ObjectPurpose::GeometryIndex | ObjectPurpose::GeometryTarget
                    )
            }
            _ => MainWindowComposerSlot::widget_release_request_is_settled(request),
        });
        match request {
            Some(RangeTextInputRequest::Page(page)) => input
                .fail_page(page.key(), gpui_text_input::PageFailure::Unavailable, cx)
                .map_err(|_| "original failed view page settlement was refused".to_owned())?,
            Some(RangeTextInputRequest::ObjectPage(page)) => {
                match input.fail_object_page(
                    page.key(),
                    gpui_text_input::ObjectPageFailure::Unavailable,
                    cx,
                ) {
                    Ok(()) => {}
                    Err(gpui_text_input::RangeTextInputError::Stale) => {
                        let remaining = input.realization_diagnostics().current;
                        if remaining.active_geometry_jobs != 0
                            || remaining.pending_geometry_objects != 0
                            || remaining.pending_object_requests != 0
                            || remaining.dispatched_object_requests != 0
                        {
                            return Ok(false);
                        }
                    }
                    Err(_) => {
                        return Err("original failed view object settlement was refused".into());
                    }
                }
            }
            Some(release) => {
                assert!(MainWindowComposerSlot::widget_release_request_is_settled(
                    &release
                ));
            }
            None => {
                if input.realization_diagnostics().current.queued_requests != 0 {
                    return Err("original failed view request remains unsupported".into());
                }
                return Ok(input.is_quiescent());
            }
        }
    }
    Ok(false)
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/failed_claim_view_demands.rs"
    ));
}
