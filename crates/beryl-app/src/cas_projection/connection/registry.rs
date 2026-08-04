use std::{
    collections::{HashMap, HashSet},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    RuntimeId, SyndicThreadId,
};

use crate::cas_projection::{ProjectionCoordinatorError, ProjectionRegistryKind};

mod recovery;

pub(in crate::cas_projection) use recovery::{
    LoadedRegistryRecoveryAudit, LoadedRegistryRecoveryAuditError, LoadedRegistryRecoveryAuthority,
    LoadedRegistryRecoveryAuthorityKind, LoadedRegistryRecoveryCommitError,
    LoadedRegistryRecoveryObservation, LoadedRegistryRecoveryToken,
    LoadedRegistryRecoveryTokenKind, authenticate_recovery_observation,
    authenticate_recovery_observations, commit_recovery_topology, recovery_audit,
    settle_recovery_observation_locally,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct ConnectionGeneration(u64);

impl ConnectionGeneration {
    pub(in crate::cas_projection) const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct LeaseToken(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct ReacquisitionAnchorToken(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct ReacquisitionReservationToken(u64);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct LoadedThreadKey {
    pub(in crate::cas_projection) runtime_id: RuntimeId,
    pub(in crate::cas_projection) process_generation: CasProcessGeneration,
    pub(in crate::cas_projection) cas_thread_id: CasThreadId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ExistingSubscription {
    Absent,
    Exact {
        generation: CasLoadedSessionGeneration,
        token: LeaseToken,
    },
    AnotherConnection,
    AnotherOwner {
        existing_owner: SyndicThreadId,
    },
    Quarantined,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ObservedSubscription {
    Absent,
    Exact(CasLoadedSessionGeneration),
    AnotherConnection,
    AnotherOwner { existing_owner: SyndicThreadId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ReleaseDisposition {
    Stale,
    Shared,
    Last,
}

#[derive(Debug)]
struct LoadedThreadEntry {
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedThreadGeneration,
    state: LoadedThreadEntryState,
}

#[derive(Debug)]
enum LoadedThreadEntryState {
    Active { leases: HashSet<LeaseToken> },
    Quarantined { token: ReacquisitionAnchorToken },
}

#[derive(Debug)]
struct ReacquisitionReservationEntry {
    key: LoadedThreadKey,
    anchor_connection: ConnectionGeneration,
    owner: SyndicThreadId,
    anchor_generation: CasLoadedThreadGeneration,
    anchor_token: ReacquisitionAnchorToken,
    token: ReacquisitionReservationToken,
}

#[derive(Default)]
struct LoadedThreadState {
    entries: HashMap<LoadedThreadKey, LoadedThreadEntry>,
    reacquisition_reservations: HashMap<ConnectionGeneration, ReacquisitionReservationEntry>,
    connection_authority_counts: HashMap<ConnectionGeneration, usize>,
}

static CONNECTION_GENERATIONS: AtomicU64 = AtomicU64::new(0);
static LOADED_GENERATIONS: AtomicU64 = AtomicU64::new(0);
static LEASE_TOKENS: AtomicU64 = AtomicU64::new(0);
static REACQUISITION_ANCHOR_TOKENS: AtomicU64 = AtomicU64::new(0);
static REACQUISITION_RESERVATION_TOKENS: AtomicU64 = AtomicU64::new(0);
static LOADED_THREADS: OnceLock<Mutex<LoadedThreadState>> = OnceLock::new();

pub(in crate::cas_projection) fn allocate_connection_generation()
-> Result<ConnectionGeneration, ProjectionCoordinatorError> {
    allocate(&CONNECTION_GENERATIONS)
        .map(ConnectionGeneration)
        .ok_or(ProjectionCoordinatorError::ProjectionConnectionGenerationExhausted)
}

fn allocate_loaded_generation() -> Result<CasLoadedThreadGeneration, ProjectionCoordinatorError> {
    let value = allocate(&LOADED_GENERATIONS)
        .ok_or(ProjectionCoordinatorError::LoadedThreadGenerationExhausted)?;
    CasLoadedThreadGeneration::new(value)
        .map_err(|_| ProjectionCoordinatorError::LoadedThreadGenerationExhausted)
}

fn allocate_lease_token() -> Result<LeaseToken, ProjectionCoordinatorError> {
    allocate(&LEASE_TOKENS)
        .map(LeaseToken)
        .ok_or(ProjectionCoordinatorError::ProjectionLeaseTokenExhausted)
}

fn allocate_reacquisition_anchor_token()
-> Result<ReacquisitionAnchorToken, ProjectionCoordinatorError> {
    allocate(&REACQUISITION_ANCHOR_TOKENS)
        .map(ReacquisitionAnchorToken)
        .ok_or(ProjectionCoordinatorError::ReacquisitionAnchorTokenExhausted)
}

fn allocate_reacquisition_reservation_token()
-> Result<ReacquisitionReservationToken, ProjectionCoordinatorError> {
    allocate(&REACQUISITION_RESERVATION_TOKENS)
        .map(ReacquisitionReservationToken)
        .ok_or(ProjectionCoordinatorError::ReacquisitionReservationTokenExhausted)
}

fn allocate(counter: &AtomicU64) -> Option<u64> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .ok()
        .and_then(|previous| previous.checked_add(1))
}

fn registry() -> &'static Mutex<LoadedThreadState> {
    LOADED_THREADS.get_or_init(|| Mutex::new(LoadedThreadState::default()))
}

#[cfg(test)]
pub(in crate::cas_projection) fn poison_loaded_registry_for_recovery_drop_test() {
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _state = registry()
            .lock()
            .expect("loaded registry starts unpoisoned for recovery-drop test");
        panic!("poison loaded registry for recovery-drop test");
    }));
    assert!(panicked.is_err());
    assert!(registry().is_poisoned());
}

#[cfg(test)]
pub(in crate::cas_projection) fn clear_loaded_registry_poison_for_test() {
    registry().clear_poison();
    assert!(!registry().is_poisoned());
}

fn lock() -> Result<std::sync::MutexGuard<'static, LoadedThreadState>, ProjectionCoordinatorError> {
    registry()
        .lock()
        .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
            registry: ProjectionRegistryKind::LoadedThreads,
        })
}

fn add_connection_authority(state: &mut LoadedThreadState, connection: ConnectionGeneration) {
    let count = state
        .connection_authority_counts
        .entry(connection)
        .or_default();
    *count = count
        .checked_add(1)
        .expect("registered connection authorities fit in memory");
}

fn remove_connection_authority(state: &mut LoadedThreadState, connection: ConnectionGeneration) {
    let remove = {
        let count = state
            .connection_authority_counts
            .get_mut(&connection)
            .expect("every registered authority contributes to its connection count");
        *count = count
            .checked_sub(1)
            .expect("connection authority count cannot underflow");
        *count == 0
    };
    if remove {
        state.connection_authority_counts.remove(&connection);
    }
}

fn remove_reacquisition_reservations(
    state: &mut LoadedThreadState,
    predicate: impl Fn(ConnectionGeneration, &ReacquisitionReservationEntry) -> bool,
) -> usize {
    let replacements = state
        .reacquisition_reservations
        .iter()
        .filter_map(|(replacement, reservation)| {
            predicate(*replacement, reservation).then_some(*replacement)
        })
        .collect::<Vec<_>>();
    for replacement in &replacements {
        state.reacquisition_reservations.remove(replacement);
        remove_connection_authority(state, *replacement);
    }
    replacements.len()
}

pub(in crate::cas_projection) fn register_new(
    key: LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
) -> Result<(CasLoadedSessionGeneration, LeaseToken), ProjectionCoordinatorError> {
    let mut state = lock()?;
    if state.reacquisition_reservations.contains_key(&connection) {
        return Err(
            ProjectionCoordinatorError::ProjectionConnectionReservedForReacquisition {
                runtime_id: key.runtime_id,
                process_generation: key.process_generation,
            },
        );
    }
    if let Some(entry) = state.entries.get(&key) {
        return Err(collision_error(&key, entry, owner));
    }
    let generation = allocate_loaded_generation()?;
    let token = allocate_lease_token()?;
    let mut leases = HashSet::new();
    leases.insert(token);
    state.entries.insert(
        key.clone(),
        LoadedThreadEntry {
            connection,
            owner,
            generation,
            state: LoadedThreadEntryState::Active { leases },
        },
    );
    add_connection_authority(&mut state, connection);
    Ok((
        CasLoadedSessionGeneration::new(key.process_generation, generation),
        token,
    ))
}

pub(in crate::cas_projection) fn acquire_existing(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
) -> Result<ExistingSubscription, ProjectionCoordinatorError> {
    let mut state = lock()?;
    if state.reacquisition_reservations.contains_key(&connection) {
        return Err(
            ProjectionCoordinatorError::ProjectionConnectionReservedForReacquisition {
                runtime_id: key.runtime_id,
                process_generation: key.process_generation,
            },
        );
    }
    let Some(entry) = state.entries.get_mut(key) else {
        return Ok(ExistingSubscription::Absent);
    };
    if entry.owner != owner {
        return Ok(ExistingSubscription::AnotherOwner {
            existing_owner: entry.owner,
        });
    }
    if entry.connection != connection {
        return Ok(ExistingSubscription::AnotherConnection);
    }
    let LoadedThreadEntryState::Active { leases } = &mut entry.state else {
        return Ok(ExistingSubscription::Quarantined);
    };
    let token = allocate_lease_token()?;
    leases.insert(token);
    Ok(ExistingSubscription::Exact {
        generation: CasLoadedSessionGeneration::new(key.process_generation, entry.generation),
        token,
    })
}

pub(in crate::cas_projection) fn contains_exact(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
) -> Result<bool, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation {
        return Ok(false);
    }
    let state = lock()?;
    Ok(state.entries.get(key).is_some_and(|entry| {
        entry.connection == connection
            && entry.owner == owner
            && entry.generation == generation.thread()
            && matches!(
                &entry.state,
                LoadedThreadEntryState::Active { leases } if leases.contains(&token)
            )
    }))
}

pub(in crate::cas_projection) fn connection_has_authority(
    connection: ConnectionGeneration,
) -> Result<bool, ProjectionCoordinatorError> {
    let state = lock()?;
    Ok(state.connection_authority_counts.contains_key(&connection))
}

pub(in crate::cas_projection) fn release_exact(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
) -> Result<ReleaseDisposition, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation {
        return Ok(ReleaseDisposition::Stale);
    }
    let mut state = lock()?;
    let Some(entry) = state.entries.get_mut(key) else {
        return Ok(ReleaseDisposition::Stale);
    };
    if entry.connection != connection
        || entry.owner != owner
        || entry.generation != generation.thread()
    {
        return Ok(ReleaseDisposition::Stale);
    }
    let LoadedThreadEntryState::Active { leases } = &mut entry.state else {
        return Ok(ReleaseDisposition::Stale);
    };
    if !leases.remove(&token) {
        return Ok(ReleaseDisposition::Stale);
    }
    if !leases.is_empty() {
        return Ok(ReleaseDisposition::Shared);
    }
    state.entries.remove(key);
    remove_connection_authority(&mut state, connection);
    Ok(ReleaseDisposition::Last)
}

pub(in crate::cas_projection) fn quarantine_exact(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
) -> Result<Option<ReacquisitionAnchorToken>, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation {
        return Ok(None);
    }
    let mut state = lock()?;
    let Some(entry) = state.entries.get_mut(key) else {
        return Ok(None);
    };
    if entry.connection != connection
        || entry.owner != owner
        || entry.generation != generation.thread()
    {
        return Ok(None);
    }
    let LoadedThreadEntryState::Active { leases } = &entry.state else {
        return Ok(None);
    };
    if !leases.contains(&token) {
        return Ok(None);
    }
    let anchor = allocate_reacquisition_anchor_token()?;
    entry.state = LoadedThreadEntryState::Quarantined { token: anchor };
    Ok(Some(anchor))
}

pub(in crate::cas_projection) fn contains_quarantined(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: ReacquisitionAnchorToken,
) -> Result<bool, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation {
        return Ok(false);
    }
    let state = lock()?;
    Ok(state.entries.get(key).is_some_and(|entry| {
        entry.connection == connection
            && entry.owner == owner
            && entry.generation == generation.thread()
            && matches!(
                &entry.state,
                LoadedThreadEntryState::Quarantined { token: current } if *current == token
            )
    }))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::cas_projection) fn reserve_reacquisition(
    key: &LoadedThreadKey,
    old_connection: ConnectionGeneration,
    new_connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    anchor: ReacquisitionAnchorToken,
) -> Result<Option<ReacquisitionReservationToken>, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation || old_connection == new_connection {
        return Ok(None);
    }
    let mut state = lock()?;
    if state
        .entries
        .values()
        .any(|entry| entry.connection == new_connection)
        || state
            .reacquisition_reservations
            .contains_key(&new_connection)
        || state
            .reacquisition_reservations
            .values()
            .any(|reservation| reservation.key == *key)
    {
        return Ok(None);
    }
    let Some(entry) = state.entries.get(key) else {
        return Ok(None);
    };
    if entry.connection != old_connection
        || entry.owner != owner
        || entry.generation != generation.thread()
        || !matches!(
            &entry.state,
            LoadedThreadEntryState::Quarantined { token } if *token == anchor
        )
    {
        return Ok(None);
    }
    let token = allocate_reacquisition_reservation_token()?;
    state.reacquisition_reservations.insert(
        new_connection,
        ReacquisitionReservationEntry {
            key: key.clone(),
            anchor_connection: old_connection,
            owner,
            anchor_generation: generation.thread(),
            anchor_token: anchor,
            token,
        },
    );
    add_connection_authority(&mut state, new_connection);
    Ok(Some(token))
}

pub(in crate::cas_projection) fn contains_reacquisition_reservation(
    key: &LoadedThreadKey,
    old_connection: ConnectionGeneration,
    new_connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    anchor: ReacquisitionAnchorToken,
    token: ReacquisitionReservationToken,
) -> Result<bool, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation {
        return Ok(false);
    }
    let state = lock()?;
    Ok(state
        .reacquisition_reservations
        .get(&new_connection)
        .is_some_and(|reservation| {
            reservation.key == *key
                && reservation.anchor_connection == old_connection
                && reservation.owner == owner
                && reservation.anchor_generation == generation.thread()
                && reservation.anchor_token == anchor
                && reservation.token == token
        }))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::cas_projection) fn transfer_quarantined(
    key: &LoadedThreadKey,
    old_connection: ConnectionGeneration,
    new_connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    anchor: ReacquisitionAnchorToken,
    reservation: ReacquisitionReservationToken,
) -> Result<Option<(CasLoadedSessionGeneration, LeaseToken)>, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation || old_connection == new_connection {
        return Ok(None);
    }
    let mut state = lock()?;
    if state
        .entries
        .values()
        .any(|entry| entry.connection == new_connection)
    {
        return Ok(None);
    }
    let reservation_matches = state
        .reacquisition_reservations
        .get(&new_connection)
        .is_some_and(|candidate| {
            candidate.key == *key
                && candidate.anchor_connection == old_connection
                && candidate.owner == owner
                && candidate.anchor_generation == generation.thread()
                && candidate.anchor_token == anchor
                && candidate.token == reservation
        });
    if !reservation_matches {
        return Ok(None);
    }
    let Some(entry) = state.entries.get_mut(key) else {
        return Ok(None);
    };
    if entry.connection != old_connection
        || entry.owner != owner
        || entry.generation != generation.thread()
        || !matches!(
            &entry.state,
            LoadedThreadEntryState::Quarantined { token } if *token == anchor
        )
    {
        return Ok(None);
    }
    let next_generation = allocate_loaded_generation()?;
    let lease = allocate_lease_token()?;
    let mut leases = HashSet::new();
    leases.insert(lease);
    entry.connection = new_connection;
    entry.generation = next_generation;
    entry.state = LoadedThreadEntryState::Active { leases };
    state.reacquisition_reservations.remove(&new_connection);
    remove_connection_authority(&mut state, old_connection);
    Ok(Some((
        CasLoadedSessionGeneration::new(key.process_generation, next_generation),
        lease,
    )))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::cas_projection) fn abandon_reacquisition_reservation(
    key: &LoadedThreadKey,
    old_connection: ConnectionGeneration,
    new_connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    anchor: ReacquisitionAnchorToken,
    token: ReacquisitionReservationToken,
) -> Result<bool, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation {
        return Ok(false);
    }
    let mut state = lock()?;
    let matches = state
        .reacquisition_reservations
        .get(&new_connection)
        .is_some_and(|reservation| {
            reservation.key == *key
                && reservation.anchor_connection == old_connection
                && reservation.owner == owner
                && reservation.anchor_generation == generation.thread()
                && reservation.anchor_token == anchor
                && reservation.token == token
        });
    if matches {
        state.reacquisition_reservations.remove(&new_connection);
        remove_connection_authority(&mut state, new_connection);
    }
    Ok(matches)
}

pub(in crate::cas_projection) fn abandon_quarantined(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: ReacquisitionAnchorToken,
) -> Result<bool, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation {
        return Ok(false);
    }
    let mut state = lock()?;
    let matches = state.entries.get(key).is_some_and(|entry| {
        entry.connection == connection
            && entry.owner == owner
            && entry.generation == generation.thread()
            && matches!(
                &entry.state,
                LoadedThreadEntryState::Quarantined { token: current } if *current == token
            )
    });
    if matches {
        state.entries.remove(key);
        remove_connection_authority(&mut state, connection);
    }
    Ok(matches)
}

pub(in crate::cas_projection) fn invalidate_connection(
    connection: ConnectionGeneration,
) -> Result<usize, ProjectionCoordinatorError> {
    let mut state = lock()?;
    let before_entries = state.entries.len();
    state
        .entries
        .retain(|_, entry| entry.connection != connection);
    let removed_entries = before_entries
        .checked_sub(state.entries.len())
        .expect("retention cannot add registry entries");
    let removed_reservations =
        remove_reacquisition_reservations(&mut state, |replacement, reservation| {
            replacement == connection || reservation.anchor_connection == connection
        });
    state.connection_authority_counts.remove(&connection);
    Ok(removed_entries
        .checked_add(removed_reservations)
        .expect("removed registry authority count fits in memory"))
}

pub(in crate::cas_projection) fn invalidate_connection_thread(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
) -> Result<bool, ProjectionCoordinatorError> {
    let mut state = lock()?;
    let remove_entry = state
        .entries
        .get(key)
        .is_some_and(|entry| entry.connection == connection);
    if remove_entry {
        state.entries.remove(key);
        remove_connection_authority(&mut state, connection);
    }
    let removed_reservations =
        remove_reacquisition_reservations(&mut state, |replacement, reservation| {
            reservation.key == *key
                && (replacement == connection || reservation.anchor_connection == connection)
        });
    Ok(remove_entry || removed_reservations != 0)
}

pub(in crate::cas_projection) fn invalidate_thread(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
) -> Result<ObservedSubscription, ProjectionCoordinatorError> {
    let mut state = lock()?;
    let Some(entry) = state.entries.get(key) else {
        return Ok(ObservedSubscription::Absent);
    };
    if entry.owner != owner {
        return Ok(ObservedSubscription::AnotherOwner {
            existing_owner: entry.owner,
        });
    }
    if entry.connection != connection {
        return Ok(ObservedSubscription::AnotherConnection);
    }
    let generation = CasLoadedSessionGeneration::new(key.process_generation, entry.generation);
    state.entries.remove(key);
    remove_connection_authority(&mut state, connection);
    remove_reacquisition_reservations(&mut state, |_, reservation| {
        reservation.key == *key && reservation.anchor_connection == connection
    });
    Ok(ObservedSubscription::Exact(generation))
}

pub(in crate::cas_projection) fn invalidate_exact_generation(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
) -> Result<bool, ProjectionCoordinatorError> {
    if generation.process() != key.process_generation {
        return Ok(false);
    }
    let mut state = lock()?;
    let Some(entry) = state.entries.get(key) else {
        return Ok(false);
    };
    if entry.connection != connection
        || entry.owner != owner
        || entry.generation != generation.thread()
    {
        return Ok(false);
    }
    state.entries.remove(key);
    remove_connection_authority(&mut state, connection);
    remove_reacquisition_reservations(&mut state, |_, reservation| {
        reservation.key == *key && reservation.anchor_connection == connection
    });
    Ok(true)
}

#[cfg(test)]
pub(in crate::cas_projection) fn live_entry_count() -> Result<usize, ProjectionCoordinatorError> {
    Ok(lock()?.entries.len())
}

#[cfg(test)]
pub(in crate::cas_projection) fn live_reacquisition_reservation_count()
-> Result<usize, ProjectionCoordinatorError> {
    Ok(lock()?.reacquisition_reservations.len())
}

fn collision_error(
    key: &LoadedThreadKey,
    entry: &LoadedThreadEntry,
    offered_owner: SyndicThreadId,
) -> ProjectionCoordinatorError {
    if entry.owner != offered_owner {
        ProjectionCoordinatorError::CasThreadOwnerCollision {
            runtime_id: key.runtime_id,
            process_generation: key.process_generation,
            cas_thread_id: key.cas_thread_id.clone(),
            existing_owner: entry.owner,
            offered_owner,
        }
    } else {
        ProjectionCoordinatorError::CasThreadConnectionCollision {
            runtime_id: key.runtime_id,
            process_generation: key.process_generation,
            cas_thread_id: key.cas_thread_id.clone(),
        }
    }
}
