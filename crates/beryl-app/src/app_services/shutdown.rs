use super::*;
use crate::{
    cas_projection::{
        ProjectionCancellationToken, ProjectionConnectionServiceCloseError,
        ProjectionConnectionServiceCloseOutcome, ShutdownCoordinatorError, ShutdownProgress,
        ShutdownWorkError, ShutdownWorkObservation, ShutdownWorkReadJob,
    },
    discussion_settlement::coordinator::HandoffCoordinatorError,
    process_admission::ProcessAdmissionError,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppServiceCloseError {
    #[error("the complete service graph is unavailable")]
    Unavailable,
    #[error("the service graph already owns a shutdown attempt")]
    AlreadyShuttingDown,
    #[error("shutdown has not drained all admitted work")]
    NotReady,
    #[error("startup retirement requires the original failed home")]
    NotFailed,
    #[error(
        "shutdown retains {enrollments} Activity enrollments and {nondispatch} nondispatch proofs"
    )]
    PendingCustody {
        enrollments: usize,
        nondispatch: usize,
    },
    #[error("home close failed; its error custody remains with the process owner")]
    Home,
    #[error("the home failed before ordinary shutdown completed")]
    PersistentFailure,
    #[error("CAS close failed; its original service and error remain with the process owner")]
    CasClosePending,
    #[error(transparent)]
    Admission(#[from] ProcessAdmissionError),
    #[error(transparent)]
    Coordinator(#[from] ShutdownCoordinatorError),
    #[error(transparent)]
    Work(#[from] ShutdownWorkError),
    #[error(transparent)]
    Handoff(#[from] HandoffCoordinatorError),
    #[error(transparent)]
    Cas(#[from] ProjectionConnectionServiceCloseError),
    #[error(transparent)]
    Catalog(#[from] CatalogSourceCoordinatorError),
}

#[derive(Debug)]
pub(crate) enum AppServiceShutdownProgress {
    Waiting,
    Ready,
    Failed {
        reason: crate::cas_projection::ShutdownFailure,
        reopened: bool,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppServiceFinalizationError {
    #[error("service teardown was not started: {0}")]
    Rejected(#[source] AppServiceCloseError),
    #[error("service teardown could not complete after consumption: {0}")]
    Consumed(#[source] AppServiceCloseError),
}

impl ProcessServiceOwner {
    pub(crate) fn prepare_shutdown_observation(
        &self,
    ) -> Result<PreparedShutdownObservation, AppServiceCloseError> {
        let graph = self
            .graph
            .as_ref()
            .ok_or(AppServiceCloseError::Unavailable)?;
        if graph.shutdown.is_some() {
            return Err(AppServiceCloseError::AlreadyShuttingDown);
        }
        Ok(PreparedShutdownObservation {
            lifetime: Arc::downgrade(
                graph
                    .restore_lifetime
                    .as_ref()
                    .ok_or(AppServiceCloseError::Unavailable)?,
            ),
            work: graph
                .cas
                .as_ref()
                .ok_or(AppServiceCloseError::Unavailable)?
                .prepare_shutdown_observation(&graph.sessions),
        })
    }

    pub(crate) fn observe_shutdown_work(
        &self,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownWorkObservation, AppServiceCloseError> {
        self.prepare_shutdown_observation()?.collect(cancellation)
    }

    pub(crate) fn try_begin_observed_shutdown(
        &mut self,
        observation: &ShutdownWorkObservation,
    ) -> Result<(), AppServiceCloseError> {
        self.try_begin_observed_shutdown_with_window(observation, None)
    }

    pub(crate) fn try_begin_observed_window_shutdown(
        &mut self,
        observation: &ShutdownWorkObservation,
        lease: &crate::window_acquisition::WindowCloseLease,
        invoking: beryl_model::WindowId,
        require_final: bool,
    ) -> Result<(), AppServiceCloseError> {
        let window = self
            .windows
            .prepare_shutdown_admission(lease, invoking, require_final)
            .map_err(ShutdownWorkError::from)?;
        self.try_begin_observed_shutdown_with_window(observation, Some(window))
    }

    fn try_begin_observed_shutdown_with_window(
        &mut self,
        observation: &ShutdownWorkObservation,
        window: Option<crate::window_acquisition::WindowShutdownAdmission<'_>>,
    ) -> Result<(), AppServiceCloseError> {
        let graph = self
            .graph
            .as_mut()
            .ok_or(AppServiceCloseError::Unavailable)?;
        if graph.shutdown.is_some() {
            return Err(AppServiceCloseError::AlreadyShuttingDown);
        }
        let cas = graph
            .cas
            .as_ref()
            .ok_or(AppServiceCloseError::Unavailable)?;
        let attempt = match window {
            Some(window) => {
                cas.try_begin_observed_window_shutdown(&graph.sessions, observation, window)?
            }
            None => cas.try_begin_observed_shutdown(&graph.sessions, observation)?,
        };
        graph.shutdown = Some(attempt);
        graph.shutdown_ready = false;
        drop(graph.restore_lifetime.take());
        Ok(())
    }

    pub(crate) fn begin_shutdown(&mut self) -> Result<(), AppServiceCloseError> {
        let graph = self
            .graph
            .as_mut()
            .ok_or(AppServiceCloseError::Unavailable)?;
        let fence = self.process.fence()?;
        drop(graph.restore_lifetime.take());
        graph.shutdown = Some(
            graph
                .cas
                .as_ref()
                .ok_or(AppServiceCloseError::Unavailable)?
                .begin_graceful_shutdown(&fence)?,
        );
        graph.shutdown_ready = false;
        Ok(())
    }

    pub(crate) fn poll_shutdown(
        &mut self,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<AppServiceShutdownProgress, AppServiceCloseError> {
        let graph = self
            .graph
            .as_mut()
            .ok_or(AppServiceCloseError::Unavailable)?;
        graph.shutdown_ready = false;
        let attempt = graph.shutdown.ok_or(AppServiceCloseError::NotReady)?;
        match graph
            .cas
            .as_ref()
            .ok_or(AppServiceCloseError::Unavailable)?
            .poll_graceful_shutdown(&graph.sessions, attempt, cancellation)?
        {
            ShutdownProgress::Waiting => {
                self.require_settled_custody()?;
                Ok(AppServiceShutdownProgress::Waiting)
            }
            ShutdownProgress::Failed { reason, reopened } => {
                if reopened {
                    graph.shutdown = None;
                    graph.restore_lifetime = Some(Arc::new(()));
                }
                Ok(AppServiceShutdownProgress::Failed { reason, reopened })
            }
            ShutdownProgress::Ready => {
                self.require_settled_custody()?;
                self.graph.as_mut().expect("installed graph").shutdown_ready = true;
                Ok(AppServiceShutdownProgress::Ready)
            }
        }
    }

    pub(crate) fn finish_shutdown(&mut self) -> Result<(), AppServiceFinalizationError> {
        self.require_shutdown_finalization()
            .map_err(AppServiceFinalizationError::Rejected)?;
        self.consume_shutdown_graph()
            .map_err(AppServiceFinalizationError::Consumed)
    }

    fn require_shutdown_finalization(&self) -> Result<(), AppServiceCloseError> {
        let graph = self
            .graph
            .as_ref()
            .or(self.closing_graph.as_ref())
            .ok_or(AppServiceCloseError::Unavailable)?;
        if !graph.shutdown_ready {
            return Err(AppServiceCloseError::NotReady);
        }
        self.require_settled_custody()?;
        if self.failed_close.is_some() {
            return Err(AppServiceCloseError::Home);
        }
        Ok(())
    }

    fn consume_shutdown_graph(&mut self) -> Result<(), AppServiceCloseError> {
        self.attempt = InitialServiceAttemptState::Blocked;
        let mut graph = self
            .graph
            .take()
            .or_else(|| self.closing_graph.take())
            .expect("ready or retained closing graph");
        if let Some(mut catalog) = graph.catalog_source.take() {
            if let Err(error) = catalog.stop_and_join() {
                assert!(self.closing_catalog_error.is_none());
                self.closing_catalog_error = Some(error);
            }
        }
        graph.private_clipboard.retire();
        let handoff = graph
            .handoff
            .as_mut()
            .map_or(Ok(()), |handoff| handoff.shutdown());
        if let Err(error) = handoff {
            self.closing_handoff_error = Some(error);
        }
        drop(graph.handoff.take());
        if let Some(activity) = graph.activity.take() {
            activity.retire();
        }
        if let Some(marker) = graph.marker.take() {
            marker.retire_home_generation();
        }
        drop(graph.theme.take());
        if let Some(mut theme) = graph.loaded_theme.take() {
            theme.retire();
        }
        let close = match graph.cas.take() {
            Some(cas) => cas.close(),
            None => self
                .failed_cas_close
                .take()
                .expect("retained graph CAS close custody")
                .retry(),
        };
        let closed = match close {
            Ok(outcome) => outcome,
            Err(failure) => {
                self.failed_cas_close = Some(failure);
                self.closing_graph = Some(graph);
                return Err(AppServiceCloseError::CasClosePending);
            }
        };
        graph.attention.close();
        let custody = self.require_settled_custody();
        let home = graph.home.take().expect("graph home").close();
        if let Err(error) = home {
            self.failed_close = Some(error);
        }
        if let Some(error) = self.closing_handoff_error.take() {
            return Err(error.into());
        }
        custody?;
        match closed {
            ProjectionConnectionServiceCloseOutcome::Closed => {}
            ProjectionConnectionServiceCloseOutcome::PersistentFailure(_) => {
                return Err(AppServiceCloseError::PersistentFailure);
            }
        }
        if self.failed_close.is_some() {
            return Err(AppServiceCloseError::Home);
        }
        drop(graph);
        #[cfg(feature = "test-faults")]
        if std::mem::take(&mut self.fail_shutdown_completion) {
            return Err(AppServiceCloseError::PersistentFailure);
        }
        self.record_initial_retirement()?;
        if let Some(error) = self.closing_catalog_error.take() {
            return Err(error.into());
        }
        Ok(())
    }

    pub(super) fn require_settled_custody(&self) -> Result<(), AppServiceCloseError> {
        let enrollments = self.enrollments.pending_count();
        let nondispatch = self.settlements.pending_nondispatch_count();
        if enrollments != 0 || nondispatch != 0 {
            Err(AppServiceCloseError::PendingCustody {
                enrollments,
                nondispatch,
            })
        } else {
            Ok(())
        }
    }
}

pub(crate) struct PreparedShutdownObservation {
    lifetime: std::sync::Weak<()>,
    work: ShutdownWorkReadJob,
}

impl PreparedShutdownObservation {
    pub(crate) fn collect(
        self,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownWorkObservation, AppServiceCloseError> {
        self.collect_then(cancellation, || {})
    }

    fn collect_then(
        self,
        cancellation: &ProjectionCancellationToken,
        after_collection: impl FnOnce(),
    ) -> Result<ShutdownWorkObservation, AppServiceCloseError> {
        if self.lifetime.upgrade().is_none() {
            return Err(AppServiceCloseError::Unavailable);
        }
        let result = self.work.collect(cancellation)?;
        after_collection();
        if cancellation.is_cancelled() {
            return Err(ShutdownWorkError::Work(
                crate::cas_projection::ProcessWorkError::Cancelled,
            )
            .into());
        }
        if self.lifetime.upgrade().is_none() {
            return Err(AppServiceCloseError::Unavailable);
        }
        Ok(result)
    }

    #[cfg(test)]
    pub(crate) fn test_collect_then(
        self,
        cancellation: &ProjectionCancellationToken,
        after_collection: impl FnOnce(),
    ) -> Result<ShutdownWorkObservation, AppServiceCloseError> {
        self.collect_then(cancellation, after_collection)
    }
}
