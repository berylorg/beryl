use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::{
    command::RetainedReconciliationDescriptor,
    health::{FailureSeverity, HealthGate},
    ownership::HomeLifecycleCustodian,
};

use super::{
    FlightState, RECONCILIATION_SCOPE_CAPACITY, ReconciliationFlight, ReconciliationHandle,
    ReconciliationReservationError, SealedCollision,
};

pub(super) enum ScopeState {
    Vacant,
    Reserved {
        token: u64,
    },
    Verifying {
        token: u64,
        descriptor: Arc<RetainedReconciliationDescriptor>,
        charged_bytes: usize,
        flight: Arc<ReconciliationFlight>,
    },
    Closed {
        token: u64,
        _facts: SealedCollision,
        flight: Arc<ReconciliationFlight>,
    },
}

pub(super) struct RegistryState {
    pub(super) accepting_reservations: bool,
    pub(super) scopes: Box<[ScopeState]>,
    pub(super) reserved_bytes: usize,
    pub(super) active_workers: usize,
    next_token: u64,
    retained_core: Option<Arc<RegistryInner>>,
}

pub(super) struct RegistryInner {
    pub(super) state: Mutex<RegistryState>,
    pub(super) worker_released: Condvar,
    pub(super) health: Arc<HealthGate>,
    _lifecycle: Arc<HomeLifecycleCustodian>,
    descriptor_byte_limit: usize,
    reserved_byte_limit: usize,
}

pub(crate) enum RetryHandle {
    Current(ReconciliationHandle),
    Terminal,
}

impl RegistryInner {
    pub(super) fn lock_state(&self) -> MutexGuard<'_, RegistryState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                self.health.signal_failure(FailureSeverity::Structural);
                poisoned.into_inner()
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct ReconciliationRegistry {
    pub(super) inner: Arc<RegistryInner>,
}

impl ReconciliationRegistry {
    pub(crate) fn new(
        descriptor_byte_limit: usize,
        reserved_byte_limit: usize,
        health: Arc<HealthGate>,
        lifecycle: Arc<HomeLifecycleCustodian>,
    ) -> Self {
        assert!(descriptor_byte_limit > 0);
        assert!(reserved_byte_limit >= descriptor_byte_limit);
        Self {
            inner: Arc::new(RegistryInner {
                state: Mutex::new(RegistryState {
                    accepting_reservations: true,
                    scopes: std::iter::repeat_with(|| ScopeState::Vacant)
                        .take(RECONCILIATION_SCOPE_CAPACITY)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                    reserved_bytes: 0,
                    active_workers: 0,
                    next_token: 1,
                    retained_core: None,
                }),
                worker_released: Condvar::new(),
                health,
                _lifecycle: lifecycle,
                descriptor_byte_limit,
                reserved_byte_limit,
            }),
        }
    }

    pub(crate) fn reserve(
        &self,
        descriptor_bytes: usize,
    ) -> Result<ReconciliationSlot, ReconciliationReservationError> {
        if descriptor_bytes > self.inner.descriptor_byte_limit {
            return Err(ReconciliationReservationError::DescriptorTooLarge {
                requested: descriptor_bytes,
                limit: self.inner.descriptor_byte_limit,
            });
        }
        let mut state = self.inner.lock_state();
        if !state.accepting_reservations {
            return Err(ReconciliationReservationError::Capacity);
        }
        let next_bytes = state
            .reserved_bytes
            .checked_add(descriptor_bytes)
            .ok_or(ReconciliationReservationError::Capacity)?;
        if next_bytes > self.inner.reserved_byte_limit {
            return Err(ReconciliationReservationError::Capacity);
        }
        let index = state
            .scopes
            .iter()
            .position(|scope| matches!(scope, ScopeState::Vacant))
            .ok_or(ReconciliationReservationError::Capacity)?;
        let token = state.next_token;
        state.next_token = state
            .next_token
            .checked_add(1)
            .expect("reconciliation scope token exhausted");
        state.scopes[index] = ScopeState::Reserved { token };
        state.reserved_bytes = next_bytes;
        Ok(ReconciliationSlot {
            inner: Arc::clone(&self.inner),
            index,
            token,
            charged_bytes: descriptor_bytes,
            releases_reservation: true,
        })
    }

    pub(crate) fn begin_close(&self) -> usize {
        let mut state = self.inner.lock_state();
        state.accepting_reservations = false;
        let pending = state
            .scopes
            .iter()
            .filter(|scope| {
                matches!(
                    scope,
                    ScopeState::Reserved { .. } | ScopeState::Verifying { .. }
                )
            })
            .count();
        if pending == 0 {
            dispose_closed_scopes(&mut state);
            state.retained_core = None;
        }
        pending
    }

    pub(crate) fn begin_drop(&self) -> usize {
        let mut state = self.inner.lock_state();
        state.accepting_reservations = false;
        let retained = state
            .scopes
            .iter()
            .filter(|scope| !matches!(scope, ScopeState::Vacant))
            .count();
        if retained != 0 && state.retained_core.is_none() {
            state.retained_core = Some(Arc::clone(&self.inner));
        }
        retained
    }

    pub(crate) fn handles(&self) -> Vec<ReconciliationHandle> {
        let state = self.inner.lock_state();
        state
            .scopes
            .iter()
            .enumerate()
            .filter_map(|(index, scope)| match scope {
                ScopeState::Verifying { token, flight, .. } => Some(ReconciliationHandle {
                    registry: Arc::downgrade(&self.inner),
                    index,
                    token: *token,
                    flight: Arc::clone(flight),
                }),
                ScopeState::Closed { token, flight, .. } => Some(ReconciliationHandle {
                    registry: Arc::downgrade(&self.inner),
                    index,
                    token: *token,
                    flight: Arc::clone(flight),
                }),
                ScopeState::Vacant | ScopeState::Reserved { .. } => None,
            })
            .collect()
    }

    pub(crate) fn retry_handle(&self, handle: &ReconciliationHandle) -> Option<RetryHandle> {
        let state = self.inner.lock_state();
        match state.scopes.get(handle.index)? {
            ScopeState::Verifying { token, flight, .. } if *token == handle.token => {
                if Arc::ptr_eq(flight, &handle.flight) {
                    reset_failed_flight(flight);
                    Some(RetryHandle::Current(ReconciliationHandle {
                        registry: Arc::downgrade(&self.inner),
                        index: handle.index,
                        token: *token,
                        flight: Arc::clone(flight),
                    }))
                } else {
                    terminal_authority(&handle.flight)
                }
            }
            ScopeState::Closed { token, flight, .. } if *token == handle.token => {
                if Arc::ptr_eq(flight, &handle.flight) {
                    Some(RetryHandle::Terminal)
                } else {
                    terminal_authority(&handle.flight)
                }
            }
            ScopeState::Vacant
            | ScopeState::Reserved { .. }
            | ScopeState::Verifying { .. }
            | ScopeState::Closed { .. } => terminal_authority(&handle.flight),
        }
    }
}

fn reset_failed_flight(flight: &Arc<ReconciliationFlight>) {
    let mut state = flight
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if matches!(&*state, FlightState::Complete(Err(_))) {
        *state = FlightState::Idle;
    }
}

fn terminal_authority(flight: &Arc<ReconciliationFlight>) -> Option<RetryHandle> {
    match &*flight
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    {
        FlightState::Running | FlightState::Complete(Ok(_)) => Some(RetryHandle::Terminal),
        FlightState::Idle | FlightState::Complete(Err(_)) => None,
    }
}

pub(crate) struct ReconciliationSlot {
    inner: Arc<RegistryInner>,
    index: usize,
    token: u64,
    charged_bytes: usize,
    releases_reservation: bool,
}

impl ReconciliationSlot {
    pub(crate) fn install(
        mut self,
        descriptor: RetainedReconciliationDescriptor,
    ) -> ReconciliationHandle {
        let flight = Arc::new(ReconciliationFlight::new());
        let handle = ReconciliationHandle {
            registry: Arc::downgrade(&self.inner),
            index: self.index,
            token: self.token,
            flight: Arc::clone(&flight),
        };
        let mut state = self.inner.lock_state();
        debug_assert!(
            matches!(state.scopes[self.index], ScopeState::Reserved { token } if token == self.token)
        );
        state.scopes[self.index] = ScopeState::Verifying {
            token: self.token,
            descriptor: Arc::new(descriptor),
            charged_bytes: self.charged_bytes,
            flight,
        };
        if state.retained_core.is_none() {
            state.retained_core = Some(Arc::clone(&self.inner));
        }
        self.releases_reservation = false;
        handle
    }
}

impl std::fmt::Debug for ReconciliationSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReconciliationSlot")
            .field("index", &self.index)
            .field("charged_bytes", &self.charged_bytes)
            .finish_non_exhaustive()
    }
}

impl Drop for ReconciliationSlot {
    fn drop(&mut self) {
        if !self.releases_reservation {
            return;
        }
        let mut state = self.inner.lock_state();
        if !matches!(state.scopes[self.index], ScopeState::Reserved { token } if token == self.token)
        {
            drop(state);
            self.inner
                .health
                .signal_failure(FailureSeverity::Structural);
            return;
        }
        state.scopes[self.index] = ScopeState::Vacant;
        match state.reserved_bytes.checked_sub(self.charged_bytes) {
            Some(remaining) => state.reserved_bytes = remaining,
            None => {
                drop(state);
                self.inner
                    .health
                    .signal_failure(FailureSeverity::Structural);
            }
        }
    }
}

pub(super) fn release_retained_core_if_idle(state: &mut RegistryState) {
    if !state.scopes.iter().any(|scope| {
        matches!(
            scope,
            ScopeState::Verifying { .. } | ScopeState::Closed { .. }
        )
    }) {
        state.retained_core = None;
    }
}

fn dispose_closed_scopes(state: &mut RegistryState) {
    let mut released_bytes = 0usize;
    for scope in &mut state.scopes {
        if matches!(scope, ScopeState::Closed { .. }) {
            let ScopeState::Closed { _facts: facts, .. } =
                std::mem::replace(scope, ScopeState::Vacant)
            else {
                unreachable!("the matched reconciliation scope is collision-closed");
            };
            released_bytes = released_bytes
                .checked_add(facts.charged_bytes)
                .expect("closed reconciliation charge accounting remains bounded");
        }
    }
    state.reserved_bytes = state
        .reserved_bytes
        .checked_sub(released_bytes)
        .expect("closed reconciliation charges remain reserved until orderly close");
}
