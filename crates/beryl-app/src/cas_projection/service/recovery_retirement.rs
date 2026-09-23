use super::*;

pub(crate) struct RetiredCasServices {
    evidence: PersistentFailureTerminalEvidence,
    home: Option<HomeStore>,
}

impl RetiredCasServices {
    pub(crate) fn into_parts(self) -> (PersistentFailureTerminalEvidence, Option<HomeStore>) {
        (self.evidence, self.home)
    }
}

pub(crate) enum CasRetirementFailure {
    Rejected(Box<ProjectionConnectionService>),
    Disposal(CasRetirementDisposalFailure),
}

pub(crate) struct CasRetirementDisposalFailure {
    error: CasRetirementError,
    home: Option<HomeStore>,
}

#[derive(Debug)]
pub(crate) struct CasRetirementTerminalCloseFailure {
    _retirement: CasRetirementError,
    _close: HomeCloseError,
}

#[derive(Debug, Error)]
pub(crate) enum CasRetirementError {
    #[error("CAS service disposal failed: {0}")]
    Disposal(#[source] ProjectionConnectionServiceCloseError),
    #[error("CAS failure cut could not prove complete retirement")]
    IncompleteCut,
}

impl CasRetirementDisposalFailure {
    pub(crate) fn error(&self) -> &CasRetirementError {
        &self.error
    }

    pub(crate) fn close(mut self) -> Result<(), CasRetirementTerminalCloseFailure> {
        self.home
            .take()
            .map_or(Ok(()), HomeStore::close)
            .map_err(|close| CasRetirementTerminalCloseFailure {
                _retirement: self.error,
                _close: close,
            })
    }
}

impl ProjectionConnectionService {
    pub(crate) fn retire_for_recovery(
        mut self,
        expected: ProjectionServiceGeneration,
    ) -> Result<RetiredCasServices, CasRetirementFailure> {
        let exact_failed_home = self.home.as_ref().is_some_and(|home| {
            let health = home.health();
            home.home_id() == self.home_id
                && health.generation() == Some(self.home_generation)
                && health.state() == HomeHealthState::Failed
        });
        if self.settled || expected != self.service_generation || !exact_failed_home {
            return Err(CasRetirementFailure::Rejected(Box::new(self)));
        }
        if !matches!(
            self.command_authorizer.observe_persistent_failure(),
            super::super::PersistentFailureNotificationStatus::Signaled
                | super::super::PersistentFailureNotificationStatus::Joined
        ) {
            return Err(CasRetirementFailure::Rejected(Box::new(self)));
        }
        let home = self.owned_home.take();
        let outcome = self.close_inner();
        drop(self);
        match outcome {
            Ok(ProjectionConnectionServiceCloseOutcome::PersistentFailure(evidence))
                if evidence.completion() == PersistentFailureCutCompletion::Finished =>
            {
                Ok(RetiredCasServices { evidence, home })
            }
            outcome => Err(CasRetirementFailure::Disposal(
                CasRetirementDisposalFailure {
                    error: match outcome {
                        Err(error) => CasRetirementError::Disposal(error),
                        Ok(_) => CasRetirementError::IncompleteCut,
                    },
                    home,
                },
            )),
        }
    }
}
