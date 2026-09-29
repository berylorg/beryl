use super::*;
use crate::cas_projection::CasRetirementFailure;
use beryl_home_store::{HomeGeneration, HomeHealthState};

pub(super) struct ServiceGraphRetirement {
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
    #[error(transparent)]
    Admission(#[from] ProcessAdmissionError),
}

impl ProcessServiceOwner {
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
            || !matches!(self.attempt, InitialServiceAttemptState::Published)
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
        self.process.fence()?;
        self.attempt = InitialServiceAttemptState::Blocked;
        self.recovery_retirement = Some(ServiceGraphRetirement {
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

    #[cfg(test)]
    pub(crate) fn test_retired_service_home(&mut self) -> Option<HomeStore> {
        let retirement = self.recovery_retirement.as_mut()?;
        retirement
            .complete
            .then(|| retirement.home.take())
            .flatten()
    }
}
