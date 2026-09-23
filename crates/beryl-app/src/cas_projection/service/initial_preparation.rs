use beryl_home_store::{HomeCandidateError, HomeOpenPublication};

use super::*;
use crate::cas_projection::initial_start::InitialStartOwner;

pub(crate) struct PreparedCasServices {
    service: Option<ProjectionConnectionService>,
    initial_start: Option<InitialStartOwner>,
    candidate: Option<HomeOpenPublication>,
}

#[derive(Debug, Error)]
pub(crate) enum CasPreparationError {
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
}

impl PreparedCasServices {
    pub(crate) fn configure_managed_sessions(
        mut self,
        sessions: &crate::cas_projection::ScheduledExecutionSessions,
        interest: crate::cas_projection::RuntimeInterestConfig,
        config: crate::cas_projection::RuntimeSessionPreparationConfig,
        attention: &Arc<crate::lifecycle_attention::ProcessLifecycleAttentionPool>,
    ) -> Result<Self, CasPreparationError> {
        let service = self.service.as_mut().expect("prepared CAS service custody");
        service.configure_runtime_interest(interest)?;
        let access = self
            .candidate
            .as_mut()
            .expect("prepared candidate custody")
            .recovery_access()?;
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
        mut candidate: HomeOpenPublication,
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
            initial_start: Some(initial_start),
            candidate: Some(candidate),
        })
    }
}

impl Drop for PreparedCasServices {
    fn drop(&mut self) {
        drop(self.initial_start.take());
        drop(self.service.take());
        drop(self.candidate.take());
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/initial_cas_preparation.rs"
    ));
}
