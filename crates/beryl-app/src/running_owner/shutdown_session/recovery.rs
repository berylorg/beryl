use super::*;
use crate::startup_owner::RunningExitRequest;

mod appearance;
mod completion;
mod construction;
mod driver;
mod process_work;
mod resident;
pub(crate) use resident::ResidentPreparationKey;
mod resident_retirement;
mod resume;
mod retirement;
mod service_preparation;
mod service_publication;
mod service_validation;
mod settlement;
mod theme_activation;
mod threadless;
pub(crate) use settlement::InterruptedExitCandidate;

pub(in crate::running_owner) struct InterruptedExitRecovery {
    request: Rc<()>,
    session: Rc<RefCell<Option<RunningShutdownSession>>>,
    settlement: Rc<RefCell<Option<settlement::CandidateSettlement>>>,
    service_validation: Rc<RefCell<Option<Result<(), String>>>>,
    theme_activation: Rc<RefCell<Option<Result<(), String>>>>,
    publication: Rc<
        RefCell<Option<Result<crate::cas_projection::initial_start::InitialStartOwner, String>>>,
    >,
    retirement: Rc<RefCell<Option<retirement::GraphRetirement>>>,
    resident: Option<resident::ResidentPreparation>,
    pending_resident_frame: Option<std::rc::Weak<()>>,
    reopen_schedule: beryl_home_store::RecoveryRetrySchedule,
    reopen_deadline: Option<std::time::Instant>,
    residents: Vec<(
        gpui::AnyWindowHandle,
        gpui::EntityId,
        crate::main_window::MainWindowConversationComposerCloseTicket,
    )>,
}

impl RunningProcessOwner {
    pub(in crate::running_owner) fn retain_reported_exit_failure(
        &mut self,
        request: &RunningExitRequest,
    ) {
        if self.interrupted_exit.is_some()
            || !self.process.commands.is_active(request)
            || self.shutdown_session().is_none()
        {
            return;
        }
        self.interrupted_exit = Some(InterruptedExitRecovery {
            request: request.identity(),
            session: Rc::new(RefCell::new(None)),
            settlement: Rc::new(RefCell::new(None)),
            service_validation: Rc::new(RefCell::new(None)),
            theme_activation: Rc::new(RefCell::new(None)),
            publication: Rc::new(RefCell::new(None)),
            retirement: Rc::new(RefCell::new(None)),
            resident: None,
            pending_resident_frame: None,
            reopen_schedule: Default::default(),
            reopen_deadline: None,
            residents: self
                .shutdown
                .as_ref()
                .and_then(|attempt| attempt.drafts.as_ref())
                .map(|drafts| drafts.borrow().recovery_residents())
                .unwrap_or_default(),
        });
    }

    pub(crate) fn retain_interrupted_exit_session(
        &mut self,
        request: &RunningExitRequest,
    ) -> Result<(), String> {
        let recovery = self
            .interrupted_exit
            .as_ref()
            .ok_or("No reported failed Exit")?;
        if !Rc::ptr_eq(&recovery.request, &request.identity())
            || !self.process.commands.is_active(request)
        {
            return Err("Interrupted Exit request changed".into());
        }
        if recovery.session.borrow().is_some() || recovery.settlement.borrow().is_some() {
            return Err("Interrupted Exit session is already retained".into());
        }
        if !matches!(
            self.shutdown_session(),
            Some(
                RunningShutdownSession::Settled(_)
                    | RunningShutdownSession::Reconciled(_)
                    | RunningShutdownSession::Unwound
            )
        ) {
            return Err("Interrupted Exit session work has not settled".into());
        }
        let session = self
            .shutdown
            .as_mut()
            .unwrap()
            .session
            .replace(RunningShutdownSession::RecoveryOwned);
        *self.interrupted_exit.as_ref().unwrap().session.borrow_mut() = session;
        Ok(())
    }

    pub(crate) fn interrupted_exit_session(
        &self,
    ) -> Option<std::cell::Ref<'_, RunningShutdownSession>> {
        std::cell::Ref::filter_map(self.interrupted_exit.as_ref()?.session.borrow(), |slot| {
            slot.as_ref()
        })
        .ok()
    }
}

impl RunningShutdownSession {
    pub(crate) fn settle_candidate(
        &mut self,
        candidate: &mut beryl_home_store::HomeRecoveryCandidate,
        session: &beryl_state::SessionState,
    ) -> Result<
        crate::exit_session::ExitSessionValidation,
        crate::exit_session::ExitSessionValidationError,
    > {
        match self {
            Self::Settled(Ok(ExitSessionExecution::Indeterminate(pending)))
            | Self::Reconciled(ExitSessionReconciled::Pending {
                reconciliation: pending,
                ..
            }) => pending.settle_candidate(candidate, session),
            Self::Settled(Ok(outcome)) => outcome.validate_candidate(candidate, session),
            Self::Reconciled(outcome) => outcome.validate_candidate(candidate, session),
            _ => Err(crate::exit_session::ExitSessionValidationError::Unproven),
        }
    }
}
