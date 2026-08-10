use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::cas_projection::{
    persistent_failure::{PersistentFailureCommandFrontier, PersistentFailureCutIdentity},
    service_config::ProjectionWorkerPermit,
    service_startup::ServiceStartupGate,
};

use super::super::ConnectionEpochIdentity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DriverParkControl {
    old_epoch: ConnectionEpochIdentity,
    cut: PersistentFailureCutIdentity,
    frontier: PersistentFailureCommandFrontier,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum DriverParkErrorReason {
    IdentityMismatch,
    SlotOccupied,
    CutNotNewer,
    DriverStopped,
    CoordinationPoisoned,
    WorkerUnavailable,
    StartupGateUnavailable,
}

#[derive(Debug)]
pub(in crate::cas_projection) struct DriverParkError {
    reason: DriverParkErrorReason,
}

pub(in crate::cas_projection) struct ParkedDriver {
    old_worker: Option<ProjectionWorkerPermit>,
    token: Option<DriverParkToken>,
}

pub(in crate::cas_projection) struct DriverParkToken {
    slot: Arc<DriverAdoptionSlot>,
    attempt: u64,
    active: bool,
}

pub(in crate::cas_projection) struct AdoptedDriverParkToken {
    slot: Arc<DriverAdoptionSlot>,
    attempt: u64,
    active: bool,
}

pub(in crate::cas_projection) struct DriverParkBindError {
    reason: DriverParkErrorReason,
    token: Option<DriverParkToken>,
    worker: Option<ProjectionWorkerPermit>,
}

struct ReplacementDriverAdmission {
    epoch: ConnectionEpochIdentity,
    worker: ProjectionWorkerPermit,
    startup: Arc<ServiceStartupGate>,
    resume: bool,
}

enum DriverAdoptionState {
    Empty {
        last_cut: Option<PersistentFailureCutIdentity>,
    },
    Pending {
        attempt: u64,
        control: DriverParkControl,
    },
    Parked {
        attempt: u64,
        control: DriverParkControl,
        old_worker: Option<ProjectionWorkerPermit>,
        replacement: Option<ReplacementDriverAdmission>,
    },
    Starting {
        attempt: u64,
        cut: PersistentFailureCutIdentity,
        worker: Option<ProjectionWorkerPermit>,
    },
    Quiesced {
        last_cut: Option<PersistentFailureCutIdentity>,
        admissions: [Option<ProjectionWorkerPermit>; 2],
    },
    Disabled {
        cut: PersistentFailureCutIdentity,
        admissions: [Option<ProjectionWorkerPermit>; 2],
    },
    Stopped {
        admissions: [Option<ProjectionWorkerPermit>; 2],
    },
}

pub(super) struct DriverAdoptionSlot {
    state: Mutex<DriverAdoptionState>,
    changed: Condvar,
    next_attempt: Mutex<u64>,
}

pub(super) enum DriverAdoptionPoll<'a> {
    Work(DriverExecutionGuard<'a>),
    Park { attempt: u64 },
    AwaitDisposition,
    Disposed,
}

pub(super) struct DriverExecutionGuard<'a> {
    state: MutexGuard<'a, DriverAdoptionState>,
    changed: &'a Condvar,
}

pub(super) enum DriverParkWaitOutcome {
    Resumed(ProjectionWorkerPermit),
    Disposed,
}

impl DriverParkControl {
    pub(super) const fn new(
        old_epoch: ConnectionEpochIdentity,
        cut: PersistentFailureCutIdentity,
        frontier: PersistentFailureCommandFrontier,
    ) -> Self {
        Self {
            old_epoch,
            cut,
            frontier,
        }
    }

    fn is_exact(self) -> bool {
        self.old_epoch.home_id() == self.cut.home_id
            && self.old_epoch.home_generation() == self.cut.home_generation
            && self.old_epoch.service_generation() == self.cut.service_generation
            && self
                .frontier
                .matches_cut(self.cut.service_generation, self.cut.failure_generation)
    }

    pub(super) const fn cut(self) -> PersistentFailureCutIdentity {
        self.cut
    }

    pub(super) const fn frontier(self) -> PersistentFailureCommandFrontier {
        self.frontier
    }
}

impl DriverParkError {
    pub(in crate::cas_projection) fn new(reason: DriverParkErrorReason) -> Self {
        Self { reason }
    }

    pub(in crate::cas_projection) const fn reason(&self) -> DriverParkErrorReason {
        self.reason
    }
}

impl std::fmt::Display for DriverParkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "connection driver adoption park failed: {:?}",
            self.reason
        )
    }
}

impl std::error::Error for DriverParkError {}

impl DriverExecutionGuard<'_> {
    /// Converts one guarded executable cycle into stable non-executable ownership.
    ///
    /// The worker admission moves under the same slot guard before hub coordination can be
    /// released to a racing exact-cut inert conversion.
    pub(super) fn quiesce_after_coordination_loss(
        mut self,
        worker: &mut Option<ProjectionWorkerPermit>,
    ) {
        let last_cut = match &*self.state {
            DriverAdoptionState::Empty { last_cut } => *last_cut,
            _ => unreachable!("only an empty adoption slot grants driver execution"),
        };
        let admissions = collect_admissions(&mut self.state, worker.take());
        *self.state = DriverAdoptionState::Quiesced {
            last_cut,
            admissions,
        };
        self.changed.notify_all();
    }
}

impl DriverAdoptionSlot {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(DriverAdoptionState::Empty { last_cut: None }),
            changed: Condvar::new(),
            next_attempt: Mutex::new(1),
        })
    }

    pub(super) fn park(
        self: &Arc<Self>,
        control: DriverParkControl,
    ) -> Result<ParkedDriver, DriverParkError> {
        if !control.is_exact() {
            return Err(DriverParkError::new(
                DriverParkErrorReason::IdentityMismatch,
            ));
        }
        let attempt = self.allocate_attempt()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| DriverParkError::new(DriverParkErrorReason::CoordinationPoisoned))?;
        match &*state {
            DriverAdoptionState::Empty { last_cut }
                if last_cut.is_some_and(|last| !cut_is_strictly_newer(last, control.cut)) =>
            {
                return Err(DriverParkError::new(DriverParkErrorReason::CutNotNewer));
            }
            DriverAdoptionState::Empty { .. } => {}
            DriverAdoptionState::Stopped { .. } => {
                return Err(DriverParkError::new(DriverParkErrorReason::DriverStopped));
            }
            DriverAdoptionState::Pending { .. }
            | DriverAdoptionState::Parked { .. }
            | DriverAdoptionState::Starting { .. }
            | DriverAdoptionState::Quiesced { .. }
            | DriverAdoptionState::Disabled { .. } => {
                return Err(DriverParkError::new(DriverParkErrorReason::SlotOccupied));
            }
        }
        *state = DriverAdoptionState::Pending { attempt, control };
        self.changed.notify_all();
        loop {
            match &mut *state {
                DriverAdoptionState::Parked {
                    attempt: parked_attempt,
                    old_worker,
                    ..
                } if *parked_attempt == attempt => {
                    let old_worker = old_worker.take().ok_or_else(|| {
                        DriverParkError::new(DriverParkErrorReason::WorkerUnavailable)
                    })?;
                    return Ok(ParkedDriver {
                        old_worker: Some(old_worker),
                        token: Some(DriverParkToken {
                            slot: Arc::clone(self),
                            attempt,
                            active: true,
                        }),
                    });
                }
                DriverAdoptionState::Disabled { .. } | DriverAdoptionState::Stopped { .. } => {
                    return Err(DriverParkError::new(DriverParkErrorReason::DriverStopped));
                }
                _ => {}
            }
            state = self.changed.wait(state).map_err(|poison| {
                let mut poisoned = poison.into_inner();
                disable_state(&mut poisoned, control.cut, None);
                self.changed.notify_all();
                DriverParkError::new(DriverParkErrorReason::CoordinationPoisoned)
            })?;
        }
    }

    pub(super) fn begin_cycle(
        &self,
        worker: &mut Option<ProjectionWorkerPermit>,
    ) -> DriverAdoptionPoll<'_> {
        let state = self.state.lock();
        let mut state = match state {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                let disposed =
                    quiesce_or_disable_after_coordination_loss(&mut state, worker.take());
                self.changed.notify_all();
                return if disposed {
                    DriverAdoptionPoll::Disposed
                } else {
                    DriverAdoptionPoll::AwaitDisposition
                };
            }
        };
        match &*state {
            DriverAdoptionState::Empty { .. } => DriverAdoptionPoll::Work(DriverExecutionGuard {
                state,
                changed: &self.changed,
            }),
            DriverAdoptionState::Pending { attempt, .. } => {
                DriverAdoptionPoll::Park { attempt: *attempt }
            }
            DriverAdoptionState::Parked { control, .. } => {
                let cut = control.cut;
                disable_state(&mut state, cut, worker.take());
                self.changed.notify_all();
                DriverAdoptionPoll::AwaitDisposition
            }
            DriverAdoptionState::Starting { cut, .. } => {
                let cut = *cut;
                disable_state(&mut state, cut, worker.take());
                self.changed.notify_all();
                DriverAdoptionPoll::AwaitDisposition
            }
            DriverAdoptionState::Quiesced { .. } | DriverAdoptionState::Disabled { .. } => {
                if worker.is_some() {
                    retain_admission(&mut state, worker.take());
                }
                DriverAdoptionPoll::AwaitDisposition
            }
            DriverAdoptionState::Stopped { .. } => {
                if worker.is_some() {
                    retain_admission(&mut state, worker.take());
                }
                DriverAdoptionPoll::Disposed
            }
        }
    }

    pub(super) fn pending_park_control(&self, attempt: u64) -> Option<DriverParkControl> {
        let state = self.state.lock().ok()?;
        match &*state {
            DriverAdoptionState::Pending {
                attempt: pending_attempt,
                control,
            } if *pending_attempt == attempt => Some(*control),
            _ => None,
        }
    }

    pub(super) fn park_and_wait(
        &self,
        attempt: u64,
        worker: &mut Option<ProjectionWorkerPermit>,
    ) -> DriverParkWaitOutcome {
        let state = self.state.lock();
        let mut state = match state {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                quiesce_or_disable_after_coordination_loss(&mut state, worker.take());
                self.changed.notify_all();
                return self.wait_awaiting_disposition(state);
            }
        };
        if matches!(
            &*state,
            DriverAdoptionState::Quiesced { .. } | DriverAdoptionState::Disabled { .. }
        ) {
            retain_admission(&mut state, worker.take());
            return self.wait_awaiting_disposition(state);
        }
        let control = match &*state {
            DriverAdoptionState::Pending {
                attempt: pending_attempt,
                control,
            } if *pending_attempt == attempt => *control,
            DriverAdoptionState::Stopped { .. } => {
                retain_admission(&mut state, worker.take());
                return DriverParkWaitOutcome::Disposed;
            }
            _ => {
                quiesce_or_disable_after_coordination_loss(&mut state, worker.take());
                self.changed.notify_all();
                return self.wait_awaiting_disposition(state);
            }
        };
        let Some(old_worker) = worker.take() else {
            disable_state(&mut state, control.cut, None);
            self.changed.notify_all();
            return self.wait_awaiting_disposition(state);
        };
        *state = DriverAdoptionState::Parked {
            attempt,
            control,
            old_worker: Some(old_worker),
            replacement: None,
        };
        self.changed.notify_all();
        loop {
            if matches!(
                &*state,
                DriverAdoptionState::Quiesced { .. } | DriverAdoptionState::Disabled { .. }
            ) {
                return self.wait_awaiting_disposition(state);
            }
            match &mut *state {
                DriverAdoptionState::Parked {
                    attempt: parked_attempt,
                    control,
                    replacement,
                    ..
                } if *parked_attempt == attempt => {
                    if replacement
                        .as_ref()
                        .is_some_and(|replacement| replacement.resume)
                    {
                        let parked_control = *control;
                        let replacement = replacement
                            .take()
                            .expect("a resumable driver retains replacement admission");
                        debug_assert_ne!(replacement.epoch, parked_control.old_epoch);
                        let startup = Arc::clone(&replacement.startup);
                        *state = DriverAdoptionState::Starting {
                            attempt,
                            cut: parked_control.cut,
                            worker: Some(replacement.worker),
                        };
                        self.changed.notify_all();
                        drop(state);
                        let startup_open = startup.wait();
                        let mut state = match self.state.lock() {
                            Ok(state) => state,
                            Err(poison) => poison.into_inner(),
                        };
                        let exact = matches!(
                            &*state,
                            DriverAdoptionState::Starting {
                                attempt: starting_attempt,
                                cut,
                                ..
                            } if *starting_attempt == attempt && *cut == parked_control.cut
                        );
                        if !exact {
                            if matches!(
                                &*state,
                                DriverAdoptionState::Quiesced { .. }
                                    | DriverAdoptionState::Disabled { .. }
                            ) {
                                return self.wait_awaiting_disposition(state);
                            }
                            quiesce_or_disable_after_coordination_loss(&mut state, None);
                            self.changed.notify_all();
                            return self.wait_awaiting_disposition(state);
                        }
                        let DriverAdoptionState::Starting { worker, .. } = &mut *state else {
                            unreachable!("the exact driver-start state remains locked")
                        };
                        let worker = worker
                            .take()
                            .expect("a driver waiting on service startup retains its admission");
                        if startup_open {
                            *state = DriverAdoptionState::Empty {
                                last_cut: Some(parked_control.cut),
                            };
                            self.changed.notify_all();
                            return DriverParkWaitOutcome::Resumed(worker);
                        }
                        disable_state(&mut state, parked_control.cut, Some(worker));
                        self.changed.notify_all();
                        return self.wait_awaiting_disposition(state);
                    }
                }
                DriverAdoptionState::Stopped { .. } => {
                    return DriverParkWaitOutcome::Disposed;
                }
                _ => {
                    quiesce_or_disable_after_coordination_loss(&mut state, None);
                    self.changed.notify_all();
                    return self.wait_awaiting_disposition(state);
                }
            }
            state = match self.changed.wait(state) {
                Ok(state) => state,
                Err(poison) => {
                    let mut state = poison.into_inner();
                    disable_state(&mut state, control.cut, None);
                    self.changed.notify_all();
                    return self.wait_awaiting_disposition(state);
                }
            };
        }
    }

    pub(super) fn wait_inert(&self) -> DriverParkWaitOutcome {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        self.wait_awaiting_disposition(state)
    }

    fn wait_awaiting_disposition<'a>(
        &self,
        mut state: MutexGuard<'a, DriverAdoptionState>,
    ) -> DriverParkWaitOutcome {
        loop {
            if matches!(&*state, DriverAdoptionState::Stopped { .. }) {
                return DriverParkWaitOutcome::Disposed;
            }
            state = match self.changed.wait(state) {
                Ok(state) => state,
                Err(poison) => poison.into_inner(),
            };
        }
    }

    /// Releases a terminal inert escrow only for an explicit consuming disposition.
    ///
    /// Ordinary stop notification deliberately cannot cross this boundary: while the slot is
    /// disabled, the backend-owning driver remains parked and performs no shutdown I/O.
    pub(super) fn release_inert_for_disposition(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let admissions = match std::mem::replace(
            &mut *state,
            DriverAdoptionState::Stopped {
                admissions: [None, None],
            },
        ) {
            DriverAdoptionState::Disabled { admissions, .. } => admissions,
            previous @ DriverAdoptionState::Stopped { .. } => {
                *state = previous;
                return true;
            }
            previous => {
                *state = previous;
                return false;
            }
        };
        self.changed.notify_all();
        drop(state);
        drop(admissions);
        true
    }

    pub(super) fn notify_stop(&self) {
        self.changed.notify_all();
    }

    pub(super) fn is_inert(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        matches!(
            &*state,
            DriverAdoptionState::Quiesced { .. } | DriverAdoptionState::Disabled { .. }
        )
    }

    pub(super) fn disable_for_failure(&self, cut: PersistentFailureCutIdentity) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        disable_state_for_exact_cut(&mut state, cut);
        drop(state);
        self.changed.notify_all();
    }

    fn allocate_attempt(&self) -> Result<u64, DriverParkError> {
        let mut next = self
            .next_attempt
            .lock()
            .map_err(|_| DriverParkError::new(DriverParkErrorReason::CoordinationPoisoned))?;
        let attempt = *next;
        *next = next
            .checked_add(1)
            .ok_or_else(|| DriverParkError::new(DriverParkErrorReason::DriverStopped))?;
        Ok(attempt)
    }
}

impl ParkedDriver {
    pub(in crate::cas_projection) fn into_parts(
        mut self,
    ) -> (ProjectionWorkerPermit, DriverParkToken) {
        let worker = self
            .old_worker
            .take()
            .expect("parked driver owns its old worker admission");
        let token = self
            .token
            .take()
            .expect("parked driver owns its one-shot park token");
        (worker, token)
    }
}

impl Drop for ParkedDriver {
    fn drop(&mut self) {
        let old_worker = self.old_worker.take();
        if let Some(mut token) = self.token.take() {
            token.disable_with(old_worker);
        }
    }
}

impl DriverParkToken {
    pub(in crate::cas_projection) fn bind_replacement(
        mut self,
        epoch: ConnectionEpochIdentity,
        worker: ProjectionWorkerPermit,
        startup: Arc<ServiceStartupGate>,
    ) -> Result<AdoptedDriverParkToken, DriverParkBindError> {
        let slot = Arc::clone(&self.slot);
        let mut state = match slot.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return Err(DriverParkBindError::new(
                    DriverParkErrorReason::CoordinationPoisoned,
                    self,
                    worker,
                ));
            }
        };
        let exact = matches!(
            &*state,
            DriverAdoptionState::Parked {
                attempt,
                control,
                replacement: None,
                ..
            } if *attempt == self.attempt && replacement_epoch_is_exact(*control, epoch)
        );
        if !exact {
            drop(state);
            return Err(DriverParkBindError::new(
                DriverParkErrorReason::IdentityMismatch,
                self,
                worker,
            ));
        }
        let DriverAdoptionState::Parked { replacement, .. } = &mut *state else {
            unreachable!("the exact parked state was checked while its guard remained held")
        };
        *replacement = Some(ReplacementDriverAdmission {
            epoch,
            worker,
            startup,
            resume: false,
        });
        drop(state);
        self.active = false;
        Ok(AdoptedDriverParkToken {
            slot,
            attempt: self.attempt,
            active: true,
        })
    }

    fn disable_with(&mut self, old_worker: Option<ProjectionWorkerPermit>) {
        if !self.active {
            return;
        }
        let mut state = match self.slot.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        if let Some(cut) = state_cut(&state) {
            disable_state(&mut state, cut, old_worker);
        } else {
            quiesce_or_disable_after_coordination_loss(&mut state, old_worker);
        }
        self.active = false;
        self.slot.changed.notify_all();
    }
}

impl Drop for DriverParkToken {
    fn drop(&mut self) {
        self.disable_with(None);
    }
}

impl AdoptedDriverParkToken {
    /// Arms the parked driver against the exact still-closed replacement startup gate.
    ///
    /// The driver may move to its starting state, but it cannot execute until the later process
    /// publication commit opens this pointer-identical gate.
    pub(in crate::cas_projection) fn arm_for_publication(
        mut self,
        startup: &Arc<ServiceStartupGate>,
    ) -> Result<(), DriverParkError> {
        let startup_guard = startup
            .lock_for_publication()
            .map_err(|()| DriverParkError::new(DriverParkErrorReason::StartupGateUnavailable))?;
        let mut state = self
            .slot
            .state
            .lock()
            .map_err(|_| DriverParkError::new(DriverParkErrorReason::CoordinationPoisoned))?;
        let DriverAdoptionState::Parked {
            attempt,
            replacement: Some(replacement),
            ..
        } = &mut *state
        else {
            return Err(DriverParkError::new(DriverParkErrorReason::DriverStopped));
        };
        if *attempt != self.attempt || !Arc::ptr_eq(&replacement.startup, startup) {
            return Err(DriverParkError::new(
                DriverParkErrorReason::IdentityMismatch,
            ));
        }
        replacement.resume = true;
        drop(state);
        drop(startup_guard);
        self.active = false;
        self.slot.changed.notify_all();
        Ok(())
    }

    fn disable(&mut self) {
        if !self.active {
            return;
        }
        let mut state = match self.slot.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        if let Some(cut) = state_cut(&state) {
            disable_state(&mut state, cut, None);
        } else {
            quiesce_or_disable_after_coordination_loss(&mut state, None);
        }
        self.active = false;
        self.slot.changed.notify_all();
    }
}

impl Drop for AdoptedDriverParkToken {
    fn drop(&mut self) {
        self.disable();
    }
}

impl DriverParkBindError {
    fn new(
        reason: DriverParkErrorReason,
        token: DriverParkToken,
        worker: ProjectionWorkerPermit,
    ) -> Self {
        Self {
            reason,
            token: Some(token),
            worker: Some(worker),
        }
    }

    pub(in crate::cas_projection) const fn reason(&self) -> DriverParkErrorReason {
        self.reason
    }

    pub(in crate::cas_projection) fn into_parts(
        mut self,
    ) -> (DriverParkToken, ProjectionWorkerPermit) {
        let token = self
            .token
            .take()
            .expect("driver bind error retains its park token");
        let worker = self
            .worker
            .take()
            .expect("driver bind error retains replacement admission");
        (token, worker)
    }
}

impl std::fmt::Debug for DriverParkBindError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DriverParkBindError")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

fn replacement_epoch_is_exact(
    control: DriverParkControl,
    replacement: ConnectionEpochIdentity,
) -> bool {
    replacement.home_id() == control.old_epoch.home_id()
        && replacement.home_generation() > control.old_epoch.home_generation()
        && replacement.service_generation() > control.old_epoch.service_generation()
}

fn cut_is_strictly_newer(
    previous: PersistentFailureCutIdentity,
    next: PersistentFailureCutIdentity,
) -> bool {
    previous.home_id == next.home_id
        && (next.service_generation > previous.service_generation
            || (next.service_generation == previous.service_generation
                && next.failure_generation > previous.failure_generation))
}

fn state_cut(state: &DriverAdoptionState) -> Option<PersistentFailureCutIdentity> {
    match state {
        DriverAdoptionState::Pending { control, .. }
        | DriverAdoptionState::Parked { control, .. } => Some(control.cut),
        DriverAdoptionState::Starting { cut, .. } => Some(*cut),
        DriverAdoptionState::Disabled { cut, .. } => Some(*cut),
        DriverAdoptionState::Empty { .. }
        | DriverAdoptionState::Quiesced { .. }
        | DriverAdoptionState::Stopped { .. } => None,
    }
}

fn disable_state_for_exact_cut(
    state: &mut DriverAdoptionState,
    cut: PersistentFailureCutIdentity,
) -> bool {
    let exact = match state {
        DriverAdoptionState::Empty { last_cut }
        | DriverAdoptionState::Quiesced { last_cut, .. } => {
            last_cut.is_none_or(|last| cut_is_strictly_newer(last, cut))
        }
        DriverAdoptionState::Pending { control, .. }
        | DriverAdoptionState::Parked { control, .. } => control.cut == cut,
        DriverAdoptionState::Starting {
            cut: starting_cut, ..
        } => *starting_cut == cut,
        DriverAdoptionState::Disabled {
            cut: disabled_cut, ..
        } => return *disabled_cut == cut,
        DriverAdoptionState::Stopped { .. } => return false,
    };
    if exact {
        disable_state(state, cut, None);
    }
    exact
}

fn disable_state(
    state: &mut DriverAdoptionState,
    cut: PersistentFailureCutIdentity,
    extra: Option<ProjectionWorkerPermit>,
) {
    let admissions = collect_admissions(state, extra);
    *state = DriverAdoptionState::Disabled { cut, admissions };
}

fn quiesce_or_disable_after_coordination_loss(
    state: &mut DriverAdoptionState,
    extra: Option<ProjectionWorkerPermit>,
) -> bool {
    match state {
        DriverAdoptionState::Empty { last_cut } => {
            let last_cut = *last_cut;
            let admissions = collect_admissions(state, extra);
            *state = DriverAdoptionState::Quiesced {
                last_cut,
                admissions,
            };
            false
        }
        DriverAdoptionState::Pending { control, .. }
        | DriverAdoptionState::Parked { control, .. } => {
            let cut = control.cut;
            disable_state(state, cut, extra);
            false
        }
        DriverAdoptionState::Starting { cut, .. } | DriverAdoptionState::Disabled { cut, .. } => {
            let cut = *cut;
            disable_state(state, cut, extra);
            false
        }
        DriverAdoptionState::Quiesced { admissions, .. } => {
            if let Some(worker) = extra {
                insert_admission(admissions, worker);
            }
            false
        }
        DriverAdoptionState::Stopped { admissions } => {
            if let Some(worker) = extra {
                insert_admission(admissions, worker);
            }
            true
        }
    }
}

fn retain_admission(state: &mut DriverAdoptionState, admission: Option<ProjectionWorkerPermit>) {
    let Some(admission) = admission else {
        return;
    };
    match state {
        DriverAdoptionState::Quiesced { admissions, .. }
        | DriverAdoptionState::Disabled { admissions, .. }
        | DriverAdoptionState::Stopped { admissions } => insert_admission(admissions, admission),
        DriverAdoptionState::Empty { .. }
        | DriverAdoptionState::Pending { .. }
        | DriverAdoptionState::Parked { .. }
        | DriverAdoptionState::Starting { .. } => {
            unreachable!("only a non-executable driver state retains a loose admission")
        }
    }
}

fn collect_admissions(
    state: &mut DriverAdoptionState,
    extra: Option<ProjectionWorkerPermit>,
) -> [Option<ProjectionWorkerPermit>; 2] {
    let previous = std::mem::replace(
        state,
        DriverAdoptionState::Stopped {
            admissions: [None, None],
        },
    );
    let mut admissions = [None, None];
    match previous {
        DriverAdoptionState::Parked {
            old_worker,
            replacement,
            ..
        } => {
            if let Some(worker) = old_worker {
                insert_admission(&mut admissions, worker);
            }
            if let Some(replacement) = replacement {
                insert_admission(&mut admissions, replacement.worker);
            }
        }
        DriverAdoptionState::Starting { worker, .. } => {
            if let Some(worker) = worker {
                insert_admission(&mut admissions, worker);
            }
        }
        DriverAdoptionState::Quiesced {
            admissions: retained,
            ..
        }
        | DriverAdoptionState::Disabled {
            admissions: retained,
            ..
        }
        | DriverAdoptionState::Stopped {
            admissions: retained,
        } => {
            for worker in retained.into_iter().flatten() {
                insert_admission(&mut admissions, worker);
            }
        }
        DriverAdoptionState::Empty { .. } | DriverAdoptionState::Pending { .. } => {}
    }
    if let Some(worker) = extra {
        insert_admission(&mut admissions, worker);
    }
    admissions
}

fn insert_admission(
    admissions: &mut [Option<ProjectionWorkerPermit>; 2],
    worker: ProjectionWorkerPermit,
) {
    if admissions[0].is_none() {
        admissions[0] = Some(worker);
    } else if admissions[1].is_none() {
        admissions[1] = Some(worker);
    } else {
        unreachable!("one driver adoption attempt can retain at most two worker admissions")
    }
}

impl std::fmt::Debug for DriverAdoptionSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = self.state.lock().map(|state| match &*state {
            DriverAdoptionState::Empty { .. } => "empty",
            DriverAdoptionState::Pending { .. } => "pending",
            DriverAdoptionState::Parked { .. } => "parked",
            DriverAdoptionState::Starting { .. } => "starting",
            DriverAdoptionState::Quiesced { .. } => "quiesced",
            DriverAdoptionState::Disabled { .. } => "disabled",
            DriverAdoptionState::Stopped { .. } => "stopped",
        });
        formatter
            .debug_struct("DriverAdoptionSlot")
            .field("status", &status)
            .finish_non_exhaustive()
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/connection_driver_adoption.rs"
    ));
}
