use super::*;
use crate::{
    app_services::recovery_composer::PreparedComposerRecoveryAdapters,
    main_window::{MainWindowConversationComposerConfigurator, MainWindowShellRoot},
};
use gpui_text_input::RangePrepublicationCurrent;

impl RunningProcessOwner {
    pub(crate) fn attach_interrupted_exit_resident(
        &mut self,
        request: &RunningExitRequest,
        key: &ResidentPreparationKey,
        root: &mut MainWindowShellRoot,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<
        (
            InterruptedExitCandidate,
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        if !self.process.commands.is_active(request) {
            return Err("Interrupted Exit request changed".into());
        }
        let drafts = self
            .shutdown
            .as_ref()
            .and_then(|attempt| attempt.drafts.as_ref())
            .ok_or("Interrupted Exit drafts are unavailable")?;
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
            || flight.session.is_none()
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
        let (candidate, close) = drafts.adopt_recovered_shell(
            root,
            resident.entity_id(),
            flight.close,
            &mut flight.preparation,
            adapters,
            configurator,
            current,
            window,
            cx,
        )?;
        captured.2 = close;
        let candidate = InterruptedExitCandidate {
            candidate,
            session: flight.session.take().unwrap(),
        };
        recovery.resident.take();
        Ok((candidate, close, record))
    }
}
