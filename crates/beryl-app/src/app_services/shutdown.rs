use super::*;
use crate::{
    cas_projection::{
        ProjectionCancellationToken, ProjectionConnectionServiceCloseError,
        ProjectionConnectionServiceCloseOutcome, ShutdownCoordinatorError, ShutdownProgress,
    },
    discussion_settlement::coordinator::HandoffCoordinatorError,
    process_admission::ProcessAdmissionError,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppServiceCloseError {
    #[error("the complete service graph is unavailable")]
    Unavailable,
    #[error("shutdown has not drained all admitted work")]
    NotReady,
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
    #[error(transparent)]
    Admission(#[from] ProcessAdmissionError),
    #[error(transparent)]
    Coordinator(#[from] ShutdownCoordinatorError),
    #[error(transparent)]
    Handoff(#[from] HandoffCoordinatorError),
    #[error(transparent)]
    Cas(#[from] ProjectionConnectionServiceCloseError),
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

impl ProcessServiceOwner {
    pub(crate) fn begin_shutdown(&mut self) -> Result<(), AppServiceCloseError> {
        let graph = self
            .graph
            .as_mut()
            .ok_or(AppServiceCloseError::Unavailable)?;
        let fence = self.process.fence()?;
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

    pub(crate) fn finish_shutdown(&mut self) -> Result<(), AppServiceCloseError> {
        let graph = self
            .graph
            .as_ref()
            .ok_or(AppServiceCloseError::Unavailable)?;
        if !graph.shutdown_ready {
            return Err(AppServiceCloseError::NotReady);
        }
        self.require_settled_custody()?;
        let mut graph = self.graph.take().expect("ready graph");
        let handoff = graph.handoff.as_mut().expect("graph handoff").shutdown();
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
        let closed = graph.cas.take().expect("graph CAS").close();
        graph.attention.close();
        let custody = self.require_settled_custody();
        let home = graph.home.take().expect("graph home").close();
        if let Err(error) = home {
            self.failed_close = Some(error);
        }
        handoff?;
        custody?;
        match closed? {
            ProjectionConnectionServiceCloseOutcome::Closed => {}
            ProjectionConnectionServiceCloseOutcome::PersistentFailure(_) => {
                return Err(AppServiceCloseError::PersistentFailure);
            }
        }
        if self.failed_close.is_some() {
            return Err(AppServiceCloseError::Home);
        }
        Ok(())
    }

    fn require_settled_custody(&self) -> Result<(), AppServiceCloseError> {
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
