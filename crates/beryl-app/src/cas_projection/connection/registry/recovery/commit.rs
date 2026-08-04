use std::collections::{HashMap, HashSet};

use crate::cas_projection::connection::lifecycle::ProjectionConnectionIdentityObservation;

use super::{
    super::{ConnectionGeneration, LoadedThreadState, lock},
    audit::{LoadedRegistryRecoveryAuditError, audit_locked, normalize_scope},
    model::{
        LoadedRegistryRecoveryAuthorityKind, LoadedRegistryRecoveryObservation,
        LoadedRegistryRecoveryToken,
    },
};

/// Failure before an expected recovery topology can commit any local disposition.
#[derive(Debug)]
pub(in crate::cas_projection) enum LoadedRegistryRecoveryCommitError {
    Audit(LoadedRegistryRecoveryAuditError),
    DuplicateExpectedAuthority,
    DuplicateRetainedCandidate,
    RetainedCandidateNotActive,
    RetainedCandidateNotExpected,
    ExpectedTopologyMismatch,
    ConnectionAuthorityCountMismatch,
}

/// Atomically keeps the exact active candidate subset and removes every other expected authority.
///
/// All topology, token, identity, and authority-count checks finish before the first mutation.
pub(in crate::cas_projection) fn commit_recovery_topology(
    connections: &[ProjectionConnectionIdentityObservation],
    complete_expected: &[LoadedRegistryRecoveryObservation],
    retained_candidates: &[LoadedRegistryRecoveryObservation],
) -> Result<(), LoadedRegistryRecoveryCommitError> {
    let requested_connections =
        normalize_scope(connections).map_err(LoadedRegistryRecoveryCommitError::Audit)?;
    let mut state = lock().map_err(|error| {
        LoadedRegistryRecoveryCommitError::Audit(LoadedRegistryRecoveryAuditError::Registry(error))
    })?;
    let audit = audit_locked(&state, requested_connections)
        .map_err(LoadedRegistryRecoveryCommitError::Audit)?;

    let expected = unique_observations(
        complete_expected,
        LoadedRegistryRecoveryCommitError::DuplicateExpectedAuthority,
    )?;
    let candidates = unique_observations(
        retained_candidates,
        LoadedRegistryRecoveryCommitError::DuplicateRetainedCandidate,
    )?;
    if retained_candidates.iter().any(|candidate| {
        candidate.authority().kind() != LoadedRegistryRecoveryAuthorityKind::ActiveLease
    }) {
        return Err(LoadedRegistryRecoveryCommitError::RetainedCandidateNotActive);
    }
    if candidates
        .iter()
        .any(|candidate| !expected.contains(candidate))
    {
        return Err(LoadedRegistryRecoveryCommitError::RetainedCandidateNotExpected);
    }

    let actual = audit.observations().iter().cloned().collect::<HashSet<_>>();
    if expected.len() != audit.observations().len() || expected != actual {
        return Err(LoadedRegistryRecoveryCommitError::ExpectedTopologyMismatch);
    }
    if !connection_authority_counts_are_exact(&state) {
        return Err(LoadedRegistryRecoveryCommitError::ConnectionAuthorityCountMismatch);
    }

    for observation in audit.observations() {
        if !candidates.contains(observation) {
            let removed = super::local::dispose_observation_locked(&mut state, observation);
            debug_assert!(removed, "preflighted recovery authority remains exact");
        }
    }
    Ok(())
}

fn unique_observations(
    observations: &[LoadedRegistryRecoveryObservation],
    duplicate: LoadedRegistryRecoveryCommitError,
) -> Result<HashSet<LoadedRegistryRecoveryObservation>, LoadedRegistryRecoveryCommitError> {
    let mut tokens = HashSet::<LoadedRegistryRecoveryToken>::with_capacity(observations.len());
    let mut unique = HashSet::with_capacity(observations.len());
    for observation in observations {
        if !tokens.insert(observation.authority().token()) || !unique.insert(observation.clone()) {
            return Err(duplicate);
        }
    }
    Ok(unique)
}

fn connection_authority_counts_are_exact(state: &LoadedThreadState) -> bool {
    let mut exact = HashMap::<ConnectionGeneration, usize>::new();
    for entry in state.entries.values() {
        *exact.entry(entry.connection).or_default() += 1;
    }
    for replacement in state.reacquisition_reservations.keys() {
        *exact.entry(*replacement).or_default() += 1;
    }
    exact == state.connection_authority_counts
}
