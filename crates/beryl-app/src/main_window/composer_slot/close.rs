use super::*;
use crate::composer_host::ComposerHostFlushTicket;
use crate::main_window::MainWindowConversationComposerCloseTicket;

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn begin_window_close_gate(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) -> Result<(), MainWindowComposerSlotError> {
        self.ensure_live()?;
        if self.pending.is_some()
            || self.disposal_stage.is_some()
            || self.submission_successor.is_some()
            || self.native_lineage_suspension.is_some()
            || self
                .selected_identity()
                .is_none_or(|selection| !ticket.matches_editor(selection))
        {
            return Err(MainWindowComposerSlotError::ActivationPending);
        }
        self.window_close = Some(ticket);
        Ok(())
    }

    pub(in crate::main_window) fn window_close_is_current(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) -> bool {
        self.window_close == Some(ticket)
            && self
                .selected_identity()
                .is_some_and(|selection| ticket.matches_editor(selection))
    }

    pub(in crate::main_window) fn release_window_close_gate(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        flush: Option<ComposerHostFlushTicket>,
    ) -> Result<bool, MainWindowComposerSlotError> {
        if !self.window_close_is_current(ticket) {
            return Ok(false);
        }
        if let Some(flush) = flush
            && !self
                .selected
                .as_mut()
                .unwrap()
                .host
                .release_window_close(flush)?
        {
            return Ok(false);
        }
        self.window_close = None;
        self.disposal_stage = None;
        Ok(true)
    }

    pub(in crate::main_window) fn authorize_window_close_disposal(
        &mut self,
        store: &HomeStore,
        ticket: MainWindowConversationComposerCloseTicket,
        flush: ComposerHostFlushTicket,
    ) -> Result<ComposerHostFlushAdvance, MainWindowComposerSlotError> {
        if !self.window_close_is_current(ticket) {
            return Ok(ComposerHostFlushAdvance::Stale);
        }
        let advance = self
            .selected
            .as_mut()
            .unwrap()
            .host
            .authorize_window_close_disposal(store, flush)?;
        if advance == ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired) {
            self.disposal_stage = Some(DisposalStage::Flushing(flush));
        }
        Ok(advance)
    }
}
