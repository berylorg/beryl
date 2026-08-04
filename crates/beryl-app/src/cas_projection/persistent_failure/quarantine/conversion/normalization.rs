use std::sync::Arc;

use crate::cas_projection::{
    connection::{
        LoadedLeaseRecoveryObservation, LocalLoadedRegistryDispositionOwner,
        PendingProjectionConnectionOwner, PendingProjectionConnectionOwnerInstallFailure,
        PersistentFailureTargetGuardObservation, ProjectionConnection,
        registry::{self, LoadedRegistryRecoveryCommitError, LoadedRegistryRecoveryObservation},
    },
    persistent_failure::{
        PersistentFailureCutIdentity, coordinator::PersistentFailureRecoveryDrain,
    },
};

use super::{
    super::{
        PendingProjectionCandidateGroup, PendingProjectionGroupIdentity,
        PendingProjectionQuarantineAuthority, PendingProjectionQuarantineOwnedTopology,
        PersistentFailurePendingProjectionQuarantineReason,
    },
    preflight::{RecoveryPreflight, audit_error_reason, unique_registry_observations},
};

#[cfg(test)]
fn connection_owner_install_observers() -> &'static std::sync::Mutex<
    std::collections::HashMap<
        PersistentFailureCutIdentity,
        (
            std::sync::mpsc::SyncSender<()>,
            std::sync::mpsc::Receiver<()>,
        ),
    >,
> {
    static OBSERVERS: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<
                PersistentFailureCutIdentity,
                (
                    std::sync::mpsc::SyncSender<()>,
                    std::sync::mpsc::Receiver<()>,
                ),
            >,
        >,
    > = std::sync::OnceLock::new();
    OBSERVERS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[cfg(test)]
pub(super) fn observe_connection_owner_install_for_test(
    identity: PersistentFailureCutIdentity,
    reached: std::sync::mpsc::SyncSender<()>,
    resume: std::sync::mpsc::Receiver<()>,
) {
    let replaced = connection_owner_install_observers()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(identity, (reached, resume));
    assert!(replaced.is_none());
}

#[cfg(test)]
fn pause_after_connection_owner_install_for_test(identity: PersistentFailureCutIdentity) {
    let observer = connection_owner_install_observers()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&identity);
    if let Some((reached, resume)) = observer {
        let _ = reached.send(());
        let _ = resume.recv();
    }
}

pub(super) fn normalize_recovery_topology(
    identity: PersistentFailureCutIdentity,
    mut drain: PersistentFailureRecoveryDrain,
    preflight: RecoveryPreflight,
) -> Result<
    PendingProjectionQuarantineAuthority,
    (
        PendingProjectionQuarantineAuthority,
        PersistentFailurePendingProjectionQuarantineReason,
    ),
> {
    drain.retained_connections = preflight.connections.clone();
    let mut settled_disposition_count = preflight.duplicate_connection_count;
    let mut groups = Vec::<PendingProjectionCandidateGroup>::new();
    let mut connection_owners = Vec::<PendingProjectionConnectionOwner>::new();
    let mut pending_local_dispositions = Vec::<LocalLoadedRegistryDispositionOwner>::new();
    let mut reason = None;

    if drain.retained_projections.len() != preflight.candidates.len() {
        let reason = PersistentFailurePendingProjectionQuarantineReason::WitnessDisagreement;
        return Err((
            normalized_authority(
                groups,
                connection_owners,
                drain,
                pending_local_dispositions,
                settled_disposition_count,
                Some(reason),
            ),
            reason,
        ));
    }
    let projections = std::mem::take(&mut drain.retained_projections);
    for (projection, candidate) in projections.into_iter().zip(preflight.candidates) {
        let owner = projection.into_pending_projection_lease_owner();
        if !owner.is_exact_candidate_for(identity)
            || group_identity_from_observation(owner.observation()) != candidate.identity
        {
            reason.get_or_insert(
                PersistentFailurePendingProjectionQuarantineReason::WitnessDisagreement,
            );
        }
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.identity == candidate.identity)
        {
            if group.witness != candidate.witness {
                reason.get_or_insert(
                    PersistentFailurePendingProjectionQuarantineReason::WitnessDisagreement,
                );
            }
            group.candidates.push(owner);
        } else {
            groups.push(PendingProjectionCandidateGroup {
                identity: candidate.identity,
                witness: candidate.witness,
                candidates: vec![owner],
            });
        }
    }

    for projection in std::mem::take(&mut drain.retained_target_projections) {
        let owner = projection.into_local_registry_disposition_owner();
        if !owner.is_exact_target_disposition_for(identity) {
            reason.get_or_insert(
                PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch,
            );
        }
        pending_local_dispositions.push(owner);
    }
    if reason.is_none() {
        let anchors = std::mem::take(&mut drain.retained_reacquisition_anchors);
        let mut anchors = anchors.into_iter();
        while let Some(anchor) = anchors.next() {
            match anchor.into_local_registry_disposition_owner() {
                Ok(owner) => {
                    if !owner.is_exact_same_native_disposition_for(identity) {
                        reason.get_or_insert(
                            PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch,
                        );
                    }
                    pending_local_dispositions.push(owner);
                }
                Err(anchor) => {
                    drain.retained_reacquisition_anchors.push(anchor);
                    drain.retained_reacquisition_anchors.extend(anchors);
                    reason = Some(
                        PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch,
                    );
                    break;
                }
            }
        }
    }
    if reason.is_none() {
        for retained in std::mem::take(&mut drain.retained_raw_loaded_leases) {
            let owner = retained.into_local_registry_disposition_owner();
            if !owner.is_exact_for_cut(identity) {
                reason.get_or_insert(
                    PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch,
                );
            }
            pending_local_dispositions.push(owner);
        }
        for retained in std::mem::take(&mut drain.retained_raw_quarantined_anchors) {
            let owner = retained.into_local_registry_disposition_owner();
            if !owner.is_exact_for_cut(identity) {
                reason.get_or_insert(
                    PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch,
                );
            }
            pending_local_dispositions.push(owner);
        }
        for retained in std::mem::take(&mut drain.retained_raw_reacquisition_reservations) {
            let owner = retained.into_local_registry_disposition_owner();
            if !owner.is_exact_for_cut(identity) {
                reason.get_or_insert(
                    PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch,
                );
            }
            pending_local_dispositions.push(owner);
        }
    }

    if let Some(reason) = reason {
        return Err((
            normalized_authority(
                groups,
                connection_owners,
                drain,
                pending_local_dispositions,
                settled_disposition_count,
                Some(reason),
            ),
            reason,
        ));
    }

    let actual_candidates = groups
        .iter()
        .flat_map(|group| {
            group
                .candidates
                .iter()
                .map(|candidate| candidate.registry_observation().clone())
        })
        .collect::<Vec<_>>();
    let actual_local = pending_local_dispositions
        .iter()
        .map(|owner| owner.registry_observation().clone())
        .collect::<Vec<_>>();
    let mut actual_expected = actual_candidates.clone();
    actual_expected.extend(actual_local);
    if same_registry_set(&actual_candidates, &preflight.retained_candidates).is_err()
        || same_registry_set(&actual_expected, &preflight.expected_registry).is_err()
    {
        let reason = PersistentFailurePendingProjectionQuarantineReason::DuplicateToken;
        return Err((
            normalized_authority(
                groups,
                connection_owners,
                drain,
                pending_local_dispositions,
                settled_disposition_count,
                Some(reason),
            ),
            reason,
        ));
    }

    if let Err(reason) = install_connection_owners(
        identity,
        &preflight.connections,
        &mut drain,
        &mut connection_owners,
        &mut settled_disposition_count,
    ) {
        return Err((
            normalized_authority(
                groups,
                connection_owners,
                drain,
                pending_local_dispositions,
                settled_disposition_count,
                Some(reason),
            ),
            reason,
        ));
    }
    #[cfg(test)]
    pause_after_connection_owner_install_for_test(identity);

    if let Err(error) = registry::commit_recovery_topology(
        &preflight.connection_scope,
        &preflight.expected_registry,
        &preflight.retained_candidates,
    ) {
        let reason = commit_error_reason(error);
        return Err((
            normalized_authority(
                groups,
                connection_owners,
                drain,
                pending_local_dispositions,
                settled_disposition_count,
                Some(reason),
            ),
            reason,
        ));
    }
    settled_disposition_count = settled_disposition_count
        .checked_add(pending_local_dispositions.len())
        .expect("bounded settled registry disposition count fits in memory");
    for owner in pending_local_dispositions.drain(..) {
        owner.finish_after_registry_commit();
    }

    if settle_target_guards(identity, &preflight.connections, &preflight.target_guards).is_err() {
        let reason = PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch;
        return Err((
            normalized_authority(
                groups,
                connection_owners,
                drain,
                pending_local_dispositions,
                settled_disposition_count,
                Some(reason),
            ),
            reason,
        ));
    }
    Ok(normalized_authority(
        groups,
        connection_owners,
        drain,
        pending_local_dispositions,
        settled_disposition_count,
        None,
    ))
}

fn group_identity_from_observation(
    observation: &LoadedLeaseRecoveryObservation,
) -> PendingProjectionGroupIdentity {
    PendingProjectionGroupIdentity {
        connection: observation.connection().clone(),
        key: observation.registry().key().clone(),
        owner: observation.registry().owner(),
        loaded_generation: observation.registry().loaded_generation(),
    }
}

fn same_registry_set(
    left: &[LoadedRegistryRecoveryObservation],
    right: &[LoadedRegistryRecoveryObservation],
) -> Result<(), ()> {
    let left = unique_registry_observations(left).map_err(|_| ())?;
    let right = unique_registry_observations(right).map_err(|_| ())?;
    if left == right { Ok(()) } else { Err(()) }
}

fn commit_error_reason(
    error: LoadedRegistryRecoveryCommitError,
) -> PersistentFailurePendingProjectionQuarantineReason {
    match error {
        LoadedRegistryRecoveryCommitError::Audit(error) => audit_error_reason(error),
        LoadedRegistryRecoveryCommitError::DuplicateExpectedAuthority
        | LoadedRegistryRecoveryCommitError::DuplicateRetainedCandidate => {
            PersistentFailurePendingProjectionQuarantineReason::DuplicateToken
        }
        LoadedRegistryRecoveryCommitError::RetainedCandidateNotActive
        | LoadedRegistryRecoveryCommitError::RetainedCandidateNotExpected => {
            PersistentFailurePendingProjectionQuarantineReason::WitnessDisagreement
        }
        LoadedRegistryRecoveryCommitError::ExpectedTopologyMismatch => {
            PersistentFailurePendingProjectionQuarantineReason::MissingSiblingToken
        }
        LoadedRegistryRecoveryCommitError::ConnectionAuthorityCountMismatch => {
            PersistentFailurePendingProjectionQuarantineReason::RegistryUnavailable
        }
    }
}

fn settle_target_guards(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    batches: &[Vec<PersistentFailureTargetGuardObservation>],
) -> Result<(), ()> {
    if connections.len() != batches.len() {
        return Err(());
    }
    for (connection, observations) in connections.iter().zip(batches) {
        connection
            .settle_persistent_failure_target_guards(identity, observations)
            .map_err(|_| ())?;
    }
    Ok(())
}

fn install_connection_owners(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
    drain: &mut PersistentFailureRecoveryDrain,
    connection_owners: &mut Vec<PendingProjectionConnectionOwner>,
    settled_disposition_count: &mut usize,
) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
    for connection in connections {
        let (promotions, remaining_promotions) =
            std::mem::take(&mut drain.retained_promotion_reservations)
                .into_iter()
                .partition(|owner| owner.matches_connection(connection));
        drain.retained_promotion_reservations = remaining_promotions;
        let (cleanup, remaining_cleanup) = std::mem::take(&mut drain.retained_cleanup_owners)
            .into_iter()
            .partition(|owner| owner.matches_connection(connection));
        drain.retained_cleanup_owners = remaining_cleanup;
        let settled = promotions
            .len()
            .checked_add(cleanup.len())
            .expect("bounded connection barrier count fits in memory");
        match connection.install_pending_projection_quarantine_owner(identity, promotions, cleanup)
        {
            Ok(owner) => {
                *settled_disposition_count = settled_disposition_count
                    .checked_add(settled)
                    .expect("bounded barrier disposition count fits in memory");
                connection_owners.push(owner);
            }
            Err(error) => {
                let (failure, promotions, cleanup) = error.into_parts();
                drain.retained_promotion_reservations.extend(promotions);
                drain.retained_cleanup_owners.extend(cleanup);
                return Err(match failure {
                    PendingProjectionConnectionOwnerInstallFailure::AuthorityUnavailable => {
                        PersistentFailurePendingProjectionQuarantineReason::ConnectionUnavailable
                    }
                    PendingProjectionConnectionOwnerInstallFailure::TopologyMismatch => {
                        PersistentFailurePendingProjectionQuarantineReason::BarrierDispositionMismatch
                    }
                });
            }
        }
    }
    if drain.retained_promotion_reservations.is_empty() && drain.retained_cleanup_owners.is_empty()
    {
        Ok(())
    } else {
        Err(PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch)
    }
}

fn normalized_authority(
    groups: Vec<PendingProjectionCandidateGroup>,
    connection_owners: Vec<PendingProjectionConnectionOwner>,
    remainder: PersistentFailureRecoveryDrain,
    pending_local_dispositions: Vec<LocalLoadedRegistryDispositionOwner>,
    settled_disposition_count: usize,
    reason: Option<PersistentFailurePendingProjectionQuarantineReason>,
) -> PendingProjectionQuarantineAuthority {
    PendingProjectionQuarantineAuthority {
        topology: PendingProjectionQuarantineOwnedTopology::Normalized {
            groups,
            connection_owners,
            remainder,
            pending_local_dispositions,
            settled_disposition_count,
        },
        reason,
    }
}
