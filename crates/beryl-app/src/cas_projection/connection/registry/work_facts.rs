use super::*;

pub(in crate::cas_projection) fn try_hold_work_revision(
    expected: u64,
) -> Result<impl Sized, crate::cas_projection::runtime_work::RuntimeWorkError> {
    use crate::cas_projection::runtime_work::RuntimeWorkError;
    let state = LOADED_THREADS
        .get()
        .ok_or(RuntimeWorkError::Unavailable)?
        .try_lock()?;
    if state.revision.ok_or(RuntimeWorkError::Unavailable)? != expected {
        return Err(RuntimeWorkError::Stale);
    }
    Ok(state)
}

pub(in crate::cas_projection) fn try_work_revision()
-> Result<u64, crate::cas_projection::runtime_work::RuntimeWorkError> {
    use crate::cas_projection::runtime_work::RuntimeWorkError;
    LOADED_THREADS
        .get()
        .ok_or(RuntimeWorkError::Unavailable)?
        .try_lock()?
        .revision
        .ok_or(RuntimeWorkError::Unavailable)
}

pub(in crate::cas_projection) fn work_revision() -> Result<u64, ProjectionCoordinatorError> {
    lock()?.revision.ok_or(
        ProjectionCoordinatorError::RegistryWorkRevisionUnavailable {
            registry: ProjectionRegistryKind::LoadedThreads,
        },
    )
}

pub(in crate::cas_projection) fn loaded_owner_prefix(
    connection: ConnectionGeneration,
    revision: u64,
    after: Option<SyndicThreadId>,
    limit: usize,
) -> Result<Vec<SyndicThreadId>, ProjectionCoordinatorError> {
    let state = lock()?;
    if state.revision != Some(revision) {
        return Err(ProjectionCoordinatorError::RegistryWorkSourceChanged {
            registry: ProjectionRegistryKind::LoadedThreads,
        });
    }
    let limit = limit.clamp(1, 257);
    let mut threads = std::collections::BTreeSet::new();
    if let Some(owners) = state.connection_authority_counts.get(&connection) {
        for &thread_id in owners.owners.keys() {
            if after.is_none_or(|after| thread_id > after) {
                threads.insert(thread_id);
                if threads.len() > limit {
                    threads.pop_last();
                }
            }
        }
    }
    Ok(threads.into_iter().collect())
}
