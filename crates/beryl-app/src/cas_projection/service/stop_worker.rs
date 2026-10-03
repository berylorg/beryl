use std::sync::Weak;

use super::commands::PreparedStop;
use super::*;
use crate::cas_projection::{
    ExactSoftStopAvailability, ExactSoftStopEligibility, ExactSoftStopUnavailable,
    ExactStopFeedback, ExactStopRequestError,
};

#[derive(Clone)]
pub struct ExactStopWorker {
    home: Weak<HomeServiceReference>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
    storage: SyndicStorage,
    command_authorizer: LiveCommandAuthorizer,
    connections: Weak<ProjectionServiceConnectionRegistry>,
    stop_coordinator: Weak<StopCoordinator>,
}

pub(super) struct ExactStopRead {
    pub(super) home: Option<Arc<HomeServiceReference>>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) storage: SyndicStorage,
    pub(super) command_authorizer: LiveCommandAuthorizer,
    pub(super) connections: Arc<ProjectionServiceConnectionRegistry>,
    pub(super) stop_coordinator: Arc<StopCoordinator>,
}

impl ProjectionConnectionService {
    pub fn exact_stop_worker(&self) -> ExactStopWorker {
        ExactStopWorker {
            home: self.home.as_ref().map_or_else(Weak::new, Arc::downgrade),
            home_id: self.home_id,
            home_generation: self.home_generation,
            service_generation: self.service_generation,
            storage: self.storage.clone(),
            command_authorizer: self.command_authorizer.clone(),
            connections: Arc::downgrade(&self.connections),
            stop_coordinator: Arc::downgrade(&self.stop_coordinator),
        }
    }

    pub(super) fn exact_stop_read(&self) -> ExactStopRead {
        ExactStopRead {
            home: self.home.clone(),
            home_id: self.home_id,
            home_generation: self.home_generation,
            storage: self.storage.clone(),
            command_authorizer: self.command_authorizer.clone(),
            connections: self.connections.clone(),
            stop_coordinator: self.stop_coordinator.clone(),
        }
    }
}

impl ExactStopWorker {
    pub fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    pub fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    pub fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    pub fn exact_soft_stop_eligibility(&self, thread: SyndicThreadId) -> ExactSoftStopAvailability {
        let Ok(_command) = self.command_authorizer.authorize() else {
            return ExactSoftStopAvailability::Unavailable(
                ExactSoftStopUnavailable::AuthorityUnavailable,
            );
        };
        match self.read() {
            Some(read) => read.exact_soft_stop_eligibility(thread),
            None => ExactSoftStopAvailability::Unavailable(
                ExactSoftStopUnavailable::AuthorityUnavailable,
            ),
        }
    }

    pub fn request_exact_soft_stop(
        &self,
        eligibility: &ExactSoftStopEligibility,
    ) -> Result<ExactStopFeedback, ExactStopRequestError> {
        let _command = self
            .command_authorizer
            .authorize()
            .map_err(|_| ExactStopRequestError::Revoked)?;
        self.read()
            .ok_or(ExactStopRequestError::Revoked)?
            .request_exact_soft_stop(eligibility)
    }

    fn read(&self) -> Option<ExactStopRead> {
        let read = ExactStopRead {
            home: Some(self.home.upgrade()?),
            home_id: self.home_id,
            home_generation: self.home_generation,
            storage: self.storage.clone(),
            command_authorizer: self.command_authorizer.clone(),
            connections: self.connections.upgrade()?,
            stop_coordinator: self.stop_coordinator.upgrade()?,
        };
        if !self.command_authorizer.is_open()
            || read.connections.service_generation() != self.service_generation
            || admission::ensure_current_home(
                read.home.as_deref(),
                read.home_id,
                read.home_generation,
                &read.storage,
            )
            .is_err()
        {
            return None;
        }
        Some(read)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_resource_strong_counts(&self) -> [usize; 3] {
        [
            self.home.strong_count(),
            self.connections.strong_count(),
            self.stop_coordinator.strong_count(),
        ]
    }
}

impl ExactStopRead {
    pub(super) fn coordinate_prepared_stop(
        &self,
        connection: Arc<ProjectionConnection>,
        proof: super::super::connection::StopTargetProof,
        cause: StopCause,
    ) -> Result<StopCoordinationOutcome, StopCoordinationError> {
        match connection.coordinate_stop(&self.stop_coordinator, proof, cause)? {
            StopOwnership::Primary(owner) => {
                #[cfg(feature = "test-faults")]
                crate::cas_projection::test_faults::pause_stop_handoff(
                    owner.operation_id().thread_id(),
                );
                connection.dispatch_exact_stop(owner)
            }
            StopOwnership::Joined {
                operation_id,
                interruption: _,
            } => Ok(StopCoordinationOutcome::Stopping {
                operation_id,
                primary_owner: false,
            }),
        }
    }

    pub(super) fn prepare_stop(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<PreparedStop, StopCoordinationError> {
        let command = self
            .command_authorizer
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        super::admission::ensure_current_home(
            self.home.as_deref(),
            self.home_id,
            self.home_generation,
            &self.storage,
        )
        .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        let home = self
            .home
            .as_deref()
            .ok_or(StopCoordinationError::HomeAuthorityLost)?;
        let read = self.storage.stop_admission_read(
            home,
            thread_id,
            SyndicPointReadLimit::new(1_000_000)
                .expect("stop coordination point-read bound is nonzero"),
        )?;
        let (target, stopping) = match read {
            StopAdmissionRead::Admissible(candidate) => (candidate.target().clone(), false),
            StopAdmissionRead::Stopping(live) => (live.target().clone(), true),
            StopAdmissionRead::Ineligible(reason) => {
                return Ok(PreparedStop::Ineligible(reason));
            }
        };
        let (connection, proof) = self.stop_connection(&target)?;
        if !command.is_current() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        Ok(PreparedStop::Exact {
            stopping,
            target,
            connection,
            proof,
        })
    }

    fn stop_connection(
        &self,
        target: &syndic_storage::StopOperationTarget,
    ) -> Result<
        (
            Arc<ProjectionConnection>,
            super::super::connection::StopTargetProof,
        ),
        StopCoordinationError,
    > {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| StopCoordinationError::TargetUnavailable)?;
        let mut found = None;
        let mut duplicate = false;
        connections.retain(|connection| {
            if connection.is_detached() {
                return false;
            }
            if connection.runtime_id() == target.runtime_id()
                && connection.process_generation() == target.loaded_generation().process()
                && let Ok(proof) = connection.stop_target(target)
            {
                if found.is_some() {
                    duplicate = true;
                    return true;
                }

                found = Some((Arc::clone(connection), proof));
            }
            true
        });
        if duplicate {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        found.ok_or(StopCoordinationError::TargetUnavailable)
    }
}
