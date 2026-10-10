use super::*;
use beryl_model::BindingRevision;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelInterest {
    bytes: [u8; 256],
    len: usize,
}

impl ModelInterest {
    fn new(model: &str) -> Option<Self> {
        if model.is_empty() || model.len() > 256 {
            return None;
        }
        let mut bytes = [0; 256];
        bytes[..model.len()].copy_from_slice(model.as_bytes());
        Some(Self {
            bytes,
            len: model.len(),
        })
    }
}

pub(super) struct ContextInterest {
    revision: Option<u64>,
    model: Option<ModelInterest>,
    quota: Option<(u64, beryl_backend::AccountQuotaObservation)>,
}

impl Default for ContextInterest {
    fn default() -> Self {
        Self {
            revision: Some(0),
            model: None,
            quota: None,
        }
    }
}

#[cfg(feature = "test-faults")]
impl ContextInterest {
    pub(super) fn diagnostics(&self) -> (Option<u64>, Option<String>, Option<u64>) {
        (
            self.revision,
            self.model
                .as_ref()
                .map(|model| String::from_utf8(model.bytes[..model.len].to_vec()).unwrap()),
            self.quota.map(|(revision, _)| revision),
        )
    }
}

fn elect_interest(
    state: &LoadedThreadState,
    connection: ConnectionGeneration,
) -> Option<ModelInterest> {
    let mut candidate = None;
    for entry in state
        .entries
        .values()
        .filter(|entry| entry.connection == connection && !entry.leases.is_empty())
    {
        entry.context_binding?;
        let model = ModelInterest::new(entry.metadata.model.as_deref()?)?;
        if candidate.as_ref().is_some_and(|current| current != &model) {
            return None;
        }
        candidate = Some(model);
    }
    candidate
}

pub(super) fn refresh_interest(state: &mut LoadedThreadState, connection: ConnectionGeneration) {
    let model = elect_interest(state, connection);
    if let Some(authority) = state.connection_authority_counts.get_mut(&connection) {
        authority.context.revision = authority
            .context
            .revision
            .and_then(|value| value.checked_add(1));
        authority.context.model = model;
        authority.context.quota = None;
    }
}

pub(in crate::cas_projection) fn observe_quota(
    connection: u64,
    quota: beryl_backend::AccountQuotaObservation,
) -> Result<(), ProjectionCoordinatorError> {
    let mut state = lock()?;
    let Some(connection) = state
        .connection_authority_counts
        .keys()
        .find(|key| key.get() == connection)
        .copied()
    else {
        return Ok(());
    };
    let model = elect_interest(&state, connection);
    let authority = state
        .connection_authority_counts
        .get_mut(&connection)
        .expect("locked exact connection exists");
    if authority.context.model != model {
        authority.context.revision = authority
            .context
            .revision
            .and_then(|value| value.checked_add(1));
        authority.context.model = model;
        authority.context.quota = None;
    }
    authority.context.quota = authority.context.revision.map(|revision| (revision, quota));
    Ok(())
}

#[derive(Debug)]
pub(super) struct ContextRecord {
    _source_revision: BindingRevision,
    observation: beryl_backend::ThreadContextObservation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct ContextStamp {
    key: LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedThreadGeneration,
    binding: BindingRevision,
    observation_revision: u64,
}

pub(in crate::cas_projection) struct ContextSnapshot {
    pub(in crate::cas_projection) stamp: ContextStamp,
    pub(in crate::cas_projection) observation: beryl_backend::ThreadContextObservation,
}

pub(in crate::cas_projection) fn context_is_current(stamp: &ContextStamp) -> bool {
    let Ok(state) = registry().try_lock() else {
        return false;
    };
    state.entries.get(&stamp.key).is_some_and(|entry| {
        entry.connection == stamp.connection
            && entry.owner == stamp.owner
            && entry.generation == stamp.generation
            && entry.context_binding == Some(stamp.binding)
            && entry.context_revision == Some(stamp.observation_revision)
            && !entry.leases.is_empty()
            && entry.context.is_some()
    })
}

pub(in crate::cas_projection) fn bind_context(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
    previous: Option<BindingRevision>,
    revision: BindingRevision,
    proven_continuity: bool,
) -> Result<(), ProjectionCoordinatorError> {
    let mut state = lock()?;
    let entry = state
        .entries
        .get_mut(key)
        .filter(|entry| {
            generation.process() == key.process_generation
                && entry.connection == connection
                && entry.owner == owner
                && entry.generation == generation.thread()
                && entry.leases.contains(&token)
        })
        .ok_or(ProjectionCoordinatorError::ProjectionWorkerStopped)?;
    if entry.context_binding == Some(revision) {
        return Ok(());
    }
    let carry = proven_continuity
        && previous == entry.context_binding
        && previous.and_then(|value| value.checked_next().ok()) == Some(revision);
    if !carry {
        entry.context = None;
    }
    entry.context_revision = entry
        .context_revision
        .and_then(|value| value.checked_add(1));
    entry.context_binding = Some(revision);
    refresh_interest(&mut state, connection);
    Ok(())
}

pub(in crate::cas_projection) fn observe_context(
    key: &LoadedThreadKey,
    connection: u64,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    observation: beryl_backend::ThreadContextObservation,
) -> Result<(), ProjectionCoordinatorError> {
    let mut state = lock()?;
    let entry = state
        .entries
        .get_mut(key)
        .filter(|entry| {
            generation.process() == key.process_generation
                && entry.connection.get() == connection
                && entry.owner == owner
                && entry.generation == generation.thread()
                && !entry.leases.is_empty()
                && key.cas_thread_id == *observation.thread_id()
        })
        .ok_or(ProjectionCoordinatorError::ProjectionWorkerStopped)?;
    if let Some(source_revision) = entry.context_binding {
        entry.context_revision = entry
            .context_revision
            .and_then(|value| value.checked_add(1));
        entry.context = Some(ContextRecord {
            _source_revision: source_revision,
            observation,
        });
    }
    Ok(())
}

pub(in crate::cas_projection) fn read_context(
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    cas_thread: &CasThreadId,
    revision: BindingRevision,
) -> Result<Option<ContextSnapshot>, ProjectionCoordinatorError> {
    let state = lock()?;
    let mut entries = state.entries.iter().filter(|(key, entry)| {
        entry.connection == connection
            && entry.owner == owner
            && &key.cas_thread_id == cas_thread
            && !entry.leases.is_empty()
    });
    let observation = entries.next().and_then(|(key, entry)| {
        if entry.context_binding != Some(revision) {
            return None;
        }
        let record = entry.context.as_ref()?;
        Some(ContextSnapshot {
            stamp: ContextStamp {
                key: key.clone(),
                connection,
                owner,
                generation: entry.generation,
                binding: revision,
                observation_revision: entry.context_revision?,
            },
            observation: record.observation.clone(),
        })
    });
    if entries.next().is_some() {
        return Ok(None);
    }
    Ok(observation)
}
