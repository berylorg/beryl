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
        !self.discussion_fresh
            || self.discussion.as_ref().is_some_and(|snapshot| {
                snapshot.composer()
                    != crate::discussion_presentation::DiscussionComposerContribution::Writable
            })
            || self.startup_interaction_gated
            || self.shutdown_interaction_gated
            || self.window_close.is_some()
            || self.failed_resident.is_some()
            || self.paste.is_some()
            || self.historical_replacement.is_some()
            || self.accepted_input_recall.is_some()
            || self.replacement_intent.is_none()
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
        let was_enabled = self.input.read(cx).is_enabled();
        let enabled = if self.is_live() || self.discussion_recovery_refresh_allowed() {
            (self.is_live() || was_enabled)
                && !self.is_pending_target()
                && !self.startup_interaction_gated
                && self.discussion.as_ref().is_none_or(|snapshot| {
                    snapshot.composer()
                        != crate::discussion_presentation::DiscussionComposerContribution::Inert
                })
        } else {
            was_enabled
        };
        self.input.update(cx, |input, cx| {
            input.set_read_only(read_only, cx);
            input.set_enabled(enabled, cx);
        });
    }

    pub(in crate::main_window) fn discussion_presentation(
        &self,
    ) -> Option<&crate::discussion_presentation::DiscussionPresentationSnapshot> {
        self.discussion.as_ref()
    }

    pub(in crate::main_window) fn discussion_presentation_reader(
        &self,
    ) -> Option<crate::discussion_presentation::DiscussionPresentationReader> {
        self.discussion_reader.clone()
    }

    pub(in crate::main_window) fn discussion_publication_epoch(&self) -> std::sync::Arc<()> {
        self.submission_observation_epoch.clone()
    }
    pub(in crate::main_window) fn discussion_presentation_fresh(&self) -> bool {
        self.discussion_fresh
    }
    pub(in crate::main_window) fn invalidate_discussion_presentation(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.discussion_fresh = false;
        self.sync_mutation_gate(cx);
        cx.notify();
    }

    pub(in crate::main_window) fn apply_discussion_presentation(
        &mut self,
        snapshot: crate::discussion_presentation::DiscussionPresentationSnapshot,
        cx: &mut Context<Self>,
    ) -> bool {
        if !(self.is_live() || self.discussion_recovery_refresh_allowed())
            || self.is_pending_target()
            || snapshot.thread_id() != self.selection.claim().thread_id()
            || snapshot.home_id() != self.selection.binding().home_id()
            || snapshot.home_generation() != self.selection.binding().home_generation()
            || self
                .discussion_reader
                .as_ref()
                .is_none_or(|reader| reader.service_generation() != snapshot.service_generation())
        {
            return false;
        }
        self.discussion_fresh = true;
        self.discussion = Some(snapshot);
        self.sync_mutation_gate(cx);
        cx.notify();
        true
    }

    pub(in crate::main_window) fn discussion_recovery_refresh_allowed(&self) -> bool {
        self.shutdown_interaction_gated
            && self
                .window_close
                .is_some_and(|close| self.recovery_binding_current(close))
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
