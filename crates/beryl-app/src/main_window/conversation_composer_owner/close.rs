use super::*;
use crate::main_window::MainWindowConversationComposerCloseTicket;

impl MainWindowConversationComposer {
    pub(in crate::main_window) fn begin_window_close_gate(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.is_live()
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
        self.input
            .update(cx, |input, cx| input.set_read_only(false, cx));
        self.schedule_pump(window, cx);
        Ok(true)
    }
}
