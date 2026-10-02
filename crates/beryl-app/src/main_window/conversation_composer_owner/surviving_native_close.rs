use super::*;
use crate::main_window::MainWindowConversationComposerCloseTicket;

impl MainWindowConversationComposer {
    #[cfg(test)]
    pub(crate) fn test_retains_detached_native_close(&self) -> bool {
        matches!(self.phase, MainWindowConversationComposerPhase::Detached)
            && self
                .detached
                .as_ref()
                .is_some_and(|detached| detached.disposing)
            && self.window_close.is_some()
            && self.shutdown_interaction_gated
            && self.service.is_none()
    }
    pub(in crate::main_window) fn validate_native_close_reattachment(
        &self,
        old: MainWindowConversationComposerCloseTicket,
        fresh: MainWindowConversationComposerCloseTicket,
        resources: &MainWindowComposerRecoveryResources,
        cx: &App,
    ) -> Result<(), String> {
        let next = fresh.selection();
        if self.window_close != Some(old)
            || !old.matches_editor(self.selection)
            || !matches!(self.phase, MainWindowConversationComposerPhase::Detached)
            || self.active_flight.is_some()
            || self.pending_dispatch.is_some()
            || !self
                .detached
                .as_ref()
                .is_some_and(|detached| detached.disposing)
            || self.service.is_some()
            || resources.service.is_none()
            || next.window_id() != self.selection.window_id()
            || next.claim().thread_id() != self.selection.claim().thread_id()
            || next.claim().generation() != self.selection.claim().generation()
            || next.binding() != self.selection.binding()
            || self.input.read(cx).history_frontier()
                != self.selection.binding().range_history_frontier()
            || !self.input.read(cx).surface().is_some_and(|surface| {
                surface.binding() == self.selection.binding().range_binding()
            })
        {
            return Err("surviving native close protected editor correspondence changed".into());
        }
        Ok(())
    }

    pub(in crate::main_window) fn reattach_native_close_custody(
        &mut self,
        fresh: MainWindowConversationComposerCloseTicket,
        resources: &mut MainWindowComposerRecoveryResources,
        cx: &mut Context<Self>,
    ) {
        self.service = resources.service.take();
        if let Some(writer) = resources.clipboard_writer.take() {
            self.clipboard_writer = Some(writer);
        }
        self.last_mutation_admission_failure = resources.mutation_failure.take();
        self.detached = None;
        self.selection = fresh.selection();
        self.window_close = Some(fresh);
        self.phase = MainWindowConversationComposerPhase::Live;
        self.scheduled = false;
        self.input
            .update(cx, |input, cx| input.set_read_only(true, cx));
        cx.notify();
    }
}
