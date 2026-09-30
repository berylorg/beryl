use super::*;
use crate::{
    app_services::recovery_composer::PreparedComposerRecoveryAdapters,
    main_window::MainWindowComposerRecoveryPreparation,
};

impl MainWindowConversationComposerMount {
    pub(in crate::main_window) fn release_interrupted_exit_mount(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.window_close_released != Some(ticket) {
            return Err("recovered draft service close is not released".into());
        }
        // The retained completion makes this a local readiness check, without service access.
        if self.release_interrupted_exit_draft(ticket, cx)?
            != MainWindowConversationComposerCloseRelease::Released
        {
            return Ok(false);
        }
        let resident = self.contribution.as_ref().unwrap().clone();
        if !resident.update(cx, |resident, cx| {
            resident.release_interrupted_exit_resident(ticket, cx)
        })? {
            return Ok(false);
        }
        self.window_close = None;
        cx.notify();
        Ok(true)
    }

    pub(crate) fn release_interrupted_exit_draft(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerCloseRelease, String> {
        if ticket.owner != cx.entity_id() || !self.recovery_binding_current(ticket) {
            return Err("recovered draft close ticket is stale".into());
        }
        let close = self.window_close.as_ref().unwrap();
        if self.window_close_task.is_some()
            || self.window_close_workers.retained() != 0
            || close.flush.is_some()
            || close.release_requested
            || close.disposal_captured
            || close.state != MainWindowConversationComposerCloseAdvance::Preparing
        {
            return Err("recovered draft close custody is unsettled".into());
        }
        let resident = self
            .contribution
            .clone()
            .ok_or("recovered draft has no resident")?;
        if !resident.update(cx, |resident, cx| {
            resident.recovered_close_release_ready(ticket, cx)
        })? {
            return Ok(MainWindowConversationComposerCloseRelease::Pending);
        }
        if self.window_close_released == Some(ticket) {
            return Ok(MainWindowConversationComposerCloseRelease::Released);
        }
        if !self.prepare_recovered_autosave(ticket.selection())? {
            return Ok(MainWindowConversationComposerCloseRelease::Pending);
        }
        match self
            .bound_service()?
            .release_window_close_gate(ticket, None)?
        {
            None => Ok(MainWindowConversationComposerCloseRelease::Pending),
            Some(false) => Err("recovered draft service close ticket is stale".into()),
            Some(true) => {
                self.window_close_released = Some(ticket);
                Ok(MainWindowConversationComposerCloseRelease::Released)
            }
        }
    }

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

    pub(crate) fn adopt_interrupted_exit_resident<C: Send + 'static>(
        &mut self,
        resident: &Entity<MainWindowConversationComposer>,
        close: MainWindowConversationComposerCloseTicket,
        preparation: &mut MainWindowComposerRecoveryPreparation<C>,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(C, MainWindowConversationComposerCloseTicket), String> {
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
