use std::sync::Arc;

use beryl_home_store::{HomeHealthState, HomeStore};

use super::{
    RecoveredServicePublicationReason, RecoveryServicePublicationAttempt,
    ReplacementWorkerPublicationState,
};
use crate::cas_projection::{
    ProjectionConnectionService,
    connection::{
        CandidateSetRecoveryPublicationBarrier, ConnectionEpochIdentity, ProjectionConnection,
        RecoveryPublicationEpochBarrier,
    },
    service::adoption::ConnectionAdoptionState,
    service_startup::ServiceStartupGate,
};

impl RecoveryServicePublicationAttempt {
    pub(super) fn preflight(&self) -> Result<(), RecoveredServicePublicationReason> {
        if self.attempt.publication_committed
            || self.attempt.fence.is_none()
            || self.attempt.inventory.is_none()
            || self.attempt.adopted_service.is_none()
            || self.attempt.adopted_beryl_state.is_none()
            || self.attempt.adopted_startup_gate.is_none()
            || self.retirement_witness.is_some()
            || self.prepared_epoch.is_some()
            || self.connection_owners.len() != self.attempt.metadata.connection_count()
            || self.attempt.connection_states.len() != self.connection_owners.len()
            || self.attempt.connections.len() != self.connection_owners.len()
        {
            return Err(RecoveredServicePublicationReason::PublicationTopologyMismatch);
        }
        let inventory = self
            .attempt
            .inventory
            .as_ref()
            .ok_or(RecoveredServicePublicationReason::PublicationTopologyMismatch)?;
        let service = self
            .attempt
            .adopted_service
            .as_ref()
            .ok_or(RecoveredServicePublicationReason::RecoveredServiceMismatch)?;
        let home = service
            .home
            .as_ref()
            .ok_or(RecoveredServicePublicationReason::RecoveredHomeMismatch)?;
        let startup = self
            .attempt
            .adopted_startup_gate
            .as_ref()
            .ok_or(RecoveredServicePublicationReason::StartupFenceUnavailable)?;
        if !Arc::ptr_eq(inventory.retained_home(), home)
            || service.home_id() != self.attempt.metadata.home_id()
            || service.home_generation() != self.attempt.metadata.new_home_generation()
            || service.service_generation() != self.attempt.metadata.new_service_generation()
        {
            return Err(RecoveredServicePublicationReason::RecoveredServiceMismatch);
        }
        let health = home.health();
        if health.state() != HomeHealthState::Healthy
            || health.generation() != Some(service.home_generation())
        {
            return Err(RecoveredServicePublicationReason::RecoveredHomeNotHealthy);
        }
        if !matches!(
            service.startup,
            super::super::super::ProjectionServiceStartupState::Ready { .. }
        ) {
            return Err(RecoveredServicePublicationReason::StartupNotConverged);
        }
        if !startup.is_closed() {
            return Err(RecoveredServicePublicationReason::StartupFenceUnavailable);
        }
        let expected_epoch = ConnectionEpochIdentity::new(
            service.home_id(),
            service.home_generation(),
            service.service_generation(),
        );
        let mut previous_generation = None;
        for (((owner, connection), state), index) in self
            .connection_owners
            .iter()
            .zip(&self.attempt.connections)
            .zip(&self.attempt.connection_states)
            .zip(0..)
        {
            let generation = owner.connection_generation_for_recovery_publication();
            if previous_generation.is_some_and(|previous| previous >= generation)
                || !Arc::ptr_eq(owner.connection_for_recovery_publication(), connection)
            {
                return Err(RecoveredServicePublicationReason::PublicationTopologyMismatch);
            }
            let ConnectionAdoptionState::Adopted(attachment) = state else {
                return Err(RecoveredServicePublicationReason::PublicationTopologyMismatch);
            };
            if index >= self.attempt.metadata.connection_count()
                || !attachment.preflight_recovery_publication(
                    connection,
                    home,
                    expected_epoch,
                    startup,
                )
            {
                return Err(RecoveredServicePublicationReason::PublicationTopologyMismatch);
            }
            previous_generation = Some(generation);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn validate_locked_publication(
        &self,
        connections: &[Arc<ProjectionConnection>],
        forwarding: &[RecoveryPublicationEpochBarrier<'_>],
        authorities: &[CandidateSetRecoveryPublicationBarrier<'_>],
        service_connections: &[Arc<ProjectionConnection>],
        service: &Arc<ProjectionConnectionService>,
        home: &Arc<HomeStore>,
        startup: &Arc<ServiceStartupGate>,
        expected_epoch: ConnectionEpochIdentity,
        worker_state: ReplacementWorkerPublicationState,
    ) -> Result<(), RecoveredServicePublicationReason> {
        if connections.len() != self.attempt.metadata.connection_count()
            || forwarding.len() != connections.len()
            || authorities.len() != connections.len()
            || self.attempt.connection_states.len() != connections.len()
            || service_connections.len() != connections.len()
            || self.connection_owners.len() != connections.len()
        {
            return Err(RecoveredServicePublicationReason::PublicationTopologyMismatch);
        }
        let witness = self
            .retirement_witness
            .as_ref()
            .ok_or(RecoveredServicePublicationReason::OldServiceEpochRetirement)?;
        if witness.cut_identity() != self.attempt.cut {
            return Err(RecoveredServicePublicationReason::OldServiceEpochRetirement);
        }
        if service.home_id() != self.attempt.metadata.home_id()
            || service.home_generation() != self.attempt.metadata.new_home_generation()
            || service.service_generation() != self.attempt.metadata.new_service_generation()
            || service
                .home
                .as_ref()
                .is_none_or(|service_home| !Arc::ptr_eq(service_home, home))
        {
            return Err(RecoveredServicePublicationReason::RecoveredServiceMismatch);
        }
        let health = home.health();
        if health.state() != HomeHealthState::Healthy
            || health.generation() != Some(service.home_generation())
        {
            return Err(RecoveredServicePublicationReason::RecoveredHomeNotHealthy);
        }
        if !matches!(
            service.startup,
            super::super::super::ProjectionServiceStartupState::Ready { .. }
        ) {
            return Err(RecoveredServicePublicationReason::StartupNotConverged);
        }
        if !startup.is_closed() {
            return Err(RecoveredServicePublicationReason::StartupFenceUnavailable);
        }
        for index in 0..connections.len() {
            if !Arc::ptr_eq(&connections[index], &service_connections[index])
                || !forwarding[index].validates(home, expected_epoch)
                || !authorities[index].validates(&connections[index], self.attempt.cut)
            {
                return Err(RecoveredServicePublicationReason::ServiceMembershipMismatch);
            }
            let ConnectionAdoptionState::Adopted(attachment) =
                &self.attempt.connection_states[index]
            else {
                return Err(RecoveredServicePublicationReason::PublicationTopologyMismatch);
            };
            let worker_state_valid = match worker_state {
                ReplacementWorkerPublicationState::RetiredUnarmed => attachment
                    .validates_retired_unarmed_publication(
                        &connections[index],
                        home,
                        expected_epoch,
                        startup,
                    ),
                ReplacementWorkerPublicationState::Armed => attachment.validates_armed_publication(
                    &connections[index],
                    home,
                    expected_epoch,
                    startup,
                ),
            };
            if !worker_state_valid {
                return Err(RecoveredServicePublicationReason::ReplacementWorkerArm);
            }
        }
        Ok(())
    }
}
