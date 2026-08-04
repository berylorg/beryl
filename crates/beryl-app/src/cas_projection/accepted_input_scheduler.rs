mod completion;
mod failure;
mod next_turn;
mod recovered_pending;
mod recovered_projection;
mod signal;
mod steering;

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use completion::{WorkerCompletion, WorkerCompletions};
pub(in crate::cas_projection) use failure::AcceptedInputSchedulerExit;
use failure::SchedulerFailure;
use syndic_storage::SyndicStorage;

pub(in crate::cas_projection) use recovered_projection::{
    RecoveredProjectionLane, RecoveredProjectionLaneParts, RecoveredProjectionLaneStageError,
    RecoveredProjectionLaneStageReason,
};
pub use signal::{AcceptedInputSchedulerDiagnostics, ActiveSteeringRetryState};
pub(in crate::cas_projection) use signal::{
    AcceptedInputSchedulerSignal, AcceptedInputWakeReason, StartupRecoveryDiagnostics,
};
use steering::ScanState;

use crate::cas_projection::{
    PersistentFailureNotificationStatus, ProjectionCancellationToken, ProjectionCoordinatorError,
    persistent_failure::{MasterCommandGate, PersistentFailureProjectionRetainer},
    scheduled_ordinary::ScheduledOrdinaryExecutionProvider,
    service_config::ProjectionWorkerPool,
    service_registry::ProjectionServiceConnectionRegistry,
};

type ConnectionRegistry = Arc<ProjectionServiceConnectionRegistry>;
type ScheduledOrdinaryProvider = Arc<Mutex<Box<dyn ScheduledOrdinaryExecutionProvider>>>;
const SCHEDULER_PASS_PAGE_BUDGET: usize = 256;

struct ScanBudget {
    remaining_pages: usize,
}

impl ScanBudget {
    const fn new(remaining_pages: usize) -> Self {
        Self { remaining_pages }
    }

    fn take_page(&mut self) -> bool {
        let Some(remaining) = self.remaining_pages.checked_sub(1) else {
            return false;
        };
        self.remaining_pages = remaining;
        true
    }
}

#[derive(Clone)]
pub(in crate::cas_projection) struct ActiveSteeringCancellationLifecycle {
    current: Arc<Mutex<ProjectionCancellationToken>>,
}

impl ActiveSteeringCancellationLifecycle {
    pub(in crate::cas_projection) fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(ProjectionCancellationToken::new())),
        }
    }

    fn snapshot(&self) -> ProjectionCancellationToken {
        self.current
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    fn is_cancelled(&self) -> bool {
        self.snapshot().is_cancelled()
    }

    fn cancel_current(&self) {
        self.snapshot().cancel();
    }

    fn renew(&self) {
        *self
            .current
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = ProjectionCancellationToken::new();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkerDisposition {
    Settled,
    Parked,
    VerificationPending,
    RecoveredProjectionContinue,
    RecoveredProjectionParked,
    RecoveredPendingContinue,
    NextContinue,
    NextParked,
    PersistentHomeFailure,
    Fatal,
}

pub(in crate::cas_projection) struct AcceptedInputSchedulerContext {
    home: Arc<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    workers: ProjectionWorkerPool,
    connections: ConnectionRegistry,
    scheduled_ordinary_provider: ScheduledOrdinaryProvider,
    command_gate: MasterCommandGate,
    projection_retainer: PersistentFailureProjectionRetainer,
    cancellation: ActiveSteeringCancellationLifecycle,
    ordinary_cancellation: ProjectionCancellationToken,
    signal: AcceptedInputSchedulerSignal,
    recovered_projection_lane: RecoveredProjectionLane,
}

impl AcceptedInputSchedulerContext {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn new(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
        workers: ProjectionWorkerPool,
        connections: ConnectionRegistry,
        scheduled_ordinary_provider: ScheduledOrdinaryProvider,
        command_gate: MasterCommandGate,
        projection_retainer: PersistentFailureProjectionRetainer,
        cancellation: ActiveSteeringCancellationLifecycle,
        signal: AcceptedInputSchedulerSignal,
        recovered_projection_lane: RecoveredProjectionLane,
    ) -> Self {
        Self {
            home,
            home_id,
            home_generation,
            storage,
            workers,
            connections,
            scheduled_ordinary_provider,
            command_gate,
            projection_retainer,
            cancellation,
            ordinary_cancellation: ProjectionCancellationToken::new(),
            signal,
            recovered_projection_lane,
        }
    }
}

pub(in crate::cas_projection) struct AcceptedInputScheduler {
    signal: AcceptedInputSchedulerSignal,
    cancellation: ActiveSteeringCancellationLifecycle,
    ordinary_cancellation: ProjectionCancellationToken,
    handle: Option<JoinHandle<AcceptedInputSchedulerExit>>,
    #[cfg(feature = "test-faults")]
    test_identity: (
        BerylHomeId,
        HomeGeneration,
        crate::cas_projection::ProjectionServiceGeneration,
    ),
}

impl AcceptedInputScheduler {
    pub(in crate::cas_projection) fn start(
        context: AcceptedInputSchedulerContext,
    ) -> Result<Self, ProjectionCoordinatorError> {
        Self::start_with_startup_gate(
            context,
            crate::cas_projection::service_startup::ServiceStartupGate::open_gate(),
        )
    }

    pub(in crate::cas_projection) fn start_with_startup_gate(
        context: AcceptedInputSchedulerContext,
        startup: Arc<crate::cas_projection::service_startup::ServiceStartupGate>,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let signal = context.signal.clone();
        let cancellation = context.cancellation.clone();
        let ordinary_cancellation = context.ordinary_cancellation.clone();
        #[cfg(feature = "test-faults")]
        let test_identity = (
            context.home_id,
            context.home_generation,
            context.command_gate.service_generation(),
        );
        let handle = std::thread::Builder::new()
            .name("beryl-accepted-input-scheduler".to_owned())
            .spawn(move || {
                if !startup.wait() {
                    let failure = recovered_projection::dispose_retained(&context).err();
                    context.signal.update_diagnostics(|diagnostics| {
                        diagnostics.stopped = true;
                        diagnostics.fatal = failure.is_some();
                    });
                    return if failure.is_some() {
                        AcceptedInputSchedulerExit::Fatal
                    } else {
                        AcceptedInputSchedulerExit::Clean
                    };
                }
                let mut runtime = SchedulerRuntime::new(context);
                match catch_unwind(AssertUnwindSafe(|| runtime.run())) {
                    Ok(exit) => exit,
                    Err(payload) => {
                        runtime.emergency_quiesce();
                        resume_unwind(payload)
                    }
                }
            })
            .map_err(
                |source| ProjectionCoordinatorError::AcceptedInputSchedulerSpawn {
                    message: source.to_string(),
                },
            )?;
        Ok(Self {
            signal,
            cancellation,
            ordinary_cancellation,
            handle: Some(handle),
            #[cfg(feature = "test-faults")]
            test_identity,
        })
    }

    pub(in crate::cas_projection) fn diagnostics(&self) -> AcceptedInputSchedulerDiagnostics {
        self.signal.diagnostics()
    }

    pub(in crate::cas_projection) fn request_shutdown(&self) {
        self.cancellation.cancel_current();
        self.ordinary_cancellation.cancel();
        self.signal.request_shutdown();
    }

    #[allow(
        dead_code,
        reason = "the renewable boundary is mounted before the later stop controller"
    )]
    pub(in crate::cas_projection) fn cancel_current_lifecycle(&self) {
        self.cancellation.cancel_current();
        self.signal
            .wake(AcceptedInputWakeReason::CancellationRequested);
    }

    #[allow(
        dead_code,
        reason = "the renewable boundary is mounted before the later stop controller"
    )]
    pub(in crate::cas_projection) fn renew_cancellation_lifecycle(&self) {
        self.cancellation.renew();
        self.signal
            .wake(AcceptedInputWakeReason::CancellationLifecycle);
    }

    pub(in crate::cas_projection) fn join(mut self) -> Result<AcceptedInputSchedulerExit, ()> {
        let handle = self.handle.take().ok_or(())?;
        #[cfg(feature = "test-faults")]
        crate::cas_projection::test_faults::observe_accepted_input_scheduler_join(
            self.test_identity.0,
            self.test_identity.1,
            self.test_identity.2,
        );
        handle.join().map_err(|_| ())
    }
}

#[derive(Clone, Copy)]
enum WorkerKind {
    Steering,
    RecoveredProjection(beryl_model::SyndicThreadId),
    Next(beryl_model::SyndicThreadId),
}

struct WorkerRecord {
    handle: JoinHandle<WorkerDisposition>,
    thread_id: std::thread::ThreadId,
    kind: WorkerKind,
}

struct SchedulerRuntime {
    context: AcceptedInputSchedulerContext,
    workers: Vec<WorkerRecord>,
    pending_launch_gate: Option<steering::LaunchGate>,
    completions: WorkerCompletions,
    scan: Option<ScanState>,
    recovered_pending_scan: Option<recovered_pending::RecoveredPendingScanState>,
    recovered_projection_pass: u64,
    recovered_projection_scan: u64,
    recovered_projection_flight_waiting: bool,
    recovered_projection_worker_waiting: bool,
    next_scan: Option<next_turn::NextScanState>,
    retry_pass_active: bool,
    retry_reopen_pending: bool,
    parked_retry: bool,
    recovered_pending_pass_active: bool,
    recovered_pending_capacity_waiting: bool,
    recovered_pending_flight_waiting: bool,
    next_capacity_waiting: bool,
    next_flight_waiting: bool,
    next_active_worker_waiting: bool,
    verification_resumed_workers: Vec<std::thread::ThreadId>,
    failure: Option<SchedulerFailure>,
}

impl SchedulerRuntime {
    fn new(context: AcceptedInputSchedulerContext) -> Self {
        let completion_capacity = context.workers.diagnostics().capacity();
        Self {
            context,
            workers: Vec::with_capacity(completion_capacity),
            pending_launch_gate: None,
            completions: WorkerCompletions::new(completion_capacity),
            scan: None,
            recovered_pending_scan: None,
            recovered_projection_pass: 0,
            recovered_projection_scan: 0,
            recovered_projection_flight_waiting: false,
            recovered_projection_worker_waiting: false,
            next_scan: None,
            retry_pass_active: false,
            retry_reopen_pending: false,
            parked_retry: false,
            recovered_pending_pass_active: false,
            recovered_pending_capacity_waiting: false,
            recovered_pending_flight_waiting: false,
            next_capacity_waiting: false,
            next_flight_waiting: false,
            next_active_worker_waiting: false,
            verification_resumed_workers: Vec::with_capacity(completion_capacity),
            failure: None,
        }
    }

    fn run(&mut self) -> AcceptedInputSchedulerExit {
        loop {
            let wake = self.context.signal.wait();
            if wake.same_generation_verified() {
                self.mark_active_workers_verification_resumed();
            }
            #[cfg(all(test, feature = "test-faults"))]
            self.spawn_injected_worker_for_test();
            #[cfg(feature = "test-faults")]
            crate::cas_projection::test_faults::panic_accepted_input_scheduler_main_if_requested(
                self.context.home_id,
                self.context.home_generation,
                self.context.command_gate.service_generation(),
            );
            let (
                recovered_projection_worker_ready,
                recovered_pending_worker_ready,
                next_worker_ready,
                worker_verification_pending,
                late_worker_verification_resumed,
            ) = self.drain_completions();
            let ordinary_shutdown = match failure::gate_status(&self.context) {
                Ok(failure::SchedulerGateStatus::PersistentHomeFailure) => {
                    self.fail_closed(SchedulerFailure::PersistentHomeFailure);
                    false
                }
                Err(failure) => {
                    self.fail_closed(failure);
                    false
                }
                Ok(failure::SchedulerGateStatus::OrdinaryShutdown) => true,
                Ok(failure::SchedulerGateStatus::Open) => false,
            };
            if self.failure.is_some() || ordinary_shutdown || wake.shutdown() {
                break;
            }
            if worker_verification_pending {
                if self.park_for_verification() {
                    break;
                }
                continue;
            }
            let same_generation_verified =
                wake.same_generation_verified() || late_worker_verification_resumed;
            if wake.opens_retry_pass() {
                self.retry_reopen_pending = true;
            }
            if !self.has_active_steering_worker()
                && (wake.opens_steering_pass() || same_generation_verified)
            {
                if self.retry_reopen_pending {
                    self.retry_pass_active = true;
                    self.retry_reopen_pending = false;
                    self.parked_retry = false;
                    self.scan = Some(ScanState::default());
                }
                if self.scan.is_none() {
                    self.scan = Some(ScanState::default());
                }
                let retry_eligible = self.retry_pass_active;
                self.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.steering_pass_count =
                        diagnostics.steering_pass_count.saturating_add(1);
                    if retry_eligible {
                        diagnostics.retry_state = ActiveSteeringRetryState::Eligible;
                    }
                });
                if let Err(failure) = self.run_pass(retry_eligible) {
                    if failure == SchedulerFailure::VerificationPending {
                        if self.park_for_verification() {
                            break;
                        }
                        continue;
                    }
                    self.fail_closed(failure);
                    break;
                }
            }
            if wake.execution_ready() || same_generation_verified {
                self.recovered_projection_pass =
                    self.recovered_projection_pass.saturating_add(1).max(1);
            }
            let opens_recovered_projection_pass = self.recovered_projection_pass != 0
                && (wake.execution_ready()
                    || same_generation_verified
                    || recovered_projection_worker_ready
                    || (wake.projection_flight_released()
                        && self.recovered_projection_flight_waiting)
                    || (wake.worker_completed() && self.recovered_projection_worker_waiting));
            if opens_recovered_projection_pass
                && let Err(failure) = recovered_projection::run_pass(self)
            {
                if failure == SchedulerFailure::VerificationPending {
                    if self.park_for_verification() {
                        break;
                    }
                    continue;
                }
                self.fail_closed(failure);
                break;
            }
            if wake.restarts_recovered_pending_pass() || same_generation_verified {
                self.recovered_pending_pass_active = true;
                self.recovered_pending_scan =
                    Some(recovered_pending::RecoveredPendingScanState::default());
                self.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.recovered_pending_retained_source_cursor = false;
                });
            }
            let opens_recovered_pending_pass = self.recovered_pending_pass_active
                && (wake.restarts_recovered_pending_pass()
                    || same_generation_verified
                    || wake.continues_recovered_pending_pass()
                    || recovered_pending_worker_ready
                    || (wake.projection_flight_released()
                        && self.recovered_pending_flight_waiting)
                    || (wake.next_worker_capacity_released()
                        && self.recovered_pending_capacity_waiting));
            let recovered_pending_handoff = if opens_recovered_pending_pass {
                match recovered_pending::run_pass(self) {
                    Ok(outcome) => outcome.opens_next_pass(),
                    Err(failure) => {
                        if failure == SchedulerFailure::VerificationPending {
                            if self.park_for_verification() {
                                break;
                            }
                            continue;
                        }
                        self.fail_closed(failure);
                        break;
                    }
                }
            } else {
                false
            };
            let opens_next_pass = wake.opens_next_pass()
                || same_generation_verified
                || next_worker_ready
                || recovered_pending_handoff
                || (wake.projection_flight_released() && self.next_flight_waiting)
                || (wake.next_worker_capacity_released() && self.next_capacity_waiting);
            if opens_next_pass && let Err(failure) = next_turn::run_pass(self) {
                if failure == SchedulerFailure::VerificationPending {
                    if self.park_for_verification() {
                        break;
                    }
                    continue;
                }
                self.fail_closed(failure);
                break;
            }
        }
        self.join_all_workers();
        match failure::gate_status(&self.context) {
            Ok(failure::SchedulerGateStatus::PersistentHomeFailure) => {
                self.fail_closed(SchedulerFailure::PersistentHomeFailure);
            }
            Err(failure) => self.fail_closed(failure),
            Ok(
                failure::SchedulerGateStatus::Open | failure::SchedulerGateStatus::OrdinaryShutdown,
            ) => {}
        }
        let queue_settlement = if self.failure == Some(SchedulerFailure::PersistentHomeFailure) {
            recovered_projection::retain_for_persistent_failure(&self.context)
        } else {
            recovered_projection::dispose_retained(&self.context)
        };
        if let Err(failure) = queue_settlement {
            self.fail_closed(failure);
        }
        self.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.steering_retained_source_cursor = false;
            diagnostics.steering_retained_candidate_cursor = false;
            diagnostics.recovered_pending_retained_source_cursor = false;
            diagnostics.next_retained_source_cursor = false;
            diagnostics.next_retained_candidate_cursor = false;
            diagnostics.stopped = true;
            diagnostics.fatal = self.failure.is_some();
        });
        match self.failure {
            None => AcceptedInputSchedulerExit::Clean,
            Some(SchedulerFailure::VerificationPending) => {
                unreachable!("verification pending never becomes a terminal scheduler failure")
            }
            Some(SchedulerFailure::PersistentHomeFailure) => {
                AcceptedInputSchedulerExit::PersistentHomeFailure
            }
            Some(SchedulerFailure::Fatal) => AcceptedInputSchedulerExit::Fatal,
        }
    }

    /// Parks one consumed scheduler wake behind the exact supervisor-owned verification flight.
    /// Every scanner is safely rebased to durable source authority; no local timer or wake is
    /// issued. The exact service slot is the sole publisher of the resume wake.
    fn park_for_verification(&mut self) -> bool {
        self.scan = Some(ScanState::default());
        self.recovered_pending_scan = Some(recovered_pending::RecoveredPendingScanState::default());
        self.next_scan = Some(next_turn::NextScanState::default());
        self.recovered_projection_flight_waiting = false;
        self.recovered_projection_worker_waiting = false;
        self.recovered_pending_capacity_waiting = false;
        self.recovered_pending_flight_waiting = false;
        self.next_capacity_waiting = false;
        self.next_flight_waiting = false;
        self.next_active_worker_waiting = false;
        self.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.verification_pauses = diagnostics.verification_pauses.saturating_add(1);
            diagnostics.steering_retained_source_cursor = false;
            diagnostics.steering_retained_candidate_cursor = false;
            diagnostics.recovered_pending_retained_source_cursor = false;
            diagnostics.next_retained_source_cursor = false;
            diagnostics.next_retained_candidate_cursor = false;
        });
        match self
            .context
            .command_gate
            .authorizer()
            .observe_persistent_failure()
        {
            PersistentFailureNotificationStatus::VerificationSignaled
            | PersistentFailureNotificationStatus::VerificationJoined
            | PersistentFailureNotificationStatus::NotFailed => false,
            PersistentFailureNotificationStatus::Signaled
            | PersistentFailureNotificationStatus::Joined
            | PersistentFailureNotificationStatus::Unavailable => {
                self.fail_closed(SchedulerFailure::PersistentHomeFailure);
                true
            }
        }
    }

    fn fail_closed(&mut self, observed: SchedulerFailure) {
        debug_assert_ne!(observed, SchedulerFailure::VerificationPending);
        let Some(failure) = failure::reconcile_failure(&self.context, observed) else {
            self.context.cancellation.cancel_current();
            self.context.ordinary_cancellation.cancel();
            return;
        };
        self.failure = Some(
            self.failure
                .map_or(failure, |current| current.merge(failure)),
        );
        if failure == SchedulerFailure::Fatal {
            self.context.command_gate.close_for_local_failure();
        }
        self.context.cancellation.cancel_current();
        self.context.ordinary_cancellation.cancel();
        self.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.fatal = true;
        });
    }

    fn emergency_quiesce(&mut self) {
        self.context.command_gate.close_for_local_failure();
        self.context.cancellation.cancel_current();
        self.context.ordinary_cancellation.cancel();
        self.context.signal.request_shutdown();
        self.release_pending_launch_gate();

        let mut joined = 0_u64;
        while let Some(worker) = self.workers.pop() {
            let _ = worker.handle.join();
            joined = joined.saturating_add(1);
        }
        let _ = self.completions.drain();
        self.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.workers_joined = diagnostics.workers_joined.saturating_add(joined);
            diagnostics.workers_active = 0;
            diagnostics.steering_retained_source_cursor = false;
            diagnostics.steering_retained_candidate_cursor = false;
            diagnostics.recovered_pending_retained_source_cursor = false;
            diagnostics.next_retained_source_cursor = false;
            diagnostics.next_retained_candidate_cursor = false;
            diagnostics.stopped = true;
            diagnostics.fatal = true;
        });
    }

    fn release_pending_launch_gate(&mut self) {
        if let Some(gate) = self.pending_launch_gate.take() {
            gate.open();
        }
    }

    #[cfg(all(test, feature = "test-faults"))]
    fn spawn_injected_worker_for_test(&mut self) {
        let Some(request) =
            crate::cas_projection::test_faults::take_accepted_input_scheduler_worker(
                self.context.home_id,
                self.context.home_generation,
                self.context.command_gate.service_generation(),
            )
        else {
            return;
        };
        let crate::cas_projection::test_faults::AcceptedInputSchedulerWorkerRequest {
            projection,
            worker,
            owner,
            release,
            registered,
        } = request;
        let completions = self.completions.clone();
        let signal = self.context.signal.clone();
        let handle = std::thread::Builder::new()
            .name("beryl-injected-scheduler-projection-worker".to_owned())
            .spawn(move || {
                let _ = release.recv();
                drop(projection);
                drop(worker);
                let disposition = WorkerDisposition::PersistentHomeFailure;
                completions.publish(WorkerCompletion {
                    thread_id: std::thread::current().id(),
                    disposition,
                });
                signal.wake(AcceptedInputWakeReason::WorkerCompleted);
                disposition
            })
            .expect("test scheduler projection worker must spawn");
        self.register_next_worker(handle, owner);
        let _ = registered.send(());
    }
}

impl SchedulerRuntime {
    fn register_steering_worker(&mut self, handle: JoinHandle<WorkerDisposition>) {
        self.register_worker(handle, WorkerKind::Steering);
    }

    fn register_next_worker(
        &mut self,
        handle: JoinHandle<WorkerDisposition>,
        syndic_thread_id: beryl_model::SyndicThreadId,
    ) {
        self.register_worker(handle, WorkerKind::Next(syndic_thread_id));
    }

    fn register_recovered_projection_worker(
        &mut self,
        handle: JoinHandle<WorkerDisposition>,
        syndic_thread_id: beryl_model::SyndicThreadId,
    ) {
        self.register_worker(handle, WorkerKind::RecoveredProjection(syndic_thread_id));
    }

    fn register_worker(&mut self, handle: JoinHandle<WorkerDisposition>, kind: WorkerKind) {
        let thread_id = handle.thread().id();
        self.workers.push(WorkerRecord {
            handle,
            thread_id,
            kind,
        });
        let active = self.workers.len();
        self.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.workers_started = diagnostics.workers_started.saturating_add(1);
            diagnostics.workers_active = active;
            diagnostics.workers_high_water = diagnostics.workers_high_water.max(active);
        });
    }

    fn has_active_steering_worker(&self) -> bool {
        self.workers
            .iter()
            .any(|worker| matches!(worker.kind, WorkerKind::Steering))
    }

    fn has_active_next_worker(&self, syndic_thread_id: beryl_model::SyndicThreadId) -> bool {
        self.workers.iter().any(|worker| {
            matches!(
                worker.kind,
                WorkerKind::Next(active) | WorkerKind::RecoveredProjection(active)
                    if active == syndic_thread_id
            )
        })
    }

    fn mark_active_workers_verification_resumed(&mut self) {
        for worker in &self.workers {
            if !self
                .verification_resumed_workers
                .contains(&worker.thread_id)
            {
                self.verification_resumed_workers.push(worker.thread_id);
            }
        }
    }

    fn take_worker_verification_resume(&mut self, thread_id: std::thread::ThreadId) -> bool {
        let Some(index) = self
            .verification_resumed_workers
            .iter()
            .position(|covered| *covered == thread_id)
        else {
            return false;
        };
        self.verification_resumed_workers.swap_remove(index);
        true
    }

    fn exact_home_is_healthy(&self) -> bool {
        let health = self.context.home.health();
        self.context.home.home_id() == self.context.home_id
            && health.state() == beryl_home_store::HomeHealthState::Healthy
            && health.generation() == Some(self.context.home_generation)
    }

    fn drain_completions(&mut self) -> (bool, bool, bool, bool, bool) {
        let mut recovered_projection_worker_ready = false;
        let mut recovered_pending_worker_ready = false;
        let mut next_worker_ready = false;
        let mut verification_pending = false;
        let mut late_verification_resumed = false;
        for completion in self.completions.drain() {
            let Some(index) = self
                .workers
                .iter()
                .position(|worker| worker.thread_id == completion.thread_id)
            else {
                self.fail_closed(SchedulerFailure::Fatal);
                continue;
            };
            let worker = self.workers.swap_remove(index);
            let verification_was_resumed =
                self.take_worker_verification_resume(completion.thread_id);
            let result = worker.handle.join();
            self.record_worker_join();
            match result {
                Ok(disposition) if disposition == completion.disposition => {
                    let (
                        recovered_projection_ready,
                        recovered_pending_ready,
                        next_ready,
                        worker_verification_pending,
                    ) = self.apply_worker_disposition(completion.disposition);
                    recovered_projection_worker_ready |= recovered_projection_ready;
                    recovered_pending_worker_ready |= recovered_pending_ready;
                    next_worker_ready |= next_ready;
                    if worker_verification_pending
                        && verification_was_resumed
                        && self.exact_home_is_healthy()
                    {
                        late_verification_resumed = true;
                    } else {
                        verification_pending |= worker_verification_pending;
                    }
                }
                Ok(_) | Err(_) => self.fail_closed(SchedulerFailure::Fatal),
            }
        }
        (
            recovered_projection_worker_ready,
            recovered_pending_worker_ready,
            next_worker_ready,
            verification_pending,
            late_verification_resumed,
        )
    }

    fn join_all_workers(&mut self) {
        while let Some(worker) = self.workers.pop() {
            let _ = self.take_worker_verification_resume(worker.thread_id);
            let result = worker.handle.join();
            self.record_worker_join();
            match result {
                Ok(disposition) => {
                    let _ = self.apply_worker_disposition(disposition);
                }
                Err(_) => self.fail_closed(SchedulerFailure::Fatal),
            }
        }
        let _ = self.completions.drain();
    }

    fn record_worker_join(&mut self) {
        self.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.workers_joined = diagnostics.workers_joined.saturating_add(1);
            diagnostics.workers_active = diagnostics.workers_active.saturating_sub(1);
        });
    }

    fn apply_worker_disposition(
        &mut self,
        disposition: WorkerDisposition,
    ) -> (bool, bool, bool, bool) {
        match disposition {
            WorkerDisposition::Parked => {
                self.parked_retry = true;
                self.retry_pass_active = false;
                self.scan = None;
                self.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.retry_state = ActiveSteeringRetryState::Parked;
                });
                (false, false, false, false)
            }
            WorkerDisposition::Settled => (false, false, false, false),
            WorkerDisposition::VerificationPending => (false, false, false, true),
            WorkerDisposition::RecoveredProjectionContinue => (true, false, false, false),
            WorkerDisposition::RecoveredProjectionParked => (true, false, false, false),
            WorkerDisposition::RecoveredPendingContinue => (
                false,
                true,
                self.next_capacity_waiting || self.next_active_worker_waiting,
                false,
            ),
            WorkerDisposition::NextContinue => {
                (false, self.recovered_pending_capacity_waiting, true, false)
            }
            WorkerDisposition::NextParked => (false, false, false, false),
            WorkerDisposition::PersistentHomeFailure => {
                self.fail_closed(SchedulerFailure::PersistentHomeFailure);
                (false, false, false, false)
            }
            WorkerDisposition::Fatal => {
                self.fail_closed(SchedulerFailure::Fatal);
                (false, false, false, false)
            }
        }
    }
}
