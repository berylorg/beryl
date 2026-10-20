use beryl_home_store::CommandCancellation;
use syndic_storage::{DraftSubmissionIntent, ReplacementEditIntent};

use super::*;
use crate::composer_host::{
    ComposerHostHistoricalAdvance, ComposerHostHistoricalCompletion, ComposerHostHistoricalTarget,
    ComposerHostHistoricalTicket,
};
use crate::composer_marker_seal::DraftMarkerSealService;
use crate::main_window::conversation_composer_mount::autosave::{
    current_timestamp, fresh_marker_authority, fresh_piece_operation_id,
};

pub(super) struct MountedHistoricalReplacement {
    cancellation: CommandCancellation,
    state: State,
}

enum State {
    Requested {
        target: Option<ComposerHostHistoricalTarget>,
        marker_seals: Option<DraftMarkerSealService>,
    },
    Running,
    Blocked {
        selection: MainWindowComposerSelectionIdentity,
        ticket: Option<ComposerHostHistoricalTicket>,
        at: syndic_storage::SyndicTimestamp,
    },
    Realizing {
        completed: Completion,
        rebind_requested: bool,
        frontier_installed: bool,
    },
}

enum WorkOutcome {
    Ready(Completion),
    Refused {
        selection: MainWindowComposerSelectionIdentity,
        error: String,
    },
}

struct WorkFailure {
    selection: MainWindowComposerSelectionIdentity,
    ticket: Option<ComposerHostHistoricalTicket>,
    error: String,
    at: syndic_storage::SyndicTimestamp,
}

struct Completion {
    service: Arc<MainWindowConversationComposerService>,
    selection: MainWindowComposerSelectionIdentity,
    ticket: ComposerHostHistoricalTicket,
    value: ComposerHostHistoricalCompletion,
    acknowledged: bool,
    initiating_selection: MainWindowComposerSelectionIdentity,
}

impl Completion {
    fn acknowledge(&mut self) -> Result<(), String> {
        self.service
            .slot
            .lock()
            .map_err(|_| "Historical replacement service lock failed")?
            .acknowledge_historical_replacement(self.selection, self.ticket, self.value)?;
        self.acknowledged = true;
        Ok(())
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        if !self.acknowledged {
            // The durable checkpoint survives when its native observer has departed.
            let _ = self.acknowledge();
        }
    }
}

impl MountedHistoricalReplacement {
    pub(super) fn cancel(&self) {
        self.cancellation.cancel();
    }
}

impl MainWindowConversationComposerService {
    pub(in crate::main_window) fn selected_submission_intent(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Option<DraftSubmissionIntent> {
        self.slot.lock().ok()?.selected_submission_intent(selection)
    }
}

impl MainWindowConversationComposer {
    pub(super) fn retire_failed_historical_replacement(&mut self) {
        if let Some(mut replacement) = self.historical_replacement.take() {
            if let State::Realizing { completed, .. } = &mut replacement.state {
                completed.acknowledged = true;
            }
        }
    }

    pub(in crate::main_window) fn historical_replacement_busy(&self) -> bool {
        self.historical_replacement.is_some() || self.replacement_intent.is_none()
    }

    pub(in crate::main_window) fn historical_edit_disabled_reason(
        &self,
        cx: &App,
    ) -> Option<&'static str> {
        if !self.is_live() || self.mutation_gated() || self.last_error.is_some() {
            return Some("The selected composer is unavailable.");
        }
        if self.replacement_intent != Some(DraftSubmissionIntent::Ordinary) {
            return Some("Finish or cancel the current message edit first.");
        }
        let root = self.selection.binding().root().summary();
        if root.logical_utf8_bytes() != 0 || root.marker_count() != 0 {
            return Some("Empty the composer before editing a historical message.");
        }
        if self.active_flight.is_some()
            || self.pending_dispatch.is_some()
            || self.mutation_evidence.is_some()
            || self.quote_insertion.is_some()
            || !self.input.read(cx).is_quiescent()
            || self
                .input
                .read(cx)
                .surface()
                .is_none_or(|surface| surface.composition().is_some())
        {
            return Some("Wait for the current composer work to finish.");
        }
        self.historical_edit_unavailable()
            .map(|reason| reason.reason())
    }

    pub(in crate::main_window) fn historical_replacement_feedback(&self) -> Option<&str> {
        self.historical_replacement_feedback.as_deref()
    }

    pub(in crate::main_window) fn clear_historical_replacement_feedback(&mut self) {
        self.historical_replacement_feedback = None;
    }

    pub(in crate::main_window) fn report_historical_replacement_refusal(
        &mut self,
        error: String,
        cx: &mut Context<Self>,
    ) {
        self.historical_replacement_feedback = Some(error);
        cx.notify();
    }

    pub(in crate::main_window) fn historical_replacement_target(
        &self,
    ) -> Option<ReplacementEditIntent> {
        match self.replacement_intent {
            Some(DraftSubmissionIntent::Replacement(intent)) => Some(intent),
            _ => None,
        }
    }

    pub(in crate::main_window) fn start_historical_replacement(
        &mut self,
        target: ComposerHostHistoricalTarget,
        cancellation: CommandCancellation,
        marker_seals: DraftMarkerSealService,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.is_live()
            || self.mutation_gated()
            || self.active_flight.is_some()
            || self.pending_dispatch.is_some()
            || self.mutation_evidence.is_some()
            || self.quote_insertion.is_some()
            || self.last_error.is_some()
            || self.replacement_intent != Some(DraftSubmissionIntent::Ordinary)
            || !self.input.read(cx).is_quiescent()
            || self
                .input
                .read(cx)
                .surface()
                .is_none_or(|surface| surface.composition().is_some())
            || self
                .selection
                .binding()
                .root()
                .summary()
                .logical_utf8_bytes()
                != 0
            || self.selection.binding().root().summary().marker_count() != 0
            || cancellation.is_cancelled()
        {
            return Err("Historical replacement is unavailable while the composer is busy".into());
        }
        self.historical_replacement = Some(MountedHistoricalReplacement {
            cancellation,
            state: State::Requested {
                target: Some(target),
                marker_seals: Some(marker_seals),
            },
        });
        self.historical_replacement_feedback = None;
        self.sync_mutation_gate(cx);
        self.schedule_pump(window, cx);
        cx.notify();
        Ok(())
    }

    pub(in crate::main_window) fn cancel_historical_replacement(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.marker_menu().is_some() || self.image_preview().is_some() {
            return;
        }
        if !self.is_live()
            || self.historical_replacement.is_some()
            || self.historical_replacement_target().is_none()
            || self.last_error.is_some()
        {
            return;
        }
        self.historical_replacement = Some(MountedHistoricalReplacement {
            cancellation: CommandCancellation::new(),
            state: State::Requested {
                target: None,
                marker_seals: None,
            },
        });
        self.sync_mutation_gate(cx);
        self.schedule_pump(window, cx);
        cx.notify();
    }

    pub(super) fn pump_historical_replacement(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut replacement) = self.historical_replacement.take() else {
            return false;
        };
        let result = match &mut replacement.state {
            State::Requested { .. }
                if !self.input.read(cx).is_semantically_quiescent()
                    || self.mutation_evidence.is_some()
                    || self.quote_insertion.is_some()
                    || self.paste.is_some() =>
            {
                self.historical_replacement = Some(replacement);
                return false;
            }
            State::Requested {
                target,
                marker_seals,
            } => {
                let target = *target;
                let marker_seals = marker_seals.take();
                replacement.state = State::Running;
                self.historical_replacement = Some(replacement);
                if let Err(error) =
                    self.launch_historical_replacement(target, marker_seals, None, window, cx)
                {
                    self.historical_replacement = None;
                    self.last_error = Some(error);
                    self.sync_mutation_gate(cx);
                }
                return true;
            }
            State::Running | State::Blocked { .. } => Ok(true),
            State::Realizing {
                completed,
                rebind_requested,
                frontier_installed,
            } => self.realize_historical_replacement(
                completed,
                rebind_requested,
                frontier_installed,
                window,
                cx,
            ),
        };
        match result {
            Ok(blocked) => {
                if matches!(&replacement.state, State::Realizing { completed, .. } if completed.acknowledged)
                {
                    self.sync_mutation_gate(cx);
                    cx.notify();
                    false
                } else {
                    self.historical_replacement = Some(replacement);
                    blocked
                }
            }
            Err(error) => {
                self.historical_replacement = Some(replacement);
                self.last_error = Some(error);
                cx.notify();
                true
            }
        }
    }

    fn realize_historical_replacement(
        &mut self,
        completed: &mut Completion,
        rebind_requested: &mut bool,
        frontier_installed: &mut bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        #[cfg(feature = "test-faults")]
        if self.historical_realization_hold {
            return Ok(true);
        }
        if self.service.as_ref().and_then(|s| s.selected_identity()) != Some(completed.selection) {
            return Err(
                "Historical replacement completion no longer owns this selected editor".into(),
            );
        }
        if completed.value.requires_rebind() {
            let binding = completed.value.binding().range_binding();
            if !*rebind_requested {
                let caret = completed
                    .value
                    .caret()
                    .ok_or("Imported caret is unavailable")?;
                let anchor = completed.value.selection().unwrap_or(caret);
                self.input
                    .update(cx, |input, cx| input.set_enabled(false, cx));
                #[cfg(feature = "test-faults")]
                if std::mem::take(&mut self.historical_rebind_test.0) {
                    self.input
                        .update(cx, |input, cx| {
                            let mut snapshot = input.resident_layout_snapshot();
                            snapshot.layout.wrap_width += gpui::px(1.0);
                            input.set_layout(snapshot.layout, snapshot.style, cx)
                        })
                        .map_err(|e| {
                            format!("Historical rebind layout qualification failed: {e}")
                        })?;
                }
                let rebind = self.input.update(cx, |input, cx| {
                    input.rebind(
                        binding,
                        Some(RangeSourceSelection {
                            anchor,
                            head: caret,
                        }),
                        window,
                        cx,
                    )
                });
                #[cfg(feature = "test-faults")]
                if matches!(rebind, Err(gpui_text_input::RangeTextInputError::Busy)) {
                    self.historical_rebind_test.1 += 1;
                }
                match rebind {
                    Ok(()) | Err(gpui_text_input::RangeTextInputError::Busy) => {
                        *rebind_requested = true
                    }
                    Err(error) => {
                        return Err(format!("Historical replacement rebind failed: {error}"));
                    }
                }
                self.admitted_positions = None;
            }
            if !*frontier_installed {
                if self.input.read(cx).history_frontier().binding() != binding {
                    return Ok(true);
                }
                self.input
                    .update(cx, |input, _| {
                        input.set_history_frontier(
                            input.history_frontier(),
                            completed.value.binding().range_history_frontier(),
                        )
                    })
                    .map_err(|e| format!("Imported history frontier is unavailable: {e}"))?;
                *frontier_installed = true;
            }
            self.selection = completed.selection;
            if !self
                .input
                .read(cx)
                .surface()
                .is_some_and(|surface| surface.binding() == binding)
                || !self.input.read(cx).is_surface_current_and_interactive()
            {
                return Ok(false);
            }
        }
        completed.acknowledge()?;
        let previous = completed.initiating_selection;
        self.selection = completed.selection;
        self.replacement_intent = Some(completed.value.intent());
        if completed.value.requires_rebind() {
            self.image_surfaces.selection_changed(self.selection);
            self.private_clipboard_owner
                .adopted(previous, self.selection, None);
        }
        if previous != self.selection {
            cx.emit(MainWindowConversationComposerEvent::SelectionAdvanced {
                previous,
                current: self.selection,
            });
        }
        if self.is_live() {
            cx.emit(
                MainWindowConversationComposerEvent::ReplacementModeChanged {
                    selection: self.selection,
                    intent: completed.value.intent(),
                },
            );
        }
        Ok(false)
    }

    fn launch_historical_replacement(
        &mut self,
        target: Option<ComposerHostHistoricalTarget>,
        marker_seals: Option<DraftMarkerSealService>,
        resume: Option<(
            MainWindowComposerSelectionIdentity,
            ComposerHostHistoricalTicket,
            syndic_storage::SyndicTimestamp,
        )>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let operation = if resume.is_none() {
            Some(fresh_piece_operation_id()?)
        } else {
            None
        };
        let at = match resume {
            Some((_, _, at)) => at,
            None => current_timestamp()?,
        };
        let (flight, service) = self.begin_flight()?;
        let selection = self.selection;
        let cancellation = self
            .historical_replacement
            .as_ref()
            .unwrap()
            .cancellation
            .clone();
        let task = cx.background_executor().spawn(async move {
            let initiating_selection = selection;
            let mut selected = selection;
            let mut retained_ticket = resume.map(|(_, ticket, _)| ticket);
            let result = (|| -> Result<WorkOutcome, String> {
                let selection = if let Some(marker_seals) = marker_seals {
                    flush_before_import(&service, selection, marker_seals, &cancellation)?
                } else {
                    resume.map_or(selection, |(selection, _, _)| selection)
                };
                selected = selection;
                let admission = if let Some((_, ticket, _)) = resume {
                    Ok(ticket)
                } else {
                    service
                        .slot
                        .lock()
                        .map_err(|_| "Historical replacement service lock failed")?
                        .begin_historical_replacement(
                            &service.store,
                            selection,
                            target,
                            operation.unwrap(),
                            at,
                        )
                };
                let ticket = match admission {
                    Ok(ticket) => ticket,
                    Err(error) => {
                        if service.store.health().state()
                            == beryl_home_store::HomeHealthState::Healthy
                        {
                            service
                                .slot
                                .lock()
                                .map_err(|_| "Historical replacement service lock failed")?
                                .clear_historical_preflush(selection);
                        }
                        return Ok(WorkOutcome::Refused { selection, error });
                    }
                };
                retained_ticket = Some(ticket);
                loop {
                    let (current, advance) = service
                        .slot
                        .lock()
                        .map_err(|_| "Historical replacement service lock failed")?
                        .advance_historical_replacement(
                            &service.store,
                            selection,
                            ticket,
                            at,
                            &cancellation,
                        )?;
                    match advance {
                        ComposerHostHistoricalAdvance::Progress => {}
                        ComposerHostHistoricalAdvance::NotCommitted => {
                            return Ok(WorkOutcome::Refused {
                                selection: current,
                                error: "Historical replacement was not committed".into(),
                            });
                        }
                        ComposerHostHistoricalAdvance::Ready(value) => {
                            return Ok(WorkOutcome::Ready(Completion {
                                service: service.clone(),
                                selection: current,
                                ticket,
                                value,
                                acknowledged: false,
                                initiating_selection,
                            }));
                        }
                    }
                }
            })();
            result.map_err(|error| WorkFailure {
                selection: selected,
                ticket: retained_ticket,
                error,
                at,
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result: Result<WorkOutcome, WorkFailure> = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_flight(flight) {
                    return;
                }
                match result {
                    Ok(WorkOutcome::Ready(completed)) => {
                        if this.selection != selection {
                            return;
                        }
                        this.historical_replacement.as_mut().unwrap().state = State::Realizing {
                            completed,
                            rebind_requested: false,
                            frontier_installed: false,
                        };
                    }
                    Ok(WorkOutcome::Refused {
                        selection: current,
                        error,
                    }) => {
                        this.historical_replacement = None;
                        this.historical_replacement_feedback = Some(error);
                        if this.selection != current {
                            let previous = this.selection;
                            this.selection = current;
                            cx.emit(MainWindowConversationComposerEvent::SelectionAdvanced {
                                previous,
                                current,
                            });
                        }
                    }
                    Err(failure) => {
                        this.historical_replacement.as_mut().unwrap().state = State::Blocked {
                            selection: failure.selection,
                            ticket: failure.ticket,
                            at: failure.at,
                        };
                        this.last_error = Some(failure.error);
                    }
                }
                this.sync_mutation_gate(cx);
                this.schedule_pump(window, cx);
                cx.notify();
            });
        })
        .detach();
        Ok(())
    }

    pub(in crate::main_window) fn resume_historical_replacement(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(MountedHistoricalReplacement {
            state:
                State::Blocked {
                    selection,
                    ticket: Some(ticket),
                    at,
                },
            ..
        }) = self.historical_replacement.as_ref()
        else {
            return Err("Historical replacement has no retained admitted ticket".into());
        };
        let resume = (*selection, *ticket, *at);
        self.launch_historical_replacement(None, None, Some(resume), window, cx)?;
        self.historical_replacement.as_mut().unwrap().state = State::Running;
        self.last_error = None;
        Ok(())
    }
}

#[cfg(feature = "test-faults")]
impl MainWindowConversationComposer {
    pub fn test_start_historical_replacement(
        &mut self,
        target: ComposerHostHistoricalTarget,
        marker_seals: DraftMarkerSealService,
        defer_rebind: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.historical_rebind_test.0 = defer_rebind;
        self.start_historical_replacement(
            target,
            CommandCancellation::new(),
            marker_seals,
            window,
            cx,
        )
    }

    pub fn test_hold_historical_realization(&mut self, hold: bool) {
        self.historical_realization_hold = hold;
    }

    pub fn test_historical_replacement_state(
        &self,
    ) -> (bool, Option<ReplacementEditIntent>, usize) {
        (
            self.historical_replacement_busy(),
            self.historical_replacement_target(),
            self.historical_rebind_test.1,
        )
    }

    pub fn test_cancel_historical_replacement(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_historical_replacement(window, cx);
    }
}

fn flush_before_import(
    service: &MainWindowConversationComposerService,
    selection: MainWindowComposerSelectionIdentity,
    marker_seals: DraftMarkerSealService,
    cancellation: &CommandCancellation,
) -> Result<MainWindowComposerSelectionIdentity, String> {
    use crate::composer_host::{
        ComposerHostFlushAdmission as Admission, ComposerHostFlushAdvance as Advance,
        ComposerHostFlushCapture as Capture, ComposerHostFlushState,
    };
    let admission = service
        .slot
        .lock()
        .map_err(|_| "Historical replacement service lock failed")?
        .begin_historical_replacement_flush(selection)
        .map_err(|e| e.to_string())?;
    let (ticket, mut state) = match admission {
        Admission::Satisfied(_) => return Ok(selection),
        Admission::Started { ticket, state } | Admission::Joined { ticket, state } => {
            (ticket, state)
        }
    };
    let mut selection = selection;
    loop {
        let mut slot = service
            .slot
            .lock()
            .map_err(|_| "Historical replacement service lock failed")?;
        if state == ComposerHostFlushState::CaptureRequired {
            let marker_authority = match slot
                .selected_autosave_capture_requirement(selection)
                .map_err(|e| e.to_string())?
            {
                MainWindowComposerAutosaveCaptureRequirement::ChangedMarkers => {
                    Some(fresh_marker_authority()?)
                }
                _ => None,
            };
            let assets = slot.assets();
            match slot
                .capture_selected_flush_publication(
                    &service.store,
                    selection,
                    ticket,
                    assets,
                    &marker_seals,
                    fresh_piece_operation_id()?,
                    marker_authority,
                    current_timestamp()?,
                    cancellation,
                )
                .map_err(|e| e.to_string())?
            {
                Capture::Satisfied(_) => {
                    return slot
                        .selected_identity()
                        .ok_or("Saved historical replacement editor disappeared".into());
                }
                Capture::Captured(_) => state = ComposerHostFlushState::PublicationPending,
                Capture::State(next) => state = next,
                Capture::Unsatisfied(_) | Capture::Stale => {
                    return Err("Historical replacement draft save was refused".into());
                }
            }
        }
        match slot
            .advance_selected_flush(&service.store, selection, ticket)
            .map_err(|e| e.to_string())?
        {
            Advance::Satisfied(_) => {
                return slot
                    .selected_identity()
                    .ok_or("Saved historical replacement editor disappeared".into());
            }
            Advance::Progress(next) => state = next,
            Advance::ReconciliationPending => {
                return Err("Historical replacement draft save is awaiting reconciliation".into());
            }
            Advance::Unsatisfied(_) | Advance::Stale => {
                return Err("Historical replacement draft save was refused".into());
            }
        }
        selection = slot
            .selected_identity()
            .ok_or("Saved historical replacement editor disappeared")?;
    }
}
