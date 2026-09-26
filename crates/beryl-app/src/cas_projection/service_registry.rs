use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, LockResult, Mutex, MutexGuard, PoisonError},
};

use beryl_model::{CasProcessGeneration, RuntimeId};

use super::persistent_failure::{LiveCommandAuthorizer, PersistentFailureTerminalDisposer};
use super::{ProjectionServiceGeneration, connection::ProjectionConnection};

mod cleanup;
pub(super) use cleanup::{
    ConnectionCleanupDisposition, ConnectionCleanupError, ConnectionCleanupMode,
};

pub(super) struct ProjectionRuntimeRetirement {
    connections: Arc<ProjectionServiceConnectionRegistry>,
    commands: LiveCommandAuthorizer,
    terminal_disposer: PersistentFailureTerminalDisposer,
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
}

impl ProjectionRuntimeRetirement {
    pub(super) fn new(
        connections: Arc<ProjectionServiceConnectionRegistry>,
        commands: LiveCommandAuthorizer,
        terminal_disposer: PersistentFailureTerminalDisposer,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Self {
        Self {
            connections,
            commands,
            terminal_disposer,
            runtime_id,
            process_generation,
        }
    }

    pub(super) fn retire(&self) -> bool {
        // A pre-cut permit keeps the cut behind this retirement. A winning cut must finish
        // assigning its stop obligations before any of its snapshotted routers can detach.
        let command = self.commands.authorize().ok();
        if command.is_none() && self.commands.is_persistent_failure_cut() {
            self.terminal_disposer.wait_for_cut_worker_exit();
        }
        let clean = self
            .connections
            .visit_cleanup_connections(
                ConnectionCleanupMode::Dispose,
                Some((self.runtime_id, self.process_generation)),
                |connection| {
                    connection
                        .shutdown_for_runtime_retirement()
                        .map(|()| ConnectionCleanupDisposition::RemoveClean)
                        .map_err(|_| ())
                },
            )
            .is_ok();
        drop(command);
        clean
    }

    pub(super) fn poll_retirements(&self) -> Result<(), super::RuntimeFailure> {
        let Ok(_command) = self.commands.authorize() else {
            return Ok(());
        };
        match self.connections.visit_cleanup_connections(
            ConnectionCleanupMode::Inspect,
            Some((self.runtime_id, self.process_generation)),
            |connection| {
                connection
                    .try_reap_ordinary_retirement()
                    .map(|complete| {
                        if complete {
                            ConnectionCleanupDisposition::RemoveClean
                        } else {
                            ConnectionCleanupDisposition::Retain
                        }
                    })
                    .map_err(|_| ())
            },
        ) {
            Ok(()) | Err(ConnectionCleanupError::Deferred) => Ok(()),
            Err(ConnectionCleanupError::Failed) => Err(super::RuntimeFailure::AppRetirement),
        }
    }
}

/// Exact connection membership owned by one projection-service generation.
///
/// The generation is carried by the synchronization boundary itself so membership cannot cross
/// service ownership through an untyped shared vector.
pub(super) struct ProjectionServiceConnectionRegistry {
    work_boundary: super::connection_work::ConnectionWorkBoundary,
    service_generation: ProjectionServiceGeneration,
    work_owner: Arc<()>,
    connections: Mutex<ConnectionRegistryState>,
}

struct ConnectionRegistryState {
    revision: Option<u64>,
    entries: Vec<Arc<ProjectionConnection>>,
}

pub(super) struct ConnectionRegistryGuard<'a> {
    state: MutexGuard<'a, ConnectionRegistryState>,
    boundary: &'a super::connection_work::ConnectionWorkBoundary,
    change: Option<super::connection_work::ConnectionWorkMutation>,
}

impl ConnectionRegistryGuard<'_> {
    pub(super) fn revision(&self) -> Option<u64> {
        self.state.revision
    }
}

impl Deref for ConnectionRegistryGuard<'_> {
    type Target = Vec<Arc<ProjectionConnection>>;

    fn deref(&self) -> &Self::Target {
        &self.state.entries
    }
}

impl DerefMut for ConnectionRegistryGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.change
            .get_or_insert_with(|| self.boundary.begin_change());
        self.state.revision = self
            .state
            .revision
            .and_then(|revision| revision.checked_add(1));
        &mut self.state.entries
    }
}

impl Drop for ConnectionRegistryGuard<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.boundary.invalidate();
        }
    }
}

impl Drop for ProjectionServiceConnectionRegistry {
    fn drop(&mut self) {
        self.work_boundary.close();
    }
}

impl ProjectionServiceConnectionRegistry {
    pub(super) fn visit_connections<E>(
        &self,
        mut visit: impl FnMut(&Arc<ProjectionConnection>) -> Result<(), E>,
    ) -> Result<(), E>
    where
        E: From<super::ConnectionWorkError>,
    {
        let expected = self
            .lock()
            .map_err(|_| super::ConnectionWorkError::Poisoned)?
            .revision()
            .ok_or(super::ConnectionWorkError::RevisionUnavailable)?;
        let mut index = 0;
        loop {
            let connection = {
                let state = self
                    .lock()
                    .map_err(|_| super::ConnectionWorkError::Poisoned)?;
                let revision = state
                    .revision()
                    .ok_or(super::ConnectionWorkError::RevisionUnavailable)?;
                if revision != expected {
                    return Err(super::ConnectionWorkError::StaleRevision.into());
                }
                state.get(index).cloned()
            };
            let Some(connection) = connection else {
                return Ok(());
            };
            visit(&connection)?;
            index += 1;
        }
    }

    pub(super) fn new(service_generation: ProjectionServiceGeneration) -> Arc<Self> {
        Arc::new(Self {
            work_boundary: super::connection_work::ConnectionWorkBoundary::new(),
            service_generation,
            work_owner: Arc::new(()),
            connections: Mutex::new(ConnectionRegistryState {
                revision: Some(0),
                entries: Vec::new(),
            }),
        })
    }

    pub(super) const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    pub(super) fn work_owner(&self) -> &Arc<()> {
        &self.work_owner
    }

    pub(super) fn work_boundary(&self) -> &super::connection_work::ConnectionWorkBoundary {
        &self.work_boundary
    }

    pub(super) fn try_work_lock(
        &self,
    ) -> Result<ConnectionRegistryGuard<'_>, super::runtime_work::RuntimeWorkError> {
        Ok(ConnectionRegistryGuard {
            state: self.connections.try_lock()?,
            boundary: &self.work_boundary,
            change: None,
        })
    }

    pub(super) fn lock(&self) -> LockResult<ConnectionRegistryGuard<'_>> {
        self.connections
            .lock()
            .map(|state| ConnectionRegistryGuard {
                state,
                boundary: &self.work_boundary,
                change: None,
            })
            .map_err(|poison| {
                self.work_boundary.invalidate();
                PoisonError::new(ConnectionRegistryGuard {
                    state: poison.into_inner(),
                    boundary: &self.work_boundary,
                    change: None,
                })
            })
    }

    /// Reaps only completed ordinary retirements without holding the service registry across a
    /// connection lifecycle boundary.
    pub(super) fn reap_finished_ordinary_retirements(&self) {
        let _ =
            self.visit_cleanup_connections(ConnectionCleanupMode::Inspect, None, |connection| {
                Ok(
                    if connection.try_reap_ordinary_retirement().unwrap_or(false) {
                        ConnectionCleanupDisposition::RemoveClean
                    } else {
                        ConnectionCleanupDisposition::Retain
                    },
                )
            });
    }

    #[cfg(test)]
    pub(super) fn exhaust_revision_for_test(&self) {
        self.work_boundary.invalidate();
        self.connections.lock().unwrap().revision = None;
    }

    #[cfg(test)]
    pub(super) fn poison_for_test(&self) {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _change = self.work_boundary.begin_change();
            let _connections = self
                .connections
                .lock()
                .expect("service connection registry starts unpoisoned");
            panic!("poison service connection registry for lifecycle test");
        }));
        assert!(panicked.is_err());
    }
}

impl std::fmt::Debug for ProjectionServiceConnectionRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectionServiceConnectionRegistry")
            .field("service_generation", &self.service_generation)
            .field(
                "connection_count",
                &self.lock().map(|connections| connections.len()),
            )
            .finish_non_exhaustive()
    }
}
