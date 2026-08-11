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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct ConnectionGeneration(u64);

impl ConnectionGeneration {
    pub(in crate::cas_projection) const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct LeaseToken(u64);

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
    leases: HashSet<LeaseToken>,
}

#[derive(Default)]
struct LoadedThreadState {
    entries: HashMap<LoadedThreadKey, LoadedThreadEntry>,
    connection_authority_counts: HashMap<ConnectionGeneration, usize>,
}

static CONNECTION_GENERATIONS: AtomicU64 = AtomicU64::new(0);
static LOADED_GENERATIONS: AtomicU64 = AtomicU64::new(0);
static LEASE_TOKENS: AtomicU64 = AtomicU64::new(0);
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

pub(in crate::cas_projection) fn register_new(
    key: LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
) -> Result<(CasLoadedSessionGeneration, LeaseToken), ProjectionCoordinatorError> {
    let mut state = lock()?;
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
            leases,
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
    let token = allocate_lease_token()?;
    entry.leases.insert(token);
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
            && entry.leases.contains(&token)
    }))
}

pub(in crate::cas_projection) fn connection_has_authority(
    connection: ConnectionGeneration,
) -> Result<bool, ProjectionCoordinatorError> {
    Ok(lock()?
        .connection_authority_counts
        .contains_key(&connection))
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
        || !entry.leases.remove(&token)
    {
        return Ok(ReleaseDisposition::Stale);
    }
    if !entry.leases.is_empty() {
        return Ok(ReleaseDisposition::Shared);
    }
    state.entries.remove(key);
    remove_connection_authority(&mut state, connection);
    Ok(ReleaseDisposition::Last)
}

pub(in crate::cas_projection) fn invalidate_connection(
    connection: ConnectionGeneration,
) -> Result<usize, ProjectionCoordinatorError> {
    let mut state = lock()?;
    let before = state.entries.len();
    state
        .entries
        .retain(|_, entry| entry.connection != connection);
    state.connection_authority_counts.remove(&connection);
    Ok(before.saturating_sub(state.entries.len()))
}

pub(in crate::cas_projection) fn invalidate_connection_thread(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
) -> Result<bool, ProjectionCoordinatorError> {
    let mut state = lock()?;
    let remove = state
        .entries
        .get(key)
        .is_some_and(|entry| entry.connection == connection);
    if remove {
        state.entries.remove(key);
        remove_connection_authority(&mut state, connection);
    }
    Ok(remove)
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
    let remove = state.entries.get(key).is_some_and(|entry| {
        entry.connection == connection
            && entry.owner == owner
            && entry.generation == generation.thread()
    });
    if remove {
        state.entries.remove(key);
        remove_connection_authority(&mut state, connection);
    }
    Ok(remove)
}

#[cfg(test)]
pub(in crate::cas_projection) fn live_entry_count() -> Result<usize, ProjectionCoordinatorError> {
    Ok(lock()?.entries.len())
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
