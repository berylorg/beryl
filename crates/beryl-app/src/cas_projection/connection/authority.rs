use super::*;

#[derive(Debug)]
pub(in crate::cas_projection) struct ConnectionRegistryAuthority {
    pub(super) generation: ConnectionGeneration,
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
    retired: AtomicBool,
    gate: Mutex<ConnectionAuthorityState>,
    retirement_changed: std::sync::Condvar,
}

#[derive(Debug)]
pub(in crate::cas_projection) struct ConnectionAuthorityState {
    session_owner_live: bool,
    scheduled_promotion: Option<ScheduledPromotionAuthority>,
    cleanup_owners: std::collections::HashMap<CleanupAuthorityId, CleanupAuthorityState>,
    next_promotion_id: u64,
    next_cleanup_id: u64,
    retirement_complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PromotionAuthorityId(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CleanupAuthorityId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactAuthorityState {
    Live,
}

type CleanupAuthorityState = ExactAuthorityState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScheduledPromotionAuthority {
    id: PromotionAuthorityId,
    state: ExactAuthorityState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ConnectionRetirementOutcome {
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ConnectionPromotionReleaseOutcome {
    Ordinary,
    PersistentFailure,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactSettlementOutcome {
    Ordinary { should_detach: bool },
    PersistentFailure,
    Closed,
}

/// One-shot authority that prevents connection retirement from overtaking durable promotion.
pub(in crate::cas_projection) struct ConnectionPromotionReservation {
    authority: Arc<ConnectionRegistryAuthority>,
    connection: Arc<ProjectionConnection>,
    id: PromotionAuthorityId,
    command: Option<crate::cas_projection::persistent_failure::LiveCommandPermit>,
    active: bool,
}

pub(in crate::cas_projection) struct ConnectionCleanupOwner {
    authority: Arc<ConnectionRegistryAuthority>,
    connection: Arc<ProjectionConnection>,
    id: CleanupAuthorityId,
    command: Option<crate::cas_projection::persistent_failure::LiveCommandPermit>,
    active: bool,
}

#[cfg(test)]
struct RetirementGateAttemptHook {
    connection_generation: u64,
    reached: std::sync::mpsc::SyncSender<()>,
}

#[cfg(test)]
pub(in crate::cas_projection) struct RetirementGateAttemptObservation {
    reached: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
static RETIREMENT_GATE_ATTEMPT_HOOK: std::sync::OnceLock<Mutex<Option<RetirementGateAttemptHook>>> =
    std::sync::OnceLock::new();

impl std::fmt::Debug for ConnectionCleanupOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionCleanupOwner")
            .field("connection_generation", &self.authority.generation)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

impl Default for ConnectionAuthorityState {
    fn default() -> Self {
        Self {
            session_owner_live: true,
            scheduled_promotion: None,
            cleanup_owners: std::collections::HashMap::new(),
            next_promotion_id: 1,
            next_cleanup_id: 1,
            retirement_complete: false,
        }
    }
}

impl ConnectionRegistryAuthority {
    pub(in crate::cas_projection) fn new(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Result<Self, ProjectionCoordinatorError> {
        Ok(Self {
            generation: allocate_connection_generation()?,
            runtime_id,
            process_generation,
            retired: AtomicBool::new(false),
            gate: Mutex::new(ConnectionAuthorityState::default()),
            retirement_changed: std::sync::Condvar::new(),
        })
    }

    pub(in crate::cas_projection) fn is_retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }

    pub(super) fn retirement_complete(&self) -> bool {
        self.gate
            .lock()
            .map(|state| state.retirement_complete)
            .unwrap_or(false)
    }

    pub(in crate::cas_projection) fn release_session_owner(
        &self,
        elect_ordinary_retirement: impl FnOnce() -> bool,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        state.session_owner_live = false;
        self.elect_detachment_locked(&mut state, elect_ordinary_retirement)
    }

    pub(in crate::cas_projection) fn mark_session_owner_released(&self) {
        let mut state = match self.gate.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.session_owner_live = false;
    }

    pub(super) fn acquire_cleanup_owner(
        self: &Arc<Self>,
        connection: &Arc<ProjectionConnection>,
        command: crate::cas_projection::persistent_failure::LiveCommandPermit,
    ) -> Result<Option<ConnectionCleanupOwner>, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        let id = CleanupAuthorityId(state.next_cleanup_id);
        state.next_cleanup_id = state
            .next_cleanup_id
            .checked_add(1)
            .expect("connection cleanup authority IDs cannot exhaust during one process");
        let replaced = state.cleanup_owners.insert(id, ExactAuthorityState::Live);
        debug_assert!(replaced.is_none());
        Ok(Some(ConnectionCleanupOwner {
            authority: Arc::clone(self),
            connection: Arc::clone(connection),
            id,
            command: Some(command),
            active: true,
        }))
    }

    fn elect_detachment_locked(
        &self,
        state: &mut ConnectionAuthorityState,
        elect_ordinary_retirement: impl FnOnce() -> bool,
    ) -> Result<bool, ProjectionCoordinatorError> {
        if state.session_owner_live
            || state.scheduled_promotion.is_some()
            || !state.cleanup_owners.is_empty()
        {
            return Ok(false);
        }
        if self.is_retired() {
            self.complete_retirement_locked(state);
            return Ok(true);
        }
        if registry::connection_has_authority(self.generation)? || !elect_ordinary_retirement() {
            return Ok(false);
        }
        self.retire_locked(state);
        Ok(true)
    }

    pub(super) fn reserve_scheduled_promotion(
        self: &Arc<Self>,
        connection: &Arc<ProjectionConnection>,
        command: crate::cas_projection::persistent_failure::LiveCommandPermit,
    ) -> Result<Option<ConnectionPromotionReservation>, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        if self.is_retired() || state.scheduled_promotion.is_some() {
            return Ok(None);
        }
        let id = PromotionAuthorityId(state.next_promotion_id);
        state.next_promotion_id = state
            .next_promotion_id
            .checked_add(1)
            .expect("connection promotion authority IDs cannot exhaust during one process");
        state.scheduled_promotion = Some(ScheduledPromotionAuthority {
            id,
            state: ExactAuthorityState::Live,
        });
        Ok(Some(ConnectionPromotionReservation {
            authority: Arc::clone(self),
            connection: Arc::clone(connection),
            id,
            command: Some(command),
            active: true,
        }))
    }

    pub(super) fn register_new(
        &self,
        key: LoadedThreadKey,
        owner: SyndicThreadId,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        seed: &mut RawLoadedLeaseSeed,
    ) -> Result<Option<()>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        match command.commit_if_current(|| {
            let (generation, token) = registry::register_new(key, self.generation, owner)?;
            seed.arm(generation, token);
            Ok(())
        }) {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        }
    }

    pub(super) fn acquire_existing(
        &self,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        seed: &mut RawLoadedLeaseSeed,
    ) -> Result<Option<ExistingSubscription>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        match command.commit_if_current(|| {
            let subscription = registry::acquire_existing(key, self.generation, owner)?;
            if let ExistingSubscription::Exact { generation, token } = subscription {
                seed.arm(generation, token);
            }
            Ok(subscription)
        }) {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn register_new_for_test(
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

    #[cfg(test)]
    pub(in crate::cas_projection) fn acquire_existing_for_test(
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

    fn invalidate_thread_locked(
        &self,
        state: &mut ConnectionAuthorityState,
        key: &LoadedThreadKey,
    ) -> Result<bool, ProjectionCoordinatorError> {
        registry::invalidate_connection_thread(key, self.generation).inspect_err(|_| {
            self.retire_locked(state);
        })
    }

    /// Revokes one remote thread only on this exact connection generation.
    ///
    /// The caller records the router-lane fence first and releases that lock before entering this
    /// connection gate. Registry invalidation then shares the same serialization used by
    /// retirement, replacement reservation, and native transfer.
    pub(in crate::cas_projection) fn record_thread_closed(
        &self,
        cas_thread_id: &CasThreadId,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        let key = LoadedThreadKey {
            runtime_id: self.runtime_id,
            process_generation: self.process_generation,
            cas_thread_id: cas_thread_id.clone(),
        };
        self.invalidate_thread_locked(&mut state, &key)
    }

    pub(in crate::cas_projection) fn retire(
        &self,
    ) -> Result<ConnectionRetirementOutcome, ProjectionCoordinatorError> {
        #[cfg(test)]
        observe_retirement_gate_attempt(self.generation.get());
        let (mut state, poisoned) = match self.gate.lock() {
            Ok(gate) => (gate, false),
            Err(poisoned) => (poisoned.into_inner(), true),
        };
        self.retire_locked(&mut state);
        if poisoned {
            return Err(Self::authority_state_error());
        }
        while !state.retirement_complete {
            state = match self.retirement_changed.wait(state) {
                Ok(state) => state,
                Err(_) => return Err(Self::authority_state_error()),
            };
            self.complete_retirement_locked(&mut state);
        }
        Ok(ConnectionRetirementOutcome::Complete)
    }

    fn settle_cleanup_owner(
        self: &Arc<Self>,
        id: CleanupAuthorityId,
        connection: &Arc<ProjectionConnection>,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
    ) -> Result<ExactSettlementOutcome, ProjectionCoordinatorError> {
        let state = self.lock()?;
        let state = std::cell::RefCell::new(state);
        let result = command.commit_or_transfer(
            || {
                let mut state = state.borrow_mut();
                if state.cleanup_owners.get(&id) != Some(&ExactAuthorityState::Live) {
                    return Err(Self::authority_state_error());
                }
                state.cleanup_owners.remove(&id);
                self.retirement_changed.notify_all();
                let should_detach = self.elect_detachment_locked(&mut state, || {
                    connection.begin_ordinary_retirement_under_gate()
                })?;
                Ok(ExactSettlementOutcome::Ordinary { should_detach })
            },
            |_failure_generation| {
                let mut state = state.borrow_mut();
                if state.cleanup_owners.get(&id) != Some(&ExactAuthorityState::Live) {
                    return Err(Self::authority_state_error());
                }
                state.cleanup_owners.remove(&id);
                if self.is_retired() {
                    self.complete_retirement_locked(&mut state);
                }
                self.retirement_changed.notify_all();
                Ok(ExactSettlementOutcome::PersistentFailure)
            },
            || {
                let mut state = state.borrow_mut();
                if state.cleanup_owners.get(&id) != Some(&ExactAuthorityState::Live) {
                    return Err(Self::authority_state_error());
                }
                state.cleanup_owners.remove(&id);
                if self.is_retired() {
                    self.complete_retirement_locked(&mut state);
                }
                self.retirement_changed.notify_all();
                Ok(ExactSettlementOutcome::Closed)
            },
        );
        match result {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(error)) => {
                let mut state = state.borrow_mut();
                if state.cleanup_owners.get(&id) == Some(&ExactAuthorityState::Live) {
                    state.cleanup_owners.remove(&id);
                }
                self.retire_locked(&mut state);
                Err(error)
            }
            Err(_) => {
                let mut state = state.borrow_mut();
                if state.cleanup_owners.get(&id) == Some(&ExactAuthorityState::Live) {
                    state.cleanup_owners.remove(&id);
                }
                self.retire_locked(&mut state);
                Err(Self::authority_state_error())
            }
        }
    }

    pub(super) fn retire_locked(&self, state: &mut ConnectionAuthorityState) {
        self.retired.store(true, Ordering::Release);
        self.complete_retirement_locked(state);
    }

    fn complete_retirement_locked(&self, state: &mut ConnectionAuthorityState) {
        if state.scheduled_promotion.is_some()
            || !state.cleanup_owners.is_empty()
            || state.retirement_complete
        {
            return;
        }
        let _ = registry::invalidate_connection(self.generation);
        state.retirement_complete = true;
        self.retirement_changed.notify_all();
    }

    fn authority_state_error() -> ProjectionCoordinatorError {
        ProjectionCoordinatorError::RegistryPoisoned {
            registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
        }
    }

    fn settle_scheduled_promotion(
        self: &Arc<Self>,
        id: PromotionAuthorityId,
        connection: &Arc<ProjectionConnection>,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
    ) -> Result<ExactSettlementOutcome, ProjectionCoordinatorError> {
        let state = self.lock()?;
        let state = std::cell::RefCell::new(state);
        let result = command.commit_or_transfer(
            || {
                let mut state = state.borrow_mut();
                if !matches!(
                    state.scheduled_promotion,
                    Some(ScheduledPromotionAuthority {
                        id: active_id,
                        state: ExactAuthorityState::Live,
                    }) if active_id == id
                ) {
                    return Err(Self::authority_state_error());
                }
                state.scheduled_promotion = None;
                self.retirement_changed.notify_all();
                let should_detach = self.elect_detachment_locked(&mut state, || {
                    connection.begin_ordinary_retirement_under_gate()
                })?;
                Ok(ExactSettlementOutcome::Ordinary { should_detach })
            },
            |_failure_generation| {
                let mut state = state.borrow_mut();
                if !matches!(
                    state.scheduled_promotion,
                    Some(ScheduledPromotionAuthority {
                        id: active_id,
                        state: ExactAuthorityState::Live,
                    }) if active_id == id
                ) {
                    return Err(Self::authority_state_error());
                }
                state.scheduled_promotion = None;
                if self.is_retired() {
                    self.complete_retirement_locked(&mut state);
                }
                self.retirement_changed.notify_all();
                Ok(ExactSettlementOutcome::PersistentFailure)
            },
            || {
                let mut state = state.borrow_mut();
                if !matches!(
                    state.scheduled_promotion,
                    Some(ScheduledPromotionAuthority {
                        id: active_id,
                        state: ExactAuthorityState::Live,
                    }) if active_id == id
                ) {
                    return Err(Self::authority_state_error());
                }
                state.scheduled_promotion = None;
                if self.is_retired() {
                    self.complete_retirement_locked(&mut state);
                }
                self.retirement_changed.notify_all();
                Ok(ExactSettlementOutcome::Closed)
            },
        );
        match result {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(error)) => {
                let mut state = state.borrow_mut();
                if matches!(
                    state.scheduled_promotion,
                    Some(ScheduledPromotionAuthority {
                        id: active_id,
                        state: ExactAuthorityState::Live,
                    }) if active_id == id
                ) {
                    state.scheduled_promotion = None;
                }
                self.retire_locked(&mut state);
                Err(error)
            }
            Err(_) => {
                let mut state = state.borrow_mut();
                if matches!(
                    state.scheduled_promotion,
                    Some(ScheduledPromotionAuthority {
                        id: active_id,
                        state: ExactAuthorityState::Live,
                    }) if active_id == id
                ) {
                    state.scheduled_promotion = None;
                }
                self.retire_locked(&mut state);
                Err(Self::authority_state_error())
            }
        }
    }

    pub(super) fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, ConnectionAuthorityState>, ProjectionCoordinatorError>
    {
        match self.gate.lock() {
            Ok(gate) => Ok(gate),
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                self.retire_locked(&mut state);
                drop(state);
                Err(ProjectionCoordinatorError::RegistryPoisoned {
                    registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
                })
            }
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn lock_for_test(
        &self,
    ) -> std::sync::MutexGuard<'_, ConnectionAuthorityState> {
        self.gate.lock().unwrap()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_for_recovery_test(&self) {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = self
                .gate
                .lock()
                .expect("connection authority starts unpoisoned");
            panic!("poison connection authority after exact registry installation");
        }));
        assert!(panicked.is_err());
        assert!(self.gate.is_poisoned());
    }

    #[cfg(test)]
    pub(in crate::cas_projection) const fn generation_for_test(&self) -> ConnectionGeneration {
        self.generation
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_retirement_gate_attempt_for_test(
        &self,
    ) -> RetirementGateAttemptObservation {
        let (reached, observation) = std::sync::mpsc::sync_channel(1);
        let slot = RETIREMENT_GATE_ATTEMPT_HOOK.get_or_init(|| Mutex::new(None));
        let mut slot = slot.lock().expect("retirement gate-attempt hook is usable");
        assert!(
            slot.is_none(),
            "only one retirement gate attempt may be observed"
        );
        *slot = Some(RetirementGateAttemptHook {
            connection_generation: self.generation.get(),
            reached,
        });
        RetirementGateAttemptObservation {
            reached: observation,
        }
    }
}

#[cfg(test)]
impl RetirementGateAttemptObservation {
    pub(in crate::cas_projection) fn wait(self, timeout: std::time::Duration) {
        self.reached
            .recv_timeout(timeout)
            .expect("the exact retirement reaches its authority-gate attempt");
    }
}

#[cfg(test)]
fn observe_retirement_gate_attempt(connection_generation: u64) {
    let slot = RETIREMENT_GATE_ATTEMPT_HOOK.get_or_init(|| Mutex::new(None));
    let hook = {
        let mut slot = slot.lock().expect("retirement gate-attempt hook is usable");
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

impl ConnectionPromotionReservation {
    fn settle(&mut self) -> Result<ExactSettlementOutcome, ProjectionCoordinatorError> {
        let command = self
            .command
            .as_ref()
            .expect("active promotion reservation retains its command permit");
        self.authority
            .settle_scheduled_promotion(self.id, &self.connection, command)
    }

    pub(in crate::cas_projection) fn release(
        mut self,
    ) -> Result<ConnectionPromotionReleaseOutcome, ProjectionCoordinatorError> {
        let connection = Arc::clone(&self.connection);
        let settlement = self.settle();
        self.active = false;
        self.command.take();
        match settlement? {
            ExactSettlementOutcome::Ordinary {
                should_detach: true,
            } => {
                connection.shutdown_after_ordinary_retirement()?;
                Ok(ConnectionPromotionReleaseOutcome::Ordinary)
            }
            ExactSettlementOutcome::Ordinary {
                should_detach: false,
            } => Ok(ConnectionPromotionReleaseOutcome::Ordinary),
            ExactSettlementOutcome::PersistentFailure => {
                Ok(ConnectionPromotionReleaseOutcome::PersistentFailure)
            }
            ExactSettlementOutcome::Closed => Ok(ConnectionPromotionReleaseOutcome::Closed),
        }
    }
}

impl ConnectionCleanupOwner {
    fn settle(&mut self) -> Result<ExactSettlementOutcome, ProjectionCoordinatorError> {
        let command = self
            .command
            .as_ref()
            .expect("active cleanup owner retains its command permit");
        self.authority
            .settle_cleanup_owner(self.id, &self.connection, command)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_loaded_release(
        &self,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        token: LeaseToken,
    ) -> Result<Option<registry::ReleaseDisposition>, ProjectionCoordinatorError> {
        if !self.active {
            return Err(ConnectionRegistryAuthority::authority_state_error());
        }
        let _state = self.authority.lock()?;
        let command = self
            .command
            .as_ref()
            .expect("active cleanup owner retains its command permit");
        match command.commit_if_current(|| {
            registry::release_exact(key, self.authority.generation, owner, generation, token)
        }) {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        }
    }

    pub(in crate::cas_projection) fn finish(mut self) -> Result<(), ProjectionCoordinatorError> {
        let connection = Arc::clone(&self.connection);
        let settlement = self.settle();
        self.active = false;
        self.command.take();
        match settlement? {
            ExactSettlementOutcome::Ordinary {
                should_detach: true,
            } => connection.shutdown_after_ordinary_retirement(),
            ExactSettlementOutcome::Ordinary {
                should_detach: false,
            }
            | ExactSettlementOutcome::PersistentFailure
            | ExactSettlementOutcome::Closed => Ok(()),
        }
    }
}

impl Drop for ConnectionCleanupOwner {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let settlement = self.settle();
        self.active = false;
        self.command.take();
        match settlement {
            Ok(ExactSettlementOutcome::Ordinary {
                should_detach: true,
            }) => self.connection.signal_ordinary_retirement(),
            Err(_) => self.connection.request_ordinary_retirement(),
            Ok(
                ExactSettlementOutcome::Ordinary {
                    should_detach: false,
                }
                | ExactSettlementOutcome::PersistentFailure
                | ExactSettlementOutcome::Closed,
            ) => {}
        }
    }
}

impl Drop for ConnectionPromotionReservation {
    fn drop(&mut self) {
        if self.active {
            let settlement = self.settle();
            self.active = false;
            self.command.take();
            match settlement {
                Ok(ExactSettlementOutcome::Ordinary {
                    should_detach: true,
                }) => self.connection.signal_ordinary_retirement(),
                Err(_) => self.connection.request_ordinary_retirement(),
                Ok(
                    ExactSettlementOutcome::Ordinary {
                        should_detach: false,
                    }
                    | ExactSettlementOutcome::PersistentFailure
                    | ExactSettlementOutcome::Closed,
                ) => {}
            }
        }
    }
}
