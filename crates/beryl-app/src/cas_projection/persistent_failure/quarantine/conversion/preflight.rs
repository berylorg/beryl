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

mod helpers;
use helpers::*;

pub(super) use helpers::{audit_error_reason, unique_registry_observations};
