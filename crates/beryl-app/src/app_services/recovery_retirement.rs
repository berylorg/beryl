use super::*;
use crate::cas_projection::CasRetirementFailure;
use crate::discussion_settlement::coordinator::HandoffCoordinatorError;
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
    #[error("original catalog repair reconciliation collided")]
    CatalogCollision,
    #[error(transparent)]
    CatalogReconciliation(#[from] beryl_home_store::ReconciliationFailure),
    #[error("original catalog query retirement still retains read custody")]
    CatalogQueryPending,
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
    joined_handoff_read_failure: Option<HandoffCoordinatorError>,
    joined_catalog_failure: std::sync::Mutex<Option<CatalogSourceCoordinatorError>>,
    catalog_query: std::sync::Mutex<Option<CatalogQueryService>>,
    joined_catalog_query_failure: std::sync::Mutex<Option<CatalogQueryServiceError>>,
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
    #[error("original runtime setup retirement retains custody: {0}")]
    RuntimeSetup(String),
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
        if cancellation.is_cancelled() {
            return Err(RetiredProcessWorkError::Cancelled);
        }
        let mut catalog_failure = retirement
            .joined_catalog_failure
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(CatalogSourceCoordinatorError::Repair(repair)) = catalog_failure.as_ref() {
            if let crate::catalog_readiness::RetainedCatalogRepair::Indeterminate {
                reconciliation,
                ..
            } = repair.as_ref()
            {
                match candidate.reconcile(reconciliation)? {
                    beryl_home_store::ReconciliationResolution::ExactOld
                    | beryl_home_store::ReconciliationResolution::ExactNew { .. }
                    | beryl_home_store::ReconciliationResolution::ExactSuccessor { .. } => {}
                    beryl_home_store::ReconciliationResolution::Collision => {
                        return Err(RetiredProcessWorkError::CatalogCollision);
                    }
                }
            }
        }
        let mut queries = retirement
            .catalog_query
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(service) = queries.as_mut() {
            if let Err(error) = service.stop_and_join() {
                let mut failure = retirement
                    .joined_catalog_query_failure
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if failure.is_none() {
                    *failure = Some(error);
                }
            }
            if !service.reads_drained() {
                return Err(RetiredProcessWorkError::CatalogQueryPending);
            }
        }
        drop(queries.take());
        drop(
            retirement
                .joined_catalog_query_failure
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take(),
        );
        drop(catalog_failure.take());
        Ok(())
    }

    pub(super) fn require_catalog_recovery_settlement(
        &self,
        expected: HomeGeneration,
    ) -> Result<(), ServiceGraphRetirementError> {
        let retirement = self
            .recovery_retirement
            .as_ref()
            .filter(|retirement| retirement.complete && retirement.generation == expected)
            .ok_or(ServiceGraphRetirementError::Incomplete)?;
        if retirement
            .joined_catalog_failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
            || retirement
                .catalog_query
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
            || retirement
                .joined_catalog_query_failure
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
        {
            return Err(ServiceGraphRetirementError::Incomplete);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_catalog_retirement_failure(&self, error: CatalogSourceCoordinatorError) {
        let mut retained = self
            .recovery_retirement
            .as_ref()
            .expect("retained graph retirement")
            .joined_catalog_failure
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert!(retained.is_none());
        *retained = Some(error);
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
            || self.failed_cas_close.is_some()
            || self.closing_graph.is_some()
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
            joined_handoff_read_failure: None,
            joined_catalog_failure: std::sync::Mutex::new(None),
            catalog_query: std::sync::Mutex::new(None),
            joined_catalog_query_failure: std::sync::Mutex::new(None),
            complete: false,
        });
        let retirement = self.recovery_retirement.as_mut().unwrap();
        let graph = retirement.graph.as_mut().unwrap();
        if let Some(mut queries) = graph.catalog_query.take() {
            if let Err(error) = queries.stop_and_join() {
                *retirement
                    .joined_catalog_query_failure
                    .get_mut()
                    .unwrap_or_else(|e| e.into_inner()) = Some(error);
            }
            assert!(queries.work_drained());
            *retirement
                .catalog_query
                .get_mut()
                .unwrap_or_else(|e| e.into_inner()) = Some(queries);
        }
        if let Some(mut catalog) = graph.catalog_source.take() {
            if let Err(error) = catalog.stop_and_join() {
                *retirement
                    .joined_catalog_failure
                    .get_mut()
                    .unwrap_or_else(|e| e.into_inner()) = Some(error);
            }
        }
        graph
            .runtime_setup
            .retire(
                self.first_cleanups
                    .get_mut()
                    .unwrap_or_else(|e| e.into_inner()),
            )
            .map_err(ServiceGraphRetirementError::RuntimeSetup)?;
        graph.private_clipboard.retire();
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
        let handoff = failed_retirement::settled_handoff(handoff, expected);
        let exact_failed_home = retirement.home.as_ref().is_some_and(|home| {
            let health = home.health();
            health.state() == HomeHealthState::Failed && health.generation() == Some(expected)
        });
        if exact_failed_home
            && matches!(
                &handoff,
                Err(HandoffCoordinatorError::Read(
                    beryl_home_store::ReadError::Storage { .. }
                )) | Err(HandoffCoordinatorError::Page(
                    beryl_state::DurableJobReadError::Read(
                        beryl_home_store::ReadError::Storage { .. }
                    )
                ))
            )
        {
            retirement.joined_handoff_read_failure = handoff.err();
        } else {
            retirement.failure = handoff.err().map(AppServiceCloseError::from);
        }
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
        if self
            .failed_retirement
            .as_mut()
            .is_some_and(|failure| failure.retry_disposal())
        {
            self.failed_retirement = None;
        }
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
    pub(crate) fn test_recovery_retirement_stage(&self) -> String {
        let graph_health = self
            .graph
            .as_ref()
            .map(|graph| graph.home().health().state());
        let retirement = self.recovery_retirement.as_ref().map(|retirement| {
            (
                retirement.complete,
                retirement.graph.is_some(),
                retirement.home.is_some(),
                retirement
                    .graph
                    .as_ref()
                    .is_some_and(|graph| graph.cas.is_some()),
                retirement.failure.as_ref().map(ToString::to_string),
                retirement
                    .joined_handoff_read_failure
                    .as_ref()
                    .map(ToString::to_string),
            )
        });
        format!(
            "graph_health={graph_health:?}, retirement={retirement:?}, first_cleanups={}, cas_disposal={}",
            self.first_cleanups
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .len(),
            self.failed_retirement.is_some()
        )
    }

    #[cfg(test)]
    pub(crate) fn test_retired_service_home(&mut self) -> Option<HomeStore> {
        let expected = self.recovery_retirement.as_ref()?.generation;
        self.take_retired_service_home(expected).ok()
    }
}
