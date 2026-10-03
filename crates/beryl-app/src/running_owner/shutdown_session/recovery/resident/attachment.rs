use super::*;
use crate::{
    app_services::recovery_composer::PreparedComposerRecoveryAdapters,
    main_window::{MainWindowConversationComposerConfigurator, MainWindowShellRoot},
};
use gpui_text_input::RangePrepublicationCurrent;

impl RunningProcessOwner {
    pub(crate) fn attach_interrupted_exit_resident(
        &mut self,
        request: &impl RecoveryIdentity,
        key: &ResidentPreparationKey,
        root: &mut MainWindowShellRoot,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<
        (
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        if !self.active_recovery_identity(&request.identity()) {
            return Err("Interrupted Exit request changed".into());
        }
        #[cfg(test)]
        if self
            .interrupted_exit
            .as_ref()
            .is_some_and(|recovery| recovery.ordinary)
            && self.reject_ordinary_recovery_attachment_after == Some(0)
        {
            self.reject_ordinary_recovery_attachment_after = None;
            return Err("preserved ordinary resident attachment was refused".into());
        }
        let drafts = self.recovery_drafts()?;
        let mut drafts = drafts
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit drafts are busy")?;
        let recovery = self
            .interrupted_exit
            .as_mut()
            .ok_or("No reported failed Exit")?;
        let flight = recovery
            .resident
            .as_mut()
            .ok_or("No resident preparation")?;
        if !Rc::ptr_eq(&recovery.request, &request.identity())
            || !Rc::ptr_eq(&flight.request, &recovery.request)
            || !Rc::ptr_eq(&flight.key.0, &key.0)
            || flight.window != window.window_handle()
        {
            return Err("Resident preparation request changed".into());
        }
        let captured = recovery
            .residents
            .iter_mut()
            .find(|captured| {
                **captured == (flight.window, flight.resident.entity_id(), flight.close)
            })
            .ok_or("Resident is not captured by the interrupted Exit")?;
        if flight.cancelled
            || flight.result != Ok(Progress::Ready)
            || flight.scheduled.is_some()
            || recovery.session.borrow().is_none()
        {
            return Err("Resident preparation is not ready for attachment".into());
        }
        let resident = flight
            .resident
            .upgrade()
            .ok_or("Recovery resident was abandoned")?;
        let (_, selection) = flight
            .preparation
            .authenticated_source()?
            .ok_or("Recovery source is unavailable")?;
        if selection.binding().home_id() != flight.home
            || selection.binding().home_generation() != flight.generation
        {
            return Err("Resident recovery candidate generation changed".into());
        }
        let record = flight.preparation.authenticated_window()?;
        let mut slot = recovery.settlement.borrow_mut();
        if !matches!(slot.as_ref(), Some(CandidateSettlement::Pending)) {
            return Err("Resident preparation graph custody changed".into());
        }
        let (graph, close) = match &mut flight.preparation {
            Preparation::Failed(preparation) => drafts.adopt_failed_recovered_shell(
                root,
                resident.entity_id(),
                flight.close,
                preparation,
                adapters,
                configurator,
                current,
                window,
                cx,
            )?,
            Preparation::Clean(preparation) => drafts.adopt_recovered_shell(
                root,
                resident.entity_id(),
                flight.close,
                preparation,
                adapters,
                configurator,
                current,
                window,
                cx,
            )?,
        };
        captured.2 = close;
        *slot = Some(CandidateSettlement::Services(Ok(graph)));
        recovery.resident.take();
        #[cfg(test)]
        if recovery.ordinary {
            if let Some(remaining) = &mut self.reject_ordinary_recovery_attachment_after {
                *remaining = remaining.saturating_sub(1);
            }
        }
        Ok((close, record))
    }
}
