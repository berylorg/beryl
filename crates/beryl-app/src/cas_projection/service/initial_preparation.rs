use beryl_home_store::{HomeCandidateError, HomeOpenPublication};

use super::*;
use crate::cas_projection::initial_start::InitialStartOwner;

pub(crate) struct PreparedCasServices {
    service: Option<ProjectionConnectionService>,
    handoff: Option<crate::discussion_settlement::coordinator::HandoffCoordinator>,
    initial_start: Option<InitialStartOwner>,
}

#[derive(Debug, Error)]
pub(crate) enum CasPreparationError {
    #[error("CAS preparation belongs to another initial candidate")]
    CandidateIdentity,
    #[error("CAS service preparation was cancelled")]
    Cancelled,
    #[error("CAS candidate access failed: {0}")]
    Candidate(#[from] HomeCandidateError),
    #[error("CAS service preparation failed: {0}")]
    Service(#[from] ProjectionCoordinatorError),
    #[error("runtime-interest configuration failed: {0}")]
    RuntimeInterest(#[from] crate::cas_projection::RuntimeInterestError),
    #[error("managed-session configuration failed: {0}")]
    Session(#[from] crate::cas_projection::RuntimeSessionPreparationError),
    #[error(transparent)]
    HandoffConvergence(#[from] crate::discussion_settlement::HandoffCandidateConvergenceError),
    #[error(transparent)]
    Handoff(#[from] crate::discussion_settlement::coordinator::HandoffCoordinatorError),
    #[error("handoff coordinator is already prepared")]
    HandoffAlreadyPrepared,
    #[error(transparent)]
    Resolution(#[from] crate::discussion_settlement::DiscussionSettlementError),
}

impl PreparedCasServices {
    pub(crate) fn catalog_source_start_gate(&self) -> Arc<InitialStartGate> {
        self.service
            .as_ref()
            .expect("prepared CAS custody")
            .catalog_source_start_gate()
    }

    pub(crate) fn install_catalog_source_waker(&self, waker: std::task::Waker) {
        self.service
            .as_ref()
            .expect("prepared CAS custody")
            .install_catalog_source_waker(waker);
    }

    pub(crate) fn activity_read_source(
        &self,
    ) -> Option<crate::cas_projection::RuntimeActivityReadSource> {
        self.service.as_ref()?.activity_read_source()
    }

    pub(crate) fn into_published_parts(
        mut self,
    ) -> (
        ProjectionConnectionService,
        crate::discussion_settlement::coordinator::HandoffCoordinator,
        InitialStartOwner,
    ) {
        let handoff = self.handoff.take().expect("complete handoff preparation");
        let service = self.service.take().expect("prepared CAS service custody");
        let initial_start = self
            .initial_start
            .take()
            .expect("publication fence custody");
        (service, handoff, initial_start)
    }

    pub(crate) fn prepare_handoff(
        mut self,
        candidate: &mut HomeOpenPublication,
        operations: crate::discussion_settlement::DiscussionSettlementOperations,
        state: beryl_state::BerylState,
        limits: crate::discussion_handoff_limits::HandoffScanLimits,
        at: syndic_storage::SyndicTimestamp,
        cancellation: beryl_home_store::CommandCancellation,
    ) -> Result<Self, CasPreparationError> {
        if self.handoff.is_some() {
            return Err(CasPreparationError::HandoffAlreadyPrepared);
        }
        self.validate_candidate(candidate)?;
        let service = self.service.as_ref().expect("prepared CAS service custody");
        self.handoff = Some(service.prepare_candidate_handoff(
            &candidate.recovery_access()?,
            operations,
            state,
            limits,
            at,
            cancellation,
        )?);
        Ok(self)
    }

    pub(crate) fn configure_managed_sessions(
        mut self,
        candidate: &mut HomeOpenPublication,
        sessions: &crate::cas_projection::ScheduledExecutionSessions,
        interest: crate::cas_projection::RuntimeInterestConfig,
        enrollments: crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations,
        config: crate::cas_projection::RuntimeSessionPreparationConfig,
        attention: &Arc<crate::lifecycle_attention::ProcessLifecycleAttentionPool>,
    ) -> Result<Self, CasPreparationError> {
        self.validate_candidate(candidate)?;
        let service = self.service.as_mut().expect("prepared CAS service custody");
        service.configure_runtime_interest(interest, enrollments)?;
        let access = candidate.recovery_access()?;
        service.configure_runtime_session_preparation_with_access(
            sessions,
            config,
            attention,
            Some(&access),
        )?;
        Ok(self)
    }

    pub(crate) fn prepare(
        process: crate::process_admission::ProcessAdmissionGate,
        candidate: &mut HomeOpenPublication,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Result<Self, CasPreparationError> {
        let (recovery, storage_revision) = {
            let access = candidate.recovery_access()?;
            let recovery =
                crate::cas_projection::accepted_delivery_recovery::recover_startup_candidate(
                    &access, &storage,
                )?;
            let revision = storage.revision_candidate(&access).map_err(|source| {
                ProjectionCoordinatorError::SyndicRevisionUnavailable { source }
            })?;
            (recovery, revision)
        };
        let initial_start = InitialStartOwner::new();
        let service = ProjectionConnectionService::construct(
            process,
            Arc::new(candidate.service_reference()),
            candidate.generation(),
            None,
            storage,
            config,
            scheduled_ordinary_provider,
            initial_start.gate(),
            storage_revision,
            recovery,
        )?;
        Ok(Self {
            service: Some(service),
            handoff: None,
            initial_start: Some(initial_start),
        })
    }

    fn validate_candidate(
        &self,
        candidate: &mut HomeOpenPublication,
    ) -> Result<(), CasPreparationError> {
        let service = self.service.as_ref().expect("prepared CAS service custody");
        if candidate.home_id() != service.home_id
            || candidate.generation() != service.home_generation
        {
            return Err(CasPreparationError::CandidateIdentity);
        }
        let access = candidate.recovery_access()?;
        service
            .storage
            .revision_candidate(&access)
            .map_err(|source| ProjectionCoordinatorError::SyndicRevisionUnavailable { source })?;
        Ok(())
    }
}

impl Drop for PreparedCasServices {
    fn drop(&mut self) {
        drop(self.initial_start.take());
        drop(self.handoff.take());
        drop(self.service.take());
    }
}

impl ProjectionConnectionService {
    pub(super) fn prepare_candidate_handoff(
        &self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        operations: crate::discussion_settlement::DiscussionSettlementOperations,
        state: beryl_state::BerylState,
        limits: crate::discussion_handoff_limits::HandoffScanLimits,
        at: syndic_storage::SyndicTimestamp,
        cancellation: beryl_home_store::CommandCancellation,
    ) -> Result<crate::discussion_settlement::coordinator::HandoffCoordinator, CasPreparationError>
    {
        operations.converge_candidate(access, &state, &self.storage, limits, at, cancellation)?;
        let settlement = crate::discussion_settlement::DiscussionSettlementService::new(
            operations,
            self.home
                .as_ref()
                .expect("prepared service home")
                .as_ref()
                .clone(),
            state,
            self.storage.clone(),
        );
        self.resolution.configure(settlement.clone())?;
        let handoff = crate::discussion_settlement::coordinator::HandoffCoordinator::prepare(
            settlement,
            limits,
            Arc::clone(&self.initial_start),
        )?;
        self.scheduler_signal
            .set_handoff_waker(Some(handoff.waker()));
        Ok(handoff)
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/initial_cas_preparation.rs"
    ));
}
