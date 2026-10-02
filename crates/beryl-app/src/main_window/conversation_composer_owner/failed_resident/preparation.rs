use super::*;
use crate::main_window::MainWindowFailedComposerRetirement;
use beryl_home_store::HomeRecoveryCandidate;
use gpui_text_input::{
    RangePrepublicationCandidate, RangePrepublicationEffect, RangePrepublicationEnvironment,
    RangePrepublicationSession, RangePrepublicationStatus, RangeResidentReservation,
    RangeSurfaceCharge,
};

type Worker = MainWindowComposerCandidateWorker<
    HomeRecoveryCandidate,
    MainWindowFailedResidentCandidateSource,
    MainWindowFailedComposerRetirement,
>;
type Custody = MainWindowComposerCandidateCustody<
    HomeRecoveryCandidate,
    MainWindowFailedResidentCandidateSource,
    MainWindowFailedComposerRetirement,
>;

pub struct MainWindowFailedResidentPreparation {
    worker: Worker,
    custody: Custody,
    capture: Option<MainWindowFailedResidentCapture>,
    session: Option<RangePrepublicationSession>,
    environment: Option<RangePrepublicationEnvironment>,
    reservation: Option<RangeResidentReservation>,
    candidate: Option<RangePrepublicationCandidate>,
    effects: VecDeque<RangePrepublicationEffect>,
    text_system: Option<Weak<gpui::WindowTextSystem>>,
}

pub struct MainWindowFailedResidentAdoption {
    ticket: MainWindowFailedResidentTicket,
    protection: RangeResidentProtection,
    retained: MainWindowFailedComposerRetirement,
}

impl MainWindowFailedResidentAdoption {
    pub fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.ticket.selection
    }
    pub fn protection(&self) -> RangeResidentProtection {
        self.protection
    }
    pub fn original_known_commit(&self) -> Option<bool> {
        self.retained.original_known_commit()
    }
    pub fn recovery_known_commit(&self) -> Option<bool> {
        self.retained.recovery_known_commit()
    }
}

impl MainWindowFailedResidentPreparation {
    pub fn new(
        candidate: HomeRecoveryCandidate,
        source: MainWindowFailedResidentCandidateSource,
        capture: MainWindowFailedResidentCapture,
    ) -> Result<
        Self,
        (
            HomeRecoveryCandidate,
            MainWindowFailedResidentCandidateSource,
            MainWindowFailedResidentCapture,
            String,
        ),
    > {
        if source.predecessor_selection() != capture.selection()
            || source.predecessor() != capture.restoration()
        {
            return Err((
                candidate,
                source,
                capture,
                "failed resident preparation predecessor changed".into(),
            ));
        }
        let (worker, custody) = Worker::from_source(
            candidate,
            source,
            MainWindowFailedResidentCandidateSource::read,
        );
        Ok(Self {
            worker,
            custody,
            capture: Some(capture),
            session: None,
            environment: None,
            reservation: None,
            candidate: None,
            effects: VecDeque::new(),
            text_system: None,
        })
    }

    pub fn authenticated_source(
        &self,
    ) -> Option<(RangeRestorationSeed, MainWindowComposerSelectionIdentity)> {
        self.custody
            .source()
            .map(|source| (source.seed(), source.selection()))
    }

    pub fn admit(
        &mut self,
        input: &RangeTextInput,
        environment: RangePrepublicationEnvironment,
        combined: RangeSurfaceCharge,
    ) -> Result<(), String> {
        if self.custody.cancelled() || self.session.is_some() {
            return Err("failed resident preparation is cancelled or already admitted".into());
        }
        let capture = self
            .capture
            .as_ref()
            .ok_or("failed resident capture is unavailable")?;
        let (seed, _) = self
            .authenticated_source()
            .ok_or("failed resident source is unavailable")?;
        if environment.config().binding != seed.binding {
            return Err("failed resident environment has another source".into());
        }
        let (session, reservation) = input
            .prepare_resident_successor(capture.protection, seed, environment.clone(), combined)
            .map_err(|e| format!("failed resident preparation refused: {e:?}"))?;
        self.worker
            .bind_prepublication(session.generation(), &environment)?;
        self.session = Some(session);
        self.environment = Some(environment);
        self.reservation = Some(reservation);
        Ok(())
    }

    pub fn advance(
        &mut self,
        input: &RangeTextInput,
        text_system: &Arc<gpui::WindowTextSystem>,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> Result<MainWindowComposerRecoveryProgress, String> {
        if self.custody.cancelled() {
            return Err("failed resident preparation is cancelled".into());
        }
        let reservation = self
            .reservation
            .as_ref()
            .ok_or("failed resident preparation is not admitted")?;
        if !input.resident_protection_is_current(reservation.protection())
            || self
                .text_system
                .as_ref()
                .is_some_and(|expected| !Weak::ptr_eq(expected, &Arc::downgrade(text_system)))
        {
            self.cancel();
            return Err("failed resident preparation protection or window changed".into());
        }
        self.text_system = Some(Arc::downgrade(text_system));
        let result = self.advance_inner(text_system, app, completed);
        if result.is_err() {
            self.cancel();
        }
        result
    }

    fn advance_inner(
        &mut self,
        text_system: &Arc<gpui::WindowTextSystem>,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> Result<MainWindowComposerRecoveryProgress, String> {
        use MainWindowComposerRecoveryProgress as Progress;
        if self.custody.pending() {
            return Ok(Progress::Waiting);
        }
        if self.candidate.is_some() {
            return Ok(Progress::Ready);
        }
        let session = self.session.as_mut().unwrap();
        if let Some(delivery) = self.custody.deliver_completion(session)? {
            if delivery != gpui_text_input::RangePrepublicationDelivery::Accepted {
                return Err(format!("failed resident delivery refused: {delivery:?}"));
            }
            self.custody.drive_cleanup(2);
            return Ok(Progress::Advancing);
        }
        if let Some(effect) = self.effects.pop_front() {
            if let Err((effect, error)) = self.worker.start(effect, app, completed) {
                self.effects.push_front(effect);
                return Err(error);
            }
            return Ok(Progress::Waiting);
        }
        let step = session.service(text_system);
        self.effects = step.effects.into();
        self.custody.drive_cleanup(2);
        match step.status {
            RangePrepublicationStatus::Ready => {
                self.candidate = Some(
                    session
                        .take_candidate()
                        .ok_or("failed resident candidate is unavailable")?,
                );
                Ok(Progress::Ready)
            }
            RangePrepublicationStatus::CapacityBlocked
            | RangePrepublicationStatus::Cancelled
            | RangePrepublicationStatus::Stale
            | RangePrepublicationStatus::Failed(_) => Err(format!(
                "failed resident preparation stopped: {:?}",
                step.status
            )),
            _ => Ok(Progress::Advancing),
        }
    }

    pub fn adopt_resident(
        &mut self,
        resident: &mut MainWindowConversationComposer,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut Context<MainWindowConversationComposer>,
    ) -> Result<
        (
            HomeRecoveryCandidate,
            Arc<MainWindowConversationComposerService>,
            MainWindowFailedResidentAdoption,
        ),
        String,
    > {
        let capture = self
            .capture
            .as_ref()
            .ok_or("failed resident capture is unavailable")?;
        resident.validate_failed_capture(capture, cx)?;
        if resident.service.is_some()
            || resident.clipboard_writer.is_some()
            || resident.last_mutation_admission_failure.is_some()
            || !resident.failed_editor_drained(cx)
            || self.candidate.is_none()
            || self.reservation.is_none()
            || !self.effects.is_empty()
        {
            return Err("failed resident adoption still owns old resources or is not ready".into());
        }
        self.custody
            .retain_adoption_cleanup(cx.background_executor().clone())?;
        let adopted = resident.input.update(cx, |input, cx| {
            input
                .adopt_protected_resident_successor(
                    self.reservation.take().unwrap(),
                    self.environment.as_ref().unwrap(),
                    self.candidate.take().unwrap(),
                    current,
                    window,
                    cx,
                )
                .map_err(|e| format!("failed resident adoption refused: {e:?}"))
        });
        let protection = match adopted {
            Ok(protection) => protection,
            Err(error) => {
                self.cancel();
                return Err(error);
            }
        };
        let (candidate, source) = self
            .custody
            .take_resources()
            .expect("checked failed resident adoption retains exact source custody");
        let selection = source.selection();
        let (service, retained) = source.into_resources();
        let old = self.capture.take().unwrap().ticket;
        let ticket = MainWindowFailedResidentTicket { selection, ..old };
        resident.selection = selection;
        resident.service = Some(service.clone());
        resident.clipboard_writer =
            Some(MainWindowConversationComposer::production_clipboard_writer());
        resident.failed_resident = Some(FailedResidentFence {
            ticket,
            capture_taken: true,
        });
        resident.admitted_positions = None;
        self.session.take();
        self.worker.cancel();
        Ok((
            candidate,
            service,
            MainWindowFailedResidentAdoption {
                ticket,
                protection,
                retained,
            },
        ))
    }

    pub fn cancel(&mut self) {
        self.worker.cancel();
        self.candidate.take();
        self.session.take();
        self.reservation.take();
    }
    pub fn advance_cleanup(&mut self) -> Result<bool, String> {
        if !self.custody.cancelled() {
            return Err("failed resident preparation is not cancelled".into());
        }
        if let Some(effect) = self.effects.pop_front() {
            if let Err((effect, error)) = self.custody.settle_undispatched(effect) {
                self.effects.push_front(effect);
                return Err(error);
            }
        }
        self.custody.drive_cleanup(2);
        Ok(self.effects.is_empty() && self.custody.cleanup_drained())
    }
    pub fn take_cancelled_resources(
        &mut self,
    ) -> Option<(
        HomeRecoveryCandidate,
        MainWindowFailedResidentCandidateSource,
        MainWindowFailedResidentCapture,
    )> {
        if !self.custody.cancelled() || !self.effects.is_empty() || !self.custody.cleanup_drained()
        {
            return None;
        }
        let (candidate, source) = self.custody.take_resources()?;
        Some((candidate, source, self.capture.take()?))
    }
}

impl MainWindowConversationComposer {
    pub fn publish_failed_resident(
        &mut self,
        adoption: &MainWindowFailedResidentAdoption,
        store: &beryl_home_store::HomeStore,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let health = store.health();
        self.bound_service()?
            .qualify_published_failed_resident(store, self.selection)?;
        if self
            .failed_resident
            .as_ref()
            .is_none_or(|f| f.ticket != adoption.ticket)
            || self.selection != adoption.selection()
            || health.state() != beryl_home_store::HomeHealthState::Healthy
            || health.generation() != Some(self.selection.binding().home_generation())
            || store.home_id() != self.selection.binding().home_id()
            || self.bound_service()?.selected_identity() != Some(self.selection)
            || !self
                .input
                .read(cx)
                .resident_protection_is_current(adoption.protection)
        {
            return Err(
                "failed resident publication requires exact fresh healthy graph and protection"
                    .into(),
            );
        }
        self.input
            .update(cx, |input, cx| {
                input.release_resident_protection(adoption.protection, cx)
            })
            .map_err(|e| format!("failed resident publication protection refused: {e:?}"))?;
        self.failed_resident = None;
        self.phase = MainWindowConversationComposerPhase::Live;
        self.input
            .update(cx, |input, cx| input.set_enabled(true, cx));
        self.sync_mutation_gate(cx);
        cx.notify();
        Ok(())
    }
}
