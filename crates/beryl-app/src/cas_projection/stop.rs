use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use std::sync::Condvar;

use beryl_backend::{
    ApprovalInterruption, ExactForegroundTurn, StopAttemptCorrelation, StopAttemptDisposition,
    StopOperationCorrelation, TurnInterruptDisposition, TurnInterruptOutcome,
};
use beryl_home_store::{CommandError, CommandOutcome, CommitReceipt, HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AbandonStopOperation, ClaimStopDispatch, JoinStopCause, SafelyReopenStopOperation,
    StopAbandonmentReason, StopAdmissionIneligibility, StopAdmissionRead, StopAttemptNonce,
    StopCause, StopCauseSet, StopOperationId, StopOperationNonce, StopOperationState,
    StopOperationTarget, SyndicLiveStopOperation, SyndicPointReadLimit, SyndicReadError,
    SyndicStorage, SyndicTimestamp,
};
use thiserror::Error;

use super::connection::{
    EventRouter, StopElectionAcquireError, StopElectionAdmission, StopElectionPermit,
    StopTargetProof,
};

mod persistent_failure;

const STOP_POINT_READ_BYTES: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalDispatchState {
    AdmittedNotClaimed,
    ClaimUnresolved,
    ClaimedNotDispatched,
    Dispatching,
    ProvenNondispatchSettling,
    PrimaryAccepted,
    PossiblyDispatched,
    DurablyAbandoned,
    FailureFrozenNondispatch,
}

#[derive(Clone, Debug)]
struct LocalStop {
    operation_id: StopOperationId,
    target: StopOperationTarget,
    attempt: Option<StopAttemptNonce>,
    dispatch: LocalDispatchState,
    timeout: std::time::Duration,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct LifecycleYieldKey {
    thread_id: SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
}

#[derive(Default)]
struct StopCoordinatorState {
    stops: HashMap<SyndicThreadId, LocalStop>,
    lifecycle_yields: HashMap<LifecycleYieldKey, crate::LifecycleYieldOutcome>,
    persistent_failure: Option<super::persistent_failure::PersistentFailureCutIdentity>,
}

pub(in crate::cas_projection) struct StopCoordinator {
    home: Weak<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    commands: super::persistent_failure::LiveCommandAuthorizer,
    state: Mutex<StopCoordinatorState>,
    #[cfg(test)]
    race_pauses: StopRacePauses,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum StopRaceStage {
    ElectionHeldBeforeAdmissionGate,
    BeforeClaimFence,
    ClaimFenceHeld,
    BeforeBeginDispatchFence,
    BeginDispatchFenceHeld,
}

#[cfg(test)]
#[derive(Default)]
struct StopRacePauses {
    installed: Mutex<HashMap<StopRaceStage, Arc<StopRacePause>>>,
}

#[cfg(test)]
#[derive(Default)]
struct StopRacePause {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

#[cfg(test)]
struct StopRacePauseController {
    pause: Arc<StopRacePause>,
}

#[cfg(test)]
impl StopRacePauseController {
    fn wait_until_reached(&self, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        let mut state = self.pause.state.lock().unwrap();
        while !state.0 {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, wait) = self.pause.changed.wait_timeout(state, remaining).unwrap();
            state = next;
            if wait.timed_out() && !state.0 {
                return false;
            }
        }
        true
    }

    fn release(&self) {
        let mut state = self.pause.state.lock().unwrap();
        state.1 = true;
        self.pause.changed.notify_all();
    }
}

#[cfg(test)]
impl Drop for StopRacePauseController {
    fn drop(&mut self) {
        self.release();
    }
}

/// Closed result of one non-GPUI exact stop request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopCoordinationOutcome {
    /// The operation remains durably stopping; the primary request is never repeated.
    Stopping {
        /// Exact durable operation selected by the request.
        operation_id: StopOperationId,
        /// Whether this caller owned the sole primary dispatch attempt.
        primary_owner: bool,
    },
    /// Proven local nondispatch safely restored the still-exact active target.
    SafelyReopened { operation_id: StopOperationId },
    /// Exact provider rejection or authority loss consumed the stop as incomplete.
    Abandoned { operation_id: StopOperationId },
    /// Current durable authority does not admit interruption.
    Ineligible(StopAdmissionIneligibility),
}

/// Result of requesting ordinary close for a healthy-home window's active operation.
#[must_use]
#[derive(Debug)]
pub enum WindowCloseStopOutcome {
    /// The window must retain its thread claim until this exact barrier converges.
    Waiting(WindowCloseStopBarrier),
    /// Proven local nondispatch left the same operation active, so the window remains open.
    SafelyReopened { operation_id: StopOperationId },
    /// The selected thread has no currently interruptible operation.
    Ineligible(StopAdmissionIneligibility),
}

/// Current state of one exact healthy-home window-close barrier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowCloseStopBarrierStatus {
    /// Durable stop, abandonment, or terminal-history work still owns the stopped turn.
    Waiting,
    /// Exact terminal-history or authority-loss convergence permits claim release.
    Converged,
}

/// Non-cloneable exact convergence barrier retained by an ordinary window-close operation.
pub struct WindowCloseStopBarrier {
    coordinator: Arc<StopCoordinator>,
    operation_id: StopOperationId,
    target_turn_id: SyndicTurnId,
    primary_owner: bool,
    converged: bool,
}

impl std::fmt::Debug for WindowCloseStopBarrier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WindowCloseStopBarrier")
            .field("operation_id", &self.operation_id)
            .field("target_turn_id", &self.target_turn_id)
            .field("primary_owner", &self.primary_owner)
            .field("converged", &self.converged)
            .finish_non_exhaustive()
    }
}

impl WindowCloseStopBarrier {
    pub(in crate::cas_projection) fn new(
        coordinator: Arc<StopCoordinator>,
        operation_id: StopOperationId,
        target_turn_id: SyndicTurnId,
        primary_owner: bool,
    ) -> Self {
        Self {
            coordinator,
            operation_id,
            target_turn_id,
            primary_owner,
            converged: false,
        }
    }

    /// Returns the exact durable stop operation retained by this barrier.
    #[must_use]
    pub const fn operation_id(&self) -> StopOperationId {
        self.operation_id
    }

    /// Returns whether this close request owned the sole primary interrupt attempt.
    #[must_use]
    pub const fn primary_owner(&self) -> bool {
        self.primary_owner
    }

    /// Reconciles whether the closing window may release its thread claim.
    ///
    /// An error or [`WindowCloseStopBarrierStatus::Waiting`] keeps the claim retained.
    pub fn poll(&mut self) -> Result<WindowCloseStopBarrierStatus, StopCoordinationError> {
        if self.converged {
            return Ok(WindowCloseStopBarrierStatus::Converged);
        }
        let status = self
            .coordinator
            .window_close_status(self.operation_id, self.target_turn_id)?;
        if status == WindowCloseStopBarrierStatus::Converged {
            self.converged = true;
        }
        Ok(status)
    }
}

/// Failure to establish or converge one exact durable stop operation.
#[derive(Debug, Error)]
pub enum StopCoordinationError {
    #[error("the healthy home generation changed during stop coordination")]
    HomeAuthorityLost,
    #[error("the exact live stop target is unavailable")]
    TargetUnavailable,
    #[error("the authenticated foreground connection is unavailable")]
    ConnectionUnavailable,
    #[error("the process-local stop operation disagrees with durable authority")]
    LocalAuthorityMismatch,
    #[error("the OS cryptographic random source is unavailable")]
    RandomUnavailable,
    #[error("the exact durable stop transition did not commit")]
    TransitionNotCommitted,
    #[error("the exact durable stop transition was proven not committed: {0}")]
    CommandNotCommitted(#[source] CommandError),
    #[error("the exact durable stop transition committed before a later failure: {later_failure}")]
    CommandCommitted {
        receipt: CommitReceipt,
        #[source]
        later_failure: CommandError,
    },
    #[error("the exact durable stop transition has an indeterminate outcome: {failure}")]
    CommandIndeterminate {
        #[source]
        failure: CommandError,
    },
    #[error("the exact durable stop transition collided with another authority")]
    TransitionCollision,
    #[error("the live-event stop election failed")]
    Election,
    #[error("the bounded stop authority read failed")]
    Read(#[from] SyndicReadError),
}

fn require_stop_committed(outcome: CommandOutcome) -> Result<(), StopCoordinationError> {
    match outcome {
        CommandOutcome::NotCommitted { evidence } => {
            Err(StopCoordinationError::CommandNotCommitted(evidence))
        }
        CommandOutcome::Committed {
            receipt: _,
            later_failure: None,
        } => Ok(()),
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
        } => Err(StopCoordinationError::CommandCommitted {
            receipt,
            later_failure,
        }),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            Err(StopCoordinationError::CommandIndeterminate { failure })
        }
    }
}

impl From<StopElectionAcquireError> for StopCoordinationError {
    fn from(_: StopElectionAcquireError) -> Self {
        Self::Election
    }
}

pub(in crate::cas_projection) enum StopOwnership {
    Primary(StopDispatchOwner),
    Joined {
        operation_id: StopOperationId,
        interruption: ApprovalInterruption,
    },
}

pub(in crate::cas_projection) struct StopDispatchOwner {
    coordinator: Arc<StopCoordinator>,
    operation_id: StopOperationId,
    target: StopOperationTarget,
    attempt: StopAttemptNonce,
    permit: Option<StopElectionPermit>,
    timeout: std::time::Duration,
    _command: super::persistent_failure::LiveCommandPermit,
    settled: bool,
}

pub(in crate::cas_projection) enum StopDispatchSettlement {
    Stopping(StopOperationId),
    SafelyReopened(StopOperationId),
    Abandoned(StopOperationId),
}

impl StopCoordinator {
    pub(in crate::cas_projection) fn new(
        home: &Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
        commands: super::persistent_failure::LiveCommandAuthorizer,
    ) -> Self {
        Self {
            home: Arc::downgrade(home),
            home_id,
            home_generation,
            storage,
            commands,
            state: Mutex::new(StopCoordinatorState::default()),
            #[cfg(test)]
            race_pauses: StopRacePauses::default(),
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn new_for_test(
        home: &Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
    ) -> Self {
        let gate = super::persistent_failure::MasterCommandGate::new(
            super::persistent_failure::ProjectionServiceGeneration::allocate()
                .expect("test service generation is available"),
            None,
        );
        Self::new(home, home_id, home_generation, storage, gate.authorizer())
    }

    #[cfg(test)]
    fn install_race_pause(&self, stage: StopRaceStage) -> StopRacePauseController {
        let pause = Arc::new(StopRacePause::default());
        assert!(
            self.race_pauses
                .installed
                .lock()
                .unwrap()
                .insert(stage, Arc::clone(&pause))
                .is_none(),
            "one stop coordinator may install only one pause for an exact race stage"
        );
        StopRacePauseController { pause }
    }

    #[cfg(test)]
    fn pause_race_if_requested(&self, stage: StopRaceStage) {
        let pause = self.race_pauses.installed.lock().unwrap().remove(&stage);
        let Some(pause) = pause else {
            return;
        };
        let mut state = pause.state.lock().unwrap();
        state.0 = true;
        pause.changed.notify_all();
        while !state.1 {
            state = pause.changed.wait(state).unwrap();
        }
    }

    pub(in crate::cas_projection) fn coordinate(
        self: &Arc<Self>,
        router: &Arc<EventRouter>,
        proof: StopTargetProof,
        cause: StopCause,
    ) -> Result<StopOwnership, StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        if !command.is_current() || state.persistent_failure.is_some() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        if state.stops.contains_key(&proof.syndic_thread_id()) {
            let ownership = {
                let local = state
                    .stops
                    .get_mut(&proof.syndic_thread_id())
                    .expect("checked local stop remains present");
                self.join_local(local, &proof, cause)?
            };
            let target = state
                .stops
                .get(&proof.syndic_thread_id())
                .expect("joined local stop remains present")
                .target
                .clone();
            cancel_automatic_continuation(&mut state, &target);
            return Ok(ownership);
        }
        drop(state);

        let permit = router.acquire_stop_election(command, &proof)?;
        #[cfg(test)]
        self.pause_race_if_requested(StopRaceStage::ElectionHeldBeforeAdmissionGate);
        #[cfg(test)]
        self.pause_race_if_requested(StopRaceStage::BeforeClaimFence);
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        if state.stops.contains_key(&proof.syndic_thread_id()) {
            if state.persistent_failure.is_some() {
                return Err(StopCoordinationError::HomeAuthorityLost);
            }
            let ownership = {
                let local = state
                    .stops
                    .get_mut(&proof.syndic_thread_id())
                    .expect("checked local stop remains present after stop election");
                self.join_local(local, &proof, cause)?
            };
            let target = state
                .stops
                .get(&proof.syndic_thread_id())
                .expect("joined local stop remains present after stop election")
                .target
                .clone();
            cancel_automatic_continuation(&mut state, &target);
            drop(state);
            permit.finish();
            return Ok(ownership);
        }

        let (live, permit, _command) = match self.read(proof.syndic_thread_id())? {
            StopAdmissionRead::Admissible(candidate) => {
                if !proof.matches(candidate.target()) {
                    return Err(StopCoordinationError::TargetUnavailable);
                }
                let operation_nonce = random_operation_nonce()?;
                let admission = candidate.admission(operation_nonce, StopCauseSet::from(cause));
                let (permit, command) = match permit
                    .admission(candidate.target().turn_id())
                    .map_err(|_| StopCoordinationError::HomeAuthorityLost)?
                {
                    StopElectionAdmission::Current { permit, command } => (permit, command),
                    StopElectionAdmission::PersistentFailure(failure) => {
                        cancel_automatic_continuation_by_identity(
                            &mut state,
                            proof.syndic_thread_id(),
                            failure.syndic_turn_id(),
                        );
                        drop(state);
                        failure
                            .preserve()
                            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
                        return Err(StopCoordinationError::HomeAuthorityLost);
                    }
                    StopElectionAdmission::Closed => {
                        return Err(StopCoordinationError::HomeAuthorityLost);
                    }
                };
                #[cfg(test)]
                self.pause_race_if_requested(StopRaceStage::ClaimFenceHeld);
                self.execute_admission(&admission)?;
                (
                    self.require_live(proof.syndic_thread_id(), admission.operation_id())?,
                    permit,
                    command,
                )
            }
            StopAdmissionRead::Stopping(_) => {
                return Err(StopCoordinationError::LocalAuthorityMismatch);
            }
            StopAdmissionRead::Ineligible(reason) => {
                return Err(match reason {
                    _ => StopCoordinationError::TargetUnavailable,
                });
            }
        };
        let live = self.join_if_missing(live, cause)?;
        cancel_automatic_continuation(&mut state, live.target());
        state.stops.insert(
            proof.syndic_thread_id(),
            LocalStop {
                operation_id: live.operation_id(),
                target: live.target().clone(),
                attempt: None,
                dispatch: LocalDispatchState::AdmittedNotClaimed,
                timeout: proof.request_timeout(),
            },
        );
        let attempt = match random_attempt_nonce() {
            Ok(attempt) => attempt,
            Err(error) => {
                drop(state);
                self.settle_unclaimed(live.operation_id())?;
                return Err(error);
            }
        };
        let claim = ClaimStopDispatch::new(
            live.operation_id(),
            live.target().clone(),
            live.current_gate_revision(),
            live.stop_revision(),
            attempt,
        );
        {
            let local = state
                .stops
                .get_mut(&proof.syndic_thread_id())
                .expect("admitted local stop remains present before its claim");
            local.attempt = Some(attempt);
            local.dispatch = LocalDispatchState::ClaimUnresolved;
        }
        if let Err(error) = self.execute_claim(&claim) {
            if matches!(error, StopCoordinationError::TransitionNotCommitted) {
                let local = state
                    .stops
                    .get_mut(&proof.syndic_thread_id())
                    .expect("prior claim reconciliation retains the admitted local stop");
                local.attempt = None;
                local.dispatch = LocalDispatchState::AdmittedNotClaimed;
            }
            drop(state);
            if matches!(error, StopCoordinationError::TransitionNotCommitted) {
                self.settle_unclaimed(live.operation_id())?;
            }
            return Err(error);
        }
        {
            let local = state
                .stops
                .get_mut(&proof.syndic_thread_id())
                .expect("admitted local stop remains present through its claim");
            local.attempt = Some(attempt);
            local.dispatch = LocalDispatchState::ClaimedNotDispatched;
        }
        let live = match self.require_live(proof.syndic_thread_id(), live.operation_id()) {
            Ok(live) => live,
            Err(error) => {
                let local = state
                    .stops
                    .get_mut(&proof.syndic_thread_id())
                    .expect("claimed local stop remains present during reconciliation");
                local.dispatch = LocalDispatchState::ClaimUnresolved;
                drop(state);
                return Err(error);
            }
        };
        if live.state() != StopOperationState::DispatchClaimed
            || live.attempt() != Some(attempt)
            || !proof.matches(live.target())
        {
            let local = state
                .stops
                .get_mut(&proof.syndic_thread_id())
                .expect("claimed local stop remains present during reconciliation");
            local.dispatch = LocalDispatchState::ClaimUnresolved;
            drop(state);
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        Ok(StopOwnership::Primary(StopDispatchOwner {
            coordinator: Arc::clone(self),
            operation_id: live.operation_id(),
            target: live.target().clone(),
            attempt,
            permit: Some(permit),
            timeout: proof.request_timeout(),
            _command,
            settled: false,
        }))
    }

    fn join_local(
        &self,
        local: &mut LocalStop,
        proof: &StopTargetProof,
        cause: StopCause,
    ) -> Result<StopOwnership, StopCoordinationError> {
        if !proof.matches(&local.target) {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        let live = self.require_live(local.target.thread_id(), local.operation_id)?;
        if live.target() != &local.target || live.attempt() != local.attempt {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        let live = self.join_if_missing(live, cause)?;
        let attempt_disposition = match local.dispatch {
            LocalDispatchState::AdmittedNotClaimed => {
                return Err(StopCoordinationError::LocalAuthorityMismatch);
            }
            LocalDispatchState::FailureFrozenNondispatch => {
                return Err(StopCoordinationError::HomeAuthorityLost);
            }
            LocalDispatchState::ClaimUnresolved => {
                return Err(StopCoordinationError::LocalAuthorityMismatch);
            }
            LocalDispatchState::ClaimedNotDispatched => {
                StopAttemptDisposition::ClaimedNotDispatched(attempt_correlation(
                    local
                        .attempt
                        .expect("claimed local stop retains its attempt"),
                ))
            }
            LocalDispatchState::Dispatching
            | LocalDispatchState::ProvenNondispatchSettling
            | LocalDispatchState::PrimaryAccepted
            | LocalDispatchState::PossiblyDispatched
            | LocalDispatchState::DurablyAbandoned => {
                StopAttemptDisposition::PossiblyDispatched(attempt_correlation(
                    local
                        .attempt
                        .expect("dispatched local stop retains its attempt"),
                ))
            }
        };
        Ok(StopOwnership::Joined {
            operation_id: live.operation_id(),
            interruption: durable_interruption(
                live.target(),
                live.operation_id(),
                attempt_disposition,
            ),
        })
    }

    fn join_if_missing(
        &self,
        live: SyndicLiveStopOperation,
        cause: StopCause,
    ) -> Result<SyndicLiveStopOperation, StopCoordinationError> {
        if live.record().causes().contains(cause) {
            return Ok(live);
        }
        let request = JoinStopCause::new(
            live.operation_id(),
            live.target().clone(),
            live.current_gate_revision(),
            live.stop_revision(),
            cause,
        );
        let home = self.current_home()?;
        require_stop_committed(
            home.execute_current(self.storage.current_join_stop_cause(request.clone())),
        )?;
        self.require_live(live.target().thread_id(), live.operation_id())
    }

    fn execute_admission(
        &self,
        request: &syndic_storage::AdmitStopOperation,
    ) -> Result<(), StopCoordinationError> {
        let home = self.current_home()?;
        require_stop_committed(
            home.execute_current(self.storage.current_admit_stop_operation(request.clone())),
        )
    }

    fn execute_claim(&self, request: &ClaimStopDispatch) -> Result<(), StopCoordinationError> {
        let home = self.current_home()?;
        require_stop_committed(
            home.execute_current(self.storage.current_claim_stop_dispatch(request.clone())),
        )
    }

    fn read(&self, thread_id: SyndicThreadId) -> Result<StopAdmissionRead, StopCoordinationError> {
        let home = self.current_home()?;
        Ok(self
            .storage
            .stop_admission_read(&home, thread_id, point_limit())?)
    }

    fn require_live(
        &self,
        thread_id: SyndicThreadId,
        operation_id: StopOperationId,
    ) -> Result<SyndicLiveStopOperation, StopCoordinationError> {
        match self.read(thread_id)? {
            StopAdmissionRead::Stopping(live) if live.operation_id() == operation_id => Ok(*live),
            StopAdmissionRead::Stopping(_)
            | StopAdmissionRead::Admissible(_)
            | StopAdmissionRead::Ineligible(_) => {
                Err(StopCoordinationError::LocalAuthorityMismatch)
            }
        }
    }

    fn ensure_current(&self) -> Result<(), StopCoordinationError> {
        self.current_home().map(drop)
    }

    fn current_home(&self) -> Result<Arc<HomeStore>, StopCoordinationError> {
        let home = self
            .home
            .upgrade()
            .ok_or(StopCoordinationError::HomeAuthorityLost)?;
        let health = home.health();
        if home.home_id() != self.home_id
            || health.generation() != Some(self.home_generation)
            || health.state() != beryl_home_store::HomeHealthState::Healthy
        {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        Ok(home)
    }

    pub(in crate::cas_projection) fn record_lifecycle_yield(
        &self,
        thread_id: SyndicThreadId,
        turn_id: beryl_model::SyndicTurnId,
        outcome: crate::LifecycleYieldOutcome,
    ) -> Result<bool, StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let key = LifecycleYieldKey { thread_id, turn_id };
        if state.lifecycle_yields.contains_key(&key) {
            return Ok(false);
        }
        if outcome == crate::LifecycleYieldOutcome::PhaseContinue
            && match self.read(thread_id)? {
                StopAdmissionRead::Stopping(live) => live.target().turn_id() == turn_id,
                StopAdmissionRead::Admissible(_) | StopAdmissionRead::Ineligible(_) => false,
            }
        {
            return Ok(false);
        }
        if !command.is_current() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        state.lifecycle_yields.insert(key, outcome);
        Ok(true)
    }

    pub(in crate::cas_projection) fn take_terminal_lifecycle_yield(
        &self,
        thread_id: SyndicThreadId,
        turn_id: beryl_model::SyndicTurnId,
    ) -> Result<Option<crate::LifecycleYieldOutcome>, StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()?;
        if !command.is_current() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        Ok(state
            .lifecycle_yields
            .remove(&LifecycleYieldKey { thread_id, turn_id }))
    }

    pub(in crate::cas_projection) fn has_terminal_phase_continue(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<bool, StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()?;
        if !command.is_current() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        Ok(state
            .lifecycle_yields
            .get(&LifecycleYieldKey { thread_id, turn_id })
            .is_some_and(|outcome| *outcome == crate::LifecycleYieldOutcome::PhaseContinue))
    }

    fn window_close_status(
        &self,
        operation_id: StopOperationId,
        target_turn_id: SyndicTurnId,
    ) -> Result<WindowCloseStopBarrierStatus, StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()?;
        let read = self.read(operation_id.thread_id())?;
        if !command.is_current() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        match read {
            StopAdmissionRead::Stopping(live)
                if live.operation_id() == operation_id
                    && live.target().turn_id() == target_turn_id =>
            {
                Ok(WindowCloseStopBarrierStatus::Waiting)
            }
            StopAdmissionRead::Stopping(_) | StopAdmissionRead::Admissible(_) => {
                Err(StopCoordinationError::LocalAuthorityMismatch)
            }
            StopAdmissionRead::Ineligible(reason) => {
                window_close_ineligible_status(reason, target_turn_id)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/stop_coordinator.rs"
    ));
}

fn cancel_automatic_continuation(state: &mut StopCoordinatorState, target: &StopOperationTarget) {
    cancel_automatic_continuation_by_identity(state, target.thread_id(), target.turn_id());
}

fn cancel_automatic_continuation_by_identity(
    state: &mut StopCoordinatorState,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
) {
    state.lifecycle_yields.retain(|key, outcome| {
        key.thread_id != thread_id
            || key.turn_id != turn_id
            || *outcome != crate::LifecycleYieldOutcome::PhaseContinue
    });
}

fn window_close_ineligible_status(
    reason: StopAdmissionIneligibility,
    target_turn_id: SyndicTurnId,
) -> Result<WindowCloseStopBarrierStatus, StopCoordinationError> {
    match reason {
        StopAdmissionIneligibility::PendingTurn { turn_id, .. }
        | StopAdmissionIneligibility::AwaitingTerminal { turn_id, .. }
        | StopAdmissionIneligibility::FinalizingHistory { turn_id, .. } => {
            Ok(if turn_id == target_turn_id {
                WindowCloseStopBarrierStatus::Waiting
            } else {
                WindowCloseStopBarrierStatus::Converged
            })
        }
        StopAdmissionIneligibility::AwaitingSteering { turn_id, .. }
        | StopAdmissionIneligibility::DeliveringSteering { turn_id, .. }
            if turn_id == target_turn_id =>
        {
            Err(StopCoordinationError::LocalAuthorityMismatch)
        }
        StopAdmissionIneligibility::Idle { .. }
        | StopAdmissionIneligibility::AwaitingSteering { .. }
        | StopAdmissionIneligibility::Compacting { .. }
        | StopAdmissionIneligibility::DeliveringSteering { .. } => {
            Ok(WindowCloseStopBarrierStatus::Converged)
        }
    }
}

fn durable_interruption(
    target: &StopOperationTarget,
    operation_id: StopOperationId,
    attempt_disposition: StopAttemptDisposition,
) -> ApprovalInterruption {
    ApprovalInterruption::DurableStopOwned {
        operation: operation_correlation(operation_id),
        target: ExactForegroundTurn::new(
            target.runtime_id(),
            target.loaded_generation(),
            target.cas_thread_id().clone(),
            target.cas_turn_id().clone(),
        ),
        attempt_disposition,
    }
}

fn operation_correlation(operation_id: StopOperationId) -> StopOperationCorrelation {
    StopOperationCorrelation::from_bytes(*operation_id.nonce().as_bytes())
}

fn attempt_correlation(attempt: StopAttemptNonce) -> StopAttemptCorrelation {
    StopAttemptCorrelation::from_bytes(*attempt.as_bytes())
}

fn random_operation_nonce() -> Result<StopOperationNonce, StopCoordinationError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| StopCoordinationError::RandomUnavailable)?;
    Ok(StopOperationNonce::from_bytes(bytes))
}

fn random_attempt_nonce() -> Result<StopAttemptNonce, StopCoordinationError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| StopCoordinationError::RandomUnavailable)?;
    Ok(StopAttemptNonce::from_bytes(bytes))
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(STOP_POINT_READ_BYTES).expect("stop point-read bound is nonzero")
}

fn system_timestamp_at_least(
    minimum: SyndicTimestamp,
) -> Result<SyndicTimestamp, StopCoordinationError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?
        .as_millis();
    let millis =
        u64::try_from(millis).map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
    Ok(SyndicTimestamp::from_unix_millis(millis).max(minimum))
}

impl StopDispatchOwner {
    pub(in crate::cas_projection) const fn operation_id(&self) -> StopOperationId {
        self.operation_id
    }

    pub(in crate::cas_projection) fn interruption(&self) -> ApprovalInterruption {
        durable_interruption(
            &self.target,
            self.operation_id,
            StopAttemptDisposition::ClaimedNotDispatched(attempt_correlation(self.attempt)),
        )
    }

    pub(in crate::cas_projection) fn exact_target(&self) -> ExactForegroundTurn {
        ExactForegroundTurn::new(
            self.target.runtime_id(),
            self.target.loaded_generation(),
            self.target.cas_thread_id().clone(),
            self.target.cas_turn_id().clone(),
        )
    }

    pub(in crate::cas_projection) fn operation_correlation(&self) -> StopOperationCorrelation {
        operation_correlation(self.operation_id)
    }

    pub(in crate::cas_projection) fn attempt_correlation(&self) -> StopAttemptCorrelation {
        attempt_correlation(self.attempt)
    }

    pub(in crate::cas_projection) const fn timeout(&self) -> std::time::Duration {
        self.timeout
    }

    pub(in crate::cas_projection) fn begin_dispatch(&self) -> Result<(), StopCoordinationError> {
        #[cfg(test)]
        self.coordinator
            .pause_race_if_requested(StopRaceStage::BeforeBeginDispatchFence);
        let mut state = self
            .coordinator
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        if !self._command.is_current() || state.persistent_failure.is_some() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        #[cfg(test)]
        self.coordinator
            .pause_race_if_requested(StopRaceStage::BeginDispatchFenceHeld);
        let local = state
            .stops
            .get_mut(&self.target.thread_id())
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if local.operation_id != self.operation_id
            || local.attempt != Some(self.attempt)
            || local.dispatch != LocalDispatchState::ClaimedNotDispatched
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        local.dispatch = LocalDispatchState::Dispatching;
        Ok(())
    }

    pub(in crate::cas_projection) fn settle_interrupt(
        mut self,
        outcome: &TurnInterruptOutcome,
    ) -> Result<StopDispatchSettlement, StopCoordinationError> {
        if outcome.request().operation_correlation() != self.operation_correlation()
            || outcome.request().attempt_correlation() != self.attempt_correlation()
            || outcome.request().target() != &self.exact_target()
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        if self.coordinator.failure_cut_is_active() {
            self.permit
                .take()
                .expect("failure-frozen stop owner retains its election")
                .finish();
            self.settled = true;
            return Ok(StopDispatchSettlement::Stopping(self.operation_id));
        }
        let permit = self
            .permit
            .take()
            .expect("stop owner retains its election until primary settlement");
        let settlement = match outcome.disposition() {
            TurnInterruptDisposition::RequestAccepted => {
                self.coordinator.mark_primary_accepted(
                    self.target.thread_id(),
                    self.operation_id,
                    self.attempt,
                )?;
                permit.finish();
                StopDispatchSettlement::Stopping(self.operation_id)
            }
            TurnInterruptDisposition::CompletionUnknown { .. } => {
                self.coordinator.mark_possibly_dispatched(
                    self.target.thread_id(),
                    self.operation_id,
                    self.attempt,
                )?;
                permit.finish();
                StopDispatchSettlement::Stopping(self.operation_id)
            }
            TurnInterruptDisposition::ProvenNotDispatched { .. } => {
                self.coordinator.prepare_proven_nondispatch(
                    self.target.thread_id(),
                    self.operation_id,
                    self.attempt,
                )?;
                permit.finish();
                self.coordinator
                    .settle_proven_nondispatch(self.operation_id, Some(self.attempt))?
            }
            TurnInterruptDisposition::RejectedBeforeCoreInterrupt => {
                let settlement = self.coordinator.abandon(
                    self.operation_id,
                    StopAbandonmentReason::ProviderRejectedBeforeCoreInterrupt,
                )?;
                permit.finish();
                settlement
            }
        };
        self.settled = true;
        Ok(settlement)
    }

    pub(in crate::cas_projection) fn settle_before_dispatch(
        mut self,
    ) -> Result<StopDispatchSettlement, StopCoordinationError> {
        if self.coordinator.failure_cut_is_active() {
            self.permit
                .take()
                .expect("failure-frozen stop owner retains its election")
                .finish();
            self.settled = true;
            return Ok(StopDispatchSettlement::Stopping(self.operation_id));
        }
        let permit = self
            .permit
            .take()
            .expect("stop owner retains its election until primary settlement");
        self.coordinator.prepare_proven_nondispatch(
            self.target.thread_id(),
            self.operation_id,
            self.attempt,
        )?;
        permit.finish();
        let settlement = self
            .coordinator
            .settle_proven_nondispatch(self.operation_id, Some(self.attempt))?;
        self.settled = true;
        Ok(settlement)
    }
}

impl Drop for StopDispatchOwner {
    fn drop(&mut self) {
        if !self.settled {
            self.coordinator.owner_dropped(
                self.target.thread_id(),
                self.operation_id,
                self.attempt,
            );
        }
    }
}

impl StopCoordinator {
    pub(in crate::cas_projection) fn terminal_consumed(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) {
        if self.failure_cut_is_active() {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            let consumed = state
                .stops
                .get(&thread_id)
                .filter(|local| local.target.turn_id() == turn_id)
                .is_some();
            if consumed {
                state.stops.remove(&thread_id);
            }
        }
    }

    pub(in crate::cas_projection) fn abandon_for_authority_loss(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<bool, StopCoordinationError> {
        if self.failure_cut_is_active() {
            return Ok(false);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let local = state
            .stops
            .get(&thread_id)
            .filter(|local| local.target.turn_id() == turn_id)
            .cloned();
        drop(state);
        let Some(local) = local else {
            return Ok(false);
        };
        if local.dispatch == LocalDispatchState::DurablyAbandoned {
            self.remove_local(local.operation_id);
            return Ok(true);
        }
        let operation_id = local.operation_id;
        match self.read(thread_id)? {
            StopAdmissionRead::Stopping(live) if live.operation_id() == operation_id => {
                self.abandon(operation_id, StopAbandonmentReason::TargetAuthorityLost)?;
                self.remove_local(operation_id);
                Ok(true)
            }
            StopAdmissionRead::Stopping(_) => Err(StopCoordinationError::LocalAuthorityMismatch),
            StopAdmissionRead::Admissible(_) | StopAdmissionRead::Ineligible(_) => {
                self.remove_local(operation_id);
                Ok(false)
            }
        }
    }

    fn mark_possibly_dispatched(
        &self,
        thread_id: SyndicThreadId,
        operation_id: StopOperationId,
        attempt: StopAttemptNonce,
    ) -> Result<(), StopCoordinationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let local = state
            .stops
            .get_mut(&thread_id)
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if local.operation_id != operation_id || local.attempt != Some(attempt) {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        local.dispatch = LocalDispatchState::PossiblyDispatched;
        Ok(())
    }

    fn mark_primary_accepted(
        &self,
        thread_id: SyndicThreadId,
        operation_id: StopOperationId,
        attempt: StopAttemptNonce,
    ) -> Result<(), StopCoordinationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let local = state
            .stops
            .get_mut(&thread_id)
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if local.operation_id != operation_id
            || local.attempt != Some(attempt)
            || local.dispatch != LocalDispatchState::Dispatching
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        local.dispatch = LocalDispatchState::PrimaryAccepted;
        Ok(())
    }

    fn owner_dropped(
        self: &Arc<Self>,
        thread_id: SyndicThreadId,
        operation_id: StopOperationId,
        attempt: StopAttemptNonce,
    ) {
        if self.failure_cut_is_active() {
            return;
        }
        {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            let Some(local) = state.stops.get_mut(&thread_id) else {
                return;
            };
            if local.operation_id != operation_id || local.attempt != Some(attempt) {
                return;
            }
            local.dispatch = match local.dispatch {
                LocalDispatchState::AdmittedNotClaimed
                | LocalDispatchState::ClaimedNotDispatched
                | LocalDispatchState::ProvenNondispatchSettling => {
                    LocalDispatchState::ClaimUnresolved
                }
                LocalDispatchState::Dispatching | LocalDispatchState::PrimaryAccepted => {
                    LocalDispatchState::PossiblyDispatched
                }
                LocalDispatchState::ClaimUnresolved
                | LocalDispatchState::PossiblyDispatched
                | LocalDispatchState::DurablyAbandoned
                | LocalDispatchState::FailureFrozenNondispatch => local.dispatch,
            };
        }
    }

    fn settle_unclaimed(
        self: &Arc<Self>,
        operation_id: StopOperationId,
    ) -> Result<StopDispatchSettlement, StopCoordinationError> {
        self.settle_proven_nondispatch(operation_id, None)
    }

    fn prepare_proven_nondispatch(
        &self,
        thread_id: SyndicThreadId,
        operation_id: StopOperationId,
        attempt: StopAttemptNonce,
    ) -> Result<(), StopCoordinationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let local = state
            .stops
            .get_mut(&thread_id)
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if local.operation_id != operation_id
            || local.attempt != Some(attempt)
            || !matches!(
                local.dispatch,
                LocalDispatchState::ClaimedNotDispatched | LocalDispatchState::Dispatching
            )
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        local.dispatch = LocalDispatchState::ProvenNondispatchSettling;
        Ok(())
    }

    fn settle_proven_nondispatch(
        self: &Arc<Self>,
        operation_id: StopOperationId,
        expected_attempt: Option<StopAttemptNonce>,
    ) -> Result<StopDispatchSettlement, StopCoordinationError> {
        if self.failure_cut_is_active() {
            return Ok(StopDispatchSettlement::Stopping(operation_id));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let local = state
            .stops
            .get(&operation_id.thread_id())
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        let exact_local_authority = local.operation_id == operation_id
            && match expected_attempt {
                None => {
                    local.attempt.is_none()
                        && local.dispatch == LocalDispatchState::AdmittedNotClaimed
                }
                Some(attempt) => {
                    local.attempt == Some(attempt)
                        && matches!(
                            local.dispatch,
                            LocalDispatchState::ClaimedNotDispatched
                                | LocalDispatchState::Dispatching
                                | LocalDispatchState::ProvenNondispatchSettling
                        )
                }
            };
        if !exact_local_authority {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        drop(state);

        let live = self.require_live(operation_id.thread_id(), operation_id)?;
        let exact_durable_authority = match expected_attempt {
            None => live.state() == StopOperationState::Admitted && live.attempt().is_none(),
            Some(attempt) => {
                live.state() == StopOperationState::DispatchClaimed
                    && live.attempt() == Some(attempt)
            }
        };
        if !exact_durable_authority {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        if live
            .record()
            .causes()
            .contains(StopCause::InterruptingApproval)
        {
            return self.abandon(operation_id, StopAbandonmentReason::TargetAuthorityLost);
        }
        let request = SafelyReopenStopOperation::new(
            operation_id,
            live.target().clone(),
            live.current_gate_revision(),
            live.stop_revision(),
        );
        let home = self.current_home()?;
        require_stop_committed(
            home.execute_current(
                self.storage
                    .current_safely_reopen_stop_operation(request.clone()),
            ),
        )?;
        self.remove_local(operation_id);
        Ok(StopDispatchSettlement::SafelyReopened(operation_id))
    }

    fn abandon(
        &self,
        operation_id: StopOperationId,
        reason: StopAbandonmentReason,
    ) -> Result<StopDispatchSettlement, StopCoordinationError> {
        if self.failure_cut_is_active() {
            return Ok(StopDispatchSettlement::Stopping(operation_id));
        }
        let live = self.require_live(operation_id.thread_id(), operation_id)?;
        let observed_at = system_timestamp_at_least(live.minimum_timestamp())?;
        let stale = live
            .startup_stale_binding("exact stop target authority lost", observed_at)
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let request = AbandonStopOperation::new(
            operation_id,
            live.target().clone(),
            live.current_gate_revision(),
            live.stop_revision(),
            live.current_state_revision(),
            reason,
            stale,
        );
        let home = self.current_home()?;
        require_stop_committed(
            home.execute_current(self.storage.current_abandon_stop_operation(request.clone())),
        )?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let local = state
            .stops
            .get_mut(&operation_id.thread_id())
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if local.operation_id != operation_id {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        local.dispatch = LocalDispatchState::DurablyAbandoned;
        Ok(StopDispatchSettlement::Abandoned(operation_id))
    }

    fn remove_local(&self, operation_id: StopOperationId) {
        if let Ok(mut state) = self.state.lock()
            && state
                .stops
                .get(&operation_id.thread_id())
                .is_some_and(|local| local.operation_id == operation_id)
        {
            state.stops.remove(&operation_id.thread_id());
        }
    }
}
