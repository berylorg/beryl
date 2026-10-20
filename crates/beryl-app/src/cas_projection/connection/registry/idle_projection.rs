use super::*;

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/idle_projection_token.rs"
    ));
}

pub(in crate::cas_projection) fn owns_only_loaded_lease(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
) -> Result<bool, ProjectionCoordinatorError> {
    let state = lock()?;
    Ok(owns_only_lease(
        &state, key, connection, owner, generation, token,
    ))
}

pub(in crate::cas_projection) fn try_owns_only_loaded_lease(
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
) -> Result<bool, ProjectionCoordinatorError> {
    let Some(state) = try_lock_for_shutdown()? else {
        return Ok(false);
    };
    Ok(owns_only_lease(
        &state, key, connection, owner, generation, token,
    ))
}

pub(in crate::cas_projection) fn try_connection_has_authority(
    connection: ConnectionGeneration,
) -> Result<Option<bool>, ProjectionCoordinatorError> {
    Ok(try_lock_for_shutdown()?
        .map(|state| state.connection_authority_counts.contains_key(&connection)))
}

fn try_lock_for_shutdown()
-> Result<Option<std::sync::MutexGuard<'static, LoadedThreadState>>, ProjectionCoordinatorError> {
    match registry().try_lock() {
        Ok(state) => Ok(Some(state)),
        Err(std::sync::TryLockError::WouldBlock) => Ok(None),
        Err(std::sync::TryLockError::Poisoned(_)) => {
            Err(ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::LoadedThreads,
            })
        }
    }
}

fn owns_only_lease(
    state: &LoadedThreadState,
    key: &LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
) -> bool {
    let Some(entry) = state.entries.get(key) else {
        return false;
    };
    generation.process() == key.process_generation
        && entry.connection == connection
        && entry.owner == owner
        && entry.generation == generation.thread()
        && entry.leases.len() == 1
        && entry.leases.contains(&token)
        && !state.entries.iter().any(|(other_key, other)| {
            other_key != key && other.connection == connection && !other.leases.is_empty()
        })
}
