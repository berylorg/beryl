use beryl_home_store::HomeRecoveryCandidate;

use super::initial_preparation::CasPreparationError;
use super::*;
use crate::cas_projection::{ProjectionCancellationToken, initial_start::InitialStartOwner};

pub(crate) struct PreparedRecoveryCasServices {
    service: Option<ProjectionConnectionService>,
    handoff: Option<crate::discussion_settlement::coordinator::HandoffCoordinator>,
    initial_start: Option<InitialStartOwner>,
    candidate: Option<HomeRecoveryCandidate>,
}

pub(crate) struct RecoveryCasPreparationFailure {
    error: CasPreparationError,
    home: HomeStore,
    retirement_confirmed: bool,
}

#[derive(Debug)]
pub(crate) struct RecoveryCasCloseFailure {
    _preparation: CasPreparationError,
    _close: HomeCloseError,
}

impl RecoveryCasPreparationFailure {
    pub(crate) fn error(&self) -> &CasPreparationError {
        &self.error
    }

    pub(crate) fn into_retry_parts(self) -> Result<(HomeStore, CasPreparationError), Self> {
        if self.retirement_confirmed {
            Ok((self.home, self.error))
        } else {
            Err(self)
        }
    }

    pub(crate) fn close(self) -> Result<(), RecoveryCasCloseFailure> {
        self.home.close().map_err(|close| RecoveryCasCloseFailure {
            _preparation: self.error,
            _close: close,
        })
    }
}

impl PreparedRecoveryCasServices {
    pub(crate) fn into_recovery_parts(
        mut self,
    ) -> (
        HomeRecoveryCandidate,
        ProjectionConnectionService,
        InitialStartOwner,
        Option<crate::discussion_settlement::coordinator::HandoffCoordinator>,
    ) {
        let candidate = self.candidate.take().expect("reopening candidate custody");
        let service = self.service.take().expect("prepared recovery CAS service");
        let initial_start = self
            .initial_start
            .take()
            .expect("publication fence custody");
        (candidate, service, initial_start, self.handoff.take())
    }

    pub(crate) fn prepare(
        process: crate::process_admission::ProcessAdmissionGate,
        candidate: HomeRecoveryCandidate,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Self, RecoveryCasPreparationFailure> {
        let mut prepared = Self {
            service: None,
            handoff: None,
            initial_start: None,
            candidate: Some(candidate),
        };
        let result = (|| {
            if cancellation.is_cancelled() {
                return Err(CasPreparationError::Cancelled);
            }
            let candidate = prepared
                .candidate
                .as_mut()
                .expect("reopening candidate custody");
            let (recovery, revision) = {
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
            if cancellation.is_cancelled() {
                return Err(CasPreparationError::Cancelled);
            }
            let start = InitialStartOwner::new();
            let service = ProjectionConnectionService::construct(
                process,
                Arc::new(candidate.service_reference()),
                candidate.generation(),
                None,
                storage,
                config,
                provider,
                start.gate(),
                revision,
                recovery,
            )?;
            prepared.initial_start = Some(start);
            prepared.service = Some(service);
            if cancellation.is_cancelled() {
                return Err(CasPreparationError::Cancelled);
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(prepared),
            Err(error) => Err(prepared.fail(error)),
        }
    }

    pub(crate) fn configure_managed_sessions(
        mut self,
        sessions: &crate::cas_projection::ScheduledExecutionSessions,
        interest: crate::cas_projection::RuntimeInterestConfig,
        enrollments: crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations,
        config: crate::cas_projection::RuntimeSessionPreparationConfig,
        attention: &Arc<crate::lifecycle_attention::ProcessLifecycleAttentionPool>,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Self, RecoveryCasPreparationFailure> {
        let result = (|| {
            if cancellation.is_cancelled() {
                return Err(CasPreparationError::Cancelled);
            }
            let service = self
                .service
                .as_mut()
                .expect("prepared recovery CAS service");
            service.configure_runtime_interest(interest, enrollments)?;
            let access = self
                .candidate
                .as_mut()
                .expect("reopening candidate custody")
                .recovery_access()?;
            service.configure_runtime_session_preparation_with_access(
                sessions,
                config,
                attention,
                Some(&access),
            )?;
            if cancellation.is_cancelled() {
                return Err(CasPreparationError::Cancelled);
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(self),
            Err(error) => Err(self.fail(error)),
        }
    }

    pub(crate) fn prepare_handoff(
        mut self,
        operations: crate::discussion_settlement::DiscussionSettlementOperations,
        state: beryl_state::BerylState,
        limits: crate::discussion_handoff_limits::HandoffScanLimits,
        at: syndic_storage::SyndicTimestamp,
        cancellation: beryl_home_store::CommandCancellation,
    ) -> Result<Self, RecoveryCasPreparationFailure> {
        let result = (|| {
            if cancellation.is_cancelled() {
                return Err(CasPreparationError::Cancelled);
            }
            if self.handoff.is_some() {
                return Err(CasPreparationError::HandoffAlreadyPrepared);
            }
            self.handoff = Some(
                self.service
                    .as_ref()
                    .expect("prepared recovery CAS service")
                    .prepare_candidate_handoff(
                        &self
                            .candidate
                            .as_mut()
                            .expect("reopening candidate custody")
                            .recovery_access()?,
                        operations,
                        state,
                        limits,
                        at,
                        cancellation.clone(),
                    )?,
            );
            if cancellation.is_cancelled() {
                return Err(CasPreparationError::Cancelled);
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(self),
            Err(error) => Err(self.fail(error)),
        }
    }

    pub(crate) fn cancel(self) -> RecoveryCasPreparationFailure {
        self.fail(CasPreparationError::Cancelled)
    }

    fn fail(mut self, error: CasPreparationError) -> RecoveryCasPreparationFailure {
        drop(self.initial_start.take());
        let handoff_retired = self
            .handoff
            .take()
            .is_none_or(|mut handoff| handoff.shutdown().is_ok());
        let service_retired = self
            .service
            .take()
            .is_none_or(|service| service.close().is_ok());
        let retirement_confirmed = handoff_retired
            && service_retired
            && !matches!(
                error,
                CasPreparationError::Service(
                    ProjectionCoordinatorError::ServiceConstructionDisposal { .. }
                )
            );
        let home = self
            .candidate
            .take()
            .expect("reopening candidate custody")
            .abort();
        RecoveryCasPreparationFailure {
            error,
            home,
            retirement_confirmed,
        }
    }
}

impl Drop for PreparedRecoveryCasServices {
    fn drop(&mut self) {
        drop(self.initial_start.take());
        drop(self.handoff.take());
        drop(self.service.take());
        drop(self.candidate.take());
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/recovery_cas_preparation.rs"
    ));
}
