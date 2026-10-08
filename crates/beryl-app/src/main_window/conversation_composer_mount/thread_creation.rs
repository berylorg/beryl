use super::*;

impl MainWindowConversationComposerMount {
    pub(in crate::main_window) fn fence_thread_predecessor(
        &mut self,
        selected: MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.window_close.is_some() || self.pending_presentation.is_some() {
            return Err("Thread creation predecessor is unavailable".into());
        }
        self.suspend_autosave()?;
        self.accept_thread_predecessor_selection(selected, cx)?;
        self.fence_contribution(selected, window, cx)
    }

    pub(in crate::main_window) fn accept_thread_predecessor_selection(
        &mut self,
        selected: MainWindowComposerSelectionIdentity,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let contribution = self
            .contribution
            .as_ref()
            .ok_or("Thread creation predecessor presentation is missing")?;
        contribution.update(cx, |composer, composer_cx| {
            let previous = composer.selection_identity();
            if previous == selected {
                return Ok(());
            }
            if previous.window_id() != selected.window_id() || previous.claim() != selected.claim()
            {
                return Err("Thread creation predecessor claim changed".into());
            }
            composer.synchronize_lifecycle_selection(previous, selected, composer_cx)
        })
    }

    pub(in crate::main_window) fn resume_thread_predecessor(
        &mut self,
        selected: MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.restore_claim_prior_selection(selected, cx)?;
        if let Some(contribution) = &self.contribution {
            contribution.update(cx, |composer, composer_cx| {
                composer.resume_after_widget_release_fence(window, composer_cx)
            })?;
        }
        Ok(())
    }
}
