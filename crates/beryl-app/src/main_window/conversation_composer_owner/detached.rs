use super::*;
use beryl_home_store::CommandCancellation;
use syndic_storage::DetachedDraftReadSourceV1;

pub(super) struct DetachedComposer {
    source: DetachedDraftReadSourceV1,
    cancellation: Option<CommandCancellation>,
    pub(super) disposing: bool,
}

impl Drop for DetachedComposer {
    fn drop(&mut self) {
        if let Some(cancellation) = &self.cancellation {
            cancellation.cancel();
        }
    }
}

impl MainWindowConversationComposer {
    pub(in crate::main_window) fn validate_destroyed_detached_source(
        &self,
        close: super::super::MainWindowConversationComposerCloseTicket,
        cx: &App,
    ) -> Result<MainWindowComposerSelectionIdentity, String> {
        let selected = self.validate_installed_detached_source(close, cx)?;
        if self.active_flight.is_some() || !self.detached.as_ref().unwrap().disposing {
            return Err("destroyed detached reader custody is not drained".into());
        }
        Ok(selected)
    }

    pub(in crate::main_window) fn validate_installed_detached_source(
        &self,
        close: super::super::MainWindowConversationComposerCloseTicket,
        cx: &App,
    ) -> Result<MainWindowComposerSelectionIdentity, String> {
        let source = self
            .detached
            .as_ref()
            .ok_or("final resident has no detached source")?;
        if !matches!(self.phase, MainWindowConversationComposerPhase::Detached)
            || !self.shutdown_interaction_gated
            || self.service.is_some()
            || self.window_close != Some(close)
            || !close.matches_editor(self.selection)
            || self.recovery_snapshot.is_some()
            || source.source.binding() != self.selection.binding().candidate()
            || source.source.root() != self.selection.binding().root()
            || self.input.read(cx).history_frontier()
                != self.selection.binding().range_history_frontier()
            || !self.input.read(cx).surface().is_some_and(|surface| {
                surface.binding() == self.selection.binding().range_binding()
            })
        {
            return Err("final installed detached resident changed".into());
        }
        Ok(self.selection)
    }

    pub(in crate::main_window) fn validate_detached_install(
        &self,
        close: super::super::MainWindowConversationComposerCloseTicket,
        source: &DetachedDraftReadSourceV1,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.shutdown_interaction_gated
            || self.is_pending_target()
            || self.detached.is_some()
            || self.recovery_snapshot.is_some()
            || !self.window_close_flush_ready(close, cx)?
            || self.pending_realizer.is_some()
            || !self.activation_seeds.is_empty()
            || self.propagated_cut.is_some()
            || self.pending_marker_metadata.is_some()
            || self.mutation_evidence.is_some()
            || self.pending_marker_removal.is_some()
            || self.image_surface_attachment.is_some()
            || !self.input.read(cx).is_quiescent()
            || source.binding() != self.selection.binding().candidate()
            || source.root() != self.selection.binding().root()
            || self.input.read(cx).history_frontier()
                != self.selection.binding().range_history_frontier()
            || !self.input.read(cx).surface().is_some_and(|surface| {
                surface.binding() == self.selection.binding().range_binding()
            })
        {
            return Err("detached composer installation lost its exact settled resident".into());
        }
        Ok(())
    }

    pub(in crate::main_window) fn install_detached_source(
        &mut self,
        source: DetachedDraftReadSourceV1,
        cx: &mut Context<Self>,
    ) -> super::super::MainWindowComposerRecoveryResources {
        self.detached = Some(DetachedComposer {
            source,
            cancellation: None,
            disposing: false,
        });
        self.phase = MainWindowConversationComposerPhase::Detached;
        self.input.update(cx, |input, cx| {
            input.set_read_only(true, cx);
            input.set_enabled(true, cx);
        });
        self.admitted_positions = None;
        self.scheduled = false;
        cx.notify();
        super::super::MainWindowComposerRecoveryResources {
            service: self.service.take(),
            clipboard_writer: None,
            mutation_failure: self.last_mutation_admission_failure.take(),
        }
    }

    pub(in crate::main_window) fn drain_detached_reads(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(detached) = self.detached.as_mut() else {
            return true;
        };
        detached.disposing = true;
        if let Some(cancellation) = &detached.cancellation {
            cancellation.cancel();
        }
        self.finish_propagated_clipboard_without_cut(cx);
        self.active_flight.is_none()
    }

    pub(in crate::main_window) fn resume_detached_reads(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(detached) = self.detached.as_mut() {
            detached.disposing = false;
            self.input.update(cx, |input, cx| {
                input.set_read_only(true, cx);
                input.set_enabled(true, cx);
            });
            self.schedule_pump(window, cx);
            cx.notify();
        }
    }

    fn begin_detached_flight(
        &mut self,
    ) -> Result<(u64, DetachedDraftReadSourceV1, CommandCancellation), String> {
        let detached = self
            .detached
            .as_mut()
            .ok_or("detached composer source is unavailable")?;
        if detached.disposing || self.active_flight.is_some() {
            return Err("detached composer lane is unavailable".into());
        }
        let flight = self.next_flight;
        self.next_flight = self
            .next_flight
            .checked_add(1)
            .ok_or("detached request identity exhausted")?;
        let cancellation = CommandCancellation::new();
        detached.cancellation = Some(cancellation.clone());
        self.active_flight = Some(flight);
        Ok((flight, detached.source.clone(), cancellation))
    }

    fn settle_detached_flight(
        &mut self,
        flight: u64,
        selection: MainWindowComposerSelectionIdentity,
    ) -> bool {
        if self.selection != selection || self.detached.is_none() || !self.settle_flight(flight) {
            return false;
        }
        let detached = self.detached.as_mut().unwrap();
        detached.cancellation.take();
        true
    }

    pub(super) fn pump_detached(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_flight.is_some()
            || self.propagated_clipboard.is_some()
            || self.detached.as_ref().is_none_or(|source| source.disposing)
        {
            return;
        }
        let Some(request) = self.input.update(cx, |input, _| input.take_request()) else {
            return;
        };
        let request = match request {
            RangeTextInputRequest::ClipboardWrite(write) => {
                let key = write.key();
                self.clipboard_operation = key.id();
                let binding = self.selection.binding().range_binding();
                let outcome =
                    if key.binding() == binding.binding() && key.revision() == binding.revision() {
                        self.write_clipboard(write.text(), cx)
                    } else {
                        gpui_text_input::ClipboardWriteOutcome::Failed
                    };
                if outcome == gpui_text_input::ClipboardWriteOutcome::Failed {
                    self.report_clipboard_feedback(
                        super::MainWindowComposerClipboardFeedbackKind::Failed,
                        cx,
                    );
                }
                if self
                    .input
                    .update(cx, |input, cx| {
                        input.settle_clipboard_write(key, outcome, cx)
                    })
                    .is_err()
                {
                    self.last_error = Some("detached clipboard settlement was rejected".into());
                }
                self.schedule_pump(window, cx);
                cx.notify();
                return;
            }
            request => request,
        };
        let settlement = match &request {
            RangeTextInputRequest::Page(request) => Some(Ok(request.key())),
            RangeTextInputRequest::ObjectPage(request) => Some(Err(request.key())),
            RangeTextInputRequest::CancelPage(_)
            | RangeTextInputRequest::ReleasePage(_)
            | RangeTextInputRequest::CancelObjectPage(_)
            | RangeTextInputRequest::ReleaseObjectPage(_)
            | RangeTextInputRequest::CancelClipboardProvenancePage(_)
            | RangeTextInputRequest::CancelClipboardWrite(_) => None,
            _ => {
                self.last_error = Some("detached composer rejected a mutation request".into());
                return;
            }
        };
        if settlement.is_none() {
            self.schedule_pump(window, cx);
            return;
        }
        let (flight, source, cancellation) = match self.begin_detached_flight() {
            Ok(flight) => flight,
            Err(error) => {
                self.last_error = Some(error);
                return;
            }
        };
        let selection = self.selection;
        let task = cx.background_executor().spawn(async move {
            if cancellation.is_cancelled() {
                return Err("detached read cancelled".into());
            }
            let result = super::super::composer_slot::dispatch::translate::detached_request(
                &source,
                selection.binding(),
                request,
            );
            if cancellation.is_cancelled() {
                Err("detached read cancelled".into())
            } else {
                result
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_detached_flight(flight, selection) {
                    return;
                }
                match result {
                    Ok(outcome) => {
                        if let Err(error) = this.apply_page_or_object_outcome(outcome, window, cx) {
                            this.last_error = Some(error);
                        }
                    }
                    Err(error) => {
                        this.input
                            .update(cx, |input, cx| match settlement.unwrap() {
                                Ok(key) => {
                                    let _ = input.fail_page(
                                        key,
                                        gpui_text_input::PageFailure::Unavailable,
                                        cx,
                                    );
                                }
                                Err(key) => {
                                    let _ = input.fail_object_page(
                                        key,
                                        gpui_text_input::ObjectPageFailure::Unavailable,
                                        cx,
                                    );
                                }
                            });
                        this.last_error = Some(error);
                    }
                }
                this.schedule_pump(window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn drive_detached_clipboard_request(
        &mut self,
        request: RangeTextInputRequest,
        selected_range: RangeSourceSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (flight, source, cancellation) = match self.begin_detached_flight() {
            Ok(flight) => flight,
            Err(error) => {
                self.finish_propagated_clipboard_without_cut(cx);
                self.last_error = Some(error);
                return;
            }
        };
        let selection = self.selection;
        let clipboard_cancellation = self.propagated_clipboard.as_ref().unwrap().cancellation();
        let task = cx.background_executor().spawn(async move {
            if cancellation.is_cancelled() || clipboard_cancellation.is_cancelled() {
                return Err("detached copy cancelled".into());
            }
            let result = super::super::composer_slot::dispatch::translate::detached_request(
                &source,
                selection.binding(),
                request,
            );
            if cancellation.is_cancelled() || clipboard_cancellation.is_cancelled() {
                Err("detached copy cancelled".into())
            } else {
                result
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_detached_flight(flight, selection) {
                    return;
                }
                if this.propagated_clipboard.is_none() {
                    return;
                }
                let result = result.and_then(|outcome| {
                    this.propagated_clipboard
                        .as_mut()
                        .ok_or_else(|| "detached clipboard scan is unavailable".to_owned())?
                        .admit(outcome)
                });
                match result {
                    Ok(()) => this.drive_propagated_clipboard(selected_range, window, cx),
                    Err(error) => {
                        this.finish_propagated_clipboard_without_cut(cx);
                        this.last_error = Some(error);
                    }
                }
                this.schedule_pump(window, cx);
                cx.notify();
            });
        })
        .detach();
    }
}
