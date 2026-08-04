use beryl_model::CasLoadedSessionGeneration;

use crate::cas_projection::{
    ProjectionCoordinatorError, connection::lifecycle::ProjectionConnectionIdentityObservation,
};

use super::{
    super::{
        ConnectionGeneration, LoadedThreadEntryState, LoadedThreadKey, LoadedThreadState, lock,
    },
    model::LoadedRegistryRecoveryObservation,
};

/// Failure to capture an exact recovery audit for a bounded connection set.
#[derive(Debug)]
pub(in crate::cas_projection) enum LoadedRegistryRecoveryAuditError {
    Registry(ProjectionCoordinatorError),
    ConflictingConnectionIdentity { connection_generation: u64 },
    ConnectionIdentityMismatch { connection_generation: u64 },
}

/// Bounded exact snapshot of loaded-registry authority related to a supplied connection set.
#[derive(Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct LoadedRegistryRecoveryAudit {
    requested_connections: Vec<ProjectionConnectionIdentityObservation>,
    observations: Vec<LoadedRegistryRecoveryObservation>,
}

impl LoadedRegistryRecoveryAudit {
    pub(in crate::cas_projection) fn requested_connections(
        &self,
    ) -> &[ProjectionConnectionIdentityObservation] {
        &self.requested_connections
    }

    pub(in crate::cas_projection) fn observations(&self) -> &[LoadedRegistryRecoveryObservation] {
        &self.observations
    }

    pub(in crate::cas_projection) fn into_observations(
        self,
    ) -> Vec<LoadedRegistryRecoveryObservation> {
        self.observations
    }
}

/// Observes all registry authority directly or reservation-linked to the supplied generations.
///
/// The supplied slice is the caller's already-bounded recovery connection set. This operation
/// holds the loaded-registry lock for the complete observation and performs no registry mutation.
pub(in crate::cas_projection) fn recovery_audit(
    connections: &[ProjectionConnectionIdentityObservation],
) -> Result<LoadedRegistryRecoveryAudit, LoadedRegistryRecoveryAuditError> {
    let requested_connections = normalize_scope(connections)?;
    let state = lock().map_err(LoadedRegistryRecoveryAuditError::Registry)?;
    audit_locked(&state, requested_connections)
}

pub(super) fn normalize_scope(
    connections: &[ProjectionConnectionIdentityObservation],
) -> Result<Vec<ProjectionConnectionIdentityObservation>, LoadedRegistryRecoveryAuditError> {
    let mut requested_connections: Vec<ProjectionConnectionIdentityObservation> =
        Vec::with_capacity(connections.len());
    for connection in connections {
        if let Some(existing) = requested_connections
            .iter()
            .find(|existing| existing.connection_generation() == connection.connection_generation())
        {
            if existing != connection {
                return Err(
                    LoadedRegistryRecoveryAuditError::ConflictingConnectionIdentity {
                        connection_generation: connection.connection_generation(),
                    },
                );
            }
        } else {
            requested_connections.push(*connection);
        }
    }
    requested_connections.sort_unstable_by_key(|connection| connection.connection_generation());
    Ok(requested_connections)
}

pub(super) fn audit_locked(
    state: &LoadedThreadState,
    requested_connections: Vec<ProjectionConnectionIdentityObservation>,
) -> Result<LoadedRegistryRecoveryAudit, LoadedRegistryRecoveryAuditError> {
    let mut observations = Vec::new();
    for (key, entry) in &state.entries {
        let Some(connection) = scoped_identity(&requested_connections, entry.connection) else {
            continue;
        };
        validate_key_identity(connection, key)?;
        let generation = CasLoadedSessionGeneration::new(key.process_generation, entry.generation);
        match &entry.state {
            LoadedThreadEntryState::Active { leases } => {
                observations.extend(leases.iter().map(|token| {
                    LoadedRegistryRecoveryObservation::active(
                        key.clone(),
                        entry.connection,
                        entry.owner,
                        generation,
                        *token,
                    )
                }));
            }
            LoadedThreadEntryState::Quarantined { token } => {
                observations.push(LoadedRegistryRecoveryObservation::quarantined(
                    key.clone(),
                    entry.connection,
                    entry.owner,
                    generation,
                    *token,
                ));
            }
        }
    }
    for (replacement_connection, reservation) in &state.reacquisition_reservations {
        let replacement = scoped_identity(&requested_connections, *replacement_connection);
        let anchor = scoped_identity(&requested_connections, reservation.anchor_connection);
        if replacement.is_none() && anchor.is_none() {
            continue;
        }
        if let Some(connection) = replacement {
            validate_key_identity(connection, &reservation.key)?;
        }
        if let Some(connection) = anchor {
            validate_key_identity(connection, &reservation.key)?;
        }
        observations.push(
            LoadedRegistryRecoveryObservation::reacquisition_reservation(
                reservation.key.clone(),
                reservation.anchor_connection,
                *replacement_connection,
                reservation.owner,
                CasLoadedSessionGeneration::new(
                    reservation.key.process_generation,
                    reservation.anchor_generation,
                ),
                reservation.anchor_token,
                reservation.token,
            ),
        );
    }
    observations.sort_unstable_by_key(LoadedRegistryRecoveryObservation::sort_key);
    Ok(LoadedRegistryRecoveryAudit {
        requested_connections,
        observations,
    })
}

fn scoped_identity(
    connections: &[ProjectionConnectionIdentityObservation],
    generation: ConnectionGeneration,
) -> Option<ProjectionConnectionIdentityObservation> {
    connections
        .iter()
        .copied()
        .find(|connection| connection.connection_generation() == generation.get())
}

fn validate_key_identity(
    connection: ProjectionConnectionIdentityObservation,
    key: &LoadedThreadKey,
) -> Result<(), LoadedRegistryRecoveryAuditError> {
    if connection.runtime_id() != key.runtime_id
        || connection.process_generation() != key.process_generation
    {
        return Err(
            LoadedRegistryRecoveryAuditError::ConnectionIdentityMismatch {
                connection_generation: connection.connection_generation(),
            },
        );
    }
    Ok(())
}
