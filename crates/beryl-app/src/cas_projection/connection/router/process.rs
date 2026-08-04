use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use beryl_model::{CasProcessGeneration, RuntimeId};

use super::{LiveEventConnectionState, LiveEventTargetCloseReason};
use crate::cas_projection::{ProjectionCoordinatorError, ProjectionRegistryKind};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ProcessKey {
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
}

/// Latest bounded lifecycle fact published by one exact projection connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiveEventConnectionFact {
    connection_generation: u64,
    state: LiveEventConnectionState,
}

impl LiveEventConnectionFact {
    /// Returns the exact process-local connection generation.
    #[must_use]
    pub const fn connection_generation(self) -> u64 {
        self.connection_generation
    }

    /// Returns the connection state observed by the process projection.
    #[must_use]
    pub const fn state(self) -> LiveEventConnectionState {
        self.state
    }
}

/// Bounded shared facts for one exact runtime process generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveEventProcessSnapshot {
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
    revision: u64,
    active_connection_count: usize,
    latest_connection_fact: Option<LiveEventConnectionFact>,
}

impl LiveEventProcessSnapshot {
    /// Returns the exact configured runtime.
    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    /// Returns the exact managed-process generation.
    #[must_use]
    pub const fn process_generation(&self) -> CasProcessGeneration {
        self.process_generation
    }

    /// Returns the monotonic shared-fact revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the number of currently live projection connections.
    #[must_use]
    pub const fn active_connection_count(&self) -> usize {
        self.active_connection_count
    }

    /// Returns the latest bounded connection lifecycle transition.
    #[must_use]
    pub const fn latest_connection_fact(&self) -> Option<LiveEventConnectionFact> {
        self.latest_connection_fact
    }
}

#[derive(Debug)]
struct ProcessState {
    revision: u64,
    active_connections: HashSet<u64>,
    latest_connection_fact: Option<LiveEventConnectionFact>,
}

#[derive(Debug)]
pub(super) struct ProcessEventProjection {
    key: ProcessKey,
    state: Mutex<ProcessState>,
}

#[derive(Debug)]
struct StableConnectionFactInner {
    process: Arc<ProcessEventProjection>,
    connection_generation: u64,
    retired: AtomicBool,
}

/// Stable-core ownership of one connection fact in its managed process.
pub(in crate::cas_projection::connection) struct StableConnectionProcessFact {
    inner: StableConnectionFactInner,
}

/// Read-only process projection supplied to replaceable epoch routers.
#[derive(Clone, Debug)]
pub(in crate::cas_projection::connection) struct ProcessEventObservation {
    process: Arc<ProcessEventProjection>,
}

#[derive(Default)]
struct ProcessRegistry {
    projections: HashMap<ProcessKey, Weak<ProcessEventProjection>>,
}

static PROCESS_PROJECTIONS: OnceLock<Mutex<ProcessRegistry>> = OnceLock::new();

pub(super) fn acquire_process_projection(
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
) -> Result<Arc<ProcessEventProjection>, ProjectionCoordinatorError> {
    let key = ProcessKey {
        runtime_id,
        process_generation,
    };
    let registry = PROCESS_PROJECTIONS.get_or_init(|| Mutex::new(ProcessRegistry::default()));
    let mut registry =
        registry
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::LiveEventProcess,
            })?;
    registry
        .projections
        .retain(|_, projection| projection.strong_count() > 0);
    if let Some(projection) = registry.projections.get(&key).and_then(Weak::upgrade) {
        return Ok(projection);
    }
    let projection = Arc::new(ProcessEventProjection {
        key,
        state: Mutex::new(ProcessState {
            revision: 0,
            active_connections: HashSet::new(),
            latest_connection_fact: None,
        }),
    });
    registry
        .projections
        .insert(key, Arc::downgrade(&projection));
    Ok(projection)
}

impl StableConnectionProcessFact {
    pub(in crate::cas_projection::connection) fn register(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        connection_generation: u64,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let process = acquire_process_projection(runtime_id, process_generation)?;
        process.register_connection(connection_generation)?;
        Ok(Self {
            inner: StableConnectionFactInner {
                process,
                connection_generation,
                retired: AtomicBool::new(false),
            },
        })
    }

    pub(in crate::cas_projection::connection) fn observe(&self) -> ProcessEventObservation {
        ProcessEventObservation {
            process: Arc::clone(&self.inner.process),
        }
    }

    pub(in crate::cas_projection::connection) fn retire(&self, reason: LiveEventTargetCloseReason) {
        self.inner.retire(reason);
    }
}

impl ProcessEventObservation {
    pub(super) fn snapshot(&self) -> Result<LiveEventProcessSnapshot, ProjectionCoordinatorError> {
        self.process.snapshot()
    }
}

impl StableConnectionFactInner {
    fn retire(&self, reason: LiveEventTargetCloseReason) {
        if !self.retired.swap(true, Ordering::AcqRel) {
            self.process
                .retire_connection(self.connection_generation, reason);
        }
    }
}

impl Drop for StableConnectionProcessFact {
    fn drop(&mut self) {
        self.inner.retire(LiveEventTargetCloseReason::WorkerStopped);
    }
}

impl ProcessEventProjection {
    pub(super) fn register_connection(
        &self,
        connection_generation: u64,
    ) -> Result<(), ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        if state.active_connections.insert(connection_generation) {
            state.latest_connection_fact = Some(LiveEventConnectionFact {
                connection_generation,
                state: LiveEventConnectionState::Active,
            });
            advance_revision(&mut state);
        }
        Ok(())
    }

    pub(super) fn retire_connection(
        &self,
        connection_generation: u64,
        reason: LiveEventTargetCloseReason,
    ) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if state.active_connections.remove(&connection_generation) {
            state.latest_connection_fact = Some(LiveEventConnectionFact {
                connection_generation,
                state: LiveEventConnectionState::Retired(reason),
            });
            advance_revision(&mut state);
        }
    }

    pub(super) fn snapshot(&self) -> Result<LiveEventProcessSnapshot, ProjectionCoordinatorError> {
        let state = self.lock()?;
        Ok(LiveEventProcessSnapshot {
            runtime_id: self.key.runtime_id,
            process_generation: self.key.process_generation,
            revision: state.revision,
            active_connection_count: state.active_connections.len(),
            latest_connection_fact: state.latest_connection_fact,
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ProcessState>, ProjectionCoordinatorError> {
        self.state
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::LiveEventProcess,
            })
    }
}

fn advance_revision(state: &mut ProcessState) {
    state.revision = state.revision.saturating_add(1);
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/stable_connection_process_fact.rs"
    ));
}
