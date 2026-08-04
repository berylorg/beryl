use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::cas_projection::{
    LoadedCasProjection, SameNativeReacquisitionAnchor,
    connection::{
        FailureRetainedCleanupOwner, FailureRetainedPromotionReservation,
        LoadedLeaseRecoveryObservation, PersistentFailureDriverResult,
        PersistentFailureNoDispatchReason, PersistentFailureTargetGuardDisposition,
        PersistentFailureTargetGuardObservation, ProjectionConnection,
        ProjectionConnectionIdentityObservation,
        registry::{
            self, LoadedRegistryRecoveryAuditError, LoadedRegistryRecoveryAuthorityKind,
            LoadedRegistryRecoveryObservation,
        },
    },
    persistent_failure::{
        PersistentFailureCutIdentity, PersistentFailureRecoveryInventory,
        coordinator::PersistentFailureRecoveryDrain,
    },
};

use super::super::{
    PendingProjectionGroupIdentity, PendingProjectionWitness,
    PersistentFailurePendingProjectionQuarantineReason,
};

pub(super) struct CandidatePreflight {
    pub(super) identity: PendingProjectionGroupIdentity,
    pub(super) witness: PendingProjectionWitness,
}

pub(super) struct RecoveryPreflight {
    pub(super) connections: Vec<Arc<ProjectionConnection>>,
    pub(super) connection_scope: Vec<ProjectionConnectionIdentityObservation>,
    pub(super) candidates: Vec<CandidatePreflight>,
    pub(super) target_guards: Vec<Vec<PersistentFailureTargetGuardObservation>>,
    pub(super) expected_registry: Vec<LoadedRegistryRecoveryObservation>,
    pub(super) retained_candidates: Vec<LoadedRegistryRecoveryObservation>,
    pub(super) duplicate_connection_count: usize,
}

pub(super) fn preflight_recovery_topology(
    inventory: &PersistentFailureRecoveryInventory,
    identity: PersistentFailureCutIdentity,
    drain: &PersistentFailureRecoveryDrain,
) -> Result<RecoveryPreflight, PersistentFailurePendingProjectionQuarantineReason> {
    let (connections, duplicate_connection_count) =
        canonicalize_connections(&drain.retained_connections)?;
    let (retained_service_connections, _) =
        canonicalize_connections(&inventory.retained_service_connections())?;
    let current_service_connections = inventory
        .current_service_connections()
        .map_err(|()| PersistentFailurePendingProjectionQuarantineReason::RetentionUnavailable)?;
    let (current_service_connections, _) = canonicalize_connections(&current_service_connections)?;
    if !same_connection_set(&connections, &retained_service_connections)
        || !same_connection_set(&connections, &current_service_connections)
    {
        return Err(PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch);
    }
    let connection_scope = connections
        .iter()
        .map(|connection| connection.identity_observation())
        .collect::<Vec<_>>();
    let mut expected_registry = Vec::new();
    let mut retained_candidates = Vec::new();
    let candidates = preflight_candidates(
        identity,
        &connections,
        &drain.retained_projections,
        &mut expected_registry,
        &mut retained_candidates,
    )?;
    let target_guards = preflight_target_results(identity, &connections, drain)?;
    preflight_target_projections(identity, &connections, drain, &mut expected_registry)?;
    preflight_reacquisition_anchors(
        identity,
        &connections,
        &drain.retained_reacquisition_anchors,
        &mut expected_registry,
    )?;
    preflight_raw_registry_owners(identity, &connections, drain, &mut expected_registry)?;
    preflight_connection_barriers(identity, &connections, drain)?;
    validate_registry_scope(&connection_scope, &expected_registry)?;
    let expected = unique_registry_observations(&expected_registry)?;
    let audit = registry::recovery_audit(&connection_scope).map_err(audit_error_reason)?;
    let actual = audit.observations().iter().cloned().collect::<HashSet<_>>();
    if actual.len() != audit.observations().len()
        || actual.len() != expected.len()
        || actual != expected
    {
        return Err(PersistentFailurePendingProjectionQuarantineReason::MissingSiblingToken);
    }
    Ok(RecoveryPreflight {
        connections,
        connection_scope,
        candidates,
        target_guards,
        expected_registry,
        retained_candidates,
        duplicate_connection_count,
    })
}

fn canonicalize_connections(
    offered: &[Arc<ProjectionConnection>],
) -> Result<
    (Vec<Arc<ProjectionConnection>>, usize),
    PersistentFailurePendingProjectionQuarantineReason,
> {
    let mut connections: Vec<Arc<ProjectionConnection>> = Vec::with_capacity(offered.len());
    let mut duplicate_count = 0usize;
    for connection in offered {
        let identity = connection.identity_observation();
        if let Some(existing) = connections.iter().find(|existing| {
            existing.identity_observation().connection_generation()
                == identity.connection_generation()
        }) {
            if !Arc::ptr_eq(existing, connection) || existing.identity_observation() != identity {
                return Err(
                    PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch,
                );
            }
            duplicate_count = duplicate_count
                .checked_add(1)
                .expect("bounded duplicate retained connection count fits in memory");
        } else {
            connections.push(Arc::clone(connection));
        }
    }
    connections.sort_unstable_by_key(|connection| {
        connection.identity_observation().connection_generation()
    });
    Ok((connections, duplicate_count))
}

fn same_connection_set(
    left: &[Arc<ProjectionConnection>],
    right: &[Arc<ProjectionConnection>],
) -> bool {
    left.len() == right.len()
        && left.iter().all(|connection| {
            right.iter().any(|candidate| {
                Arc::ptr_eq(connection, candidate)
                    && connection.identity_observation() == candidate.identity_observation()
            })
        })
}

fn preflight_candidates(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    projections: &[LoadedCasProjection],
    expected_registry: &mut Vec<LoadedRegistryRecoveryObservation>,
    retained_candidates: &mut Vec<LoadedRegistryRecoveryObservation>,
) -> Result<Vec<CandidatePreflight>, PersistentFailurePendingProjectionQuarantineReason> {
    let mut witnesses = HashMap::<PendingProjectionGroupIdentity, PendingProjectionWitness>::new();
    let mut candidates = Vec::with_capacity(projections.len());
    for projection in projections {
        let observation = projection.recovery_observation();
        validate_candidate_projection(identity, connections, projection, &observation)?;
        let registry = observation.registry().clone();
        let group_identity = PendingProjectionGroupIdentity {
            connection: observation.connection().clone(),
            key: registry.key().clone(),
            owner: registry.owner(),
            loaded_generation: registry.loaded_generation(),
        };
        let witness = projection_witness(projection);
        if let Some(existing) = witnesses.get(&group_identity) {
            if existing != &witness {
                return Err(
                    PersistentFailurePendingProjectionQuarantineReason::WitnessDisagreement,
                );
            }
        } else {
            witnesses.insert(group_identity.clone(), witness.clone());
        }
        expected_registry.push(registry.clone());
        retained_candidates.push(registry);
        candidates.push(CandidatePreflight {
            identity: group_identity,
            witness,
        });
    }
    Ok(candidates)
}

fn validate_candidate_projection(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    projection: &LoadedCasProjection,
    observation: &LoadedLeaseRecoveryObservation,
) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
    let registry = observation.registry();
    if projection.home_id() != identity.home_id
        || projection.home_generation() != identity.home_generation
        || projection.syndic_thread_id() != registry.owner()
        || projection.cas_thread_id() != &registry.key().cas_thread_id
        || projection.loaded_session_generation() != registry.loaded_generation()
        || projection.execution_binding().runtime_id() != registry.key().runtime_id
        || !observation.is_active_candidate_for(identity)
        || matching_stable_connection(connections, observation).is_none()
    {
        return Err(PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch);
    }
    Ok(())
}

fn projection_witness(projection: &LoadedCasProjection) -> PendingProjectionWitness {
    PendingProjectionWitness {
        home_id: projection.home_id(),
        home_generation: projection.home_generation(),
        syndic_thread_id: projection.syndic_thread_id(),
        binding_revision: projection.binding_revision(),
        execution_binding: projection.execution_binding().clone(),
        cas_thread_id: projection.cas_thread_id().clone(),
        loaded_session_generation: projection.loaded_session_generation(),
        lineage_proof: projection.lineage_proof(),
    }
}

fn preflight_target_results(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    drain: &PersistentFailureRecoveryDrain,
) -> Result<
    Vec<Vec<PersistentFailureTargetGuardObservation>>,
    PersistentFailurePendingProjectionQuarantineReason,
> {
    let mut batches = vec![Vec::new(); connections.len()];
    for target in &drain.retained_results {
        if target.witness.cut_identity() != identity {
            return Err(PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch);
        }
        let Some((index, connection)) =
            connection_for_identity(connections, target.witness.connection())
        else {
            return Err(
                PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch,
            );
        };
        let observation = target.witness.observe_guard(connection).map_err(|_| {
            PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch
        })?;
        if !target_result_matches_guard(target.result, observation.disposition()) {
            return Err(
                PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch,
            );
        }
        batches[index].push(observation);
    }
    for (connection, observations) in connections.iter().zip(&batches) {
        connection
            .validate_persistent_failure_target_guard_topology(identity, observations)
            .map_err(|_| {
                PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch
            })?;
    }
    Ok(batches)
}

fn target_result_matches_guard(
    result: PersistentFailureDriverResult,
    guard: PersistentFailureTargetGuardDisposition,
) -> bool {
    match result {
        PersistentFailureDriverResult::NoDispatch(
            PersistentFailureNoDispatchReason::DriverUnavailable,
        ) => true,
        PersistentFailureDriverResult::NoDispatch(PersistentFailureNoDispatchReason::Router(_)) => {
            guard == PersistentFailureTargetGuardDisposition::Frozen
        }
        PersistentFailureDriverResult::NoDispatch(
            PersistentFailureNoDispatchReason::RandomUnavailable
            | PersistentFailureNoDispatchReason::BindRejected
            | PersistentFailureNoDispatchReason::AuthorizationRejected,
        )
        | PersistentFailureDriverResult::Attempted { .. } => {
            guard == PersistentFailureTargetGuardDisposition::Spent
        }
    }
}

fn preflight_target_projections(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    drain: &PersistentFailureRecoveryDrain,
    expected_registry: &mut Vec<LoadedRegistryRecoveryObservation>,
) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
    let mut paired_results = vec![false; drain.retained_results.len()];
    for projection in &drain.retained_target_projections {
        let observation = projection.recovery_observation();
        let registry = observation.registry();
        if projection.home_id() != identity.home_id
            || projection.home_generation() != identity.home_generation
            || projection.syndic_thread_id() != registry.owner()
            || projection.cas_thread_id() != &registry.key().cas_thread_id
            || projection.loaded_session_generation() != registry.loaded_generation()
            || projection.execution_binding().runtime_id() != registry.key().runtime_id
            || !is_exact_target_observation(identity, &observation)
            || matching_stable_connection(connections, &observation).is_none()
        {
            return Err(
                PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch,
            );
        }
        let mut matched = None;
        for (index, target) in drain.retained_results.iter().enumerate() {
            if target.witness.connection() == observation.connection().identity()
                && target.witness.syndic_thread_id() == registry.owner()
                && target.witness.cas_thread_id() == &registry.key().cas_thread_id
                && target.witness.loaded_generation() == registry.loaded_generation()
            {
                if matched.is_some() || paired_results[index] {
                    return Err(
                        PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch,
                    );
                }
                matched = Some(index);
            }
        }
        let Some(index) = matched else {
            return Err(
                PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch,
            );
        };
        paired_results[index] = true;
        expected_registry.push(registry.clone());
    }
    Ok(())
}

fn is_exact_target_observation(
    identity: PersistentFailureCutIdentity,
    observation: &LoadedLeaseRecoveryObservation,
) -> bool {
    observation.is_active()
        && observation.retained_cut_identity().is_none()
        && observation.is_exact_for_connection()
        && observation.registry().authority().kind()
            == LoadedRegistryRecoveryAuthorityKind::ActiveLease
        && observation
            .surrender_cut_identity()
            .map_or(true, |surrender| surrender == identity)
}

fn preflight_reacquisition_anchors(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    anchors: &[SameNativeReacquisitionAnchor],
    expected_registry: &mut Vec<LoadedRegistryRecoveryObservation>,
) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
    for anchor in anchors {
        let observation = anchor.recovery_observation();
        let registry = observation.registry();
        if anchor.home_id() != identity.home_id
            || anchor.home_generation() != identity.home_generation
            || anchor.syndic_thread_id() != registry.owner()
            || anchor.cas_thread_id() != &registry.key().cas_thread_id
            || anchor.execution_binding().runtime_id() != registry.key().runtime_id
            || !observation.is_active()
            || observation.retained_cut_identity().is_some()
            || observation.surrender_cut_identity() != Some(identity)
            || !observation.is_exact_for_connection()
            || registry.authority().kind() != LoadedRegistryRecoveryAuthorityKind::QuarantinedAnchor
            || matching_stable_connection(connections, &observation).is_none()
        {
            return Err(
                PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch,
            );
        }
        expected_registry.push(registry.clone());
    }
    Ok(())
}

fn preflight_raw_registry_owners(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    drain: &PersistentFailureRecoveryDrain,
    expected_registry: &mut Vec<LoadedRegistryRecoveryObservation>,
) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
    for retained in &drain.retained_raw_loaded_leases {
        let observation = retained.recovery_observation();
        validate_raw_observation(
            identity,
            connections,
            &observation,
            LoadedRegistryRecoveryAuthorityKind::ActiveLease,
            true,
        )?;
        expected_registry.push(observation.registry().clone());
    }
    for retained in &drain.retained_raw_quarantined_anchors {
        let observation = retained.recovery_observation();
        validate_raw_observation(
            identity,
            connections,
            &observation,
            LoadedRegistryRecoveryAuthorityKind::QuarantinedAnchor,
            true,
        )?;
        expected_registry.push(observation.registry().clone());
    }
    for retained in &drain.retained_raw_reacquisition_reservations {
        let observation = retained.recovery_observation();
        validate_raw_observation(
            identity,
            connections,
            &observation,
            LoadedRegistryRecoveryAuthorityKind::ReacquisitionReservation,
            false,
        )?;
        expected_registry.push(observation.registry().clone());
    }
    Ok(())
}

fn validate_raw_observation(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    observation: &LoadedLeaseRecoveryObservation,
    kind: LoadedRegistryRecoveryAuthorityKind,
    surrender_required: bool,
) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
    let surrender_matches = if surrender_required {
        observation.surrender_cut_identity() == Some(identity)
    } else {
        observation
            .surrender_cut_identity()
            .map_or(true, |surrender| surrender == identity)
    };
    if !observation.is_active()
        || observation.retained_cut_identity() != Some(identity)
        || observation.registry().authority().kind() != kind
        || !observation.is_exact_for_connection()
        || !surrender_matches
        || matching_stable_connection(connections, observation).is_none()
    {
        return Err(PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch);
    }
    Ok(())
}

fn preflight_connection_barriers(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    drain: &PersistentFailureRecoveryDrain,
) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
    let mut promotion_counts = vec![0usize; connections.len()];
    let mut cleanup_counts = vec![0usize; connections.len()];
    for retained in &drain.retained_promotion_reservations {
        let index = preflight_promotion_barrier(identity, connections, retained)?;
        promotion_counts[index] = promotion_counts[index]
            .checked_add(1)
            .expect("bounded retained promotion count fits in memory");
    }
    for retained in &drain.retained_cleanup_owners {
        let index = preflight_cleanup_barrier(identity, connections, retained)?;
        cleanup_counts[index] = cleanup_counts[index]
            .checked_add(1)
            .expect("bounded retained cleanup count fits in memory");
    }
    for (index, connection) in connections.iter().enumerate() {
        connection
            .validate_failure_retained_barrier_topology(
                identity,
                promotion_counts[index],
                cleanup_counts[index],
            )
            .map_err(|_| {
                PersistentFailurePendingProjectionQuarantineReason::BarrierDispositionMismatch
            })?;
    }
    Ok(())
}

fn preflight_promotion_barrier(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    retained: &FailureRetainedPromotionReservation,
) -> Result<usize, PersistentFailurePendingProjectionQuarantineReason> {
    let witness = retained
        .observe_for_recovery(identity)
        .map_err(|_| {
            PersistentFailurePendingProjectionQuarantineReason::BarrierDispositionMismatch
        })?
        .ok_or(PersistentFailurePendingProjectionQuarantineReason::BarrierDispositionMismatch)?;
    connection_for_identity(connections, witness.connection())
        .map(|(index, _)| index)
        .ok_or(PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch)
}

fn preflight_cleanup_barrier(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    retained: &FailureRetainedCleanupOwner,
) -> Result<usize, PersistentFailurePendingProjectionQuarantineReason> {
    let witness = retained
        .observe_for_recovery(identity)
        .map_err(|_| {
            PersistentFailurePendingProjectionQuarantineReason::BarrierDispositionMismatch
        })?
        .ok_or(PersistentFailurePendingProjectionQuarantineReason::BarrierDispositionMismatch)?;
    connection_for_identity(connections, witness.connection())
        .map(|(index, _)| index)
        .ok_or(PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch)
}

fn matching_stable_connection<'a>(
    connections: &'a [Arc<ProjectionConnection>],
    observation: &LoadedLeaseRecoveryObservation,
) -> Option<&'a Arc<ProjectionConnection>> {
    connections
        .iter()
        .find(|connection| observation.connection().matches_connection(connection))
}

fn connection_for_identity(
    connections: &[Arc<ProjectionConnection>],
    identity: ProjectionConnectionIdentityObservation,
) -> Option<(usize, &Arc<ProjectionConnection>)> {
    connections
        .iter()
        .enumerate()
        .find(|(_, connection)| connection.identity_observation() == identity)
}

fn validate_registry_scope(
    scope: &[ProjectionConnectionIdentityObservation],
    observations: &[LoadedRegistryRecoveryObservation],
) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
    for observation in observations {
        let Some(connection) = scope.iter().find(|connection| {
            connection.connection_generation() == observation.connection_generation().get()
        }) else {
            return Err(
                PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch,
            );
        };
        if connection.runtime_id() != observation.key().runtime_id
            || connection.process_generation() != observation.key().process_generation
            || observation.loaded_generation().process() != observation.key().process_generation
        {
            return Err(
                PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch,
            );
        }
        if let Some(anchor) = observation.authority().anchor_connection()
            && !scope.iter().any(|connection| {
                connection.connection_generation() == anchor.get()
                    && connection.runtime_id() == observation.key().runtime_id
                    && connection.process_generation() == observation.key().process_generation
            })
        {
            return Err(
                PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch,
            );
        }
    }
    Ok(())
}

pub(super) fn unique_registry_observations(
    observations: &[LoadedRegistryRecoveryObservation],
) -> Result<
    HashSet<LoadedRegistryRecoveryObservation>,
    PersistentFailurePendingProjectionQuarantineReason,
> {
    let mut tokens = HashSet::with_capacity(observations.len());
    let mut unique = HashSet::with_capacity(observations.len());
    for observation in observations {
        if !tokens.insert(observation.authority().token()) || !unique.insert(observation.clone()) {
            return Err(PersistentFailurePendingProjectionQuarantineReason::DuplicateToken);
        }
    }
    Ok(unique)
}

pub(super) fn audit_error_reason(
    error: LoadedRegistryRecoveryAuditError,
) -> PersistentFailurePendingProjectionQuarantineReason {
    match error {
        LoadedRegistryRecoveryAuditError::Registry(_) => {
            PersistentFailurePendingProjectionQuarantineReason::RegistryUnavailable
        }
        LoadedRegistryRecoveryAuditError::ConflictingConnectionIdentity { .. }
        | LoadedRegistryRecoveryAuditError::ConnectionIdentityMismatch { .. } => {
            PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch
        }
    }
}
