use super::*;
use beryl_home_store::HomeRecoveryCandidate;
use gpui_text_input::{
    RangePrepublicationCandidate, RangePrepublicationDelivery, RangePrepublicationEffect,
    RangePrepublicationEnvironment, RangePrepublicationSession, RangePrepublicationStatus,
    RangeResidentProtection, RangeResidentReservation, RangeSurfaceCharge,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerRecoveryProgress {
    Waiting,
    Advancing,
    Ready,
}

pub struct MainWindowComposerRecoveryPreparation {
    predecessor: RangeRestorationSeed,
    worker: MainWindowComposerCandidateWorker,
    custody: MainWindowComposerCandidateCustody,
    session: Option<RangePrepublicationSession>,
    environment: Option<RangePrepublicationEnvironment>,
    reservation: Option<RangeResidentReservation>,
    candidate: Option<RangePrepublicationCandidate>,
    effects: VecDeque<RangePrepublicationEffect>,
    text_system: Option<Weak<gpui::WindowTextSystem>>,
}

impl MainWindowComposerRecoveryPreparation {
    pub fn prepare(
        candidate: HomeRecoveryCandidate,
        retired: crate::main_window::MainWindowComposerRetiredClose,
        storage: syndic_storage::SyndicStorage,
        state: beryl_state::BerylState,
        predecessor: RangeRestorationSeed,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> Self {
        let (worker, custody) = MainWindowComposerCandidateWorker::prepare(
            candidate,
            retired,
            storage,
            state,
            predecessor,
            app,
            completed,
        );
        Self {
            predecessor,
            worker,
            custody,
            session: None,
            environment: None,
            reservation: None,
            candidate: None,
            effects: VecDeque::new(),
            text_system: None,
        }
    }

    pub fn authenticated_source(
        &self,
    ) -> Result<Option<(RangeRestorationSeed, MainWindowComposerSelectionIdentity)>, String> {
        if let Some(error) = self.custody.preparation_error() {
            return Err(error);
        }
        Ok(self
            .custody
            .source()
            .map(|source| (source.seed(), source.selection())))
    }

    pub fn admit(
        &mut self,
        input: &RangeTextInput,
        protection: RangeResidentProtection,
        environment: RangePrepublicationEnvironment,
        combined: RangeSurfaceCharge,
    ) -> Result<(), String> {
        if self.custody.cancelled() || self.session.is_some() {
            return Err("resident preparation is cancelled or already admitted".into());
        }
        if protection.seed() != self.predecessor {
            return Err("resident preparation predecessor changed".into());
        }
        let (seed, _) = self
            .authenticated_source()?
            .ok_or("resident preparation authentication has not returned")?;
        if environment.config().binding != seed.binding {
            return Err("resident preparation environment has another source".into());
        }
        let (session, reservation) = input
            .prepare_resident_successor(protection, seed, environment.clone(), combined)
            .map_err(|error| format!("resident preparation admission refused: {error:?}"))?;
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
            return Err("resident preparation is cancelled".into());
        }
        let reservation = self
            .reservation
            .as_ref()
            .ok_or("resident preparation is not admitted")?;
        if !input.resident_protection_is_current(reservation.protection()) {
            self.cancel();
            return Err("resident preparation protection changed".into());
        }
        if self
            .text_system
            .as_ref()
            .is_some_and(|expected| !Weak::ptr_eq(expected, &Arc::downgrade(text_system)))
        {
            self.cancel();
            return Err("resident preparation window changed".into());
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
            if delivery != RangePrepublicationDelivery::Accepted {
                return Err(format!(
                    "resident preparation delivery refused: {delivery:?}"
                ));
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
                        .ok_or("resident preparation candidate unavailable")?,
                );
                Ok(Progress::Ready)
            }
            RangePrepublicationStatus::CapacityBlocked
            | RangePrepublicationStatus::Cancelled
            | RangePrepublicationStatus::Stale
            | RangePrepublicationStatus::Failed(_) => {
                Err(format!("resident preparation stopped: {:?}", step.status))
            }
            _ => Ok(Progress::Advancing),
        }
    }

    pub fn cancel(&mut self) {
        self.worker.cancel();
        self.candidate.take();
        self.session.take();
        self.reservation.take();
    }

    pub fn advance_cleanup(&mut self) -> Result<bool, String> {
        if !self.custody.cancelled() {
            return Err("resident preparation is not cancelled".into());
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
        Result<
            MainWindowComposerCandidateSource,
            (crate::main_window::MainWindowComposerRetiredClose, String),
        >,
    )> {
        if !self.custody.cancelled() || !self.effects.is_empty() || !self.custody.cleanup_drained()
        {
            return None;
        }
        if let Some((candidate, source)) = self.custody.take_resources() {
            return Some((candidate, Ok(source)));
        }
        self.custody
            .take_refused_resources()
            .map(|(candidate, retired, error)| (candidate, Err((retired, error))))
    }
}
