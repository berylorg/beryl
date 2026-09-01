use std::{
    collections::{HashMap, HashSet},
    num::{NonZeroU64, NonZeroUsize},
    sync::{Arc, Mutex},
};

use beryl_home_store::HomeGeneration;
use beryl_model::{BerylHomeId, BindingRevision, SyndicThreadId};

use super::{
    NativeLineageOperation, NativeLineageRecoveryDecision, ProjectionCancellationToken,
    ProjectionServiceGeneration, ScheduledOrdinaryExecutionLease,
    accepted_input_scheduler::{AcceptedInputSchedulerSignal, AcceptedInputWakeReason},
    scheduled_ordinary::ParkedScheduledOrdinaryExecution,
    service_config::ProjectionWorkerPermit,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeLineageRecoveryKey {
    home_id: BerylHomeId,
    home_generation: Option<HomeGeneration>,
    service_generation: ProjectionServiceGeneration,
    thread_id: SyndicThreadId,
    sequence: NonZeroU64,
}

impl NativeLineageRecoveryKey {
    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLineageRecoveryCommand {
    Retry,
    RecoverFromSyndic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLineageRecoveryStatus {
    Loading,
    Ready {
        recovery_available: bool,
    },
    Running {
        command: NativeLineageRecoveryCommand,
    },
    Failed {
        command: NativeLineageRecoveryCommand,
        recovery_available: bool,
    },
    Unavailable,
    Leaving {
        pending_turn_continues: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeLineageRecoverySnapshot {
    key: NativeLineageRecoveryKey,
    source_thread_id: SyndicThreadId,
    source_binding_revision: BindingRevision,
    operation: NativeLineageOperation,
    failed_attempts: u8,
    status: NativeLineageRecoveryStatus,
}

impl NativeLineageRecoverySnapshot {
    #[must_use]
    pub const fn key(self) -> NativeLineageRecoveryKey {
        self.key
    }

    #[must_use]
    pub const fn source_thread_id(self) -> SyndicThreadId {
        self.source_thread_id
    }

    #[must_use]
    pub const fn source_binding_revision(self) -> BindingRevision {
        self.source_binding_revision
    }

    #[must_use]
    pub const fn operation(self) -> NativeLineageOperation {
        self.operation
    }

    #[must_use]
    pub const fn failed_attempts(self) -> u8 {
        self.failed_attempts
    }

    #[must_use]
    pub const fn status(self) -> NativeLineageRecoveryStatus {
        self.status
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLineageRecoveryCommandError {
    Unavailable,
    Stale,
    NotActionable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum NativeLineageParkDisposition {
    Parked,
    Rejected,
}

pub(in crate::cas_projection) enum NativeLineageRouteReservationError {
    CapacityFull,
    Rejected,
}

struct ParkedContinuation {
    decision: Box<NativeLineageRecoveryDecision>,
    execution: ParkedScheduledOrdinaryExecution,
}

struct RouteState {
    snapshot: NativeLineageRecoverySnapshot,
    command: Option<NativeLineageRecoveryCommand>,
    continuation: Option<ParkedContinuation>,
    test_only: bool,
}

struct Route {
    state: Mutex<RouteState>,
    cancellation: ProjectionCancellationToken,
}

struct ControlState {
    next_sequence: u64,
    closed: bool,
    routes: HashMap<SyndicThreadId, Arc<Route>>,
    reservations: HashSet<SyndicThreadId>,
    capacity_waiting: bool,
}

struct ControlInner {
    capacity: usize,
    home_id: BerylHomeId,
    home_generation: Option<HomeGeneration>,
    service_generation: ProjectionServiceGeneration,
    signal: AcceptedInputSchedulerSignal,
    state: Mutex<ControlState>,
}

#[derive(Clone)]
pub struct NativeLineageRecoveryControl {
    inner: Arc<ControlInner>,
}

pub(in crate::cas_projection) struct NativeLineageRecoveryWork {
    attempt: Option<NativeLineageRecoveryAttempt>,
    command: NativeLineageRecoveryCommand,
    decision: Option<Box<NativeLineageRecoveryDecision>>,
    execution: Option<ScheduledOrdinaryExecutionLease>,
}

pub(in crate::cas_projection) struct NativeLineageRecoveryAttempt {
    control: NativeLineageRecoveryControl,
    route: Arc<Route>,
    key: NativeLineageRecoveryKey,
    retained: bool,
}

pub(in crate::cas_projection) struct NativeLineageRouteReservation {
    control: NativeLineageRecoveryControl,
    thread_id: SyndicThreadId,
    retained: bool,
}

impl NativeLineageRecoveryControl {
    pub(in crate::cas_projection) fn new(
        capacity: NonZeroUsize,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        signal: AcceptedInputSchedulerSignal,
    ) -> Self {
        Self::with_identity(
            capacity,
            home_id,
            Some(home_generation),
            service_generation,
            signal,
        )
    }

    fn with_identity(
        capacity: NonZeroUsize,
        home_id: BerylHomeId,
        home_generation: Option<HomeGeneration>,
        service_generation: ProjectionServiceGeneration,
        signal: AcceptedInputSchedulerSignal,
    ) -> Self {
        Self {
            inner: Arc::new(ControlInner {
                capacity: capacity.get(),
                home_id,
                home_generation,
                service_generation,
                signal,
                state: Mutex::new(ControlState {
                    next_sequence: 1,
                    closed: false,
                    routes: HashMap::with_capacity(capacity.get()),
                    reservations: HashSet::with_capacity(capacity.get()),
                    capacity_waiting: false,
                }),
            }),
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn for_test(capacity: NonZeroUsize) -> Self {
        Self::with_identity(
            capacity,
            BerylHomeId::from_bytes([234; 16]),
            None,
            ProjectionServiceGeneration::allocate()
                .expect("test recovery control obtains a service generation"),
            AcceptedInputSchedulerSignal::new(),
        )
    }

    #[cfg(feature = "test-faults")]
    pub fn install_route_for_test(
        &self,
        thread_id: SyndicThreadId,
        source_thread_id: SyndicThreadId,
        source_binding_revision: BindingRevision,
        operation: NativeLineageOperation,
        failed_attempts: u8,
        recovery_available: bool,
    ) -> Option<NativeLineageRecoveryKey> {
        let mut control = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if control.closed
            || control.routes.len() + control.reservations.len() >= self.inner.capacity
            || control.routes.contains_key(&thread_id)
            || control.reservations.contains(&thread_id)
        {
            return None;
        }
        let key = self.allocate_key(&mut control, thread_id)?;
        control.routes.insert(
            thread_id,
            Arc::new(Route {
                state: Mutex::new(RouteState {
                    snapshot: NativeLineageRecoverySnapshot {
                        key,
                        source_thread_id,
                        source_binding_revision,
                        operation,
                        failed_attempts,
                        status: NativeLineageRecoveryStatus::Ready { recovery_available },
                    },
                    command: None,
                    continuation: None,
                    test_only: true,
                }),
                cancellation: ProjectionCancellationToken::new(),
            }),
        );
        Some(key)
    }

    #[cfg(feature = "test-faults")]
    pub fn take_command_for_test(
        &self,
        key: NativeLineageRecoveryKey,
    ) -> Option<NativeLineageRecoveryCommand> {
        let route = self.route(key.thread_id)?;
        let mut state = route
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.snapshot.key != key {
            return None;
        }
        state.command.take()
    }

    #[cfg(feature = "test-faults")]
    pub fn set_status_for_test(
        &self,
        key: NativeLineageRecoveryKey,
        status: NativeLineageRecoveryStatus,
    ) -> bool {
        let Some(route) = self.route(key.thread_id) else {
            return false;
        };
        let mut state = route
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.snapshot.key != key {
            return false;
        }
        state.snapshot.status = status;
        state.command = None;
        true
    }

    #[cfg(feature = "test-faults")]
    pub fn close_for_test(&self) {
        self.close();
    }

    #[must_use]
    pub fn snapshot_for_thread(
        &self,
        thread_id: SyndicThreadId,
    ) -> Option<NativeLineageRecoverySnapshot> {
        let route = self.route(thread_id)?;
        Some(
            route
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .snapshot,
        )
    }

    pub fn submit(
        &self,
        key: NativeLineageRecoveryKey,
        command: NativeLineageRecoveryCommand,
    ) -> Result<(), NativeLineageRecoveryCommandError> {
        let control = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if control.closed {
            return Err(NativeLineageRecoveryCommandError::Unavailable);
        }
        let route = control
            .routes
            .get(&key.thread_id)
            .cloned()
            .ok_or(NativeLineageRecoveryCommandError::Unavailable)?;
        let mut state = route
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.snapshot.key != key {
            return Err(NativeLineageRecoveryCommandError::Stale);
        }
        let recovery_available = match state.snapshot.status {
            NativeLineageRecoveryStatus::Ready { recovery_available }
            | NativeLineageRecoveryStatus::Failed {
                recovery_available, ..
            } => recovery_available,
            _ => return Err(NativeLineageRecoveryCommandError::NotActionable),
        };
        if (!state.test_only && state.continuation.is_none())
            || (command == NativeLineageRecoveryCommand::RecoverFromSyndic && !recovery_available)
        {
            return Err(NativeLineageRecoveryCommandError::NotActionable);
        }
        state.command = Some(command);
        state.snapshot.status = NativeLineageRecoveryStatus::Running { command };
        drop(state);
        drop(control);
        self.inner
            .signal
            .wake(AcceptedInputWakeReason::NativeLineageReady);
        Ok(())
    }

    pub fn cancel(
        &self,
        key: NativeLineageRecoveryKey,
    ) -> Result<(), NativeLineageRecoveryCommandError> {
        let route = self.remove_exact(key)?;
        route.cancellation.cancel();
        Ok(())
    }

    pub fn acknowledge_leaving(
        &self,
        key: NativeLineageRecoveryKey,
    ) -> Result<(), NativeLineageRecoveryCommandError> {
        let mut control = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let route = control
            .routes
            .get(&key.thread_id)
            .cloned()
            .ok_or(NativeLineageRecoveryCommandError::Unavailable)?;
        let state = route
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.snapshot.key != key {
            return Err(NativeLineageRecoveryCommandError::Stale);
        }
        if !matches!(
            state.snapshot.status,
            NativeLineageRecoveryStatus::Leaving { .. }
        ) {
            return Err(NativeLineageRecoveryCommandError::NotActionable);
        }
        drop(state);
        control.routes.remove(&key.thread_id);
        let wakes_capacity_retry = take_capacity_retry(&mut control, self.inner.capacity);
        drop(control);
        if wakes_capacity_retry {
            self.inner
                .signal
                .wake(AcceptedInputWakeReason::NativeLineageRouteCapacityReleased);
        }
        Ok(())
    }

    pub(in crate::cas_projection) fn park(
        &self,
        mut decision: Box<NativeLineageRecoveryDecision>,
        recovery_available: bool,
        execution: ParkedScheduledOrdinaryExecution,
    ) -> NativeLineageParkDisposition {
        if decision.home_id() != self.inner.home_id
            || Some(decision.home_generation()) != self.inner.home_generation
        {
            return NativeLineageParkDisposition::Rejected;
        }
        let thread_id = decision.target_thread_id();
        let Some(mut reservation) = decision.take_route_reservation() else {
            return NativeLineageParkDisposition::Rejected;
        };
        if !Arc::ptr_eq(&reservation.control.inner, &self.inner)
            || reservation.thread_id != thread_id
        {
            return NativeLineageParkDisposition::Rejected;
        }
        let mut control = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if control.closed
            || control.routes.contains_key(&thread_id)
            || !control.reservations.contains(&thread_id)
        {
            return NativeLineageParkDisposition::Rejected;
        }
        let Some(key) = self.allocate_key(&mut control, thread_id) else {
            return NativeLineageParkDisposition::Rejected;
        };
        if !control.reservations.remove(&thread_id) {
            return NativeLineageParkDisposition::Rejected;
        }
        control.routes.insert(
            thread_id,
            Arc::new(Route {
                state: Mutex::new(RouteState {
                    snapshot: snapshot_for(
                        key,
                        &decision,
                        NativeLineageRecoveryStatus::Ready { recovery_available },
                    ),
                    command: None,
                    continuation: Some(ParkedContinuation {
                        decision,
                        execution,
                    }),
                    test_only: false,
                }),
                cancellation: ProjectionCancellationToken::new(),
            }),
        );
        reservation.retained = true;
        NativeLineageParkDisposition::Parked
    }

    pub(in crate::cas_projection) fn try_reserve_route(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<NativeLineageRouteReservation, NativeLineageRouteReservationError> {
        let mut control = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if control.closed
            || control.routes.contains_key(&thread_id)
            || control.reservations.contains(&thread_id)
        {
            return Err(NativeLineageRouteReservationError::Rejected);
        }
        if control.routes.len() + control.reservations.len() >= self.inner.capacity {
            control.capacity_waiting = true;
            return Err(NativeLineageRouteReservationError::CapacityFull);
        }
        control.reservations.insert(thread_id);
        Ok(NativeLineageRouteReservation {
            control: self.clone(),
            thread_id,
            retained: false,
        })
    }

    pub(in crate::cas_projection) fn has_ready_work(&self) -> bool {
        let control = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        !control.closed
            && control.routes.values().any(|route| {
                let state = route
                    .state
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                state.command.is_some() && state.continuation.is_some()
            })
    }

    pub(in crate::cas_projection) fn take_ready_work(
        &self,
        worker: ProjectionWorkerPermit,
    ) -> Option<NativeLineageRecoveryWork> {
        let control = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if control.closed {
            return None;
        }
        for route in control.routes.values() {
            let mut state = route
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let (Some(command), Some(continuation)) = (state.command, state.continuation.take())
            else {
                continue;
            };
            state.command = None;
            let key = state.snapshot.key;
            drop(state);
            return Some(NativeLineageRecoveryWork {
                attempt: Some(NativeLineageRecoveryAttempt {
                    control: self.clone(),
                    route: Arc::clone(route),
                    key,
                    retained: false,
                }),
                command,
                decision: Some(continuation.decision),
                execution: Some(continuation.execution.resume(worker)),
            });
        }
        None
    }

    pub(in crate::cas_projection) fn close(&self) {
        let routes = {
            let mut control = self
                .inner
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            control.closed = true;
            control.reservations.clear();
            control.capacity_waiting = false;
            std::mem::take(&mut control.routes)
        };
        for route in routes.into_values() {
            route.cancellation.cancel();
        }
        self.inner
            .signal
            .wake(AcceptedInputWakeReason::NativeLineageReady);
    }

    fn allocate_key(
        &self,
        control: &mut ControlState,
        thread_id: SyndicThreadId,
    ) -> Option<NativeLineageRecoveryKey> {
        let sequence = NonZeroU64::new(control.next_sequence)?;
        control.next_sequence = control.next_sequence.checked_add(1).unwrap_or(0);
        Some(NativeLineageRecoveryKey {
            home_id: self.inner.home_id,
            home_generation: self.inner.home_generation,
            service_generation: self.inner.service_generation,
            thread_id,
            sequence,
        })
    }

    fn route(&self, thread_id: SyndicThreadId) -> Option<Arc<Route>> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .routes
            .get(&thread_id)
            .cloned()
    }

    fn remove_exact(
        &self,
        key: NativeLineageRecoveryKey,
    ) -> Result<Arc<Route>, NativeLineageRecoveryCommandError> {
        let mut control = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let route = control
            .routes
            .get(&key.thread_id)
            .cloned()
            .ok_or(NativeLineageRecoveryCommandError::Unavailable)?;
        if route
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .snapshot
            .key
            != key
        {
            return Err(NativeLineageRecoveryCommandError::Stale);
        }
        control.routes.remove(&key.thread_id);
        let wakes_capacity_retry = take_capacity_retry(&mut control, self.inner.capacity);
        drop(control);
        if wakes_capacity_retry {
            self.inner
                .signal
                .wake(AcceptedInputWakeReason::NativeLineageRouteCapacityReleased);
        }
        Ok(route)
    }
}

impl NativeLineageRecoveryWork {
    pub(in crate::cas_projection) fn thread_id(&self) -> SyndicThreadId {
        self.attempt
            .as_ref()
            .expect("unconsumed recovery work retains its route attempt")
            .key
            .thread_id
    }

    pub(in crate::cas_projection) fn into_parts(
        mut self,
    ) -> (
        NativeLineageRecoveryAttempt,
        NativeLineageRecoveryCommand,
        Box<NativeLineageRecoveryDecision>,
        ScheduledOrdinaryExecutionLease,
    ) {
        (
            self.attempt
                .take()
                .expect("unconsumed recovery work retains its route attempt"),
            self.command,
            self.decision
                .take()
                .expect("unconsumed recovery work retains its decision"),
            self.execution
                .take()
                .expect("unconsumed recovery work retains its execution"),
        )
    }
}

impl NativeLineageRecoveryAttempt {
    pub(in crate::cas_projection) fn cancellation(&self) -> &ProjectionCancellationToken {
        &self.route.cancellation
    }

    pub(in crate::cas_projection) fn failed(
        mut self,
        decision: Box<NativeLineageRecoveryDecision>,
        execution: ScheduledOrdinaryExecutionLease,
        command: NativeLineageRecoveryCommand,
        recovery_available: bool,
    ) {
        let continuation = ParkedContinuation {
            decision,
            execution: execution.park(),
        };
        let control = self
            .control
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !control.closed
            && control
                .routes
                .get(&self.key.thread_id)
                .is_some_and(|route| Arc::ptr_eq(route, &self.route))
        {
            let mut state = self
                .route
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.snapshot = snapshot_for(
                self.key,
                &continuation.decision,
                NativeLineageRecoveryStatus::Failed {
                    command,
                    recovery_available,
                },
            );
            state.command = None;
            state.continuation = Some(continuation);
            self.retained = true;
        }
    }

    pub(in crate::cas_projection) fn leaving(mut self) {
        let control = self
            .control
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !control.closed
            && control
                .routes
                .get(&self.key.thread_id)
                .is_some_and(|route| Arc::ptr_eq(route, &self.route))
        {
            let mut state = self
                .route
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.snapshot.status = NativeLineageRecoveryStatus::Leaving {
                pending_turn_continues: true,
            };
            state.command = None;
            state.continuation = None;
            self.retained = true;
        }
    }
}

impl Drop for NativeLineageRecoveryWork {
    fn drop(&mut self) {
        drop(self.attempt.take());
    }
}

impl Drop for NativeLineageRecoveryAttempt {
    fn drop(&mut self) {
        if self.retained {
            return;
        }
        let _ = self.control.remove_exact(self.key);
    }
}

impl Drop for NativeLineageRouteReservation {
    fn drop(&mut self) {
        if self.retained {
            return;
        }
        let mut control = self
            .control
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let released = control.reservations.remove(&self.thread_id);
        let wakes_capacity_retry =
            released && take_capacity_retry(&mut control, self.control.inner.capacity);
        drop(control);
        if wakes_capacity_retry {
            self.control
                .inner
                .signal
                .wake(AcceptedInputWakeReason::NativeLineageRouteCapacityReleased);
        }
    }
}

fn take_capacity_retry(control: &mut ControlState, capacity: usize) -> bool {
    if !control.closed
        && control.capacity_waiting
        && control.routes.len() + control.reservations.len() < capacity
    {
        control.capacity_waiting = false;
        true
    } else {
        false
    }
}

fn snapshot_for(
    key: NativeLineageRecoveryKey,
    decision: &NativeLineageRecoveryDecision,
    status: NativeLineageRecoveryStatus,
) -> NativeLineageRecoverySnapshot {
    NativeLineageRecoverySnapshot {
        key,
        source_thread_id: decision.source_thread_id(),
        source_binding_revision: decision.source_binding_revision(),
        operation: decision.operation(),
        failed_attempts: decision.failed_attempts(),
        status,
    }
}
