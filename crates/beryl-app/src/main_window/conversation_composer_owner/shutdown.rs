use super::*;

impl MainWindowConversationComposer {
    pub(in crate::main_window) fn set_shutdown_interaction_gated(
        &mut self,
        gated: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.is_live() {
            return Err("conversation composer is being released".to_owned());
        }
        self.shutdown_interaction_gated = gated;
        self.sync_mutation_gate(cx);
        cx.notify();
        Ok(())
    }

    pub(in crate::main_window) fn mutation_gated(&self) -> bool {
        self.startup_interaction_gated
            || self.shutdown_interaction_gated
            || self.window_close.is_some()
            || self.failed_resident.is_some()
            || self.paste.is_some()
            || self.mutation_feedback().is_some_and(|feedback| {
                matches!(
                    feedback.kind,
                    MainWindowComposerMutationFeedbackKind::Unavailable
                        | MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable
                        | MainWindowComposerMutationFeedbackKind::CommittedUnavailable
                )
            })
    }

    pub(super) fn sync_mutation_gate(&self, cx: &mut Context<Self>) {
        let read_only = self.mutation_gated() || self.is_pending_target();
        self.input
            .update(cx, |input, cx| input.set_read_only(read_only, cx));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_set_shutdown_interaction_gated(
        &mut self,
        gated: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.set_shutdown_interaction_gated(gated, cx)
    }
}
