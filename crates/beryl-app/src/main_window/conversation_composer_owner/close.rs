use super::*;
use crate::main_window::MainWindowConversationComposerCloseTicket;

impl MainWindowConversationComposer {
    pub(in crate::main_window) fn release_interrupted_exit_resident(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if !self.prepare_interrupted_exit_resident(ticket, cx)? {
            return Ok(false);
        }
        self.phase = MainWindowConversationComposerPhase::Live;
        self.window_close = None;
        self.failed_resident = None;
        self.sync_mutation_gate(cx);
        cx.notify();
        Ok(true)
    }

    pub(in crate::main_window) fn prepare_interrupted_exit_resident(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if !self.recovered_close_release_ready(ticket, cx)? {
            return Ok(false);
        }
        if let Some((close, protection)) = self.unpublished_recovery_protection {
            if close != ticket
                || !self
                    .input
                    .read(cx)
                    .resident_protection_is_current(protection)
            {
                return Err("unpublished recovery resident protection changed".into());
            }
            self.input
                .update(cx, |input, cx| {
                    input.release_resident_protection(protection, cx)
                })
                .map_err(|error| {
                    format!("recovered resident protection release was rejected: {error:?}")
                })?;
            self.unpublished_recovery_protection = None;
            if self.fresh_recovery_gui.is_some() {
                self.activation_seeds.clear();
            }
        }
        self.input.update(cx, |input, cx| {
            input.set_read_only(true, cx);
            input.set_enabled(true, cx);
            if input.is_enabled() {
                Ok(())
            } else {
                Err("recovered composer could not enable input".to_owned())
            }
        })?;
        Ok(true)
    }

    pub(in crate::main_window) fn recovered_close_release_ready(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if !self.recovery_binding_current(ticket)
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated
        {
            return Err("recovered composer close binding is not fenced".into());
        }
        if let Some(error) = &self.last_error {
            return Err(error.clone());
        }
        Ok(self.active_flight.is_none()
            && self.pending_dispatch.is_none()
            && self.propagated_clipboard.is_none()
            && self
                .input
                .update(cx, |input, _| input.is_semantically_quiescent()))
    }

    pub(in crate::main_window) fn begin_window_close_gate(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.is_live()
            || self.startup_interaction_gated
            || self.is_pending_target()
            || !ticket.matches_editor(self.selection)
            || self.window_close.is_some_and(|current| current != ticket)
        {
            return Err("conversation composer cannot admit window close".to_owned());
        }
        self.window_close = Some(ticket);
        self.input
            .update(cx, |input, cx| input.set_read_only(true, cx));
        self.schedule_pump(window, cx);
        Ok(())
    }

    pub(in crate::main_window) fn window_close_flush_ready(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.window_close != Some(ticket) || !ticket.matches_editor(self.selection) {
            return Err("conversation composer close ticket is stale".to_owned());
        }
        if let Some(error) = &self.last_error {
            return Err(error.clone());
        }
        Ok(self.is_live()
            && self.active_flight.is_none()
            && self.pending_dispatch.is_none()
            && self.propagated_clipboard.is_none()
            && self
                .input
                .update(cx, |input, _| input.is_semantically_quiescent()))
    }

    pub(in crate::main_window) fn release_window_close_gate(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.window_close != Some(ticket) || !ticket.matches_editor(self.selection) {
            return Ok(false);
        }
        if !self.is_live() {
            return Err("conversation composer close disposal is already active".to_owned());
        }
        self.window_close = None;
        self.sync_mutation_gate(cx);
        self.schedule_pump(window, cx);
        Ok(true)
    }
}
