use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(in crate::cas_projection) enum ConnectionCleanupMode {
    Inspect,
    Dispose,
}

pub(in crate::cas_projection) enum ConnectionCleanupDisposition {
    Retain,
    RemoveClean,
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ConnectionCleanupError {
    Deferred,
    Failed,
}

impl ProjectionServiceConnectionRegistry {
    pub(in crate::cas_projection) fn visit_cleanup_connections(
        &self,
        mode: ConnectionCleanupMode,
        runtime: Option<(RuntimeId, CasProcessGeneration)>,
        mut visit: impl FnMut(&Arc<ProjectionConnection>) -> Result<ConnectionCleanupDisposition, ()>,
    ) -> Result<(), ConnectionCleanupError> {
        let mut failed = false;
        let (mut expected, upper) = {
            let state = self.lock_cleanup(mode, &mut failed)?;
            let expected = state.revision();
            validate_revision(expected, expected, mode, &mut failed)?;
            let upper = state
                .iter()
                .filter(|connection| matches_runtime(connection, runtime))
                .map(connection_identity)
                .max();
            (expected, upper)
        };
        let mut after = None;
        loop {
            let connection = {
                let state = self.lock_cleanup(mode, &mut failed)?;
                validate_revision(state.revision(), expected, mode, &mut failed)?;
                state
                    .iter()
                    .filter(|connection| matches_runtime(connection, runtime))
                    .filter(|connection| {
                        let identity = connection_identity(connection);
                        after.is_none_or(|after| identity > after)
                            && upper.is_some_and(|upper| identity <= upper)
                    })
                    .min_by_key(|connection| connection_identity(connection))
                    .cloned()
            };
            let Some(connection) = connection else {
                return if failed {
                    Err(ConnectionCleanupError::Failed)
                } else {
                    Ok(())
                };
            };
            after = Some(connection_identity(&connection));
            match visit(&connection) {
                Ok(ConnectionCleanupDisposition::Retain) => {}
                Ok(ConnectionCleanupDisposition::RemoveClean) => {
                    let mut state = self.lock_cleanup(mode, &mut failed)?;
                    validate_revision(state.revision(), expected, mode, &mut failed)?;
                    if let Some(index) = state
                        .iter()
                        .position(|retained| Arc::ptr_eq(retained, &connection))
                    {
                        state.remove(index);
                        expected = state.revision();
                        validate_revision(expected, expected, mode, &mut failed)?;
                    }
                }
                Err(()) => {
                    if mode == ConnectionCleanupMode::Inspect {
                        return Err(ConnectionCleanupError::Failed);
                    }
                    failed = true;
                }
            }
        }
    }

    fn lock_cleanup(
        &self,
        mode: ConnectionCleanupMode,
        failed: &mut bool,
    ) -> Result<ConnectionRegistryGuard<'_>, ConnectionCleanupError> {
        match mode {
            ConnectionCleanupMode::Inspect => match self.connections.try_lock() {
                Ok(state) => Ok(ConnectionRegistryGuard { state }),
                Err(std::sync::TryLockError::WouldBlock) => Err(ConnectionCleanupError::Deferred),
                Err(std::sync::TryLockError::Poisoned(_)) => Err(ConnectionCleanupError::Failed),
            },
            ConnectionCleanupMode::Dispose => match self.lock() {
                Ok(state) => Ok(state),
                Err(poison) => {
                    *failed = true;
                    Ok(poison.into_inner())
                }
            },
        }
    }
}

fn validate_revision(
    actual: Option<u64>,
    expected: Option<u64>,
    mode: ConnectionCleanupMode,
    failed: &mut bool,
) -> Result<(), ConnectionCleanupError> {
    if actual.is_none() || actual != expected {
        if mode == ConnectionCleanupMode::Inspect {
            return Err(if actual.is_none() {
                ConnectionCleanupError::Failed
            } else {
                ConnectionCleanupError::Deferred
            });
        }
        *failed = true;
    }
    Ok(())
}

fn matches_runtime(
    connection: &ProjectionConnection,
    runtime: Option<(RuntimeId, CasProcessGeneration)>,
) -> bool {
    runtime.is_none_or(|(runtime, process)| {
        connection.runtime_id() == runtime && connection.process_generation() == process
    })
}

fn connection_identity(connection: &Arc<ProjectionConnection>) -> u64 {
    connection.identity_observation().connection_generation()
}
