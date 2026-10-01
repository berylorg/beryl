use super::*;
use crate::cas_projection::CasRetirementFailure;
use beryl_home_store::{HomeGeneration, HomeHealthState, HomeRecoveryCandidate, HomeRecoveryError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum RetiredHomeRecoveryError {
    #[error("recovery candidate construction was cancelled")]
    Cancelled,
    #[error(transparent)]
    Retirement(#[from] ServiceGraphRetirementError),
    #[error(transparent)]
    Reopen(#[from] HomeRecoveryError),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RetiredProcessWorkError {
    #[error("process work settlement requires completed graph retirement")]
    RetirementIncomplete,
    #[error("process work settlement requires a replacement candidate for the same home")]
    StaleCandidate,
    #[error("process work settlement was cancelled")]
    Cancelled,
    #[error(transparent)]
    Enrollment(#[from] ActivityEnrollmentCustodyError),
    #[error(transparent)]
    Settlement(#[from] HandoffCandidateConvergenceError),
    #[error(transparent)]
    Custody(#[from] AppServiceCloseError),
}

pub(super) struct ServiceGraphRetirement {
    pub(super) fence: ProcessAdmissionFence,
    generation: HomeGeneration,
    graph: Option<PublishedAppServices>,
    marker: Option<DraftMarkerSealService>,
    home: Option<HomeStore>,
    failure: Option<AppServiceCloseError>,
    complete: bool,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ServiceGraphRetirementError {
    #[error("service graph recovery retirement is already retained")]
    AlreadyRetained,
    #[error("recovery retirement requires the exact published failed generation")]
    Stale,
    #[error("service graph retirement is incomplete; original custody remains retained")]
    Incomplete,
    #[error("retired marker drives are still settling")]
    MarkerDrivesPending,
    #[error("retired home custody has already transferred")]
    HomeTransferred,
    #[error("return requires vacant retired custody and the same failed home")]
    InvalidHomeReturn,
    #[error(transparent)]
    Admission(#[from] ProcessAdmissionError),
}

impl ProcessServiceOwner {
    pub(crate) fn return_retired_service_home(
        &mut self,
        expected: HomeGeneration,
        home: &mut Option<HomeStore>,
    ) -> Result<(), ServiceGraphRetirementError> {
        self.validate_retired_service_home_return(expected, home.as_ref().map(HomeStore::home_id))?;
        if home.as_ref().unwrap().health().state() != HomeHealthState::Failed {
            return Err(ServiceGraphRetirementError::InvalidHomeReturn);
        }
        self.recovery_retirement.as_mut().unwrap().home = home.take();
        Ok(())
    }

    pub(crate) fn retired_service_generation_for_home_return(
        &self,
        home_id: beryl_model::BerylHomeId,
    ) -> Result<HomeGeneration, ServiceGraphRetirementError> {
        let generation = self
            .recovery_retirement
            .as_ref()
            .ok_or(ServiceGraphRetirementError::Stale)?
            .generation;
        self.validate_retired_service_home_return(generation, Some(home_id))?;
        Ok(generation)
    }

    pub(crate) fn validate_retired_service_home_return(
        &self,
        expected: HomeGeneration,
        home_id: Option<beryl_model::BerylHomeId>,
    ) -> Result<(), ServiceGraphRetirementError> {
        let retirement = self
            .recovery_retirement
            .as_ref()
            .filter(|retirement| retirement.generation == expected)
            .ok_or(ServiceGraphRetirementError::Stale)?;
        if !retirement.complete {
            return Err(ServiceGraphRetirementError::Incomplete);
        }
        if retirement.home.is_some() || home_id != Some(self.home_id) {
            return Err(ServiceGraphRetirementError::InvalidHomeReturn);
        }
        Ok(())
    }

    pub(crate) fn recover_retired_service_home(
        &mut self,
        expected: HomeGeneration,
    ) -> Result<HomeRecoveryCandidate, RetiredHomeRecoveryError> {
        let home = self.take_retired_service_home(expected)?;
        match home.recover_same_home() {
            Ok(candidate) => Ok(candidate),
            Err(failure) => {
                let (home, error) = failure.into_parts();
                self.recovery_retirement.as_mut().unwrap().home = Some(home);
                Err(RetiredHomeRecoveryError::Reopen(error))
            }
        }
    }

    pub(crate) fn settle_retired_process_work(
        &self,
        candidate: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
        syndic: &SyndicStorage,
        cancellation: &CommandCancellation,
    ) -> Result<(), RetiredProcessWorkError> {
        let retirement = self
            .recovery_retirement
            .as_ref()
            .filter(|retirement| retirement.complete)
            .ok_or(RetiredProcessWorkError::RetirementIncomplete)?;
        if candidate.home_id() != self.home_id || candidate.generation() == retirement.generation {
            return Err(RetiredProcessWorkError::StaleCandidate);
        }
        if cancellation.is_cancelled() {
            return Err(RetiredProcessWorkError::Cancelled);
        }
        self.enrollments
            .settle_retired_candidate(candidate, syndic, cancellation)?;
        self.settlements.settle_retained_nondispatch_candidate(
            candidate,
            state,
            syndic,
            cancellation.clone(),
        )?;
        self.require_settled_custody()?;
        Ok(())
    }

    pub(crate) fn validate_failed_service_graph_retirement(
        &self,
        expected: HomeGeneration,
    ) -> Result<(), ServiceGraphRetirementError> {
        if self.recovery_retirement.is_some() {
            return Err(ServiceGraphRetirementError::AlreadyRetained);
        }
        let graph = self
            .graph
            .as_ref()
            .ok_or(ServiceGraphRetirementError::Stale)?;
        let health = graph.home().health();
        if health.state() != HomeHealthState::Failed
            || health.generation() != Some(expected)
            || !matches!(self.attempt, InitialServiceAttemptState::Published(_))
            || self.failed_close.is_some()
            || self.failed_retirement.is_some()
        {
            return Err(ServiceGraphRetirementError::Stale);
        }
        Ok(())
    }

    pub(crate) fn retire_failed_service_graph(
        &mut self,
        expected: HomeGeneration,
    ) -> Result<(), ServiceGraphRetirementError> {
        self.validate_failed_service_graph_retirement(expected)?;
        let fence = self.process.fence()?;
        self.attempt = InitialServiceAttemptState::Blocked;
        self.recovery_retirement = Some(ServiceGraphRetirement {
            fence,
            generation: expected,
            graph: self.graph.take(),
            marker: None,
            home: None,
            failure: None,
            complete: false,
        });
        let retirement = self.recovery_retirement.as_mut().unwrap();
        let graph = retirement.graph.as_mut().unwrap();
        drop(graph.restore_lifetime.take());
        let cas = graph.cas.take().expect("complete graph CAS");
        let generation = cas.service_generation();
        match cas.retire_for_recovery(generation) {
            Ok(retired) => {
                let (_, owned_home) = retired.into_parts();
                assert!(owned_home.is_none(), "graph CAS borrows its home");
            }
            Err(CasRetirementFailure::Rejected(cas)) => {
                graph.cas = Some(*cas);
                return Err(ServiceGraphRetirementError::Incomplete);
            }
            Err(CasRetirementFailure::Disposal(failure)) => {
                self.failed_retirement = Some(failure);
            }
        }
        let handoff = graph.handoff.as_mut().expect("graph handoff").shutdown();
        drop(graph.handoff.take());
        if let Some(activity) = graph.activity.take() {
            activity.retire();
        }
        retirement.marker = graph.marker.take();
        if let Some(marker) = retirement.marker.as_ref() {
            marker.retire_home_generation();
        }
        drop(graph.theme.take());
        if let Some(mut theme) = graph.loaded_theme.take() {
            theme.retire();
        }
        graph.attention.close();
        retirement.home = graph.home.take();
        drop(retirement.graph.take());
        retirement.failure = failed_retirement::settled_handoff(handoff, expected)
            .err()
            .map(AppServiceCloseError::from);
        #[cfg(feature = "test-faults")]
        if std::mem::take(&mut self.fail_shutdown_completion) {
            retirement.failure = Some(AppServiceCloseError::PersistentFailure);
        }
        self.finish_service_graph_retirement(expected)
    }

    pub(crate) fn finish_service_graph_retirement(
        &mut self,
        expected: HomeGeneration,
    ) -> Result<(), ServiceGraphRetirementError> {
        let retirement = self
            .recovery_retirement
            .as_mut()
            .filter(|retirement| retirement.generation == expected)
            .ok_or(ServiceGraphRetirementError::Stale)?;
        if retirement.graph.is_some()
            || retirement.home.is_none()
            || retirement.failure.is_some()
            || self.failed_retirement.is_some()
        {
            return Err(ServiceGraphRetirementError::Incomplete);
        }
        if let Some(marker) = retirement.marker.as_ref() {
            if marker.retire_home_generation().settling_drives() != 0 {
                return Err(ServiceGraphRetirementError::MarkerDrivesPending);
            }
        }
        drop(retirement.marker.take());
        retirement.complete = true;
        Ok(())
    }

    pub(crate) fn take_retired_service_home(
        &mut self,
        expected: HomeGeneration,
    ) -> Result<HomeStore, ServiceGraphRetirementError> {
        self.validate_retired_service_home(expected)?;
        Ok(self
            .recovery_retirement
            .as_mut()
            .unwrap()
            .home
            .take()
            .unwrap())
    }

    pub(crate) fn retired_service_generation(
        &self,
    ) -> Result<HomeGeneration, ServiceGraphRetirementError> {
        let generation = self
            .recovery_retirement
            .as_ref()
            .ok_or(ServiceGraphRetirementError::Stale)?
            .generation;
        self.validate_retired_service_home(generation)?;
        Ok(generation)
    }

    pub(crate) fn validate_retired_service_home(
        &self,
        expected: HomeGeneration,
    ) -> Result<(), ServiceGraphRetirementError> {
        let retirement = self
            .recovery_retirement
            .as_ref()
            .filter(|retirement| retirement.generation == expected)
            .ok_or(ServiceGraphRetirementError::Stale)?;
        if !retirement.complete {
            return Err(ServiceGraphRetirementError::Incomplete);
        }
        retirement
            .home
            .as_ref()
            .map(|_| ())
            .ok_or(ServiceGraphRetirementError::HomeTransferred)
    }

    #[cfg(test)]
    pub(crate) fn test_retired_service_home(&mut self) -> Option<HomeStore> {
        let expected = self.recovery_retirement.as_ref()?.generation;
        self.take_retired_service_home(expected).ok()
    }
}
