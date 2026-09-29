use super::*;
mod attachment;
use crate::main_window::{
    MainWindowComposerCandidateSource, MainWindowComposerRecoveryPreparation,
    MainWindowComposerRecoveryProgress as Progress, MainWindowComposerRetiredClose,
    MainWindowComposerSelectionIdentity, MainWindowConversationComposer,
    MainWindowConversationComposerCloseTicket,
};
use gpui::{AnyWindowHandle, WeakEntity, Window};
use gpui_text_input::{RangePrepublicationEnvironment, RangeRestorationSeed, RangeSurfaceCharge};

type Environment = Box<
    dyn FnOnce(
        RangeRestorationSeed,
        MainWindowComposerSelectionIdentity,
        &Window,
    ) -> Result<(RangePrepublicationEnvironment, RangeSurfaceCharge), String>,
>;
type Completion = Box<dyn FnOnce(&Rc<RefCell<RunningProcessOwner>>, &mut App)>;

#[derive(Clone)]
pub(crate) struct ResidentPreparationKey(Rc<()>);

pub(crate) struct CancelledResidentPreparation {
    pub(crate) candidate: InterruptedExitCandidate,
    pub(crate) source:
        Result<MainWindowComposerCandidateSource, (MainWindowComposerRetiredClose, String)>,
}

pub(super) struct ResidentPreparation {
    key: ResidentPreparationKey,
    request: Rc<()>,
    resident: WeakEntity<MainWindowConversationComposer>,
    close: MainWindowConversationComposerCloseTicket,
    home: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    window: AnyWindowHandle,
    preparation: MainWindowComposerRecoveryPreparation,
    session: Option<beryl_state::SessionState>,
    environment: Option<Environment>,
    result: Result<Progress, String>,
    cancelled: bool,
    scheduled: Option<Rc<()>>,
    returned: Option<Box<CancelledResidentPreparation>>,
    completed: Option<Completion>,
    _window_closed: gpui::Subscription,
}

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_resident_preparation_state(
        &self,
    ) -> (bool, bool, bool, bool, Result<Progress, String>) {
        let flight = self
            .interrupted_exit
            .as_ref()
            .unwrap()
            .resident
            .as_ref()
            .unwrap();
        (
            flight.preparation.worker_pending(),
            flight.scheduled.is_some(),
            flight.cancelled,
            flight.returned.is_some(),
            flight.result.clone(),
        )
    }

    #[cfg(test)]
    pub(crate) fn test_retain_resident_recovery(
        &mut self,
        request: &RunningExitRequest,
        captured: (
            AnyWindowHandle,
            gpui::EntityId,
            MainWindowConversationComposerCloseTicket,
        ),
    ) {
        assert!(self.interrupted_exit.is_none());
        assert!(self.process.commands.is_active(request));
        self.interrupted_exit = Some(InterruptedExitRecovery {
            request: request.identity(),
            session: Rc::new(RefCell::new(Some(RunningShutdownSession::Unwound))),
            settlement: Rc::new(RefCell::new(None)),
            retirement: Rc::new(RefCell::new(None)),
            resident: None,
            pending_resident_frame: None,
            residents: vec![captured],
        });
    }

    pub(crate) fn prepare_interrupted_exit_resident(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        resident: &Entity<MainWindowConversationComposer>,
        close: MainWindowConversationComposerCloseTicket,
        window: AnyWindowHandle,
        candidate: &mut Option<InterruptedExitCandidate>,
        retired: &mut Option<MainWindowComposerRetiredClose>,
        storage: syndic_storage::SyndicStorage,
        state: beryl_state::BerylState,
        environment: impl FnOnce(
            RangeRestorationSeed,
            MainWindowComposerSelectionIdentity,
            &Window,
        )
            -> Result<(RangePrepublicationEnvironment, RangeSurfaceCharge), String>
        + 'static,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<ResidentPreparationKey, String> {
        let mut retained = owner.borrow_mut();
        if !retained.process.commands.is_active(request) {
            return Err("Interrupted Exit request changed".into());
        }
        let recovery = retained
            .interrupted_exit
            .as_mut()
            .ok_or("No reported failed Exit")?;
        if !Rc::ptr_eq(&recovery.request, &request.identity()) {
            return Err("Interrupted Exit request changed".into());
        }
        if !recovery
            .residents
            .contains(&(window, resident.entity_id(), close))
        {
            return Err("Resident is not captured by the interrupted Exit".into());
        }
        if recovery
            .pending_resident_frame
            .as_ref()
            .is_some_and(|wake| wake.strong_count() != 0)
        {
            return Err("Previous resident frame has not returned".into());
        }
        if recovery.resident.is_some()
            || recovery.settlement.borrow().is_some()
            || recovery.session.borrow().is_none()
            || matches!(
                recovery.retirement.borrow().as_ref(),
                Some(retirement::GraphRetirement::Pending)
            )
        {
            return Err("Interrupted Exit recovery custody is unavailable".into());
        }
        let composer = resident.read(app);
        composer.validate_recovery_retirement(close, app)?;
        let snapshot = composer.recovery_snapshot().unwrap();
        let source = retired
            .as_ref()
            .ok_or("Resident retirement is unavailable")?;
        let fresh = candidate
            .as_ref()
            .ok_or("Recovery candidate is unavailable")?;
        if source.close_ticket() != close
            || source.selection() != snapshot.selection()
            || source.host().close_ticket() != snapshot.flush_ticket()
            || snapshot.retired_close().is_some()
            || fresh.candidate.home_id() != snapshot.selection().binding().home_id()
            || fresh.candidate.generation() == snapshot.selection().binding().home_generation()
        {
            return Err("Resident recovery source correspondence changed".into());
        }
        let predecessor = *snapshot.restoration();
        let key = ResidentPreparationKey(Rc::new(()));
        let wake_owner = owner.clone();
        let wake_key = key.clone();
        let fresh = candidate.take().unwrap();
        let home = fresh.candidate.home_id();
        let generation = fresh.candidate.generation();
        let weak_owner = Rc::downgrade(owner);
        let closed_key = key.clone();
        let window_closed = app.on_window_closed(move |app| {
            if !app.windows().contains(&window) {
                if let Some(owner) = weak_owner.upgrade() {
                    let _ = Self::cancel_interrupted_exit_resident(&owner, &closed_key, app);
                }
            }
        });
        let preparation = MainWindowComposerRecoveryPreparation::prepare(
            fresh.candidate,
            retired.take().unwrap(),
            storage,
            state,
            predecessor,
            app,
            move |app| Self::schedule_resident_preparation(&wake_owner, &wake_key, app),
        );
        recovery.resident = Some(ResidentPreparation {
            key: key.clone(),
            request: request.identity(),
            resident: resident.downgrade(),
            close,
            home,
            generation,
            window,
            preparation,
            session: Some(fresh.session),
            environment: Some(Box::new(environment)),
            result: Ok(Progress::Waiting),
            cancelled: false,
            scheduled: None,
            returned: None,
            completed: Some(Box::new(completed)),
            _window_closed: window_closed,
        });
        Ok(key)
    }

    fn schedule_resident_preparation(
        owner: &Rc<RefCell<Self>>,
        key: &ResidentPreparationKey,
        app: &mut App,
    ) {
        let wake = Rc::new(());
        let (window, cancelled) = {
            let mut retained = owner.borrow_mut();
            let Some(flight) = retained
                .interrupted_exit
                .as_mut()
                .and_then(|r| r.resident.as_mut())
            else {
                return;
            };
            if !Rc::ptr_eq(&flight.key.0, &key.0)
                || flight.scheduled.is_some()
                || flight.returned.is_some()
            {
                return;
            }
            flight.scheduled = Some(wake.clone());
            let target = (flight.window, flight.cancelled);
            if !target.1 {
                retained
                    .interrupted_exit
                    .as_mut()
                    .unwrap()
                    .pending_resident_frame = Some(Rc::downgrade(&wake));
            }
            target
        };
        let owner = owner.clone();
        let key = key.clone();
        if cancelled {
            let delay = app
                .background_executor()
                .timer(std::time::Duration::from_millis(16));
            app.spawn(async move |cx| {
                delay.await;
                let _ =
                    cx.update(|app| Self::advance_resident_preparation(&owner, &key, &wake, app));
            })
            .detach();
        } else {
            let frame_owner = Rc::downgrade(&owner);
            let frame_key = key.clone();
            let frame_wake = wake.clone();
            if window
                .update(app, |_, window, _| {
                    window.on_next_frame(move |_, app| {
                        app.defer(move |app| {
                            if let Some(owner) = frame_owner.upgrade() {
                                Self::advance_resident_preparation(
                                    &owner,
                                    &frame_key,
                                    &frame_wake,
                                    app,
                                )
                            }
                        });
                    });
                })
                .is_err()
            {
                app.defer(move |app| Self::advance_resident_preparation(&owner, &key, &wake, app));
            }
        }
    }

    fn advance_resident_preparation(
        owner: &Rc<RefCell<Self>>,
        key: &ResidentPreparationKey,
        wake: &Rc<()>,
        app: &mut App,
    ) {
        let (again, completed) = {
            let mut retained = owner.borrow_mut();
            let Some(recovery) = retained.interrupted_exit.as_ref() else {
                return;
            };
            let Some(flight) = recovery.resident.as_ref() else {
                return;
            };
            if !Rc::ptr_eq(&flight.key.0, &key.0)
                || !flight
                    .scheduled
                    .as_ref()
                    .is_some_and(|current| Rc::ptr_eq(current, wake))
            {
                return;
            }
            let current = Rc::ptr_eq(&recovery.request, &flight.request)
                && recovery.residents.contains(&(
                    flight.window,
                    flight.resident.entity_id(),
                    flight.close,
                ))
                && retained
                    .process
                    .commands
                    .is_active_identity(&flight.request);
            let flight = retained
                .interrupted_exit
                .as_mut()
                .unwrap()
                .resident
                .as_mut()
                .unwrap();
            flight.scheduled = None;
            if !current {
                flight.cancel("Interrupted Exit request changed".into());
            }
            if !flight.cancelled {
                let wake_owner = owner.clone();
                let wake_key = key.clone();
                let step = flight
                    .window
                    .update(app, |_, window, app| {
                        flight.advance(window, app, move |app| {
                            Self::schedule_resident_preparation(&wake_owner, &wake_key, app)
                        })
                    })
                    .map_err(|error| error.to_string())
                    .and_then(|value| value);
                match step {
                    Ok(progress) => flight.result = Ok(progress),
                    Err(error) => flight.cancel(error),
                }
            }
            let mut cleanup_failed = false;
            let again = if flight.cancelled {
                match flight.preparation.advance_cleanup() {
                    Ok(true) => {
                        if let Some((candidate, source)) =
                            flight.preparation.take_cancelled_resources()
                        {
                            flight.returned = Some(Box::new(CancelledResidentPreparation {
                                candidate: InterruptedExitCandidate {
                                    candidate,
                                    session: flight.session.take().unwrap(),
                                },
                                source,
                            }));
                        }
                        false
                    }
                    Ok(false) => !flight.preparation.worker_pending(),
                    Err(error) => {
                        flight.result = Err(error);
                        cleanup_failed = true;
                        false
                    }
                }
            } else {
                flight.result == Ok(Progress::Advancing)
            };
            let completed = if flight.returned.is_some()
                || flight.result == Ok(Progress::Ready)
                || cleanup_failed
            {
                flight.completed.take()
            } else {
                None
            };
            (again, completed)
        };
        if again {
            Self::schedule_resident_preparation(owner, key, app);
        }
        if let Some(completed) = completed {
            completed(owner, app);
        }
    }

    pub(crate) fn cancel_interrupted_exit_resident(
        owner: &Rc<RefCell<Self>>,
        key: &ResidentPreparationKey,
        app: &mut App,
    ) -> Result<(), String> {
        {
            let mut retained = owner.borrow_mut();
            let flight = retained
                .interrupted_exit
                .as_mut()
                .and_then(|r| r.resident.as_mut())
                .ok_or("No resident preparation")?;
            if !Rc::ptr_eq(&flight.key.0, &key.0) {
                return Err("Resident preparation changed".into());
            }
            flight.cancel("Resident preparation cancelled".into());
        }
        Self::schedule_resident_preparation(owner, key, app);
        Ok(())
    }

    pub(crate) fn interrupted_exit_resident_result(
        &self,
        key: &ResidentPreparationKey,
    ) -> Result<Progress, String> {
        let recovery = self
            .interrupted_exit
            .as_ref()
            .ok_or("No reported failed Exit")?;
        let flight = recovery
            .resident
            .as_ref()
            .ok_or("No resident preparation")?;
        if !Rc::ptr_eq(&flight.key.0, &key.0)
            || !Rc::ptr_eq(&recovery.request, &flight.request)
            || !self.process.commands.is_active_identity(&flight.request)
        {
            return Err("Resident preparation request changed".into());
        }
        flight.result.clone()
    }

    pub(crate) fn take_cancelled_resident_preparation(
        &mut self,
        key: &ResidentPreparationKey,
    ) -> Option<Box<CancelledResidentPreparation>> {
        let slot = &mut self.interrupted_exit.as_mut()?.resident;
        let flight = slot.as_mut()?;
        if !Rc::ptr_eq(&flight.key.0, &key.0) {
            return None;
        }
        let resources = flight.returned.take()?;
        slot.take();
        Some(resources)
    }
}

impl ResidentPreparation {
    fn cancel(&mut self, error: String) {
        if !self.cancelled {
            self.cancelled = true;
            self.scheduled = None;
            self.result = Err(error);
            self.environment.take();
            self.preparation.cancel();
        }
    }

    fn advance(
        &mut self,
        window: &Window,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> Result<Progress, String> {
        let resident = self
            .resident
            .upgrade()
            .ok_or("Recovery resident was abandoned")?;
        let composer = resident.read(app);
        composer.validate_recovery_retirement(self.close, app)?;
        let Some((seed, selection)) = self.preparation.authenticated_source()? else {
            return Ok(Progress::Waiting);
        };
        if selection.binding().home_id() != self.home
            || selection.binding().home_generation() != self.generation
        {
            return Err("Resident recovery candidate generation changed".into());
        }
        let protection = composer.recovery_snapshot().unwrap().protection();
        let input = composer.gpui_input();
        if let Some(environment) = self.environment.take() {
            let (environment, capacity) = environment(seed, selection, window)?;
            self.preparation
                .admit(input.read(app), protection, environment, capacity)?;
        }
        // The entity read cannot borrow App across a worker dispatch.
        input.update(app, |input, app| {
            self.preparation
                .advance(input, window.text_system(), app, completed)
        })
    }
}
