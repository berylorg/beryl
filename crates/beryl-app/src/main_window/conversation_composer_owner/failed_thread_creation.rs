use super::*;

pub(crate) struct MainWindowFailedThreadCreationGuiAuthority {
    pub(crate) prior: MainWindowComposerSelectionIdentity,
    pub(crate) receipt: Option<MainWindowComposerActivationReceipt>,
    pub(crate) successor: Option<MainWindowComposerSelectionIdentity>,
    pub(crate) committed: bool,
    pub(crate) release: Option<MainWindowComposerWidgetRelease>,
    pub(crate) widget_disposal_requested: bool,
}

pub(crate) struct MainWindowFailedThreadCreationCapture {
    pub(crate) selection: MainWindowComposerSelectionIdentity,
    pub(crate) prior: Option<MainWindowFailedResidentCapture>,
    protection: Option<gpui_text_input::RangeResidentProtection>,
    release: Option<MainWindowComposerWidgetRelease>,
    widget_disposal_requested: bool,
}

pub(crate) struct MainWindowFailedThreadCreationResources {
    pub(crate) service: Option<Arc<MainWindowConversationComposerService>>,
    pub(crate) clipboard_writer: Option<ComposerClipboardWriter>,
}

impl MainWindowFailedThreadCreationCapture {
    pub(crate) fn restoration(&self) -> Option<RangeRestorationSeed> {
        self.prior.as_ref().map(|prior| prior.restoration())
    }
}

impl MainWindowConversationComposer {
    pub(crate) fn gate_failed_thread_creation(
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
    pub(crate) fn release_failed_thread_creation_widget(
        &mut self,
        capture: &MainWindowFailedThreadCreationCapture,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<RangeTextInputRequest>, String> {
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
        self.input
            .update(cx, |input, input_cx| {
                input
                    .release_resident_protection(protection, input_cx)
                    .map_err(|error| {
                        format!("retired widget protection release failed: {error:?}")
                    })?;
                Ok::<_, String>(input.dispose(window, input_cx))
            })
            .inspect(|_| {
                self.phase = MainWindowConversationComposerPhase::Releasing;
            })
    }

    pub(crate) fn accept_failed_thread_creation_widget_release(
        &mut self,
        capture: &MainWindowFailedThreadCreationCapture,
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

    pub(crate) fn capture_failed_thread_creation(
        &mut self,
        authority: &MainWindowFailedThreadCreationGuiAuthority,
        service: &Arc<MainWindowConversationComposerService>,
        cx: &mut Context<Self>,
    ) -> Result<Option<MainWindowFailedThreadCreationCapture>, String> {
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
        if !authority.committed && !self.is_pending_target() {
            let ticket = self.begin_failed_resident(cx)?;
            return self.capture_failed_resident(ticket, cx).map(|capture| {
                capture.map(|prior| MainWindowFailedThreadCreationCapture {
                    selection: self.selection,
                    protection: None,
                    release: None,
                    prior: Some(prior),
                    widget_disposal_requested: false,
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
            return Ok(Some(MainWindowFailedThreadCreationCapture {
                selection: self.selection,
                prior: None,
                protection: None,
                release: Some(release),
                widget_disposal_requested: true,
            }));
        }
        if authority.widget_disposal_requested
            && matches!(self.phase, MainWindowConversationComposerPhase::Releasing)
            && self.failed_editor_drained(cx)
        {
            return Ok(Some(MainWindowFailedThreadCreationCapture {
                selection: self.selection,
                prior: None,
                protection: None,
                release: None,
                widget_disposal_requested: true,
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
        self.phase = MainWindowConversationComposerPhase::RecoveryFenced;
        self.admitted_positions = None;
        self.scheduled = false;
        cx.notify();
        Ok(Some(MainWindowFailedThreadCreationCapture {
            selection: self.selection,
            prior: None,
            protection: Some(protection),
            release: None,
            widget_disposal_requested: false,
        }))
    }

    pub(crate) fn detach_failed_thread_creation_resources(
        &mut self,
        capture: &MainWindowFailedThreadCreationCapture,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowFailedThreadCreationResources, String> {
        if let Some(prior) = capture.prior.as_ref() {
            let resources = self.detach_failed_resident_resources(prior, cx)?;
            return Ok(MainWindowFailedThreadCreationResources {
                service: resources.service,
                clipboard_writer: resources.clipboard_writer,
            });
        }
        if self.selection != capture.selection
            || !self.failed_editor_drained(cx)
            || match (capture.protection, capture.release) {
                (Some(protection), None) => !self
                    .input
                    .read(cx)
                    .resident_protection_is_current(protection),
                (None, Some(release)) => {
                    !matches!(self.phase, MainWindowConversationComposerPhase::Released(current) if current == release)
                }
                (None, None) => {
                    !capture.widget_disposal_requested
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
        Ok(MainWindowFailedThreadCreationResources {
            service: self.service.take(),
            clipboard_writer: self.clipboard_writer.take(),
        })
    }
}
