use std::sync::{Arc, Mutex, MutexGuard};

use beryl_backend::{
    CheckedSteeringUserMessage, CheckedSteeringUserMessageSubmitError, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamRejection, OrderedTurnStreamSink,
    OrderedTurnStreamSubmitCause, OrderedTurnStreamSubmitError, SteeringUserMessageAbandonReason,
    SteeringUserMessageSelection, SteeringUserMessageSelectionError, SteeringUserMessageSource,
};
use beryl_model::CasThreadId;

use super::{ConnectionRegistryAuthority, ConnectionServiceEpoch, ConnectionThreadClosedOutcome};
use crate::cas_projection::{
    ProjectionCoordinatorError, ProjectionRegistryKind, ProjectionServiceGeneration,
};

pub(super) struct ForwardingEpochEndpoint {
    epoch: Arc<ConnectionServiceEpoch>,
    sink: Box<dyn OrderedTurnStreamSink>,
}

struct ForwardingHubState {
    endpoint: Option<ForwardingEpochEndpoint>,
    inert: bool,
}

/// Backend-bound stable forwarding barrier for one exact connection generation.
pub(super) struct ForwardingHub {
    authority: Arc<ConnectionRegistryAuthority>,
    state: Mutex<ForwardingHubState>,
}

pub(super) struct ForwardingHubEpochGuard<'a> {
    state: MutexGuard<'a, ForwardingHubState>,
}

#[cfg(test)]
struct ForwardingHubLockAttemptHook {
    connection_generation: u64,
    reached: std::sync::mpsc::SyncSender<()>,
}

#[cfg(test)]
pub(in crate::cas_projection) struct ForwardingHubLockAttemptObservation {
    reached: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
static FORWARDING_HUB_LOCK_ATTEMPT_HOOK: std::sync::OnceLock<
    Mutex<Option<ForwardingHubLockAttemptHook>>,
> = std::sync::OnceLock::new();

struct ForwardingHubSink {
    hub: Arc<ForwardingHub>,
}

impl ForwardingEpochEndpoint {
    pub(super) fn new(
        epoch: Arc<ConnectionServiceEpoch>,
        sink: Box<dyn OrderedTurnStreamSink>,
    ) -> Self {
        Self { epoch, sink }
    }

    pub(super) fn service_generation(&self) -> ProjectionServiceGeneration {
        self.epoch.identity.service_generation()
    }

    pub(super) fn epoch(&self) -> &Arc<ConnectionServiceEpoch> {
        &self.epoch
    }
}

impl ForwardingHub {
    pub(super) fn new(authority: Arc<ConnectionRegistryAuthority>) -> Arc<Self> {
        Arc::new(Self {
            authority,
            state: Mutex::new(ForwardingHubState {
                endpoint: None,
                inert: false,
            }),
        })
    }

    pub(super) fn bind_sink(self: &Arc<Self>) -> Box<dyn OrderedTurnStreamSink> {
        Box::new(ForwardingHubSink {
            hub: Arc::clone(self),
        })
    }

    pub(super) fn install_initial(
        &self,
        endpoint: ForwardingEpochEndpoint,
    ) -> Result<(), ProjectionCoordinatorError> {
        let mut state = self.lock_state()?;
        if state.inert || state.endpoint.is_some() {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        state.endpoint = Some(endpoint);
        Ok(())
    }

    pub(super) fn lock_epoch(
        &self,
    ) -> Result<ForwardingHubEpochGuard<'_>, ProjectionCoordinatorError> {
        #[cfg(test)]
        observe_forwarding_hub_lock_attempt(self.authority.generation.get());
        Ok(ForwardingHubEpochGuard {
            state: self.lock_state()?,
        })
    }

    pub(super) fn record_thread_closed(
        &self,
        thread_id: &CasThreadId,
    ) -> Result<ConnectionThreadClosedOutcome, ProjectionCoordinatorError> {
        let mut state = self.lock_state()?;
        if state.inert {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        let endpoint = state
            .endpoint
            .as_mut()
            .ok_or(ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        super::record_connection_thread_closed(&self.authority, &endpoint.epoch.router, thread_id)
    }

    pub(super) fn current_epoch(
        &self,
    ) -> Result<Arc<ConnectionServiceEpoch>, ProjectionCoordinatorError> {
        let state = self.lock_state()?;
        if state.inert {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        state
            .endpoint
            .as_ref()
            .map(|endpoint| Arc::clone(&endpoint.epoch))
            .ok_or(ProjectionCoordinatorError::ProjectionWorkerStopped)
    }

    pub(super) fn is_detached(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.endpoint.is_none())
    }

    /// Recovers the epoch barrier even after poison and atomically removes its executable endpoint.
    ///
    /// The returned endpoint remains the caller's exact inert ownership attachment. Any work that
    /// cancellation can wake must happen after this method releases the hub lock.
    pub(super) fn detach_inert_recovering_poison(&self) -> Option<ForwardingEpochEndpoint> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.inert = true;
        state.endpoint.take()
    }

    /// Recovers the epoch barrier after poison and terminalizes it without moving its endpoint.
    ///
    /// This is the allocation-free fallback used when adoption cannot reserve storage for a
    /// detached endpoint. The returned epoch clone lets the caller request cancellation after the
    /// hub lock is released; explicit disposition remains able to detach and join the resident
    /// endpoint later.
    pub(super) fn mark_inert_in_place_recovering_poison(
        &self,
    ) -> Option<Arc<ConnectionServiceEpoch>> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.inert = true;
        state
            .endpoint
            .as_ref()
            .map(|endpoint| Arc::clone(endpoint.epoch()))
    }

    #[cfg(test)]
    pub(super) fn poison_epoch_barrier_for_test(&self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = self.state.lock().unwrap();
            panic!("poison forwarding-hub epoch barrier for test");
        }));
    }

    #[cfg(test)]
    pub(super) fn is_inert_and_detached_recovering_poison_for_test(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.inert && state.endpoint.is_none()
    }

    #[cfg(test)]
    pub(super) fn is_inert_and_attached_recovering_poison_for_test(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.inert && state.endpoint.is_some()
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, ForwardingHubState>, ProjectionCoordinatorError> {
        self.state
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionConnection,
            })
    }
}

#[cfg(test)]
impl super::ProjectionConnection {
    pub(in crate::cas_projection) fn observe_next_forwarding_hub_lock_attempt_for_test(
        &self,
    ) -> ForwardingHubLockAttemptObservation {
        let (reached, receiver) = std::sync::mpsc::sync_channel(1);
        let slot = FORWARDING_HUB_LOCK_ATTEMPT_HOOK.get_or_init(|| Mutex::new(None));
        let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
        assert!(
            slot.is_none(),
            "only one forwarding-hub lock observation may be armed"
        );
        *slot = Some(ForwardingHubLockAttemptHook {
            connection_generation: self.authority.generation.get(),
            reached,
        });
        ForwardingHubLockAttemptObservation { reached: receiver }
    }
}

#[cfg(test)]
impl ForwardingHubLockAttemptObservation {
    pub(in crate::cas_projection) fn wait(self) {
        self.reached
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the exact forwarding hub must reach its epoch-lock attempt");
    }
}

#[cfg(test)]
fn observe_forwarding_hub_lock_attempt(connection_generation: u64) {
    let slot = FORWARDING_HUB_LOCK_ATTEMPT_HOOK.get_or_init(|| Mutex::new(None));
    let hook = {
        let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
        if slot
            .as_ref()
            .is_some_and(|hook| hook.connection_generation == connection_generation)
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(hook) = hook {
        let _ = hook.reached.send(());
    }
}

impl ForwardingHubEpochGuard<'_> {
    pub(super) fn epoch(&self) -> Option<&Arc<ConnectionServiceEpoch>> {
        self.state
            .endpoint
            .as_ref()
            .map(ForwardingEpochEndpoint::epoch)
    }

    pub(super) fn service_generation(&self) -> Option<ProjectionServiceGeneration> {
        self.state
            .endpoint
            .as_ref()
            .map(ForwardingEpochEndpoint::service_generation)
    }

    pub(super) fn replace(
        &mut self,
        endpoint: ForwardingEpochEndpoint,
    ) -> Option<ForwardingEpochEndpoint> {
        debug_assert!(!self.state.inert);
        self.state.endpoint.replace(endpoint)
    }

    pub(super) fn mark_inert(&mut self) -> Option<ForwardingEpochEndpoint> {
        self.state.inert = true;
        self.state.endpoint.take()
    }

    pub(super) fn mark_inert_in_place(&mut self) {
        self.state.inert = true;
    }

    pub(super) fn is_inert(&self) -> bool {
        self.state.inert
    }
}

impl ForwardingHubSink {
    fn record_thread_closed(
        &self,
        thread_id: &CasThreadId,
    ) -> Result<OrderedTurnStreamCompletion, ()> {
        self.hub
            .record_thread_closed(thread_id)
            .ok()
            .filter(|outcome| !outcome.connection_retired())
            .map(|_| OrderedTurnStreamCompletion::Applied)
            .ok_or(())
    }
}

impl OrderedTurnStreamSink for ForwardingHubSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        if let OrderedTurnStreamOperation::ThreadClosed(closed) = &operation {
            return self.record_thread_closed(closed.thread_id()).map_err(|()| {
                OrderedTurnStreamSubmitError::new(
                    operation,
                    OrderedTurnStreamSubmitCause::Rejected(
                        OrderedTurnStreamRejection::InvalidControl,
                    ),
                )
            });
        }
        let mut state = match self.hub.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return Err(OrderedTurnStreamSubmitError::new(
                    operation,
                    OrderedTurnStreamSubmitCause::Cancelled,
                ));
            }
        };
        if state.inert {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Cancelled,
            ));
        }
        let Some(endpoint) = state.endpoint.as_mut() else {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Cancelled,
            ));
        };
        endpoint.sink.submit(operation)
    }

    fn select_steering_user_message(
        &mut self,
        selection: SteeringUserMessageSelection,
    ) -> Result<SteeringUserMessageSource, SteeringUserMessageSelectionError> {
        let mut state = match self.hub.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return Err(SteeringUserMessageSelectionError::new(
                    selection,
                    OrderedTurnStreamSubmitCause::Cancelled,
                ));
            }
        };
        if state.inert {
            return Err(SteeringUserMessageSelectionError::new(
                selection,
                OrderedTurnStreamSubmitCause::Cancelled,
            ));
        }
        let Some(endpoint) = state.endpoint.as_mut() else {
            return Err(SteeringUserMessageSelectionError::new(
                selection,
                OrderedTurnStreamSubmitCause::Cancelled,
            ));
        };
        endpoint.sink.select_steering_user_message(selection)
    }

    fn submit_checked_steering_user_message(
        &mut self,
        message: CheckedSteeringUserMessage,
    ) -> Result<(), CheckedSteeringUserMessageSubmitError> {
        let mut state = match self.hub.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return Err(CheckedSteeringUserMessageSubmitError::new(
                    message,
                    OrderedTurnStreamSubmitCause::Cancelled,
                ));
            }
        };
        if state.inert {
            return Err(CheckedSteeringUserMessageSubmitError::new(
                message,
                OrderedTurnStreamSubmitCause::Cancelled,
            ));
        }
        let Some(endpoint) = state.endpoint.as_mut() else {
            return Err(CheckedSteeringUserMessageSubmitError::new(
                message,
                OrderedTurnStreamSubmitCause::Cancelled,
            ));
        };
        endpoint.sink.submit_checked_steering_user_message(message)
    }

    fn abandon_steering_user_message(
        &mut self,
        reason: SteeringUserMessageAbandonReason,
    ) -> Result<(), OrderedTurnStreamSubmitCause> {
        let mut state = self
            .hub
            .state
            .lock()
            .map_err(|_| OrderedTurnStreamSubmitCause::Cancelled)?;
        if state.inert {
            return Err(OrderedTurnStreamSubmitCause::Cancelled);
        }
        let Some(endpoint) = state.endpoint.as_mut() else {
            return Err(OrderedTurnStreamSubmitCause::Cancelled);
        };
        endpoint.sink.abandon_steering_user_message(reason)
    }
}
