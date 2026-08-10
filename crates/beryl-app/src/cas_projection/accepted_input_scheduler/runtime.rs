use super::{
    AcceptedInputSchedulerContext, AcceptedInputSchedulerExit, ActiveSteeringRetryState, ScanState,
    SchedulerFailure, WorkerCompletions, WorkerRecord, failure, next_turn, recovered_pending,
    recovered_projection, steering,
};
#[cfg(all(test, feature = "test-faults"))]
use super::{AcceptedInputWakeReason, WorkerCompletion, WorkerDisposition};
use crate::cas_projection::PersistentFailureNotificationStatus;

pub(super) struct SchedulerRuntime {
    pub(super) context: AcceptedInputSchedulerContext,
    pub(super) workers: Vec<WorkerRecord>,
    pub(super) pending_launch_gate: Option<steering::LaunchGate>,
    pub(super) completions: WorkerCompletions,
    pub(super) scan: Option<ScanState>,
    pub(super) recovered_pending_scan: Option<recovered_pending::RecoveredPendingScanState>,
    pub(super) recovered_projection_pass: u64,
    pub(super) recovered_projection_scan: u64,
    pub(super) recovered_projection_flight_waiting: bool,
    pub(super) recovered_projection_worker_waiting: bool,
    pub(super) next_scan: Option<next_turn::NextScanState>,
    pub(super) retry_pass_active: bool,
    pub(super) retry_reopen_pending: bool,
    pub(super) parked_retry: bool,
    pub(super) recovered_pending_pass_active: bool,
    pub(super) recovered_pending_capacity_waiting: bool,
    pub(super) recovered_pending_flight_waiting: bool,
    pub(super) next_capacity_waiting: bool,
    pub(super) next_flight_waiting: bool,
    pub(super) next_active_worker_waiting: bool,
    pub(super) verification_resumed_workers: Vec<std::thread::ThreadId>,
    pub(super) failure: Option<SchedulerFailure>,
}

impl SchedulerRuntime {
    pub(super) fn new(context: AcceptedInputSchedulerContext) -> Self {
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

    pub(super) fn run(&mut self) -> AcceptedInputSchedulerExit {
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
    pub(super) fn park_for_verification(&mut self) -> bool {
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

    pub(super) fn fail_closed(&mut self, observed: SchedulerFailure) {
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

    pub(super) fn emergency_quiesce(&mut self) {
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

    pub(super) fn release_pending_launch_gate(&mut self) {
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
