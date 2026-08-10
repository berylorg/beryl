use std::sync::Arc;

use beryl_state::BerylState;

use super::super::super::{
    connection::{
        ConnectionEpochAdoptionBarrier, ConnectionReplacementContext, ProjectionConnection,
    },
    persistent_failure::{
        PendingProjectionAdoptionTopology, PersistentFailureAdoptionFence,
        PersistentFailureCutIdentity, PersistentFailureRecoveryInventory,
    },
    service_config::{ProjectionPreactivationRecoveryHold, ProjectionWorkerPermitError},
    service_startup::ServiceStartupGate,
};
use super::{
    ConnectionAdoptionState, PersistentFailureServiceAdoptionReason, ProjectionConnectionService,
    ServiceAdoptionAttempt, UnpublishedProjectionConnectionService,
};

#[cfg(test)]
mod test_support;

#[cfg(test)]
pub(super) use test_support::{
    panic_after_old_ingester_join_if_armed, pause_before_commit_if_armed,
    retain_late_authority_before_commit_if_armed,
};

impl ServiceAdoptionAttempt {
    pub(super) fn preallocate_attempt_storage(
        &mut self,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        self.connection_states
            .try_reserve_exact(self.connections.len())
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.connection_states
            .resize_with(self.connections.len(), || ConnectionAdoptionState::Stable);
        self.replacement_candidate_holds
            .try_reserve_exact(self.metadata.candidate_count())
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.failed_candidate_permits
            .try_reserve_exact(self.metadata.candidate_count())
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.old_candidate_holds
            .try_reserve_exact(self.metadata.candidate_count())
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.connection_check_scratch
            .try_reserve_exact(self.connections.len())
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;

        let replacement = self
            .replacement
            .as_ref()
            .expect("an active adoption attempt retains its replacement service");
        let service = replacement.service();
        if service.connections.service_generation() != service.service_generation() {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }
        let mut connections = service
            .connections
            .lock()
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementRegistryUnavailable)?;
        if !connections.is_empty() {
            return Err(PersistentFailureServiceAdoptionReason::ReplacementRegistryNotEmpty);
        }
        connections
            .try_reserve_exact(self.connections.len())
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        Ok(())
    }

    pub(super) fn reserve_replacement_topology(
        &mut self,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        let replacement = self
            .replacement
            .as_ref()
            .expect("an active adoption attempt retains its replacement service");
        let service = replacement.service();
        let persistent_failure = service
            .persistent_failure
            .as_ref()
            .expect("an unpublished service retains its dormant failure coordinator");
        let context = ConnectionReplacementContext {
            home: Arc::clone(
                service
                    .home
                    .as_ref()
                    .expect("an unpublished service owns its recovered home"),
            ),
            home_id: service.home_id,
            home_generation: service.home_generation,
            storage: service.storage,
            commands: service.command_authorizer.clone(),
            stop: Arc::clone(&service.stop_coordinator),
            compaction: Arc::clone(
                service
                    .context_compaction
                    .as_ref()
                    .expect("an unpublished service retains dormant compaction"),
            ),
            scheduler: service.scheduler_signal.clone(),
            failure_notification: persistent_failure.notification(),
            retainer: persistent_failure
                .projection_retainer(service.home_id, service.home_generation),
            startup: Arc::clone(replacement.startup_gate()),
        };
        let workers = service.workers.clone();
        for (index, connection) in self.connections.iter().enumerate() {
            #[cfg(test)]
            let connection_generation = connection.identity_observation().connection_generation();
            #[cfg(test)]
            if self
                .replacement_resource_failure
                .as_mut()
                .is_some_and(|selector| {
                    selector.take_worker_capacity_for_connection(connection_generation)
                })
            {
                return Err(PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable);
            }
            let pair = workers
                .try_acquire_pair()
                .map_err(map_worker_permit_error)?;
            #[cfg(test)]
            let broker_spawn_failure =
                self.replacement_resource_failure
                    .as_mut()
                    .is_some_and(|selector| {
                        selector.take_broker_spawn_for_connection(connection_generation)
                    });
            #[cfg(test)]
            let prepared = if broker_spawn_failure {
                connection
                    .prepare_replacement_epoch_with_broker_spawn_failure_for_test(&context, pair)
            } else {
                connection.prepare_replacement_epoch(&context, pair)
            };
            #[cfg(not(test))]
            let prepared = connection.prepare_replacement_epoch(&context, pair);
            match prepared {
                Ok(prepared) => {
                    self.connection_states[index] = ConnectionAdoptionState::Prepared(prepared);
                }
                Err(failure) => {
                    self.connection_states[index] =
                        ConnectionAdoptionState::PreparationFailed(failure);
                    return Err(PersistentFailureServiceAdoptionReason::ConnectionPreparation);
                }
            }
        }
        for _ in 0..self.metadata.candidate_count() {
            let permit = workers
                .try_acquire_scheduled_ordinary_or_arm()
                .map_err(map_worker_permit_error)?;
            match permit.into_preactivation_recovery_hold() {
                Ok(hold) => self.replacement_candidate_holds.push(hold),
                Err(permit) => {
                    self.failed_candidate_permits.push(permit);
                    return Err(PersistentFailureServiceAdoptionReason::ConnectionPreparation);
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_and_order_connections(
        &mut self,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        let topology = self
            .topology
            .as_ref()
            .expect("adoption topology is checked out before connection validation");
        if topology.group_count() != self.metadata.group_count()
            || topology.candidate_count() != self.metadata.candidate_count()
            || topology.retained_connection_count() != self.metadata.connection_count()
            || topology.connection_owner_count() != self.metadata.connection_count()
            || topology.local_disposition_count() != self.metadata.local_disposition_count()
        {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }
        if self.connections.len() != self.metadata.connection_count() {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }
        sort_connections(&mut self.connections);
        if has_duplicate_connections(&self.connections) {
            return Err(PersistentFailureServiceAdoptionReason::DuplicateConnection);
        }

        let mut owner_connections = Vec::new();
        owner_connections
            .try_reserve_exact(topology.connection_owner_count())
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.rejected_connections
            .try_reserve_exact(topology.connection_owner_count())
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        let maximum_inert_attachment_count = self
            .connections
            .len()
            .checked_add(topology.connection_owner_count())
            .ok_or(PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.inert_attachments
            .try_reserve_exact(maximum_inert_attachment_count)
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.inert_attachment_limit = Some(maximum_inert_attachment_count);
        for owner in topology.connection_owners() {
            let retained = owner.stable_connection_for_inert_failure();
            if !contains_connection(&self.connections, retained) {
                push_unique_connection(&mut self.rejected_connections, Arc::clone(retained));
            }
            let connection = owner
                .observe_for_adoption(self.cut)
                .map_err(|_| PersistentFailureServiceAdoptionReason::ConnectionUnavailable)?
                .ok_or(PersistentFailureServiceAdoptionReason::ConnectionUnavailable)?;
            owner_connections.push(connection);
        }
        sort_connections(&mut owner_connections);
        if has_duplicate_connections(&owner_connections) {
            return Err(PersistentFailureServiceAdoptionReason::DuplicateConnection);
        }
        if !same_connection_set(&self.connections, &owner_connections) {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }

        let inventory = self
            .inventory
            .as_ref()
            .expect("an active adoption attempt retains its quarantine inventory");
        let registry = inventory.retained_connection_registry();
        if registry.service_generation() != inventory.service_generation() {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }
        let mut registered = inventory
            .current_service_connections()
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementRegistryUnavailable)?;
        sort_connections(&mut registered);
        if has_duplicate_connections(&registered) {
            return Err(PersistentFailureServiceAdoptionReason::DuplicateConnection);
        }
        if !same_connection_set(&self.connections, &registered) {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }
        self.validate_candidate_connections(topology)
    }

    fn validate_candidate_connections(
        &self,
        topology: &PendingProjectionAdoptionTopology,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        if self.connections.is_empty()
            && (topology.group_count() != 0
                || topology.candidate_count() != 0
                || topology.connection_owner_count() != 0)
        {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }
        for group in topology.groups() {
            let group_connection = group.identity().connection();
            if !self
                .connections
                .iter()
                .any(|connection| group_connection.matches_connection(connection))
                || *group.witness().home_id() != self.cut.home_id
                || *group.witness().home_generation() != self.cut.home_generation
            {
                return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
            }
            for candidate in group.candidates() {
                if !candidate.is_exact_candidate_for(self.cut)
                    || !candidate
                        .observation()
                        .connection()
                        .same_connection(group_connection)
                {
                    return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
                }
            }
        }
        Ok(())
    }
}

fn sort_connections(connections: &mut [Arc<ProjectionConnection>]) {
    connections.sort_unstable_by_key(|connection| {
        connection.identity_observation().connection_generation()
    });
}

fn has_duplicate_connections(connections: &[Arc<ProjectionConnection>]) -> bool {
    connections.windows(2).any(|pair| {
        Arc::ptr_eq(&pair[0], &pair[1])
            || pair[0].identity_observation().connection_generation()
                == pair[1].identity_observation().connection_generation()
    })
}

fn contains_connection(
    connections: &[Arc<ProjectionConnection>],
    candidate: &Arc<ProjectionConnection>,
) -> bool {
    connections.iter().any(|connection| {
        Arc::ptr_eq(connection, candidate)
            && connection.identity_observation() == candidate.identity_observation()
    })
}

fn push_unique_connection(
    connections: &mut Vec<Arc<ProjectionConnection>>,
    candidate: Arc<ProjectionConnection>,
) {
    if !contains_connection(connections, &candidate) {
        connections.push(candidate);
    }
}

fn same_connection_set(
    left: &[Arc<ProjectionConnection>],
    right: &[Arc<ProjectionConnection>],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            Arc::ptr_eq(left, right) && left.identity_observation() == right.identity_observation()
        })
}

fn validate_fenced_topology(
    topology: &PendingProjectionAdoptionTopology,
    connections: &[Arc<ProjectionConnection>],
    cut: PersistentFailureCutIdentity,
    scratch: &mut Vec<Arc<ProjectionConnection>>,
) -> Result<(), PersistentFailureServiceAdoptionReason> {
    scratch.clear();
    if scratch.capacity() < topology.connection_owner_count() {
        return Err(PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable);
    }
    for owner in topology.connection_owners() {
        let connection = owner
            .observe_for_adoption(cut)
            .map_err(|_| PersistentFailureServiceAdoptionReason::ConnectionUnavailable)?
            .ok_or(PersistentFailureServiceAdoptionReason::ConnectionUnavailable)?;
        scratch.push(connection);
    }
    sort_connections(scratch);
    if has_duplicate_connections(scratch) || !same_connection_set(connections, scratch) {
        return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
    }
    for group in topology.groups() {
        let group_connection = group.identity().connection();
        if !connections
            .iter()
            .any(|connection| group_connection.matches_connection(connection))
            || *group.witness().home_id() != cut.home_id
            || *group.witness().home_generation() != cut.home_generation
            || group.candidates().iter().any(|candidate| {
                !candidate.is_exact_candidate_for(cut)
                    || !candidate
                        .observation()
                        .connection()
                        .same_connection(group_connection)
            })
        {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }
    }
    Ok(())
}

fn registry_matches_connections(
    registered: &[Arc<ProjectionConnection>],
    connections: &[Arc<ProjectionConnection>],
) -> bool {
    registered.len() == connections.len()
        && registered
            .iter()
            .all(|registered| contains_connection(connections, registered))
        && registered.iter().enumerate().all(|(index, candidate)| {
            !registered[index + 1..]
                .iter()
                .any(|other| Arc::ptr_eq(candidate, other))
        })
}

fn map_worker_permit_error(
    _error: ProjectionWorkerPermitError,
) -> PersistentFailureServiceAdoptionReason {
    PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable
}

#[allow(clippy::too_many_arguments)]
pub(super) fn commit_fenced<'a>(
    cut: PersistentFailureCutIdentity,
    connections: &'a [Arc<ProjectionConnection>],
    states: &mut [ConnectionAdoptionState],
    topology: &mut PendingProjectionAdoptionTopology,
    replacement_holds: &mut Vec<ProjectionPreactivationRecoveryHold>,
    old_holds: &mut Vec<ProjectionPreactivationRecoveryHold>,
    connection_scratch: &mut Vec<Arc<ProjectionConnection>>,
    inventory: &PersistentFailureRecoveryInventory,
    fence: &PersistentFailureAdoptionFence,
    replacement: &mut UnpublishedProjectionConnectionService,
    adopted_service: &mut Option<ProjectionConnectionService>,
    adopted_beryl_state: &mut Option<BerylState>,
    adopted_startup_gate: &mut Option<Arc<ServiceStartupGate>>,
    barriers: &mut Vec<ConnectionEpochAdoptionBarrier<'a>>,
) -> Result<(), PersistentFailureServiceAdoptionReason> {
    let old_registry = inventory.retained_connection_registry();
    let new_registry = Arc::clone(&replacement.service().connections);
    if old_registry.service_generation() != cut.service_generation
        || new_registry.service_generation() <= old_registry.service_generation()
    {
        return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
    }

    for connection in connections {
        barriers.push(
            connection
                .lock_epoch_for_adoption()
                .map_err(|_| PersistentFailureServiceAdoptionReason::ForwardingHubUnavailable)?,
        );
    }
    if barriers.len() != states.len()
        || barriers.iter().zip(states.iter()).any(|(barrier, state)| {
            let ConnectionAdoptionState::Joined(bound, _) = state else {
                return true;
            };
            !barrier.validates(cut, bound)
        })
    {
        return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
    }
    validate_fenced_topology(topology, connections, cut, connection_scratch)?;

    let mut old_connections = old_registry
        .lock()
        .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementRegistryUnavailable)?;
    let mut new_connections = new_registry
        .lock()
        .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementRegistryUnavailable)?;
    if !registry_matches_connections(&old_connections, connections) {
        return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
    }
    if !new_connections.is_empty() {
        return Err(PersistentFailureServiceAdoptionReason::ReplacementRegistryNotEmpty);
    }
    if new_connections.capacity() < connections.len() {
        return Err(PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable);
    }
    inventory
        .commit_pending_quarantine_adoption(fence, || {
            debug_assert_eq!(replacement_holds.len(), topology.candidate_count());
            debug_assert!(old_holds.capacity() - old_holds.len() >= topology.candidate_count());
            let mut replacements = replacement_holds.drain(..);
            for group in topology.groups_mut() {
                for candidate in group.candidates_mut() {
                    let replacement = replacements
                        .next()
                        .expect("preflight reserved one replacement hold per candidate");
                    old_holds.push(candidate.exchange_recovery_hold(replacement));
                }
            }
            debug_assert!(replacements.next().is_none());

            for index in 0..states.len() {
                let state = std::mem::replace(&mut states[index], ConnectionAdoptionState::Vacant);
                let ConnectionAdoptionState::Joined(bound, stopped) = state else {
                    unreachable!("all connection states were fenced and validated before commit")
                };
                let adopted = barriers[index].commit(bound, stopped);
                states[index] = ConnectionAdoptionState::Adopted(adopted);
            }
            new_connections.append(&mut old_connections);
            debug_assert!(old_connections.is_empty());
            debug_assert_eq!(new_connections.len(), connections.len());
            drop(new_connections);
            drop(old_connections);
            barriers.clear();

            *adopted_service = Some(replacement.take_service());
            *adopted_beryl_state = Some(replacement.take_beryl_state());
            *adopted_startup_gate = Some(replacement.take_startup_gate());
        })
        .map_err(PersistentFailureServiceAdoptionReason::LatePublication)?;
    Ok(())
}
