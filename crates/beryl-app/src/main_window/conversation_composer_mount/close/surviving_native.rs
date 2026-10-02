use super::*;

impl MainWindowConversationComposerMount {
    pub(in crate::main_window) fn reattach_native_close_custody(
        &mut self,
        resources: &mut MainWindowComposerMountRecoveryResources,
        fresh: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let active = self
            .window_close
            .ok_or("surviving native close gate is unavailable")?;
        let resident = self
            .contribution
            .as_ref()
            .ok_or("surviving native close resident is unavailable")?
            .clone();
        if active.ticket != resources.close
            || !active.resources_detached
            || active.disposing
            || self.service.is_some()
            || resources.service.is_none()
            || !resources
                .service
                .as_ref()
                .zip(resources.resident.service.as_ref())
                .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
            || resources.service.as_ref().unwrap().selected_identity() != Some(fresh.selection())
            || resources.publication_adapters.is_none()
            || resources.configurator.is_none()
            || resources.submission_source.is_none()
        {
            return Err("surviving native close reversible mount custody changed".into());
        }
        resident.read(cx).validate_native_close_reattachment(
            resources.close,
            fresh,
            &resources.resident,
            cx,
        )?;
        let publication = self.autosave.recovery_adapters()?;
        let submission = self.submission.recovery_source()?;
        if publication.is_some() || submission.is_some() {
            return Err("surviving native close adapters are already attached".into());
        }
        *publication = resources.publication_adapters.take();
        *submission = resources.submission_source.take();
        self.service = resources.service.take();
        self.configurator = resources.configurator.take();
        self.native_lineage_recovery = resources.native_lineage_control.take();
        resident.update(cx, |resident, cx| {
            resident.reattach_native_close_custody(fresh, &mut resources.resident, cx)
        });
        let active = self.window_close.as_mut().unwrap();
        active.ticket = fresh;
        active.resources_detached = false;
        active.restore_enabled = None;
        cx.notify();
        Ok(())
    }
}
