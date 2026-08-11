use std::sync::{Arc, Condvar, Mutex};

use beryl_home_store::CursorReadLimits;
use syndic_storage::{
    ACCEPTED_READY_PAGE_MAX_BYTES, AcceptedInputLifecycle, AcceptedReadyCandidateCursor,
    AcceptedReadySourceCursor, AcceptedReadySourceRecord, SyndicReadySteeringInput,
};

use super::{
    AcceptedInputSchedulerContext, SCHEDULER_PASS_PAGE_BUDGET, ScanBudget, SchedulerFailure,
    SchedulerRuntime, WorkerCompletion, WorkerDisposition, failure,
    signal::{AcceptedInputWakeReason, ActiveSteeringRetryState},
};
use crate::cas_projection::{
    active_steering::{ActiveSteeringTarget, deliver_prepared},
    connection::{
        ActiveSteeringAttemptAcquireError, ActiveSteeringAttemptPermit,
        ActiveSteeringTargetLookupError,
    },
    input_replay::point_limit,
    service_config::{ProjectionWorkerPermit, ProjectionWorkerPermitError},
};

#[derive(Default)]
pub(super) struct ScanState {
    revision: Option<beryl_model::DomainRevision>,
    source_cursor: Option<AcceptedReadySourceCursor>,
    current_source: Option<AcceptedReadySourceRecord>,
    next_source_cursor: Option<AcceptedReadySourceCursor>,
    candidate_cursor: Option<AcceptedReadyCandidateCursor>,
    source_exhausted: bool,
}

enum ScanOutcome {
    Selected(SyndicReadySteeringInput),
    Exhausted,
    Stale,
    Yield,
}

impl ScanState {
    fn next(
        &mut self,
        context: &AcceptedInputSchedulerContext,
        retry_eligible: bool,
        budget: &mut ScanBudget,
    ) -> Result<ScanOutcome, SchedulerFailure> {
        loop {
            if self.source_exhausted {
                return Ok(ScanOutcome::Exhausted);
            }
            if self.current_source.is_none() {
                match self.load_source(context, budget)? {
                    LoadSourceOutcome::Loaded => {}
                    LoadSourceOutcome::Exhausted => {
                        return Ok(ScanOutcome::Exhausted);
                    }
                    LoadSourceOutcome::Stale => return Ok(ScanOutcome::Stale),
                    LoadSourceOutcome::Yield => return Ok(ScanOutcome::Yield),
                }
            }
            let source = self
                .current_source
                .expect("a loaded ready source remains current");
            if !budget.take_page() {
                return Ok(ScanOutcome::Yield);
            }
            context.signal.update_diagnostics(|diagnostics| {
                diagnostics.steering_candidate_page_reads =
                    diagnostics.steering_candidate_page_reads.saturating_add(1);
                diagnostics.steering_retained_candidate_cursor = self.candidate_cursor.is_some();
            });
            let page = match context.storage.accepted_ready_candidate_page(
                &context.home,
                source,
                self.candidate_cursor,
                ready_page_limits(),
            ) {
                Ok(page) => page,
                Err(
                    syndic_storage::SyndicReadError::StaleAcceptedReadyCandidateSource
                    | syndic_storage::SyndicReadError::ConcurrentChange { .. },
                ) => return Ok(ScanOutcome::Stale),
                Err(error) => {
                    return Err(failure::from_syndic_read(&error, context.home_generation));
                }
            };
            let candidate = page.records().first().copied();
            self.candidate_cursor = page.next_cursor();
            if candidate.is_none() {
                if self.candidate_cursor.is_some() {
                    continue;
                }
                self.finish_source();
                continue;
            }
            let candidate = candidate.expect("checked candidate presence");
            if candidate.lifecycle() == AcceptedInputLifecycle::Retryable && !retry_eligible {
                if self.candidate_cursor.is_none() {
                    self.finish_source();
                }
                continue;
            }
            if candidate.lifecycle() != AcceptedInputLifecycle::Admitted
                && candidate.lifecycle() != AcceptedInputLifecycle::Retryable
            {
                return Err(SchedulerFailure::Fatal);
            }
            context.signal.update_diagnostics(|diagnostics| {
                diagnostics.point_reads = diagnostics.point_reads.saturating_add(1);
            });
            let ready = match context.storage.ready_steering_input(
                &context.home,
                candidate.input_id(),
                point_limit(),
            ) {
                Ok(Some(ready)) => ready,
                Ok(None) | Err(syndic_storage::SyndicReadError::ConcurrentChange { .. }) => {
                    return Ok(ScanOutcome::Stale);
                }
                Err(error) => {
                    return Err(failure::from_syndic_read(&error, context.home_generation));
                }
            };
            if ready.input().id() != candidate.input_id()
                || ready.input().thread_id() != source.thread_id()
                || ready.input().ordinal() != candidate.ordinal()
                || ready.lifecycle() != candidate.lifecycle()
                || ready.accepted_input_revision() != candidate.leaf_revision()
                || ready.gate_revision() != source.gate_revision()
                || ready.route().generation() != source.generation()
                || ready.route().revision() != source.generation_revision()
            {
                return Ok(ScanOutcome::Stale);
            }
            return Ok(ScanOutcome::Selected(ready));
        }
    }

    fn load_source(
        &mut self,
        context: &AcceptedInputSchedulerContext,
        budget: &mut ScanBudget,
    ) -> Result<LoadSourceOutcome, SchedulerFailure> {
        let revision = match self.revision {
            Some(revision) => revision,
            None => {
                let revision = context
                    .storage
                    .revision(&context.home)
                    .map_err(|error| failure::from_read(&error, context.home_generation))?;
                self.revision = Some(revision);
                revision
            }
        };
        if !budget.take_page() {
            return Ok(LoadSourceOutcome::Yield);
        }
        context.signal.update_diagnostics(|diagnostics| {
            diagnostics.steering_source_page_reads =
                diagnostics.steering_source_page_reads.saturating_add(1);
            diagnostics.steering_retained_source_cursor = self.source_cursor.is_some();
        });
        let page = match context.storage.accepted_ready_source_page(
            &context.home,
            revision,
            self.source_cursor,
            ready_page_limits(),
        ) {
            Ok(page) => page,
            Err(
                syndic_storage::SyndicReadError::StaleAcceptedReadySourceScan
                | syndic_storage::SyndicReadError::ConcurrentChange { .. },
            ) => return Ok(LoadSourceOutcome::Stale),
            Err(error) => {
                return Err(failure::from_syndic_read(&error, context.home_generation));
            }
        };
        let Some(source) = page.records().first().copied() else {
            self.source_exhausted = true;
            return Ok(LoadSourceOutcome::Exhausted);
        };
        self.current_source = Some(source);
        self.next_source_cursor = page.next_cursor();
        self.candidate_cursor = None;
        Ok(LoadSourceOutcome::Loaded)
    }

    fn finish_source(&mut self) {
        self.current_source = None;
        self.candidate_cursor = None;
        match self.next_source_cursor.take() {
            Some(cursor) => self.source_cursor = Some(cursor),
            None => self.source_exhausted = true,
        }
    }
}

enum LoadSourceOutcome {
    Loaded,
    Exhausted,
    Stale,
    Yield,
}

fn ready_page_limits() -> CursorReadLimits {
    CursorReadLimits::new(1, ACCEPTED_READY_PAGE_MAX_BYTES)
        .expect("accepted-ready scheduler bounds are nonzero")
}

#[derive(Clone)]
pub(super) struct LaunchGate {
    inner: Arc<(Mutex<bool>, Condvar)>,
}

impl LaunchGate {
    fn closed() -> Self {
        Self {
            inner: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    fn wait(&self) {
        let (open, changed) = &*self.inner;
        let mut open = open.lock().unwrap_or_else(|poison| poison.into_inner());
        while !*open {
            open = changed
                .wait(open)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    pub(super) fn open(&self) {
        let (open, changed) = &*self.inner;
        *open.lock().unwrap_or_else(|poison| poison.into_inner()) = true;
        changed.notify_all();
    }
}

impl SchedulerRuntime {
    pub(super) fn run_pass(&mut self, retry_eligible: bool) -> Result<(), SchedulerFailure> {
        let Some(command) = failure::authorize(&self.context)? else {
            return Ok(());
        };
        let gate = LaunchGate::closed();
        debug_assert!(self.pending_launch_gate.is_none());
        self.pending_launch_gate = Some(gate.clone());
        let mut scan = self.scan.take().unwrap_or_default();
        let mut budget = ScanBudget::new(SCHEDULER_PASS_PAGE_BUDGET);
        let result = self.fill_batch(&gate, retry_eligible, &mut scan, &mut budget, &command);
        self.release_pending_launch_gate();
        match result {
            Ok(PassStop::Exhausted) => {
                self.scan = None;
                self.retry_pass_active = false;
                self.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.steering_retained_source_cursor = false;
                    diagnostics.steering_retained_candidate_cursor = false;
                    diagnostics.retry_state = if self.parked_retry {
                        ActiveSteeringRetryState::Parked
                    } else {
                        ActiveSteeringRetryState::Ineligible
                    };
                });
                Ok(())
            }
            Ok(PassStop::Capacity) => {
                self.record_retained_scan(&scan);
                self.scan = Some(scan);
                Ok(())
            }
            Ok(PassStop::Attempt) => {
                self.scan = Some(ScanState::default());
                self.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.steering_retained_source_cursor = false;
                    diagnostics.steering_retained_candidate_cursor = false;
                });
                Ok(())
            }
            Ok(PassStop::Stale) => {
                self.scan = Some(ScanState::default());
                self.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.steering_retained_source_cursor = false;
                    diagnostics.steering_retained_candidate_cursor = false;
                });
                self.context
                    .signal
                    .wake(AcceptedInputWakeReason::AcceptedReady);
                Ok(())
            }
            Ok(PassStop::Yield) => {
                self.record_retained_scan(&scan);
                self.scan = Some(scan);
                self.context
                    .signal
                    .wake(AcceptedInputWakeReason::AcceptedReady);
                Ok(())
            }
            Err(failure) => Err(failure),
        }
    }

    fn fill_batch(
        &mut self,
        gate: &LaunchGate,
        retry_eligible: bool,
        scan: &mut ScanState,
        budget: &mut ScanBudget,
        command: &crate::cas_projection::LiveCommandPermit,
    ) -> Result<PassStop, SchedulerFailure> {
        loop {
            if !command.is_current() {
                return Ok(PassStop::Exhausted);
            }
            if self.context.signal.is_shutdown() || self.context.cancellation.is_cancelled() {
                return Ok(PassStop::Exhausted);
            }
            let mut permit = match self
                .context
                .workers
                .try_acquire_steering_critical_quiet_or_arm()
            {
                Ok(permit) => permit,
                Err(ProjectionWorkerPermitError::CapacityFull { .. }) => {
                    self.context.signal.update_diagnostics(|diagnostics| {
                        diagnostics.steering_capacity_waits =
                            diagnostics.steering_capacity_waits.saturating_add(1);
                    });
                    return Ok(PassStop::Capacity);
                }
                Err(ProjectionWorkerPermitError::Poisoned) => {
                    return Err(SchedulerFailure::Fatal);
                }
            };
            let selected = match scan.next(&self.context, retry_eligible, budget) {
                Ok(ScanOutcome::Selected(ready)) => ready,
                Ok(ScanOutcome::Exhausted) => {
                    return Ok(PassStop::Exhausted);
                }
                Ok(ScanOutcome::Stale) => {
                    self.context.signal.update_diagnostics(|diagnostics| {
                        diagnostics.steering_stale_scans =
                            diagnostics.steering_stale_scans.saturating_add(1);
                    });
                    return Ok(PassStop::Stale);
                }
                Ok(ScanOutcome::Yield) => return Ok(PassStop::Yield),
                Err(failure) => return Err(failure),
            };
            let Some(target) = self.lookup_target(&selected)? else {
                self.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.target_misses = diagnostics.target_misses.saturating_add(1);
                });
                scan.finish_source();
                continue;
            };
            let attempt = match target.acquire_attempt(&selected, true) {
                Ok(attempt) => attempt,
                Err(ActiveSteeringAttemptAcquireError::Busy) => {
                    self.context.signal.update_diagnostics(|diagnostics| {
                        diagnostics.attempt_waits = diagnostics.attempt_waits.saturating_add(1);
                    });
                    return Ok(PassStop::Attempt);
                }
                Err(
                    ActiveSteeringAttemptAcquireError::TargetClosed(_)
                    | ActiveSteeringAttemptAcquireError::TargetMismatch,
                ) => {
                    scan.finish_source();
                    continue;
                }
                Err(
                    ActiveSteeringAttemptAcquireError::GenerationExhausted
                    | ActiveSteeringAttemptAcquireError::Router,
                ) => return Err(SchedulerFailure::Fatal),
            };
            permit.commit_steering_worker();
            self.spawn_delivery(gate.clone(), permit, target, selected, attempt)?;
        }
    }

    fn record_retained_scan(&self, scan: &ScanState) {
        self.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.steering_retained_source_cursor =
                scan.source_cursor.is_some() || scan.current_source.is_some();
            diagnostics.steering_retained_candidate_cursor = scan.candidate_cursor.is_some();
        });
    }

    fn lookup_target(
        &self,
        ready: &SyndicReadySteeringInput,
    ) -> Result<Option<ActiveSteeringTarget>, SchedulerFailure> {
        let connections = {
            let mut registry = self
                .context
                .connections
                .lock()
                .map_err(|_| SchedulerFailure::Fatal)?;
            registry.retain(|connection| !connection.is_detached());
            registry.clone()
        };
        let mut found = None;
        for connection in connections {
            match ActiveSteeringTarget::lookup(connection, ready) {
                Ok(target) if found.is_none() => found = Some(target),
                Ok(_) => return Err(SchedulerFailure::Fatal),
                Err(ActiveSteeringTargetLookupError::MissingOrStale) => {}
                Err(ActiveSteeringTargetLookupError::Router) => {
                    return Err(SchedulerFailure::Fatal);
                }
            }
        }
        Ok(found)
    }

    fn spawn_delivery(
        &mut self,
        gate: LaunchGate,
        permit: ProjectionWorkerPermit,
        target: ActiveSteeringTarget,
        ready: SyndicReadySteeringInput,
        attempt: ActiveSteeringAttemptPermit,
    ) -> Result<(), SchedulerFailure> {
        let home = Arc::clone(&self.context.home);
        let home_id = self.context.home_id;
        let home_generation = self.context.home_generation;
        let storage = self.context.storage;
        let cancellation = self.context.cancellation.snapshot();
        let completions = self.completions.clone();
        let handle = std::thread::Builder::new()
            .name("beryl-active-steering-delivery".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    gate.wait();
                    if !attempt.command_is_current() {
                        return WorkerDisposition::Parked;
                    }
                    let delivery = deliver_prepared(
                        &home,
                        home_id,
                        home_generation,
                        storage,
                        &permit,
                        target,
                        ready,
                        &cancellation,
                        attempt,
                    );
                    failure::classify_active_steering_delivery(&delivery, home_generation)
                }));
                let disposition = result.unwrap_or(WorkerDisposition::Fatal);
                completions.publish(WorkerCompletion {
                    thread_id: std::thread::current().id(),
                });
                disposition
            })
            .map_err(|_| SchedulerFailure::Fatal)?;
        self.register_steering_worker(handle);
        Ok(())
    }
}

enum PassStop {
    Exhausted,
    Capacity,
    Attempt,
    Stale,
    Yield,
}
