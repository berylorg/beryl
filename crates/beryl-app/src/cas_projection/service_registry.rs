use std::sync::{Arc, LockResult, Mutex, MutexGuard};

use super::{ProjectionServiceGeneration, connection::ProjectionConnection};

/// Exact connection membership owned by one projection-service generation.
///
/// The generation is carried by the synchronization boundary itself so recovery adoption cannot
/// accidentally exchange membership through an untyped shared vector.
pub(super) struct ProjectionServiceConnectionRegistry {
    service_generation: ProjectionServiceGeneration,
    connections: Mutex<Vec<Arc<ProjectionConnection>>>,
}

impl ProjectionServiceConnectionRegistry {
    pub(super) fn new(service_generation: ProjectionServiceGeneration) -> Arc<Self> {
        Arc::new(Self {
            service_generation,
            connections: Mutex::new(Vec::new()),
        })
    }

    pub(super) const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    pub(super) fn lock(&self) -> LockResult<MutexGuard<'_, Vec<Arc<ProjectionConnection>>>> {
        self.connections.lock()
    }

    /// Reaps only completed ordinary retirements without holding the service registry across a
    /// connection lifecycle boundary.
    pub(super) fn reap_finished_ordinary_retirements(&self) {
        let snapshot = self
            .connections
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
        self.connections
            .lock()
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
            panic!("poison service connection registry for adoption test");
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
                &self.connections.lock().map(|connections| connections.len()),
            )
            .finish_non_exhaustive()
    }
}
