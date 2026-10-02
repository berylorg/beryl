use super::*;
use crate::{
    app_services::{AppServiceCloseError, PreparedShutdownObservation},
    cas_projection::{ProjectionCancellationToken, ShutdownWorkObservation},
    window_acquisition::WindowCloseLease,
};
use beryl_model::WindowId;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunningShutdownStatus {
    AwaitingObservation,
    Observing,
    Admitted,
    WorkReady,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ConfirmedShutdownError {
    #[error("shutdown observation belongs to an inactive attempt")]
    StaleObservation,
    #[error(transparent)]
    Service(#[from] AppServiceCloseError),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum IdleShutdownError {
    #[error("the running owner already retains shutdown intent custody")]
    IntentBusy,
    #[error("process work requires shutdown confirmation")]
    ConfirmationRequired,
    #[error("shutdown window custody is unavailable: {0}")]
    Window(String),
    #[error(transparent)]
    Preparation(#[from] crate::app_services::CloseConfirmationPreparationError),
    #[error(transparent)]
    Service(#[from] AppServiceCloseError),
}

pub(super) struct RunningShutdownAttempt {
    pub(super) session: Option<super::shutdown_session::RunningShutdownSession>,
    invoking: WindowId,
    intent: ShutdownIntent,
    pub(super) lease: WindowCloseLease,
    pending: Option<Arc<()>>,
    pub(super) admitted: bool,
    pub(super) work_ready: bool,
    pub(super) drafts: Option<Rc<RefCell<super::shutdown_drafts::RunningShutdownDrafts>>>,
    pub(super) placements:
        Option<Rc<RefCell<super::shutdown_placements::RunningShutdownPlacements>>>,
}

impl RunningShutdownAttempt {
    pub(super) fn intent(&self) -> ShutdownIntent {
        self.intent
    }
}

impl RunningProcessOwner {
    pub(super) fn begin_nonfinal_close(
        &mut self,
        invoking: WindowId,
        app: &App,
    ) -> Result<bool, String> {
        if self.shutdown.is_some()
            || self.confirmation.is_some()
            || self.progress.is_some()
            || self.observing_initial_work
        {
            return Err("window close owner is busy".into());
        }
        let services = self
            .process
            .services
            .as_ref()
            .ok_or("window close services are unavailable")?;
        let lease = services.admit_ordinary_close(self.process.windows.window_ids())?;
        if lease.is_final(invoking).map_err(|e| e.to_string())? {
            return Ok(false);
        }
        self.ordinary_close_window = Some(
            self.process
                .windows
                .shells()
                .iter()
                .find(|shell| {
                    shell
                        .window()
                        .read(app)
                        .ok()
                        .and_then(|root| root.controller())
                        .is_some_and(|controller| controller.window_id() == invoking)
                })
                .ok_or("ordinary close invoking window is unavailable")?
                .window(),
        );
        self.shutdown = Some(RunningShutdownAttempt {
            session: None,
            invoking,
            intent: ShutdownIntent::NonfinalWindowClose,
            lease,
            pending: None,
            admitted: true,
            work_ready: true,
            drafts: None,
            placements: None,
        });
        Ok(true)
    }
}

pub(crate) struct PreparedConfirmedShutdownObservation {
    identity: Arc<()>,
    work: PreparedShutdownObservation,
}

pub(crate) struct CompletedConfirmedShutdownObservation {
    identity: Arc<()>,
    result: Result<ShutdownWorkObservation, AppServiceCloseError>,
}

impl PreparedConfirmedShutdownObservation {
    pub(crate) fn collect(
        self,
        cancellation: &ProjectionCancellationToken,
    ) -> CompletedConfirmedShutdownObservation {
        CompletedConfirmedShutdownObservation {
            identity: self.identity,
            result: self.work.collect(cancellation),
        }
    }
}

impl CompletedConfirmedShutdownObservation {
    #[cfg(test)]
    pub(crate) fn test_duplicate_success(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            result: Ok(self.result.as_ref().unwrap().clone()),
        }
    }
}

impl RunningProcessOwner {
    pub(crate) fn try_begin_idle_shutdown(
        &mut self,
        invoking: WindowId,
        intent: ShutdownIntent,
        observation: &ShutdownWorkObservation,
        app: &App,
    ) -> Result<(), IdleShutdownError> {
        if self.observing_initial_work
            || self.confirmation.is_some()
            || self.shutdown.is_some()
            || self.progress.is_some()
        {
            return Err(IdleShutdownError::IntentBusy);
        }
        if observation.has_work() {
            return Err(IdleShutdownError::ConfirmationRequired);
        }
        if !self.process.windows.shells().iter().any(|shell| {
            shell
                .window()
                .read(app)
                .ok()
                .and_then(|root| root.controller())
                .is_some_and(|controller| controller.window_id() == invoking)
        }) {
            return Err(IdleShutdownError::Window(
                "the invoking main window is unavailable".into(),
            ));
        }
        let (snapshot, _) = self
            .process
            .services
            .as_ref()
            .ok_or(AppServiceCloseError::Unavailable)?
            .prepare_close_confirmation(self.process.windows.window_ids(), invoking, observation)?;
        let lease = self
            .process
            .services
            .as_ref()
            .ok_or(AppServiceCloseError::Unavailable)?
            .admit_close_confirmation(
                snapshot,
                invoking,
                intent == ShutdownIntent::FinalWindowClose,
            )
            .map_err(IdleShutdownError::Window)?;
        self.process
            .services
            .as_mut()
            .ok_or(AppServiceCloseError::Unavailable)?
            .try_begin_observed_window_shutdown(
                observation,
                &lease,
                invoking,
                intent == ShutdownIntent::FinalWindowClose,
            )?;
        self.shutdown = Some(RunningShutdownAttempt {
            invoking,
            intent,
            lease,
            pending: None,
            admitted: true,
            work_ready: false,
            drafts: None,
            placements: None,
            session: None,
        });
        Ok(())
    }

    pub(crate) fn begin_confirmed_shutdown(
        &mut self,
        context: confirmation::ShutdownConfirmationContext,
    ) -> Result<(), String> {
        if self.observing_initial_work
            || self.confirmation.is_some()
            || self.shutdown.is_some()
            || self.progress.is_some()
        {
            return Err("the running owner already retains shutdown intent custody".into());
        }
        let lease = self
            .process
            .services
            .as_ref()
            .ok_or("the complete service owner is on a worker")?
            .admit_close_confirmation(
                context.snapshot,
                context.invoking,
                context.intent == ShutdownIntent::FinalWindowClose,
            )?;
        self.shutdown = Some(RunningShutdownAttempt {
            invoking: context.invoking,
            intent: context.intent,
            lease,
            pending: None,
            admitted: false,
            work_ready: false,
            drafts: None,
            placements: None,
            session: None,
        });
        Ok(())
    }

    pub(crate) fn shutdown_status(
        &self,
    ) -> Option<(WindowId, ShutdownIntent, RunningShutdownStatus)> {
        self.shutdown.as_ref().map(|attempt| {
            (
                attempt.invoking,
                attempt.intent,
                if attempt.work_ready {
                    RunningShutdownStatus::WorkReady
                } else if attempt.admitted {
                    RunningShutdownStatus::Admitted
                } else if attempt.pending.is_some() {
                    RunningShutdownStatus::Observing
                } else {
                    RunningShutdownStatus::AwaitingObservation
                },
            )
        })
    }

    pub(crate) fn prepare_confirmed_shutdown_observation(
        &mut self,
    ) -> Result<PreparedConfirmedShutdownObservation, String> {
        let attempt = self
            .shutdown
            .as_mut()
            .ok_or("no confirmed shutdown intent is retained")?;
        if attempt.admitted || attempt.pending.is_some() {
            return Err("confirmed shutdown already owns observation or admission".into());
        }
        let work = self
            .process
            .services
            .as_ref()
            .ok_or("the complete service owner is on a worker")?
            .prepare_shutdown_observation()
            .map_err(|error| error.to_string())?;
        let identity = Arc::new(());
        attempt.pending = Some(identity.clone());
        Ok(PreparedConfirmedShutdownObservation { identity, work })
    }

    pub(crate) fn complete_confirmed_shutdown_observation(
        &mut self,
        completion: CompletedConfirmedShutdownObservation,
    ) -> Result<(), ConfirmedShutdownError> {
        self.settle_confirmed_shutdown_observation(&completion)?;
        let observation = completion.result?;
        let attempt = self.shutdown.as_mut().unwrap();
        self.process
            .services
            .as_mut()
            .ok_or(AppServiceCloseError::Unavailable)?
            .try_begin_observed_window_shutdown(
                &observation,
                &attempt.lease,
                attempt.invoking,
                attempt.intent == ShutdownIntent::FinalWindowClose,
            )?;
        attempt.admitted = true;
        Ok(())
    }

    pub(crate) fn discard_confirmed_shutdown_observation(
        &mut self,
        completion: CompletedConfirmedShutdownObservation,
    ) -> Result<(), ConfirmedShutdownError> {
        self.settle_confirmed_shutdown_observation(&completion)
    }

    fn settle_confirmed_shutdown_observation(
        &mut self,
        completion: &CompletedConfirmedShutdownObservation,
    ) -> Result<(), ConfirmedShutdownError> {
        let attempt = self
            .shutdown
            .as_mut()
            .ok_or(ConfirmedShutdownError::StaleObservation)?;
        if attempt.admitted
            || !attempt
                .pending
                .as_ref()
                .is_some_and(|identity| Arc::ptr_eq(identity, &completion.identity))
        {
            return Err(ConfirmedShutdownError::StaleObservation);
        }
        attempt.pending = None;
        Ok(())
    }

    pub(crate) fn end_unadmitted_shutdown(&mut self) -> Result<(), String> {
        let attempt = self
            .shutdown
            .as_ref()
            .ok_or("no confirmed shutdown intent is retained")?;
        if attempt.admitted || attempt.pending.is_some() {
            return Err("confirmed shutdown still owns observation or admission".into());
        }
        self.shutdown.take();
        Ok(())
    }
}
