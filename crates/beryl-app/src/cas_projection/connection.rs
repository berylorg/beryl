use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use beryl_backend::{ManagedBackendError, ManagedBackendSession, ThreadUnsubscribeStatus};
use beryl_model::{
    CasLoadedSessionGeneration, CasProcessGeneration, CasThreadId, RuntimeId, SyndicThreadId,
};
use thiserror::Error;

use crate::cas_projection::{ProjectionCoordinatorError, ProjectionExecutionError};

mod driver;
mod lease;
pub(super) mod registry;
mod router;
mod target_command;

pub use router::{
    LIVE_EVENT_TARGET_QUEUE_BYTE_LIMIT, LIVE_EVENT_TARGET_QUEUE_COUNT_LIMIT,
    LiveEventConnectionFact, LiveEventConnectionState, LiveEventPoll, LiveEventProcessSnapshot,
    LiveEventRouterSnapshot, LiveEventTarget, LiveEventTargetCloseReason, LiveEventTargetError,
    LiveEventTargetRegistrationError, RoutedLiveEvent,
};

use driver::ConnectionDriver;
pub(in crate::cas_projection) use driver::{
    ConnectionCommandOutcome, ConnectionRequestSession, ConnectionRoutingFailure,
};
pub(super) use lease::{ExistingLease, LoadedProjectionLease, ThreadRetirement};
pub(in crate::cas_projection) use router::LiveEventTargetHandoffError;
use router::{EventRouter, TargetRegistration};
pub(in crate::cas_projection) use target_command::TargetTurnStartOutcome;
pub(in crate::cas_projection) use target_command::turn_start_allows_not_started;

use registry::{
    ConnectionGeneration, ExistingSubscription, LeaseToken, LoadedThreadKey, ObservedSubscription,
    allocate_connection_generation,
};

#[derive(Debug)]
pub(super) struct ConnectionRegistryAuthority {
    generation: ConnectionGeneration,
    retired: AtomicBool,
    gate: Mutex<()>,
}

impl ConnectionRegistryAuthority {
    pub(super) fn new() -> Result<Self, ProjectionCoordinatorError> {
        Ok(Self {
            generation: allocate_connection_generation()?,
            retired: AtomicBool::new(false),
            gate: Mutex::new(()),
        })
    }

    pub(super) fn is_retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }

    pub(super) fn register_new(
        &self,
        key: LoadedThreadKey,
        owner: SyndicThreadId,
    ) -> Result<Option<(CasLoadedSessionGeneration, LeaseToken)>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        registry::register_new(key, self.generation, owner).map(Some)
    }

    pub(super) fn acquire_existing(
        &self,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
    ) -> Result<Option<ExistingSubscription>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        registry::acquire_existing(key, self.generation, owner).map(Some)
    }

    pub(super) fn retire(&self) {
        let _gate = match self.gate.lock() {
            Ok(gate) => gate,
            Err(poisoned) => poisoned.into_inner(),
        };
        self.retired.store(true, Ordering::Release);
        let _ = registry::invalidate_connection(self.generation);
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, ProjectionCoordinatorError> {
        match self.gate.lock() {
            Ok(gate) => Ok(gate),
            Err(poisoned) => {
                let gate = poisoned.into_inner();
                self.retired.store(true, Ordering::Release);
                let _ = registry::invalidate_connection(self.generation);
                drop(gate);
                Err(ProjectionCoordinatorError::RegistryPoisoned {
                    registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
                })
            }
        }
    }

    #[cfg(test)]
    pub(super) fn lock_for_test(&self) -> std::sync::MutexGuard<'_, ()> {
        self.gate.lock().unwrap()
    }
}

/// Non-authorizing outcome of consuming one loaded-projection lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadedProjectionReleaseOutcome {
    /// Another exact local lease still owns the same CAS subscription.
    SharedSubscriptionRemains,
    /// The exact local lease was already revoked by a broader invalidation.
    AlreadyRevoked,
    /// CAS classified the exact connection-scoped unsubscribe request.
    Unsubscribe(ThreadUnsubscribeStatus),
    /// Local authority was revoked and the whole connection was retired.
    ConnectionRetired,
}

/// Failure after local authority for a consumed projection lease was revoked.
#[derive(Debug, Error)]
pub enum LoadedProjectionReleaseError {
    #[error("loaded-projection registry could not revoke the exact lease")]
    Registry(#[source] ProjectionCoordinatorError),
    #[error("thread/unsubscribe failed after local projection authority was revoked")]
    Backend(#[source] Box<ManagedBackendError>),
    #[error("live-event target {thread_id} closed while thread/unsubscribe completed: {reason:?}")]
    LiveEventRouting {
        thread_id: CasThreadId,
        reason: LiveEventTargetCloseReason,
    },
}

#[derive(Debug)]
pub(super) struct ProjectionConnection {
    authority: Arc<ConnectionRegistryAuthority>,
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
    router: Arc<EventRouter>,
    driver: ConnectionDriver,
}

impl ProjectionConnection {
    pub(super) fn new(
        backend: ManagedBackendSession,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Result<Arc<Self>, ProjectionCoordinatorError> {
        let authority = Arc::new(ConnectionRegistryAuthority::new()?);
        let router = Arc::new(EventRouter::new(
            runtime_id,
            process_generation,
            authority.generation.get(),
        )?);
        let driver = ConnectionDriver::start(backend, Arc::clone(&authority), Arc::clone(&router))?;
        Ok(Arc::new(Self {
            authority,
            runtime_id,
            process_generation,
            router,
            driver,
        }))
    }

    pub(super) const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    pub(super) const fn process_generation(&self) -> CasProcessGeneration {
        self.process_generation
    }

    pub(super) fn call<T>(
        &self,
        operation: impl FnOnce(&mut ConnectionRequestSession<'_>) -> Result<T, ManagedBackendError>
        + Send
        + 'static,
    ) -> Result<T, ProjectionExecutionError>
    where
        T: Send + 'static,
    {
        let command = self.call_ordered(operation)?;
        self.publish_ordered_result(command)
    }

    fn publish_ordered_result<T>(
        &self,
        command: ConnectionCommandOutcome<Result<T, ManagedBackendError>>,
    ) -> Result<T, ProjectionExecutionError> {
        let (operation_result, routing_failure) = command.into_parts();
        if operation_result
            .as_ref()
            .is_err_and(ManagedBackendError::invalidates_connection_authority)
        {
            let Err(error) = operation_result else {
                unreachable!("invalidating operation result must be an error")
            };
            return Err(ProjectionExecutionError::from(error));
        }
        match routing_failure {
            Some(ConnectionRoutingFailure::Backend | ConnectionRoutingFailure::Router) => {
                Err(self.unavailable().into())
            }
            Some(ConnectionRoutingFailure::Target { thread_id, reason }) => {
                Err(ProjectionExecutionError::LiveEventRouting { thread_id, reason })
            }
            None => operation_result.map_err(ProjectionExecutionError::from),
        }
    }

    pub(in crate::cas_projection) fn call_ordered<T>(
        &self,
        operation: impl FnOnce(&mut ConnectionRequestSession<'_>) -> Result<T, ManagedBackendError>
        + Send
        + 'static,
    ) -> Result<ConnectionCommandOutcome<Result<T, ManagedBackendError>>, ProjectionCoordinatorError>
    where
        T: Send + 'static,
    {
        if self.authority.is_retired() {
            return Err(self.unavailable());
        }
        self.driver.call(operation)
    }

    pub(super) fn retire(&self) {
        self.authority.retire();
        self.router
            .retire(LiveEventTargetCloseReason::ConnectionRetired);
        self.driver.request_stop();
    }

    fn unavailable(&self) -> ProjectionCoordinatorError {
        ProjectionCoordinatorError::ProjectionConnectionUnavailable {
            runtime_id: self.runtime_id,
            process_generation: self.process_generation,
        }
    }

    fn key(&self, cas_thread_id: CasThreadId) -> LoadedThreadKey {
        LoadedThreadKey {
            runtime_id: self.runtime_id,
            process_generation: self.process_generation,
            cas_thread_id,
        }
    }

    pub(super) fn register_new(
        self: &Arc<Self>,
        cas_thread_id: CasThreadId,
        owner: SyndicThreadId,
        unsubscribe_timeout: Duration,
    ) -> Result<LoadedProjectionLease, ProjectionCoordinatorError> {
        let key = self.key(cas_thread_id);
        let Some((generation, token)) = self.authority.register_new(key.clone(), owner)? else {
            return Err(self.unavailable());
        };
        Ok(LoadedProjectionLease::new(
            Arc::clone(self),
            key,
            owner,
            generation,
            token,
            unsubscribe_timeout,
        ))
    }

    pub(super) fn acquire_existing(
        self: &Arc<Self>,
        cas_thread_id: &CasThreadId,
        owner: SyndicThreadId,
        unsubscribe_timeout: Duration,
    ) -> Result<ExistingLease, ProjectionCoordinatorError> {
        let key = self.key(cas_thread_id.clone());
        let Some(subscription) = self.authority.acquire_existing(&key, owner)? else {
            return Err(self.unavailable());
        };
        Ok(match subscription {
            ExistingSubscription::Absent => ExistingLease::Absent,
            ExistingSubscription::AnotherConnection => ExistingLease::AnotherConnection,
            ExistingSubscription::AnotherOwner { existing_owner } => {
                ExistingLease::AnotherOwner { existing_owner }
            }
            ExistingSubscription::Exact { generation, token } => {
                ExistingLease::Exact(LoadedProjectionLease::new(
                    Arc::clone(self),
                    key,
                    owner,
                    generation,
                    token,
                    unsubscribe_timeout,
                ))
            }
        })
    }

    pub(super) fn live_event_snapshot(
        &self,
    ) -> Result<LiveEventRouterSnapshot, ProjectionCoordinatorError> {
        self.router.snapshot()
    }

    pub(super) fn live_event_process_snapshot(
        &self,
    ) -> Result<LiveEventProcessSnapshot, ProjectionCoordinatorError> {
        self.router.process_snapshot()
    }

    fn register_event_target(
        self: &Arc<Self>,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        token: LeaseToken,
        turn_id: Option<beryl_model::CasTurnId>,
    ) -> Result<TargetRegistration, LiveEventTargetRegistrationError> {
        let gate = self.authority.lock()?;
        if self.authority.is_retired() {
            return Err(LiveEventTargetRegistrationError::ConnectionRetired);
        }
        if !registry::contains_exact(key, self.authority.generation, owner, generation, token)? {
            return Err(LiveEventTargetRegistrationError::ProjectionNotLive);
        }
        let result = self
            .router
            .register(key.clone(), owner, generation, turn_id);
        drop(gate);
        if matches!(
            result,
            Err(LiveEventTargetRegistrationError::ConnectionRetired
                | LiveEventTargetRegistrationError::RouterPoisoned)
        ) {
            self.retire();
        }
        result
    }

    pub(super) fn confirm_target_turn(
        &self,
        registration: &TargetRegistration,
        turn_id: beryl_model::CasTurnId,
    ) -> Result<(), LiveEventTargetError> {
        let result = self.router.confirm_turn(registration, turn_id);
        match &result {
            Err(LiveEventTargetError::ConflictingTurnIdentity { .. }) => {
                self.invalidate_target_generation(registration);
            }
            Err(LiveEventTargetError::ConnectionRetired | LiveEventTargetError::RouterPoisoned) => {
                self.retire();
            }
            _ => {}
        }
        result
    }

    pub(super) fn abandon_target(&self, registration: &TargetRegistration) {
        if self
            .router
            .unregister(registration, LiveEventTargetCloseReason::ReceiverAbandoned)
        {
            self.retire();
        } else {
            self.invalidate_target_generation(registration);
        }
    }

    fn invalidate_target_generation(&self, registration: &TargetRegistration) {
        if registry::invalidate_exact_generation(
            registration.key(),
            self.authority.generation,
            registration.owner(),
            registration.loaded_generation(),
        )
        .is_err()
        {
            self.retire();
        }
    }

    pub(super) fn retire_thread(
        &self,
        cas_thread_id: &CasThreadId,
        owner: SyndicThreadId,
        timeout: Duration,
    ) -> Result<ThreadRetirement, LoadedProjectionReleaseError> {
        let key = self.key(cas_thread_id.clone());
        let observed = match registry::invalidate_thread(&key, self.authority.generation, owner) {
            Ok(observed) => observed,
            Err(error) => {
                self.retire();
                return Err(LoadedProjectionReleaseError::Registry(error));
            }
        };
        match observed {
            ObservedSubscription::Absent => Ok(ThreadRetirement::Absent),
            ObservedSubscription::AnotherConnection => Ok(ThreadRetirement::AnotherConnection),
            ObservedSubscription::AnotherOwner { existing_owner } => {
                Ok(ThreadRetirement::AnotherOwner { existing_owner })
            }
            ObservedSubscription::Exact(generation) => {
                self.try_unsubscribe(cas_thread_id, timeout)?;
                let _ = generation;
                Ok(ThreadRetirement::Retired)
            }
        }
    }

    fn try_unsubscribe(
        &self,
        thread_id: &CasThreadId,
        timeout: Duration,
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        if self.authority.is_retired() {
            return Ok(LoadedProjectionReleaseOutcome::ConnectionRetired);
        }
        let thread_id = thread_id.clone();
        let (operation_result, routing_failure) = self
            .driver
            .call(move |session| session.unsubscribe_thread(&thread_id, timeout))
            .map_err(LoadedProjectionReleaseError::Registry)?
            .into_parts();
        if !operation_result
            .as_ref()
            .is_err_and(ManagedBackendError::invalidates_connection_authority)
        {
            match routing_failure {
                Some(ConnectionRoutingFailure::Target { thread_id, reason }) => {
                    return Err(LoadedProjectionReleaseError::LiveEventRouting {
                        thread_id,
                        reason,
                    });
                }
                Some(ConnectionRoutingFailure::Backend | ConnectionRoutingFailure::Router) => {
                    return Ok(LoadedProjectionReleaseOutcome::ConnectionRetired);
                }
                None => {}
            }
        }
        match operation_result {
            Ok(response) => Ok(LoadedProjectionReleaseOutcome::Unsubscribe(response.status)),
            Err(error) => Err(LoadedProjectionReleaseError::Backend(Box::new(error))),
        }
    }
}

impl Drop for ProjectionConnection {
    fn drop(&mut self) {
        self.authority.retire();
        self.router
            .retire(LiveEventTargetCloseReason::WorkerStopped);
    }
}
