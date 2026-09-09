use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, LockResult, Mutex, MutexGuard, PoisonError},
};

use beryl_model::{CasProcessGeneration, RuntimeId};

use super::persistent_failure::{LiveCommandAuthorizer, PersistentFailureTerminalDisposer};
use super::{ProjectionServiceGeneration, connection::ProjectionConnection};

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
        let (connections, mut clean) = match self.connections.lock() {
            Ok(connections) => (connections, true),
            Err(poison) => (poison.into_inner(), false),
        };
        let matching = connections
            .iter()
            .filter(|connection| {
                connection.runtime_id() == self.runtime_id
                    && connection.process_generation() == self.process_generation
            })
            .cloned()
            .collect::<Vec<_>>();
        drop(connections);
        for connection in matching {
            clean &= connection.shutdown().is_ok();
            clean &= connection.shutdown_after_ordinary_retirement().is_ok();
        }
        self.connections.reap_finished_ordinary_retirements();
        drop(command);
        clean
    }
}

/// Exact connection membership owned by one projection-service generation.
///
/// The generation is carried by the synchronization boundary itself so membership cannot cross
/// service ownership through an untyped shared vector.
pub(super) struct ProjectionServiceConnectionRegistry {
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
        self.state.revision = self
            .state
            .revision
            .and_then(|revision| revision.checked_add(1));
        &mut self.state.entries
    }
}

impl ProjectionServiceConnectionRegistry {
    pub(super) fn new(service_generation: ProjectionServiceGeneration) -> Arc<Self> {
        Arc::new(Self {
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

    pub(super) fn lock(&self) -> LockResult<ConnectionRegistryGuard<'_>> {
        self.connections
            .lock()
            .map(|state| ConnectionRegistryGuard { state })
            .map_err(|poison| {
                PoisonError::new(ConnectionRegistryGuard {
                    state: poison.into_inner(),
                })
            })
    }

    /// Reaps only completed ordinary retirements without holding the service registry across a
    /// connection lifecycle boundary.
    pub(super) fn reap_finished_ordinary_retirements(&self) {
        let snapshot = self
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        let mut reaped = Vec::new();
        for connection in snapshot {
            if connection.try_reap_ordinary_retirement() {
                reaped.push(connection);
            }
        }
        if reaped.is_empty() {
            return;
        }
        self.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .retain(|connection| !reaped.iter().any(|reaped| Arc::ptr_eq(connection, reaped)));
    }

    #[cfg(test)]
    pub(super) fn poison_for_test(&self) {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
