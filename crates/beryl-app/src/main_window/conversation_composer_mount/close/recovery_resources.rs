use super::*;

pub struct MainWindowComposerMountRecoveryResources {
    pub resident: crate::main_window::MainWindowComposerRecoveryResources,
    pub service: Option<Arc<MainWindowConversationComposerService>>,
    pub publication_adapters: Option<(beryl_state::AssetState, DraftMarkerSealService)>,
    pub configurator: Option<MainWindowConversationComposerConfigurator>,
    pub submission_source: Option<MainWindowComposerSubmissionRequestSource>,
    pub native_lineage_control: Option<crate::cas_projection::NativeLineageRecoveryControl>,
}

impl MainWindowConversationComposerMount {
    pub fn detach_interrupted_exit_resources(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<MainWindowComposerMountRecoveryResources>, String> {
        self.validate_recovery_adapter_detachment(ticket, cx)?;
        self.validate_recovery_native_resources()?;
        let close = self.window_close.as_mut().unwrap();
        if close.resources_detached {
            return Ok(None);
        }
        let adapters = self.autosave.recovery_adapters()?;
        let source = self.submission.recovery_source()?;
        let resident = self
            .contribution
            .as_ref()
            .unwrap()
            .update(cx, |resident, cx| {
                resident.detach_recovery_resources(ticket, cx)
            })?;
        let resources = MainWindowComposerMountRecoveryResources {
            resident,
            service: self.service.take(),
            publication_adapters: adapters.take(),
            configurator: self.configurator.take(),
            submission_source: source.take(),
            native_lineage_control: self.native_lineage_recovery.take(),
        };
        self.native_lineage_refresh_task.take();
        close.resources_detached = true;
        Ok(Some(resources))
    }
}
