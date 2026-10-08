use super::*;

impl MainWindowConversationComposerMount {
    pub(crate) fn retire_unpublished_first_mount(
        &mut self,
        release: &MainWindowComposerWidgetRelease,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let resident = self
            .contribution
            .as_ref()
            .ok_or("first conversation resident is missing")?;
        if self.selected_identity() != Some(release.selection())
            || !resident.read(cx).startup_interaction_gated()
            || self.window_close.is_some()
            || self.failed_resident.is_some()
        {
            return Err("first conversation widget release belongs to another mount".into());
        }
        self.suspend_autosave()?;
        self.native_lineage_refresh_task.take();
        if self.window_close_task.is_some()
            || self.window_close_workers.retained() != 0
            || !self.autosave.workers_drained()
            || !self.submission.workers_drained()
            || self.submission.is_active()
            || self.pending_presentation.is_some()
            || self.native_lineage_snapshot.is_some()
            || self.native_lineage_validation_task.is_some()
            || self.native_lineage_workers.retained() != 0
            || self.pending_cleanup_workers.retained() != 0
            || self.native_disposal_workers.retained() != 0
            || self.native_lineage_disposal_active
            || self.native_lineage_disposal_task.is_some()
            || self.native_lineage_disposal_flush.is_some()
        {
            return Ok(false);
        }
        self.autosave.detach_recovery_adapters()?;
        self.submission.detach_recovery_source()?;
        self.contribution_subscription.take();
        self.service.take();
        self.configurator.take();
        self.native_lineage_recovery.take();
        Ok(true)
    }
}
