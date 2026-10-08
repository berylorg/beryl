use super::*;
use crate::cas_projection::CasRetirementFailure;
use crate::discussion_settlement::coordinator::HandoffCoordinatorError;
use beryl_home_store::{HomeGeneration, HomeHealthState, ReadError};
use beryl_state::DurableJobReadError;

impl ProcessServiceOwner {
    pub(crate) fn retire_failed_startup(&mut self) -> Result<(), AppServiceCloseError> {
        let graph = self
            .graph
            .as_ref()
            .ok_or(AppServiceCloseError::Unavailable)?;
        if graph.home().health().state() != HomeHealthState::Failed {
            return Err(AppServiceCloseError::NotFailed);
        }
        if !matches!(self.attempt, InitialServiceAttemptState::Published(_))
            || self.failed_close.is_some()
            || self.failed_retirement.is_some()
            || self.failed_cas_close.is_some()
            || self.closing_graph.is_some()
            || self.windows.main_window_occupancy() != 0
        {
            return Err(AppServiceCloseError::NotReady);
        }
        self.require_settled_custody()?;
        let fence = self.process.fence()?;
        if self.windows.main_window_occupancy() != 0 {
            return Err(AppServiceCloseError::NotReady);
        }
        self.attempt = InitialServiceAttemptState::Blocked;
        let mut graph = self.graph.take().expect("admitted failed graph");
        if let Some(mut catalog) = graph.catalog_source.take() {
            if let Err(error) = catalog.stop_and_join() {
                assert!(self.closing_catalog_error.is_none());
                self.closing_catalog_error = Some(error);
            }
        }
        graph.private_clipboard.retire();
        let home_generation = graph
            .home()
            .health()
            .generation()
            .expect("failed original generation");
        drop(graph.restore_lifetime.take());
        let cas = graph.cas.take().expect("complete graph CAS");
        let generation = cas.service_generation();
        let retired = match cas.retire_for_recovery(generation) {
            Ok(retired) => {
                let (_, owned_home) = retired.into_parts();
                assert!(
                    owned_home.is_none(),
                    "graph CAS cannot own its borrowed home"
                );
                true
            }
            Err(CasRetirementFailure::Rejected(cas)) => {
                graph.cas = Some(*cas);
                self.graph = Some(graph);
                return Err(AppServiceCloseError::PersistentFailure);
            }
            Err(CasRetirementFailure::Disposal(failure)) => {
                self.failed_retirement = Some(failure);
                false
            }
        };
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
        graph.attention.close();
        let custody = self.require_settled_custody();
        if let Err(error) = graph.home.take().expect("failed graph home").close() {
            self.failed_close = Some(error);
        }
        drop(graph);
        settled_handoff(handoff, home_generation)?;
        custody?;
        if !retired {
            return Err(AppServiceCloseError::PersistentFailure);
        }
        if self.failed_close.is_some() {
            return Err(AppServiceCloseError::Home);
        }
        #[cfg(feature = "test-faults")]
        if std::mem::take(&mut self.fail_shutdown_completion) {
            return Err(AppServiceCloseError::PersistentFailure);
        }
        self.attempt = InitialServiceAttemptState::Retired(fence);
        if let Some(error) = self.closing_catalog_error.take() {
            return Err(error.into());
        }
        Ok(())
    }
}

pub(super) fn settled_handoff(
    result: Result<(), HandoffCoordinatorError>,
    expected: HomeGeneration,
) -> Result<(), HandoffCoordinatorError> {
    match result {
        Err(HandoffCoordinatorError::Read(ReadError::HealthGate(gate)))
        | Err(HandoffCoordinatorError::Page(DurableJobReadError::Read(ReadError::HealthGate(
            gate,
        )))) if gate.state() == HomeHealthState::Failed && gate.generation() == expected => Ok(()),
        result => result,
    }
}
