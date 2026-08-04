use std::collections::HashMap;

use super::{
    super::{
        ConnectionGeneration, LoadedThreadEntryState, LoadedThreadState, registry,
        remove_connection_authority,
    },
    model::{
        LoadedRegistryRecoveryAuthority, LoadedRegistryRecoveryObservation,
        LoadedRegistryRecoveryToken, RecoveryToken,
    },
};
use crate::cas_projection::{ProjectionCoordinatorError, ProjectionRegistryKind};

/// Authenticates one opaque recovery observation while holding the actual loaded-registry lock.
///
/// Poison remains a typed, content-free failure. Unlike local disposition, authentication never
/// recovers a poisoned guard into evidence that authority is live.
pub(in crate::cas_projection) fn authenticate_recovery_observation(
    observation: &LoadedRegistryRecoveryObservation,
) -> Result<bool, ProjectionCoordinatorError> {
    if observation.generation.process() != observation.key.process_generation {
        return Ok(false);
    }
    let state = registry()
        .lock()
        .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
            registry: ProjectionRegistryKind::LoadedThreads,
        })?;
    Ok(observation_matches_locked(&state, observation))
}

/// Authenticates one complete observation set under a single loaded-registry lock.
///
/// The first mismatching index is returned without mutating registry state. This is the final
/// candidate-set seal primitive used while every corresponding connection-retirement gate is held.
pub(in crate::cas_projection) fn authenticate_recovery_observations(
    observations: &[LoadedRegistryRecoveryObservation],
) -> Result<Option<usize>, ProjectionCoordinatorError> {
    if let Some(index) = observations.iter().position(|observation| {
        observation.generation.process() != observation.key.process_generation
    }) {
        return Ok(Some(index));
    }
    let state = registry()
        .lock()
        .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
            registry: ProjectionRegistryKind::LoadedThreads,
        })?;
    Ok(observations
        .iter()
        .position(|observation| !observation_matches_locked(&state, observation)))
}

/// Conservatively revokes one recovery observation without command or backend authority.
///
/// This destructor-only path recovers a poisoned registry guard and trusts only the observation's
/// globally unique primary token. Stale location or identity metadata therefore cannot preserve
/// authority after its local owner is dropped.
pub(in crate::cas_projection) fn settle_recovery_observation_locally(
    observation: &LoadedRegistryRecoveryObservation,
) {
    let mut state = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let token = observation.authority().token();

    revoke_recovery_token_everywhere(&mut state, token);
    rebuild_connection_authority_counts(&mut state);

    assert!(
        !recovery_token_is_present(&state, token),
        "locally settled recovery token must be absent before its owner is disarmed"
    );
}

fn revoke_recovery_token_everywhere(
    state: &mut LoadedThreadState,
    token: LoadedRegistryRecoveryToken,
) {
    match token.0 {
        RecoveryToken::ActiveLease(token) => {
            state.entries.retain(|_, entry| match &mut entry.state {
                LoadedThreadEntryState::Active { leases } => {
                    let removed = leases.remove(&token);
                    !removed || !leases.is_empty()
                }
                LoadedThreadEntryState::Quarantined { .. } => true,
            });
        }
        RecoveryToken::QuarantinedAnchor(token) => {
            state.entries.retain(|_, entry| {
                !matches!(
                    &entry.state,
                    LoadedThreadEntryState::Quarantined { token: current } if *current == token
                )
            });
        }
        RecoveryToken::ReacquisitionReservation(token) => {
            // The reservation's anchor token is a link to separately owned anchor authority.
            state
                .reacquisition_reservations
                .retain(|_, reservation| reservation.token != token);
        }
    }
}

fn rebuild_connection_authority_counts(state: &mut LoadedThreadState) {
    let mut rebuilt = HashMap::<ConnectionGeneration, usize>::new();
    for entry in state.entries.values() {
        increment_connection_authority_count(&mut rebuilt, entry.connection);
    }
    for replacement in state.reacquisition_reservations.keys() {
        increment_connection_authority_count(&mut rebuilt, *replacement);
    }
    state.connection_authority_counts = rebuilt;
}

fn increment_connection_authority_count(
    counts: &mut HashMap<ConnectionGeneration, usize>,
    connection: ConnectionGeneration,
) {
    let count = counts.entry(connection).or_default();
    *count = count
        .checked_add(1)
        .expect("registered connection authorities fit in memory");
}

fn recovery_token_is_present(
    state: &LoadedThreadState,
    token: LoadedRegistryRecoveryToken,
) -> bool {
    match token.0 {
        RecoveryToken::ActiveLease(token) => state.entries.values().any(|entry| {
            matches!(
                &entry.state,
                LoadedThreadEntryState::Active { leases } if leases.contains(&token)
            )
        }),
        RecoveryToken::QuarantinedAnchor(token) => state.entries.values().any(|entry| {
            matches!(
                &entry.state,
                LoadedThreadEntryState::Quarantined { token: current } if *current == token
            )
        }),
        RecoveryToken::ReacquisitionReservation(token) => state
            .reacquisition_reservations
            .values()
            .any(|reservation| reservation.token == token),
    }
}

pub(super) fn dispose_observation_locked(
    state: &mut LoadedThreadState,
    observation: &LoadedRegistryRecoveryObservation,
) -> bool {
    match observation.authority {
        LoadedRegistryRecoveryAuthority::ActiveLease { token } => {
            dispose_active_locked(state, observation, token)
        }
        LoadedRegistryRecoveryAuthority::QuarantinedAnchor { token } => {
            dispose_quarantined_locked(state, observation, token)
        }
        LoadedRegistryRecoveryAuthority::ReacquisitionReservation {
            anchor_connection,
            anchor_token,
            token,
        } => dispose_reservation_locked(state, observation, anchor_connection, anchor_token, token),
    }
}

fn observation_matches_locked(
    state: &LoadedThreadState,
    observation: &LoadedRegistryRecoveryObservation,
) -> bool {
    match observation.authority {
        LoadedRegistryRecoveryAuthority::ActiveLease { token } => {
            let Some(token) = token.active_raw() else {
                return false;
            };
            state.entries.get(&observation.key).is_some_and(|entry| {
                entry.connection == observation.connection
                    && entry.owner == observation.owner
                    && entry.generation == observation.generation.thread()
                    && matches!(
                        &entry.state,
                        LoadedThreadEntryState::Active { leases } if leases.contains(&token)
                    )
            })
        }
        LoadedRegistryRecoveryAuthority::QuarantinedAnchor { token } => {
            let Some(token) = token.quarantined_raw() else {
                return false;
            };
            state.entries.get(&observation.key).is_some_and(|entry| {
                entry.connection == observation.connection
                    && entry.owner == observation.owner
                    && entry.generation == observation.generation.thread()
                    && matches!(
                        &entry.state,
                        LoadedThreadEntryState::Quarantined { token: current } if *current == token
                    )
            })
        }
        LoadedRegistryRecoveryAuthority::ReacquisitionReservation {
            anchor_connection,
            anchor_token,
            token,
        } => {
            let (Some(anchor_token), Some(token)) =
                (anchor_token.quarantined_raw(), token.reservation_raw())
            else {
                return false;
            };
            state
                .reacquisition_reservations
                .get(&observation.connection)
                .is_some_and(|reservation| {
                    reservation.key == observation.key
                        && reservation.anchor_connection == anchor_connection
                        && reservation.owner == observation.owner
                        && reservation.anchor_generation == observation.generation.thread()
                        && reservation.anchor_token == anchor_token
                        && reservation.token == token
                })
        }
    }
}

fn dispose_active_locked(
    state: &mut LoadedThreadState,
    observation: &LoadedRegistryRecoveryObservation,
    token: LoadedRegistryRecoveryToken,
) -> bool {
    let Some(token) = token.active_raw() else {
        return false;
    };
    let remove_entry = {
        let Some(entry) = state.entries.get_mut(&observation.key) else {
            return false;
        };
        if entry.connection != observation.connection
            || entry.owner != observation.owner
            || entry.generation != observation.generation.thread()
        {
            return false;
        }
        let LoadedThreadEntryState::Active { leases } = &mut entry.state else {
            return false;
        };
        if !leases.remove(&token) {
            return false;
        }
        leases.is_empty()
    };
    if remove_entry {
        state.entries.remove(&observation.key);
        remove_connection_authority(state, observation.connection);
    }
    true
}

fn dispose_quarantined_locked(
    state: &mut LoadedThreadState,
    observation: &LoadedRegistryRecoveryObservation,
    token: LoadedRegistryRecoveryToken,
) -> bool {
    let Some(token) = token.quarantined_raw() else {
        return false;
    };
    let exact = state.entries.get(&observation.key).is_some_and(|entry| {
        entry.connection == observation.connection
            && entry.owner == observation.owner
            && entry.generation == observation.generation.thread()
            && matches!(
                &entry.state,
                LoadedThreadEntryState::Quarantined { token: current } if *current == token
            )
    });
    if exact {
        state.entries.remove(&observation.key);
        remove_connection_authority(state, observation.connection);
    }
    exact
}

fn dispose_reservation_locked(
    state: &mut LoadedThreadState,
    observation: &LoadedRegistryRecoveryObservation,
    anchor_connection: ConnectionGeneration,
    anchor_token: LoadedRegistryRecoveryToken,
    token: LoadedRegistryRecoveryToken,
) -> bool {
    let (Some(anchor_token), Some(token)) =
        (anchor_token.quarantined_raw(), token.reservation_raw())
    else {
        return false;
    };
    let exact = state
        .reacquisition_reservations
        .get(&observation.connection)
        .is_some_and(|reservation| {
            reservation.key == observation.key
                && reservation.anchor_connection == anchor_connection
                && reservation.owner == observation.owner
                && reservation.anchor_generation == observation.generation.thread()
                && reservation.anchor_token == anchor_token
                && reservation.token == token
        });
    if exact {
        state
            .reacquisition_reservations
            .remove(&observation.connection);
        remove_connection_authority(state, observation.connection);
    }
    exact
}
