use std::sync::Arc;

use beryl_home_store::{HomeGeneration, HomeHealthState, ReadError};
use beryl_model::{BerylHomeId, SyndicThreadId};

use super::super::AcceptedInputSchedulerContext;
use crate::cas_projection::{
    ProjectionCoordinatorError, ProjectionRegistryKind, ScheduledOrdinaryAdmission,
    ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionLease, connection::ConnectionPromotionReservation,
    persistent_failure::LiveCommandGateStatus,
    service_registry::ProjectionServiceConnectionRegistry,
};

pub(in crate::cas_projection::accepted_input_scheduler) struct LeaseValidationAuthority {
    pub(in crate::cas_projection::accepted_input_scheduler) home: Arc<beryl_home_store::HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: syndic_storage::SyndicStorage,
    connections: Arc<ProjectionServiceConnectionRegistry>,
    command: crate::cas_projection::LiveCommandPermit,
    terminal_disposer: crate::cas_projection::persistent_failure::PersistentFailureTerminalDisposer,
}

pub(in crate::cas_projection::accepted_input_scheduler) fn expected_admission_drift(
    error: &ScheduledOrdinaryAdmissionError,
) -> bool {
    match error {
        ScheduledOrdinaryAdmissionError::RuntimeMismatch { .. }
        | ScheduledOrdinaryAdmissionError::SessionAuthorityUnavailable { .. }
        | ScheduledOrdinaryAdmissionError::AssetAuthority {
            source: ReadError::HealthGate(_),
        } => true,
        ScheduledOrdinaryAdmissionError::Authority(error) => expected_coordinator_drift(error),
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn expected_coordinator_drift(
    error: &ProjectionCoordinatorError,
) -> bool {
    match error {
        ProjectionCoordinatorError::HomeNotHealthy { .. }
        | ProjectionCoordinatorError::LiveCommandOrdinaryShutdown => true,
        ProjectionCoordinatorError::HomeGenerationMismatch {
            expected,
            actual,
            state,
        } => *state != HomeHealthState::Healthy || *actual == Some(*expected),
        _ => false,
    }
}

pub(super) fn obsolete_admission_generation(error: &ScheduledOrdinaryAdmissionError) -> bool {
    matches!(
        error,
        ScheduledOrdinaryAdmissionError::Authority(error)
            if obsolete_coordinator_generation(error)
    )
}

pub(super) fn obsolete_coordinator_generation(error: &ProjectionCoordinatorError) -> bool {
    matches!(
        error,
        ProjectionCoordinatorError::HomeGenerationMismatch {
            expected,
            actual: Some(actual),
            state: HomeHealthState::Healthy,
        } if actual > expected
    )
}

impl AcceptedInputSchedulerContext {
    pub(in crate::cas_projection::accepted_input_scheduler) fn issue_scheduled_ordinary_execution(
        &self,
        thread_id: SyndicThreadId,
        execution_binding: beryl_model::ExecutionBinding,
        worker: crate::cas_projection::service_config::ProjectionWorkerPermit,
        flight: crate::cas_projection::service::ProjectionFlight,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let Ok(command) = self.command_gate.authorizer().authorize() else {
            return Ok(ScheduledOrdinaryAdmissionResult::Unavailable(
                crate::cas_projection::ScheduledOrdinaryExecutionUnavailable::ShuttingDown,
            ));
        };
        let validator = self.lease_validator(command);
        validator.ensure_current()?;
        let expected_binding = execution_binding.clone();
        let admission = ScheduledOrdinaryAdmission::new(
            self.home_id,
            self.home_generation,
            thread_id,
            execution_binding,
            worker,
            self.terminal_disposer.clone(),
            flight,
        );
        let mut result = self
            .scheduled_ordinary_provider
            .lock()
            .map_err(|_| ScheduledOrdinaryAdmissionError::ProviderPoisoned)?
            .try_issue(admission)?;
        if let ScheduledOrdinaryAdmissionResult::Issued(lease) = &result
            && (lease.home_id() != self.home_id
                || lease.home_generation() != self.home_generation
                || lease.thread_id() != thread_id
                || lease.execution_binding() != &expected_binding)
        {
            return Err(ScheduledOrdinaryAdmissionError::LeaseMismatch { thread_id });
        }
        if let ScheduledOrdinaryAdmissionResult::Issued(lease) = &mut result {
            validator.validate(lease)?;
        }
        validator.ensure_command_current()?;
        Ok(result)
    }

    pub(in crate::cas_projection::accepted_input_scheduler) fn lease_validator(
        &self,
        command: crate::cas_projection::LiveCommandPermit,
    ) -> LeaseValidationAuthority {
        LeaseValidationAuthority {
            home: Arc::clone(&self.home),
            home_id: self.home_id,
            home_generation: self.home_generation,
            storage: self.storage,
            connections: Arc::clone(&self.connections),
            command,
            terminal_disposer: self.terminal_disposer.clone(),
        }
    }
}

impl LeaseValidationAuthority {
    pub(in crate::cas_projection::accepted_input_scheduler) const fn home_generation(
        &self,
    ) -> HomeGeneration {
        self.home_generation
    }

    pub(super) fn ensure_current(&self) -> Result<(), ProjectionCoordinatorError> {
        self.ensure_command_current()?;
        self.ensure_generation_current()
    }

    fn ensure_command_current(&self) -> Result<(), ProjectionCoordinatorError> {
        match self.command.status_exact() {
            Ok(LiveCommandGateStatus::Open) => Ok(()),
            Ok(LiveCommandGateStatus::OrdinaryShutdown) => {
                Err(ProjectionCoordinatorError::LiveCommandOrdinaryShutdown)
            }
            Ok(LiveCommandGateStatus::PersistentFailure) => Err(
                ProjectionCoordinatorError::LiveCommandPersistentHomeFailure {
                    generation: self.home_generation,
                },
            ),
            Ok(LiveCommandGateStatus::LocalFailure) => {
                Err(ProjectionCoordinatorError::LiveCommandLocalFailure)
            }
            Err(_) => Err(ProjectionCoordinatorError::LiveCommandGateUnavailable),
        }
    }

    pub(in crate::cas_projection::accepted_input_scheduler) fn observe_persistent_failure(
        &self,
    ) -> bool {
        let _ = self.command.observe_persistent_failure();
        self.terminal_disposer.failure_observed() && !self.command.is_current()
    }

    pub(in crate::cas_projection::accepted_input_scheduler) fn retain_failed_projection(
        &self,
        projection: crate::cas_projection::LoadedCasProjection,
    ) {
        self.terminal_disposer.dispose_loaded(projection)
    }

    fn ensure_generation_current(&self) -> Result<(), ProjectionCoordinatorError> {
        if self.home.home_id() != self.home_id {
            return Err(ProjectionCoordinatorError::HomeIdentityMismatch {
                expected: self.home_id,
                actual: self.home.home_id(),
            });
        }
        let health = self.home.health();
        if health.state() != HomeHealthState::Healthy
            || health.generation() != Some(self.home_generation)
        {
            return Err(ProjectionCoordinatorError::HomeGenerationMismatch {
                expected: self.home_generation,
                actual: health.generation(),
                state: health.state(),
            });
        }
        self.storage
            .revision(&self.home)
            .map_err(|source| ProjectionCoordinatorError::SyndicRevisionUnavailable { source })?;
        Ok(())
    }

    pub(in crate::cas_projection::accepted_input_scheduler) fn validate(
        &self,
        lease: &mut ScheduledOrdinaryExecutionLease,
    ) -> Result<(), ScheduledOrdinaryAdmissionError> {
        self.ensure_current()?;
        lease
            .assets()
            .revision(&self.home)
            .map_err(|source| ScheduledOrdinaryAdmissionError::AssetAuthority { source })?;
        let retained_process_generation = lease.process_generation();
        let expected_connection = Arc::clone(lease.connection());
        let (runtime_id, process_generation, session_matches_connection) = {
            let session = lease.session();
            (
                session.runtime_id(),
                session.process_generation(),
                Arc::ptr_eq(session.connection(), &expected_connection),
            )
        };
        let is_owned = self
            .connections
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionConnection,
            })?
            .iter()
            .any(|candidate| Arc::ptr_eq(candidate, &expected_connection));
        if !is_owned
            || !session_matches_connection
            || expected_connection.is_retired()
            || expected_connection.is_detached()
            || runtime_id != lease.execution_binding().runtime_id()
            || process_generation != retained_process_generation
        {
            return Err(
                ScheduledOrdinaryAdmissionError::SessionAuthorityUnavailable {
                    runtime_id,
                    process_generation,
                },
            );
        }
        self.ensure_current()?;
        Ok(())
    }

    pub(super) fn reserve_promotion(
        &self,
        lease: &mut ScheduledOrdinaryExecutionLease,
    ) -> Result<Option<ConnectionPromotionReservation>, ScheduledOrdinaryAdmissionError> {
        self.ensure_current()?;
        lease
            .assets()
            .revision(&self.home)
            .map_err(|source| ScheduledOrdinaryAdmissionError::AssetAuthority { source })?;
        let retained_process_generation = lease.process_generation();
        let expected_connection = Arc::clone(lease.connection());
        let (runtime_id, process_generation, session_matches_connection) = {
            let session = lease.session();
            (
                session.runtime_id(),
                session.process_generation(),
                Arc::ptr_eq(session.connection(), &expected_connection),
            )
        };
        let reservation = {
            let registry = self.connections.lock().map_err(|_| {
                ProjectionCoordinatorError::RegistryPoisoned {
                    registry: ProjectionRegistryKind::ProjectionConnection,
                }
            })?;
            self.ensure_command_current()?;
            if !registry
                .iter()
                .any(|candidate| Arc::ptr_eq(candidate, &expected_connection))
                || !session_matches_connection
                || runtime_id != lease.execution_binding().runtime_id()
                || process_generation != retained_process_generation
                || expected_connection.is_detached()
            {
                return Ok(None);
            }
            expected_connection.reserve_scheduled_promotion()?
        };
        self.ensure_generation_current()?;
        Ok(reservation)
    }
}
