use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::cas_projection::{
    CasProjectionCoordinator, LiveEventTarget, LoadedCasProjection,
    accepted_input_scheduler::{AcceptedInputSchedulerSignal, AcceptedInputWakeReason},
    connection::{ExactContextCompactionDispatch, ExistingLease, ProjectionConnection},
    persistent_failure::{LiveCommandAuthorizer, LiveCommandPermit},
    service_registry::ProjectionServiceConnectionRegistry,
    stop::StopCoordinator,
};
use beryl_backend::{CompactThreadDisposition, CompactionAttemptCorrelation};
use beryl_home_store::{CommandOutcome, HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    BindingState, ClaimCompactionDispatch, CompactionAbandonmentReason,
    CompactionAdmissionIneligibility, CompactionAdmissionRead, CompactionAttemptNonce,
    CompactionMarkerLifecycle, CompactionOperationId, CompactionOperationNonce,
    CompactionOperationRecord, CompactionOperationState, CompactionProviderEvent,
    CompactionProviderSequence, CompactionRequestDisposition, CompactionRequestTransitionStatus,
    CompactionSettlement, PublishCompactionProviderEvent, PublishCompactionRequestDisposition,
    SettleCompactionOperation, SettleLifecycleCompaction, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp,
};

use super::ContextCompactionTargetAuthority;
use super::{ContextCompactionTimeoutPolicy, ResolvedContextCompactionTimeout};

mod admission;
pub(in crate::cas_projection) mod custody;
use crate::cas_projection::compaction_work::{
    CompactionCommandWorkStage,
    source::{CompactionLocalObservation, CompactionWorkHandle},
};
use custody::{CompactionCommandCustody, CompactionCustodyPool};
pub(in crate::cas_projection) mod dispatch;
#[cfg(test)]
#[path = "../../../tests/unit/context_compaction_driver.rs"]
mod driver_tests;
pub(super) mod model;
mod settlement;
#[cfg(feature = "test-faults")]
mod test_faults;
mod work_facts;

#[cfg(feature = "test-faults")]
use model::validate_completion_timeout;
pub use model::{
    ContextCompactionDiagnostics, ContextCompactionError, ContextCompactionOutcome,
    ContextCompactionRequest,
};

fn require_committed_command(outcome: CommandOutcome) -> Result<(), ContextCompactionError> {
    match outcome {
        CommandOutcome::NotCommitted { evidence } => {
            Err(ContextCompactionError::CommandNotCommitted(evidence))
        }
        CommandOutcome::Committed {
            receipt: _,
            later_failure: None,
            local_finalization: _,
        } => Ok(()),
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
            local_finalization: _,
        } => Err(ContextCompactionError::CommandCommitted {
            receipt,
            later_failure,
        }),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            Err(ContextCompactionError::CommandIndeterminate { failure })
        }
    }
}
#[cfg(feature = "test-faults")]
pub use test_faults::{
    CompactionCustodyPauseController, CompactionCustodyPressureGuard, CompactionCustodyTestStage,
    ContextCompactionCapacityTestGuard, ContextCompactionLifecycleTestHarness,
    ContextCompactionSettlementPauseController, ContextCompactionStagingPauseController,
    ContextCompactionTerminalResponseTestOutcome, ContextCompactionWaitTestHarness,
};

const COMPACTION_POINT_READ_BYTES: usize = 1_000_000;
const COMPACTION_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const COMPACTION_QUEUE_CAPACITY: usize = 64;
const COMPACTION_WORKER_CAPACITY: usize = 8;

pub(in crate::cas_projection) struct ContextCompactionCoordinator {
    home: Arc<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    connections: Arc<ProjectionServiceConnectionRegistry>,
    stop: Arc<StopCoordinator>,
    commands: LiveCommandAuthorizer,
    scheduler_signal: AcceptedInputSchedulerSignal,
    closing: AtomicBool,
    settlement_fence: Mutex<()>,
    custody: Arc<CompactionCustodyPool>,
    #[cfg(feature = "test-faults")]
    custody_pauses: Mutex<test_faults::CompactionCustodyPauses>,
    operations: Mutex<HashMap<SyndicThreadId, RegisteredCompaction>>,
    work: Mutex<Option<mpsc::SyncSender<CompactionWork>>>,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
    queued_current: AtomicUsize,
    queued_high_water: AtomicUsize,
    workers_current: AtomicUsize,
    workers_high_water: AtomicUsize,
    denied_admissions: AtomicU64,
    lifecycle_continuation_failures: AtomicU64,
    #[cfg(feature = "test-faults")]
    fail_next_lifecycle_staging: AtomicBool,
    #[cfg(feature = "test-faults")]
    lifecycle_staging_pause: Mutex<Option<Arc<test_faults::LifecycleStagingPause>>>,
    #[cfg(feature = "test-faults")]
    lifecycle_settlement_pause: Mutex<Option<Arc<test_faults::LifecycleStagingPause>>>,
}

pub(in crate::cas_projection) enum LifecycleCompactionAdmission {
    Launched,
    NotLaunched(LoadedCasProjection),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompactionOrigin {
    Manual,
    Lifecycle { yielding_turn_id: SyndicTurnId },
}

struct LocalCompaction {
    operation_id: CompactionOperationId,
    attempt: CompactionAttemptNonce,
    origin: CompactionOrigin,
    completion_timeout: ResolvedContextCompactionTimeout,
    command: Mutex<Option<CompactionCommandCustody>>,
    observation: CompactionWorkHandle,
    mutation: Mutex<()>,
    wait: Mutex<CompactionWait>,
    changed: Condvar,
}

struct RegisteredCompaction {
    _membership: CompactionLocalObservation,
    local: Arc<LocalCompaction>,
}

impl std::ops::Deref for RegisteredCompaction {
    type Target = LocalCompaction;
    fn deref(&self) -> &Self::Target {
        &self.local
    }
}

#[derive(Default)]
struct CompactionWait {
    deadline: Option<Instant>,
    result: Option<ContextCompactionOutcome>,
}

enum CompactionWork {
    Operation {
        local: Arc<LocalCompaction>,
        target: LiveEventTarget,
    },
    #[cfg(feature = "test-faults")]
    Hold(Arc<test_faults::TestHoldGate>),
    #[cfg(feature = "test-faults")]
    Probe,
}

impl CompactionWork {
    fn into_nondispatch_target(self) -> Option<LiveEventTarget> {
        match self {
            Self::Operation { target, .. } => Some(target),
            #[cfg(feature = "test-faults")]
            Self::Hold(_) | Self::Probe => None,
        }
    }
}

impl ContextCompactionCoordinator {
    pub(in crate::cas_projection) fn new(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
        connections: Arc<ProjectionServiceConnectionRegistry>,
        stop: Arc<StopCoordinator>,
        commands: LiveCommandAuthorizer,
        scheduler_signal: AcceptedInputSchedulerSignal,
    ) -> Result<Arc<Self>, ContextCompactionError> {
        Self::new_with_initial_start(
            home,
            home_id,
            home_generation,
            storage,
            connections,
            stop,
            commands,
            scheduler_signal,
            crate::cas_projection::initial_start::InitialStartGate::ready(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn new_with_initial_start(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
        connections: Arc<ProjectionServiceConnectionRegistry>,
        stop: Arc<StopCoordinator>,
        commands: LiveCommandAuthorizer,
        scheduler_signal: AcceptedInputSchedulerSignal,
        initial_start: Arc<crate::cas_projection::initial_start::InitialStartGate>,
    ) -> Result<Arc<Self>, ContextCompactionError> {
        let (work, receiver) = mpsc::sync_channel(COMPACTION_QUEUE_CAPACITY);
        let receiver = Arc::new(Mutex::new(receiver));
        let custody = Arc::clone(&stop.compaction_custody);
        let coordinator = Arc::new(Self {
            home,
            home_id,
            home_generation,
            storage,
            connections,
            stop,
            commands,
            scheduler_signal,
            closing: AtomicBool::new(false),
            settlement_fence: Mutex::new(()),
            custody,
            #[cfg(feature = "test-faults")]
            custody_pauses: Mutex::new(test_faults::CompactionCustodyPauses::default()),
            operations: Mutex::new(HashMap::new()),
            work: Mutex::new(Some(work)),
            workers: Mutex::new(Vec::with_capacity(COMPACTION_WORKER_CAPACITY)),
            queued_current: AtomicUsize::new(0),
            queued_high_water: AtomicUsize::new(0),
            workers_current: AtomicUsize::new(0),
            workers_high_water: AtomicUsize::new(0),
            denied_admissions: AtomicU64::new(0),
            lifecycle_continuation_failures: AtomicU64::new(0),
            #[cfg(feature = "test-faults")]
            fail_next_lifecycle_staging: AtomicBool::new(false),
            #[cfg(feature = "test-faults")]
            lifecycle_staging_pause: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            lifecycle_settlement_pause: Mutex::new(None),
        });
        let mut workers = Vec::with_capacity(COMPACTION_WORKER_CAPACITY);
        for index in 0..COMPACTION_WORKER_CAPACITY {
            let weak = Arc::downgrade(&coordinator);
            let receiver = Arc::clone(&receiver);
            let initial_start = Arc::clone(&initial_start);
            workers.push(
                std::thread::Builder::new()
                    .name(format!("beryl-context-compaction-{index}"))
                    .spawn(move || {
                        if initial_start.wait() {
                            dispatch::run_worker(weak, receiver);
                        }
                    })
                    .map_err(|_| ContextCompactionError::Unavailable)?,
            );
        }
        *coordinator
            .workers
            .lock()
            .map_err(|_| ContextCompactionError::Unavailable)? = workers;
        Ok(coordinator)
    }

    pub(in crate::cas_projection) fn compact_thread(
        self: &Arc<Self>,
        request: ContextCompactionRequest,
    ) -> Result<ContextCompactionOutcome, ContextCompactionError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| ContextCompactionError::Unavailable)?;
        request.validate()?;
        self.ensure_current()?;
        let read = self
            .storage
            .compaction_admission_read(&self.home, request.thread_id(), point_limit())
            .map_err(|_| ContextCompactionError::Storage)?;
        let local = match read {
            CompactionAdmissionRead::Existing(operation) => {
                self.existing_local(operation.as_ref())?
            }
            CompactionAdmissionRead::Ineligible(reason) => {
                return Err(ContextCompactionError::Ineligible(reason));
            }
            CompactionAdmissionRead::Admissible(candidate) => {
                self.admit_manual(candidate.as_ref(), request.timeout_policy(), command)?
            }
        };
        Ok(local.wait())
    }

    pub(in crate::cas_projection) fn begin_lifecycle_continuation(
        self: &Arc<Self>,
        projection: LoadedCasProjection,
        yielding_turn_id: SyndicTurnId,
        timeout_policy: &ContextCompactionTimeoutPolicy,
    ) -> Result<LifecycleCompactionAdmission, ContextCompactionError> {
        let _fence = self
            .settlement_fence
            .lock()
            .map_err(|_| ContextCompactionError::Unavailable)?;
        let command = self
            .commands
            .authorize()
            .map_err(|_| ContextCompactionError::Unavailable)?;
        let Some(reservation) = self
            .stop
            .share_terminal_continuation_custody(projection.syndic_thread_id(), yielding_turn_id)
            .map_err(|_| ContextCompactionError::Unavailable)?
        else {
            timeout_policy.validate()?;
            self.ensure_current()?;
            return Ok(LifecycleCompactionAdmission::NotLaunched(projection));
        };
        let command = CompactionCommandCustody {
            command,
            preparation: reservation,
        };
        // Keep preparation custody until the projection has disposed, including on unwind.
        let projection = projection;
        #[cfg(feature = "test-faults")]
        self.pause_compaction_custody(CompactionCustodyTestStage::LifecyclePreparation);
        timeout_policy.validate()?;
        self.ensure_current()?;
        let read = self
            .storage
            .compaction_admission_read(&self.home, projection.syndic_thread_id(), point_limit())
            .map_err(|_| ContextCompactionError::Storage)?;
        let candidate = match read {
            CompactionAdmissionRead::Admissible(candidate) => candidate,
            CompactionAdmissionRead::Ineligible(
                CompactionAdmissionIneligibility::AcceptedNextEffective { .. },
            ) => {
                let _ = self
                    .stop
                    .take_terminal_lifecycle_yield(projection.syndic_thread_id(), yielding_turn_id);
                return Ok(LifecycleCompactionAdmission::NotLaunched(projection));
            }
            CompactionAdmissionRead::Existing(_) => {
                return Err(ContextCompactionError::AuthorityMismatch);
            }
            CompactionAdmissionRead::Ineligible(reason) => {
                return Err(ContextCompactionError::Ineligible(reason));
            }
        };
        if candidate.thread_id() != projection.syndic_thread_id()
            || candidate.binding_revision() != projection.binding_revision()
            || candidate.runtime_id() != projection.execution_binding().runtime_id()
            || candidate.cas_thread_id() != projection.cas_thread_id()
            || candidate.lineage() != projection.lineage_proof()
        {
            return Err(ContextCompactionError::AuthorityMismatch);
        }
        let completion_timeout = timeout_policy.resolve(&self.home)?;
        self.admit_lifecycle(
            projection,
            candidate.as_ref(),
            yielding_turn_id,
            completion_timeout,
            command,
        )?;
        Ok(LifecycleCompactionAdmission::Launched)
    }

    pub(in crate::cas_projection) fn shutdown(&self) -> Result<(), ContextCompactionError> {
        self.request_shutdown();
        let (workers, poisoned) = match self.workers.lock() {
            Ok(mut workers) => (std::mem::take(&mut *workers), false),
            Err(poison) => {
                let mut workers = poison.into_inner();
                (std::mem::take(&mut *workers), true)
            }
        };
        if join_all_workers(workers) || poisoned {
            return Err(ContextCompactionError::Unavailable);
        }
        Ok(())
    }

    pub(in crate::cas_projection) fn request_shutdown(&self) {
        {
            let _fence = self
                .settlement_fence
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            self.closing.store(true, Ordering::Release);
            if !self.commands.is_persistent_failure_cut() {
                self.stop.cancel_all_lifecycle_continuations();
            }
            self.work
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .take();
        }
        let locals = self
            .operations
            .lock()
            .map(|operations| {
                operations
                    .values()
                    .map(|entry| Arc::clone(&entry.local))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for local in locals {
            if matches!(local.origin, CompactionOrigin::Lifecycle { .. }) {
                let _mutation = local
                    .mutation
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                self.cancel_lifecycle_intent(&local);
            }
        }
    }

    pub(in crate::cas_projection) fn cancel_window_close_continuation(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<(), crate::cas_projection::stop::StopCoordinationError> {
        let _fence = self.settlement_fence.lock().map_err(|_| {
            crate::cas_projection::stop::StopCoordinationError::LocalAuthorityMismatch
        })?;
        self.ensure_current()
            .map_err(|_| crate::cas_projection::stop::StopCoordinationError::HomeAuthorityLost)?;
        self.stop.cancel_window_close_continuation(thread_id)
    }

    pub(in crate::cas_projection) fn diagnostics(&self) -> ContextCompactionDiagnostics {
        let retained_operations = self
            .operations
            .lock()
            .map(|operations| operations.len())
            .unwrap_or_default();
        ContextCompactionDiagnostics {
            queue_capacity: COMPACTION_QUEUE_CAPACITY,
            worker_capacity: COMPACTION_WORKER_CAPACITY,
            queued_current: self.queued_current.load(Ordering::Acquire),
            queued_high_water: self.queued_high_water.load(Ordering::Acquire),
            workers_current: self.workers_current.load(Ordering::Acquire),
            workers_high_water: self.workers_high_water.load(Ordering::Acquire),
            denied_admissions: self.denied_admissions.load(Ordering::Acquire),
            retained_operations,
            lifecycle_continuation_failures: self
                .lifecycle_continuation_failures
                .load(Ordering::Acquire),
        }
    }
}

fn join_all_workers(workers: Vec<std::thread::JoinHandle<()>>) -> bool {
    let mut failed = false;
    for worker in workers {
        failed |= worker.join().is_err();
    }
    failed
}

#[cfg(test)]
mod join_tests {
    use super::join_all_workers;

    #[test]
    fn join_failure_does_not_detach_later_workers() {
        let failed = std::thread::spawn(|| panic!("synthetic first worker failure"));
        let (settled, observed) = std::sync::mpsc::sync_channel(1);
        let later = std::thread::spawn(move || {
            settled.send(()).expect("observer remains live");
        });

        assert!(join_all_workers(vec![failed, later]));
        observed
            .try_recv()
            .expect("the later worker was joined despite the earlier failure");
    }

    #[test]
    fn poisoned_worker_registry_still_yields_every_exact_handle_for_join() {
        let (settled, observed) = std::sync::mpsc::sync_channel(2);
        let workers = std::sync::Mutex::new(vec![
            std::thread::spawn({
                let settled = settled.clone();
                move || settled.send(1).expect("observer remains live")
            }),
            std::thread::spawn(move || settled.send(2).expect("observer remains live")),
        ]);
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _guard = workers.lock().expect("worker registry begins usable");
                    panic!("poison worker registry while preserving its handles");
                })
                .join()
                .expect_err("poison worker must panic");
        });
        let mut recovered = workers
            .lock()
            .expect_err("worker registry must remain poisoned")
            .into_inner();
        assert!(!join_all_workers(std::mem::take(&mut *recovered)));
        assert!(observed.recv().is_ok());
        assert!(observed.recv().is_ok());
        assert!(recovered.is_empty());
    }
}

fn update_high_water(high_water: &AtomicUsize, candidate: usize) {
    let mut current = high_water.load(Ordering::Acquire);
    while candidate > current {
        match high_water.compare_exchange_weak(
            current,
            candidate,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

fn increment_bounded(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        Some(current.saturating_add(1))
    });
}

fn random_operation_nonce() -> Result<CompactionOperationNonce, ContextCompactionError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| ContextCompactionError::Unavailable)?;
    Ok(CompactionOperationNonce::from_bytes(bytes))
}

fn random_attempt_nonce() -> Result<CompactionAttemptNonce, ContextCompactionError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| ContextCompactionError::Unavailable)?;
    Ok(CompactionAttemptNonce::from_bytes(bytes))
}

fn timestamp_now() -> Result<SyndicTimestamp, ContextCompactionError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ContextCompactionError::Unavailable)?
        .as_millis();
    let millis = u64::try_from(millis).map_err(|_| ContextCompactionError::Unavailable)?;
    Ok(SyndicTimestamp::from_unix_millis(millis))
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(COMPACTION_POINT_READ_BYTES)
        .expect("compaction point-read bound is nonzero")
}

impl LocalCompaction {
    fn new(
        operation_id: CompactionOperationId,
        attempt: CompactionAttemptNonce,
        origin: CompactionOrigin,
        completion_timeout: ResolvedContextCompactionTimeout,
        command: CompactionCommandCustody,
    ) -> Self {
        let observation = command.observation();
        Self {
            operation_id,
            attempt,
            origin,
            completion_timeout,
            command: Mutex::new(Some(command)),
            observation,
            mutation: Mutex::new(()),
            wait: Mutex::new(CompactionWait::default()),
            changed: Condvar::new(),
        }
    }

    fn mark_accepted(&self) {
        let mut wait = self
            .wait
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if wait.result.is_none() && wait.deadline.is_none() {
            wait.deadline = Some(Instant::now() + self.completion_timeout.duration());
        }
        drop(wait);
        self.changed.notify_all();
    }

    const fn yielding_turn_id(&self) -> Option<SyndicTurnId> {
        match self.origin {
            CompactionOrigin::Manual => None,
            CompactionOrigin::Lifecycle { yielding_turn_id } => Some(yielding_turn_id),
        }
    }

    fn complete(&self, result: ContextCompactionOutcome) {
        let mut wait = self
            .wait
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if wait.result.is_none() {
            wait.result = Some(result);
            self.observation.complete(result);
        }
        drop(wait);
        self.changed.notify_all();
    }

    fn is_finished(&self) -> bool {
        self.wait
            .lock()
            .map(|wait| wait.result.is_some())
            .unwrap_or(true)
    }

    fn command_is_current(&self) -> bool {
        self.command
            .lock()
            .map(|command| {
                command
                    .as_ref()
                    .is_some_and(|custody| custody.command.is_current())
            })
            .unwrap_or(false)
    }

    fn release_command(&self) {
        let command = self
            .command
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        drop(command);
    }

    fn wait(&self) -> ContextCompactionOutcome {
        let mut wait = self
            .wait
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        loop {
            if let Some(result) = wait.result {
                return result;
            }
            match wait.deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return ContextCompactionOutcome::StillRunning;
                    }
                    let (next, timed) = self
                        .changed
                        .wait_timeout(wait, deadline.saturating_duration_since(now))
                        .unwrap_or_else(|poison| poison.into_inner());
                    wait = next;
                    if timed.timed_out() && wait.result.is_none() {
                        return ContextCompactionOutcome::StillRunning;
                    }
                }
                None => {
                    wait = self
                        .changed
                        .wait(wait)
                        .unwrap_or_else(|poison| poison.into_inner());
                }
            }
        }
    }
}
