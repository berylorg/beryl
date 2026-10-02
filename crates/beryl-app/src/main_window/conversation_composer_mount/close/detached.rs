use super::*;
use syndic_storage::DetachedDraftReadSourceV1;

impl MainWindowConversationComposerMount {
    fn validate_final_close(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &Context<Self>,
    ) -> Result<(), String> {
        let close = self
            .window_close
            .filter(|close| close.ticket == ticket && ticket.owner == cx.entity_id())
            .ok_or("final shutdown close custody changed")?;
        if close.state != MainWindowConversationComposerCloseAdvance::Ready
            || close.disposing
            || close.release_requested
            || close.recovery_fenced
            || close.flush.is_none()
            || self.window_close_task.is_some()
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
            return Err("final shutdown adapter work is not drained".into());
        }
        self.validate_recovery_native_resources()
    }

    pub(in crate::main_window) fn detached_export_service(
        &self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &Context<Self>,
    ) -> Result<Arc<MainWindowConversationComposerService>, String> {
        self.validate_final_close(close, cx)?;
        Ok(self.bound_service()?.clone())
    }

    pub(in crate::main_window) fn validate_detached_install(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        source: &DetachedDraftReadSourceV1,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.validate_final_close(close, cx)?;
        if self.window_close.unwrap().resources_detached {
            return Err("final shutdown resources already detached".into());
        }
        self.autosave.recovery_adapters()?;
        self.submission.recovery_source()?;
        let resident = self
            .contribution
            .as_ref()
            .ok_or("final shutdown lost its resident")?
            .clone();
        resident.update(cx, |resident, cx| {
            resident.validate_detached_install(close, source, cx)
        })
    }

    pub(in crate::main_window) fn install_detached_source(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        source: DetachedDraftReadSourceV1,
        cx: &mut Context<Self>,
    ) -> MainWindowComposerMountRecoveryResources {
        let adapters = self
            .autosave
            .recovery_adapters()
            .expect("preflighted final autosave custody")
            .take();
        let submission_source = self
            .submission
            .recovery_source()
            .expect("preflighted final submission custody")
            .take();
        let resident = self
            .contribution
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |resident, cx| {
                resident.install_detached_source(source, cx)
            });
        let resources = MainWindowComposerMountRecoveryResources {
            close,
            flush: self.window_close.unwrap().flush.unwrap(),
            resident,
            service: self.service.take(),
            publication_adapters: adapters,
            configurator: self.configurator.take(),
            submission_source,
            native_lineage_control: self.native_lineage_recovery.take(),
        };
        self.native_lineage_refresh_task.take();
        self.window_close.as_mut().unwrap().resources_detached = true;
        resources
    }

    pub(in crate::main_window) fn drain_detached_reads(&mut self, cx: &mut Context<Self>) -> bool {
        self.contribution.clone().is_none_or(|resident| {
            resident.update(cx, |resident, cx| resident.drain_detached_reads(cx))
        })
    }

    pub(in crate::main_window) fn resume_detached_reads(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(resident) = self.contribution.clone() {
            resident.update(cx, |resident, cx| {
                resident.resume_detached_reads(window, cx)
            });
        }
    }
}
