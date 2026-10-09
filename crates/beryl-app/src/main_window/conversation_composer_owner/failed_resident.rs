use super::*;
use gpui_text_input::RangeResidentProtection;
pub(super) mod preparation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowFailedResidentTicket {
    owner: gpui::EntityId,
    generation: u64,
    selection: MainWindowComposerSelectionIdentity,
}

pub(super) struct FailedResidentFence {
    pub(super) ticket: MainWindowFailedResidentTicket,
    pub(super) capture_taken: bool,
}

pub struct MainWindowFailedResidentCapture {
    ticket: MainWindowFailedResidentTicket,
    selection: MainWindowComposerSelectionIdentity,
    protection: RangeResidentProtection,
    restoration: RangeRestorationSeed,
    current_ticket: Option<MainWindowFailedResidentTicket>,
}

impl MainWindowFailedResidentCapture {
    pub fn ticket(&self) -> MainWindowFailedResidentTicket {
        self.ticket
    }
    pub fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }
    pub fn protection(&self) -> RangeResidentProtection {
        self.protection
    }
    pub fn restoration(&self) -> RangeRestorationSeed {
        self.restoration
    }

    pub(in crate::main_window) fn native_selection(&self) -> MainWindowComposerSelectionIdentity {
        self.current_ticket.unwrap_or(self.ticket).selection
    }
}

impl MainWindowConversationComposer {
    pub fn begin_failed_resident(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowFailedResidentTicket, String> {
        if let Some(fence) = &self.failed_resident {
            return Ok(fence.ticket);
        }
        if self.is_pending_target()
            || self.recovery_snapshot.is_some()
            || self.unpublished_recovery_protection.is_some()
            || !matches!(
                self.phase,
                MainWindowConversationComposerPhase::Live
                    | MainWindowConversationComposerPhase::Fencing
            )
        {
            return Err("failed resident capture requires the selected attached editor".into());
        }
        self.bound_service()?
            .qualify_failed_resident_home(self.selection)?;
        let generation = self
            .failed_resident_generation
            .checked_add(1)
            .ok_or("failed resident generation exhausted")?;
        let ticket = MainWindowFailedResidentTicket {
            owner: cx.entity_id(),
            generation,
            selection: self.selection,
        };
        self.failed_resident_generation = generation;
        self.failed_resident = Some(FailedResidentFence {
            ticket,
            capture_taken: false,
        });
        self.sync_mutation_gate(cx);
        cx.notify();
        Ok(ticket)
    }

    pub fn capture_failed_resident(
        &mut self,
        ticket: MainWindowFailedResidentTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<MainWindowFailedResidentCapture>, String> {
        self.validate_failed_ticket(ticket, cx)?;
        if self.failed_resident.as_ref().unwrap().capture_taken {
            return Err("failed resident capture is already owned".into());
        }
        if !self.failed_editor_drained(cx) {
            return Ok(None);
        }
        self.bound_service()?
            .qualify_failed_resident_source(self.selection)?;
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
            .map_err(|e| format!("failed resident protection refused: {e:?}"))?;
        self.failed_resident.as_mut().unwrap().capture_taken = true;
        self.phase = MainWindowConversationComposerPhase::RecoveryFenced;
        self.admitted_positions = None;
        self.scheduled = false;
        cx.notify();
        Ok(Some(MainWindowFailedResidentCapture {
            ticket,
            selection: self.selection,
            protection,
            restoration: protection.seed(),
            current_ticket: None,
        }))
    }

    pub fn detach_failed_resident_resources(
        &mut self,
        capture: &MainWindowFailedResidentCapture,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowComposerRecoveryResources, String> {
        self.validate_failed_capture(capture, cx)?;
        if !self.failed_editor_drained(cx) {
            return Err("failed resident editor work is not drained".into());
        }
        Ok(MainWindowComposerRecoveryResources {
            service: self.service.take(),
            clipboard_writer: self.clipboard_writer.take(),
            mutation_failure: self.last_mutation_admission_failure.take(),
        })
    }

    pub(crate) fn validate_failed_capture(
        &self,
        capture: &MainWindowFailedResidentCapture,
        cx: &App,
    ) -> Result<(), String> {
        let current = capture.current_ticket.unwrap_or(capture.ticket);
        self.validate_failed_ticket(current, cx)?;
        if self.selection != current.selection
            || !self.failed_resident.as_ref().unwrap().capture_taken
            || !self
                .input
                .read(cx)
                .resident_protection_is_current(capture.protection)
        {
            return Err("failed resident protection changed".into());
        }
        Ok(())
    }

    fn validate_failed_ticket(
        &self,
        ticket: MainWindowFailedResidentTicket,
        _cx: &App,
    ) -> Result<(), String> {
        if self
            .failed_resident
            .as_ref()
            .is_none_or(|f| f.ticket != ticket)
            || !same_resident(self.selection, ticket.selection)
        {
            return Err("failed resident request or selection changed".into());
        }
        Ok(())
    }

    pub(super) fn failed_editor_drained(&self, cx: &App) -> bool {
        self.failed_editor_work_drained() && self.input.read(cx).is_quiescent()
    }

    pub(super) fn failed_editor_work_drained(&self) -> bool {
        self.active_flight.is_none()
            && self.pending_dispatch.is_none()
            && self.pending_realizer.is_none()
            && self.activation_seeds.is_empty()
            && self.propagated_clipboard.is_none()
            && self.propagated_cut.is_none()
            && self.pending_marker_metadata.is_none()
            && self.mutation_evidence.is_none()
            && self.pending_marker_removal.is_none()
            && self.image_surface_attachment.is_none()
            && self.startup_release_completion.is_none()
    }

    #[cfg(test)]
    pub(crate) fn test_failed_editor_drain_diagnostics(&self, cx: &App) -> String {
        format!(
            "drained={},pending_target={},active_flight={},flight_id={:?},dispatch_flight={:?},last_object_delivery={:?},pending_dispatch={},pending_realizer={},activation_seeds={},clipboard={},cut={},marker_metadata={},mutation_evidence={},marker_removal={},image_surface={},startup_release={},input_quiescent={},pending_surface_ready={},last_error={:?}",
            self.failed_editor_drained(cx),
            self.is_pending_target(),
            self.active_flight.is_some(),
            self.active_flight,
            self.test_dispatch_flight
                .as_ref()
                .map(|flight| flight.diagnostics()),
            self.test_object_delivery,
            self.pending_dispatch.is_some(),
            self.pending_realizer.is_some(),
            self.activation_seeds.len(),
            self.propagated_clipboard.is_some(),
            self.propagated_cut.is_some(),
            self.pending_marker_metadata.is_some(),
            self.mutation_evidence.is_some(),
            self.pending_marker_removal.is_some(),
            self.image_surface_attachment.is_some(),
            self.startup_release_completion.is_some(),
            self.input.read(cx).is_quiescent(),
            self.pending_surface_ready(cx),
            self.last_error,
        )
    }
}

fn same_resident(
    current: MainWindowComposerSelectionIdentity,
    admitted: MainWindowComposerSelectionIdentity,
) -> bool {
    current.window_id() == admitted.window_id()
        && current.claim() == admitted.claim()
        && current.binding().home_id() == admitted.binding().home_id()
        && current.binding().home_generation() == admitted.binding().home_generation()
        && current.binding().host_generation() == admitted.binding().host_generation()
        && current.binding().candidate().session_id() == admitted.binding().candidate().session_id()
        && current.binding().presentation_generation()
            == admitted.binding().presentation_generation()
}
