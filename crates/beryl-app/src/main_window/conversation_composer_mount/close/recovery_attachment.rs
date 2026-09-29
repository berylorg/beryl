use super::*;
use crate::{
    app_services::recovery_composer::PreparedComposerRecoveryAdapters,
    main_window::MainWindowComposerRecoveryPreparation,
};

impl MainWindowConversationComposerMount {
    pub(in crate::main_window) fn recovery_binding_current(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) -> bool {
        self.window_close.is_some_and(|close| {
            close.ticket == ticket
                && close.recovery_fenced
                && !close.resources_detached
                && !close.disposing
        }) && self.service.is_some()
            && self.configurator.is_some()
            && self.native_lineage_recovery.is_some()
    }

    pub(crate) fn adopt_interrupted_exit_resident(
        &mut self,
        resident: &Entity<MainWindowConversationComposer>,
        close: MainWindowConversationComposerCloseTicket,
        preparation: &mut MainWindowComposerRecoveryPreparation,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<
        (
            beryl_home_store::HomeRecoveryCandidate,
            MainWindowConversationComposerCloseTicket,
        ),
        String,
    > {
        self.validate_recovery_retirement(close, cx)?;
        if self.contribution.as_ref() != Some(resident) {
            return Err("recovery mount belongs to another resident".into());
        }
        let (_, selection) = preparation
            .authenticated_source()?
            .ok_or("recovery source is unavailable")?;
        if !adapters.as_ref().is_some_and(|adapters| {
            adapters.matches(
                selection.binding().home_id(),
                selection.binding().home_generation(),
            )
        }) || configurator.is_none()
        {
            return Err(
                "recovery mount adapters are missing or belong to another candidate".into(),
            );
        }
        let publication_slot = self.autosave.recovery_adapters()?;
        let submission_slot = self.submission.recovery_source()?;
        let (candidate, service, fresh_close) = resident.update(cx, |resident, cx| {
            preparation.adopt_resident(resident, close, current, window, cx)
        })?;
        let (assets, marker, submission, native) = adapters.take().unwrap().into_parts();
        *publication_slot = Some((assets, marker));
        *submission_slot = Some(submission);
        self.service = Some(service);
        self.configurator = configurator.take();
        self.native_lineage_recovery = Some(native);
        let active = self.window_close.as_mut().unwrap();
        active.ticket = fresh_close;
        active.flush = None;
        active.state = MainWindowConversationComposerCloseAdvance::Preparing;
        active.resources_detached = false;
        active.restore_enabled = None;
        Ok((candidate, fresh_close))
    }
}
