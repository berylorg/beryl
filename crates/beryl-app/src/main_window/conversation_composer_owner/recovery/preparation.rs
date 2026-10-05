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

pub struct MainWindowComposerRecoveryPreparation<C = HomeRecoveryCandidate> {
    predecessor: RangeRestorationSeed,
    worker: MainWindowComposerCandidateWorker<C>,
    custody: MainWindowComposerCandidateCustody<C>,
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
        Self::from_worker(predecessor, worker, custody)
    }
}

impl<C: Send + 'static> MainWindowComposerRecoveryPreparation<C> {
    fn from_worker(
        predecessor: RangeRestorationSeed,
        worker: MainWindowComposerCandidateWorker<C>,
        custody: MainWindowComposerCandidateCustody<C>,
    ) -> Self {
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

    pub(crate) fn authenticated_window(&self) -> Result<beryl_state::SessionWindowRecord, String> {
        if let Some(error) = self.custody.preparation_error() {
            return Err(error);
        }
        self.custody
            .source()
            .map(|source| source.window().clone())
            .ok_or_else(|| "resident recovery source is unavailable".into())
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

    pub fn adopt(
        &mut self,
        input: &mut RangeTextInput,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut Context<RangeTextInput>,
    ) -> Result<(C, MainWindowComposerCandidateSource), String> {
        if self.candidate.is_none() || self.reservation.is_none() || !self.effects.is_empty() {
            return Err("resident preparation is not ready for adoption".into());
        }
        self.custody
            .retain_adoption_cleanup(cx.background_executor().clone())?;
        let result = input.adopt_resident_successor(
            self.reservation.take().unwrap(),
            self.environment.as_ref().unwrap(),
            self.candidate.take().unwrap(),
            current,
            window,
            cx,
        );
        if let Err(error) = result {
            self.cancel();
            return Err(format!("resident adoption refused: {error:?}"));
        }
        // No callback runs between checked adoption and this exact resource transfer.
        let resources = self
            .custody
            .take_resources()
            .expect("checked adoption custody");
        self.session.take();
        self.worker.cancel();
        Ok(resources)
    }

    pub fn adopt_resident(
        &mut self,
        resident: &mut MainWindowConversationComposer,
        close: MainWindowConversationComposerCloseTicket,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut Context<MainWindowConversationComposer>,
    ) -> Result<
        (
            C,
            Arc<MainWindowConversationComposerService>,
            MainWindowConversationComposerCloseTicket,
        ),
        String,
    > {
        resident.validate_recovery_retirement(close, cx)?;
        let snapshot = resident.recovery_snapshot.as_ref().unwrap();
        if snapshot.restoration != self.predecessor || snapshot.retired.is_some() {
            return Err("resident recovery predecessor is not owned by preparation".into());
        }
        let protection = snapshot.protection;
        {
            let source = self
                .custody
                .source()
                .ok_or("resident recovery source is unavailable")?;
            if source.predecessor() != close {
                return Err("resident recovery source belongs to another close".into());
            }
        }
        let clipboard_writer = MainWindowConversationComposer::production_clipboard_writer();
        let (candidate, source) = resident.input.update(cx, |input, cx| {
            let adopted = self.adopt(input, current, window, cx)?;
            input
                .release_resident_protection(protection, cx)
                .expect("adopted resident retains its exact predecessor protection");
            resident.unpublished_recovery_protection = Some((
                adopted.1.close_ticket(),
                input
                    .protect_resident(cx)
                    .expect("fresh prepared successor retains a quiescent disabled widget"),
            ));
            Ok::<_, String>(adopted)
        })?;
        let selection = source.selection();
        let fresh_close = source.close_ticket();
        let service = source.into_service();
        resident.selection = selection;
        resident.service = Some(service.clone());
        resident.clipboard_writer = Some(clipboard_writer);
        resident.private_clipboard_owner = service
            .private_clipboard_owner()
            .unwrap_or_else(|| MainWindowPrivateClipboardOwner::for_app(cx));
        resident.paste_queue = resident.private_clipboard_owner.paste_queue();
        service.set_private_clipboard_owner(resident.private_clipboard_owner.clone());
        resident.enable_checked_clipboard_writer();
        resident.window_close = Some(fresh_close);
        resident.recovery_snapshot = None;
        resident.admitted_positions = None;
        Ok((candidate, service, fresh_close))
    }

    pub fn cancel(&mut self) {
        self.worker.cancel();
        self.candidate.take();
        self.session.take();
        self.reservation.take();
    }

    pub(crate) fn worker_pending(&self) -> bool {
        self.custody.pending()
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
        C,
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

impl
    MainWindowComposerRecoveryPreparation<
        crate::app_services::recovery_graph::PreparedRecoveryServiceGraph,
    >
{
    pub(crate) fn prepare_graph(
        graph: crate::app_services::recovery_graph::PreparedRecoveryServiceGraph,
        retired: crate::main_window::MainWindowComposerRetiredClose,
        predecessor: RangeRestorationSeed,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> Self {
        let (worker, custody) = MainWindowComposerCandidateWorker::prepare_graph(
            graph,
            retired,
            predecessor,
            app,
            completed,
        );
        Self::from_worker(predecessor, worker, custody)
    }
}
