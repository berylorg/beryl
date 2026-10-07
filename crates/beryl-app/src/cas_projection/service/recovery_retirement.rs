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
    service: Option<Box<ProjectionConnectionService>>,
}

pub(crate) struct CasRetirementTerminalCloseFailure {
    _retirement: CasRetirementError,
    _close: Option<HomeCloseError>,
    _service: Option<Box<ProjectionConnectionService>>,
    _home: Option<HomeStore>,
}

impl std::fmt::Debug for CasRetirementTerminalCloseFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CasRetirementTerminalCloseFailure")
            .field("retirement", &self._retirement)
            .field("close", &self._close)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub(crate) enum CasRetirementError {
    #[error("CAS service disposal failed: {0}")]
    Disposal(#[source] ProjectionConnectionServiceCloseError),
    #[error("CAS failure cut could not prove complete retirement")]
    IncompleteCut,
}

impl CasRetirementDisposalFailure {
    pub(crate) fn retry_disposal(&mut self) -> bool {
        let Some(service) = self.service.as_mut() else {
            return false;
        };
        if service.shutdown_started || service.close_retry_error.is_some() {
            return false;
        }
        match service.close_inner() {
            Ok(ProjectionConnectionServiceCloseOutcome::PersistentFailure(evidence))
                if evidence.completion() == PersistentFailureCutCompletion::Finished =>
            {
                self.service = None;
                true
            }
            Err(error) => {
                if !matches!(
                    error,
                    ProjectionConnectionServiceCloseError::RuntimeRetirement
                ) {
                    service.close_retry_error = Some(error);
                }
                false
            }
            Ok(_) => false,
        }
    }
    pub(crate) fn error(&self) -> &CasRetirementError {
        &self.error
    }

    pub(crate) fn close(mut self) -> Result<(), CasRetirementTerminalCloseFailure> {
        if self.service.is_some() && !self.retry_disposal() {
            return Err(CasRetirementTerminalCloseFailure {
                _retirement: self.error,
                _close: None,
                _service: self.service,
                _home: self.home,
            });
        }
        self.home
            .take()
            .map_or(Ok(()), HomeStore::close)
            .map_err(|close| CasRetirementTerminalCloseFailure {
                _retirement: self.error,
                _close: Some(close),
                _service: self.service,
                _home: None,
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
        if self.settled
            || self.shutdown_started
            || expected != self.service_generation
            || !exact_failed_home
        {
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
                    service: Some(Box::new(self)),
                },
            )),
        }
    }
}
