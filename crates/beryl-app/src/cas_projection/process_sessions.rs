use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, Weak},
};

use beryl_home_store::HomeGeneration;
use beryl_model::{BerylHomeId, ExecutionBinding, SyndicThreadId};
use beryl_state::AssetState;
use thiserror::Error;

use super::{
    AdmittedProjectionSession, LiveCommandAuthorizer, OrdinaryDynamicToolAuthority,
    ProjectionServiceConfig, ProjectionServiceGeneration, ScheduledOrdinaryRequestPolicy,
    accepted_input_scheduler::{AcceptedInputSchedulerSignal, AcceptedInputWakeReason},
    connection::ProjectionConnection,
    service_config::CONNECTION_WORKER_PERMITS,
    service_registry::ProjectionServiceConnectionRegistry,
};

mod checkout;
mod control;
mod work_facts;
pub use work_facts::{
    ScheduledSessionFact, ScheduledSessionPreparationFact, ScheduledSessionWorkCursor,
    ScheduledSessionWorkError, ScheduledSessionWorkPage, ScheduledSessionWorkPageLimits,
    ScheduledSessionWorkRecord, ScheduledSessionWorkRevision, ScheduledSessionWorkState,
};
pub(in crate::cas_projection) mod preparation;
pub use preparation::{
    RuntimeSessionPreparationConfig, RuntimeSessionPreparationError, RuntimeTokenDirectories,
};

#[derive(Clone)]
pub struct ScheduledExecutionProviderContext {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
    capacity: usize,
    commands: LiveCommandAuthorizer,
    connections: Weak<ProjectionServiceConnectionRegistry>,
    ready: ExecutionReadyNotifier,
}

#[derive(Clone)]
struct ExecutionReadyNotifier {
    commands: LiveCommandAuthorizer,
    signal: AcceptedInputSchedulerSignal,
}

impl ExecutionReadyNotifier {
    fn notify(&self) {
        if self.commands.is_open() {
            self.signal.wake(AcceptedInputWakeReason::ExecutionReady);
        }
    }
}

impl ScheduledExecutionProviderContext {
    pub(in crate::cas_projection) fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        config: ProjectionServiceConfig,
        commands: LiveCommandAuthorizer,
        connections: Weak<ProjectionServiceConnectionRegistry>,
        signal: AcceptedInputSchedulerSignal,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            service_generation,
            capacity: config.worker_capacity().get() / CONNECTION_WORKER_PERMITS,
            commands: commands.clone(),
            connections,
            ready: ExecutionReadyNotifier { commands, signal },
        }
    }

    fn owns(&self, session: &AdmittedProjectionSession) -> bool {
        let Some(connections) = self.connections.upgrade() else {
            return false;
        };
        let Ok(connections) = connections.lock() else {
            return false;
        };
        !session.connection().is_retired()
            && !session.connection().is_detached()
            && connections
                .iter()
                .any(|connection| Arc::ptr_eq(connection, session.connection()))
    }
}

pub struct ProcessScheduledExecutionProvider {
    sessions: ScheduledExecutionSessions,
}

#[derive(Clone)]
pub struct ScheduledExecutionSessions {
    state: Arc<Mutex<SessionState>>,
    work_identity: Arc<()>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledSessionRegistration {
    service_generation: ProjectionServiceGeneration,
    thread_id: SyndicThreadId,
    serial: u64,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ScheduledSessionRegistrationError {
    #[error("the execution provider is not attached to a service")]
    Unattached,
    #[error("the execution session owner is closed")]
    Closed,
    #[error("the thread already has an execution session reservation")]
    ThreadOccupied,
    #[error("the execution session reservation capacity is full")]
    CapacityFull,
    #[error("the admitted session does not belong to this healthy service and runtime")]
    SessionAuthorityUnavailable,
    #[error("execution sessions require non-ephemeral thread policy")]
    EphemeralThreadPolicy,
    #[error("execution session registration identities are exhausted")]
    RegistrationExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledSessionDiagnostics {
    pub capacity: usize,
    pub retained: usize,
    pub available: usize,
    pub checked_out: usize,
    pub retiring: usize,
    pub high_water: usize,
    pub closed: bool,
}

struct SessionState {
    context: Option<ScheduledExecutionProviderContext>,
    closed: bool,
    next_serial: u64,
    high_water: usize,
    work_revision: Option<u64>,
    slots: BTreeMap<SyndicThreadId, SessionSlot>,
    preparation: Option<Arc<preparation::PreparationContext>>,
    preparing: BTreeMap<SyndicThreadId, preparation::PreparationWorker>,
}

impl SessionState {
    fn work_changed(&mut self) {
        self.work_revision = self
            .work_revision
            .and_then(|revision| revision.checked_add(1));
    }
}

struct SessionSlot {
    registration: ScheduledSessionRegistration,
    binding: ExecutionBinding,
    policy: ScheduledOrdinaryRequestPolicy,
    assets: AssetState,
    connection: Arc<ProjectionConnection>,
    resources: Option<SessionResources>,
    checked_out: bool,
    retiring: bool,
}

struct SessionResources {
    session: AdmittedProjectionSession,
    tools: Box<dyn OrdinaryDynamicToolAuthority>,
}

impl ProcessScheduledExecutionProvider {
    pub fn new() -> (Self, ScheduledExecutionSessions) {
        let sessions = ScheduledExecutionSessions {
            work_identity: Arc::new(()),
            state: Arc::new(Mutex::new(SessionState {
                context: None,
                closed: false,
                next_serial: 0,
                high_water: 0,
                work_revision: Some(1),
                slots: BTreeMap::new(),
                preparation: None,
                preparing: BTreeMap::new(),
            })),
        };
        (
            Self {
                sessions: sessions.clone(),
            },
            sessions,
        )
    }
}

impl ScheduledExecutionSessions {
    fn lock(&self) -> MutexGuard<'_, SessionState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                if !state.closed {
                    state.work_changed();
                }
                state.closed = true;
                state
            }
        }
    }

    fn reap(&self) {
        self.reap_preparation();
        let resources: Vec<_> = {
            let mut state = self.lock();
            let was_closed = state.closed;
            if state
                .context
                .as_ref()
                .is_some_and(|context| !context.commands.is_open())
            {
                state.closed = true;
            }
            let closed = state.closed;
            let mut changed = was_closed != closed;
            let resources = state
                .slots
                .values_mut()
                .filter_map(|slot| {
                    if closed || slot.connection.is_retired() || slot.connection.is_detached() {
                        changed |= !slot.retiring;
                        slot.retiring = true;
                    }
                    slot.retiring.then(|| slot.resources.take()).flatten()
                })
                .collect();
            if changed {
                state.work_changed();
            }
            resources
        };
        drop(resources);
        let candidates: Vec<_> = self
            .lock()
            .slots
            .values()
            .filter(|slot| slot.retiring && !slot.checked_out && slot.resources.is_none())
            .map(|slot| (slot.registration, Arc::clone(&slot.connection)))
            .collect();
        for (registration, connection) in candidates {
            if !connection.try_reap_ordinary_retirement().unwrap_or(false)
                && !connection.is_detached()
            {
                continue;
            }
            let ready = {
                let mut state = self.lock();
                if state
                    .slots
                    .get(&registration.thread_id)
                    .is_some_and(|slot| {
                        slot.registration == registration
                            && slot.retiring
                            && !slot.checked_out
                            && slot.resources.is_none()
                    })
                {
                    state.slots.remove(&registration.thread_id);
                    state.work_changed();
                    state.context.as_ref().map(|context| context.ready.clone())
                } else {
                    None
                }
            };
            if let Some(ready) = ready {
                ready.notify();
            }
        }
    }
}
