use beryl_home_store::{CursorReadLimits, ReadError};
use syndic_storage::{
    DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES, RecoveredPendingCursor, RecoveredPendingSource,
    SyndicReadError,
};

use super::{
    AcceptedInputSchedulerContext, AcceptedInputWakeReason, ScanBudget, SchedulerFailure,
    SchedulerRuntime, WorkerCompletion, WorkerDisposition, failure,
};
use crate::cas_projection::{
    CasProjectionCoordinator, ProjectionCoordinatorError, ScheduledOrdinaryAdmissionResult,
    service_config::ProjectionWorkerPermitError,
};

#[derive(Default)]
pub(super) struct RecoveredPendingScanState {
    revision: Option<beryl_model::DomainRevision>,
    cursor: Option<RecoveredPendingCursor>,
    current_source: Option<RecoveredPendingSource>,
    next_cursor: Option<RecoveredPendingCursor>,
    exhausted: bool,
}

enum SourceOutcome {
    Loaded,
    Exhausted,
    Stale,
    Yield,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PassOutcome {
    Settled,
    NextReady,
}

impl PassOutcome {
    pub(super) const fn opens_next_pass(self) -> bool {
        matches!(self, Self::NextReady)
    }
}

impl RecoveredPendingScanState {
    fn load_source(
        &mut self,
        context: &AcceptedInputSchedulerContext,
        budget: &mut ScanBudget,
    ) -> Result<SourceOutcome, SchedulerFailure> {
        if self.exhausted {
            return Ok(SourceOutcome::Exhausted);
        }
        if self.current_source.is_some() {
            return Ok(SourceOutcome::Loaded);
        }
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
        loop {
            if !budget.take_page() {
                return Ok(SourceOutcome::Yield);
            }
            context.signal.update_diagnostics(|diagnostics| {
                diagnostics.recovered_pending_source_page_reads = diagnostics
                    .recovered_pending_source_page_reads
                    .saturating_add(1);
            });
            let page = match context.storage.recovered_pending_page(
                &context.home,
                revision,
                self.cursor,
                recovered_pending_page_limits(),
                crate::cas_projection::input_replay::point_limit(),
            ) {
                Ok(page) => page,
                Err(
                    SyndicReadError::StaleRecoveredPendingScan
                    | SyndicReadError::ConcurrentChange { .. },
                ) => return Ok(SourceOutcome::Stale),
                Err(error) => {
                    return Err(failure::from_syndic_read(&error, context.home_generation));
                }
            };
            let next_cursor = page.next_cursor();
            if let Some(source) = page.records().first().cloned() {
                self.current_source = Some(source);
                self.next_cursor = next_cursor;
                return Ok(SourceOutcome::Loaded);
            }
            match next_cursor {
                Some(cursor) => self.cursor = Some(cursor),
                None => {
                    self.cursor = None;
                    self.exhausted = true;
                    return Ok(SourceOutcome::Exhausted);
                }
            }
        }
    }

    fn finish_source(&mut self) {
        self.current_source = None;
        match self.next_cursor.take() {
            Some(cursor) => self.cursor = Some(cursor),
            None => {
                self.cursor = None;
                self.exhausted = true;
            }
        }
    }

    fn rebase_after_stale(
        &mut self,
        context: &AcceptedInputSchedulerContext,
    ) -> Result<(), SchedulerFailure> {
        debug_assert!(self.current_source.is_none());
        debug_assert!(self.next_cursor.is_none());
        match self.cursor {
            Some(cursor) => {
                let cursor = context
                    .storage
                    .rebase_recovered_pending_cursor(&context.home, cursor)
                    .map_err(|error| failure::from_syndic_read(&error, context.home_generation))?;
                self.revision = Some(cursor.source_revision());
                self.cursor = Some(cursor);
            }
            None => {
                self.revision = Some(
                    context
                        .storage
                        .revision(&context.home)
                        .map_err(|error| failure::from_read(&error, context.home_generation))?,
                );
            }
        }
        Ok(())
    }
}

fn recovered_pending_page_limits() -> CursorReadLimits {
    CursorReadLimits::new(1, DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES)
        .expect("recovered-pending scheduler bounds are nonzero")
}

pub(super) fn run_pass(runtime: &mut SchedulerRuntime) -> Result<PassOutcome, SchedulerFailure> {
    if runtime.context.signal.is_shutdown() || runtime.context.ordinary_cancellation.is_cancelled()
    {
        runtime.recovered_pending_capacity_waiting = false;
        runtime.recovered_pending_flight_waiting = false;
        runtime.recovered_pending_pass_active = false;
        runtime.recovered_pending_scan = None;
        clear_retained_scan(runtime);
        return Ok(PassOutcome::Settled);
    }
    let Some(command) = failure::authorize(&runtime.context)? else {
        return Ok(PassOutcome::Settled);
    };
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.recovered_pending_pass_count =
            diagnostics.recovered_pending_pass_count.saturating_add(1);
    });
    let worker = match runtime
        .context
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
    {
        Ok(worker) => {
            runtime.recovered_pending_capacity_waiting = false;
            worker
        }
        Err(ProjectionWorkerPermitError::CapacityFull { .. }) => {
            runtime.recovered_pending_capacity_waiting = true;
            runtime.context.signal.update_diagnostics(|diagnostics| {
                diagnostics.recovered_pending_capacity_waits = diagnostics
                    .recovered_pending_capacity_waits
                    .saturating_add(1);
            });
            return Ok(PassOutcome::Settled);
        }
        Err(ProjectionWorkerPermitError::Poisoned) => return Err(SchedulerFailure::Fatal),
    };
    runtime.recovered_pending_flight_waiting = false;
    let mut scan = runtime.recovered_pending_scan.take().unwrap_or_default();
    let mut budget = ScanBudget::new(super::SCHEDULER_PASS_PAGE_BUDGET);
    if !command.is_current() {
        runtime.recovered_pending_pass_active = false;
        clear_retained_scan(runtime);
        return Ok(PassOutcome::Settled);
    }
    match scan.load_source(&runtime.context, &mut budget)? {
        SourceOutcome::Loaded => {}
        SourceOutcome::Exhausted => {
            runtime.recovered_pending_pass_active = false;
            clear_retained_scan(runtime);
            return Ok(if runtime.next_capacity_waiting {
                PassOutcome::NextReady
            } else {
                PassOutcome::Settled
            });
        }
        SourceOutcome::Stale => {
            runtime.context.signal.update_diagnostics(|diagnostics| {
                diagnostics.recovered_pending_stale_scans =
                    diagnostics.recovered_pending_stale_scans.saturating_add(1);
            });
            scan.rebase_after_stale(&runtime.context)?;
            record_retained_scan(runtime, &scan);
            runtime.recovered_pending_scan = Some(scan);
            runtime
                .context
                .signal
                .wake(AcceptedInputWakeReason::RecoveredPendingContinue);
            return Ok(PassOutcome::Settled);
        }
        SourceOutcome::Yield => {
            record_retained_scan(runtime, &scan);
            runtime.recovered_pending_scan = Some(scan);
            runtime
                .context
                .signal
                .wake(AcceptedInputWakeReason::RecoveredPendingContinue);
            return Ok(PassOutcome::Settled);
        }
    }
    let source = scan
        .current_source
        .as_ref()
        .expect("loaded recovered-pending source remains current")
        .clone();
    if runtime.has_active_next_worker(source.thread_id()) {
        scan.finish_source();
        retain_and_continue(runtime, scan);
        return Ok(PassOutcome::Settled);
    }
    let coordinator = CasProjectionCoordinator::for_healthy_home(&runtime.context.home)
        .map_err(|error| failure::from_coordinator(&error, runtime.context.home_generation))?;
    let flight =
        match coordinator.begin_scheduled_projection(source.thread_id(), &runtime.context.signal) {
            Ok(flight) => flight,
            Err(ProjectionCoordinatorError::ProjectionInFlight { .. }) => {
                runtime.recovered_pending_flight_waiting = true;
                runtime.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.recovered_pending_flight_waits =
                        diagnostics.recovered_pending_flight_waits.saturating_add(1);
                });
                record_retained_scan(runtime, &scan);
                runtime.recovered_pending_scan = Some(scan);
                return Ok(PassOutcome::Settled);
            }
            Err(error) => {
                return Err(failure::from_coordinator(
                    &error,
                    runtime.context.home_generation,
                ));
            }
        };
    let execution = match runtime.context.storage.thread_execution(
        &runtime.context.home,
        source.thread_id(),
        crate::cas_projection::input_replay::point_limit(),
    ) {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            scan.finish_source();
            record_execution_unavailable(runtime);
            retain_and_continue(runtime, scan);
            return Ok(PassOutcome::Settled);
        }
        Err(SyndicReadError::Read(ReadError::HealthGate(error)))
            if error.state() != beryl_home_store::HomeHealthState::Healthy
                && error.generation() == runtime.context.home_generation =>
        {
            return Err(SchedulerFailure::PersistentHomeFailure);
        }
        Err(SyndicReadError::Read(ReadError::HealthGate(error)))
            if error.state() != beryl_home_store::HomeHealthState::Failed
                || error.generation() != runtime.context.home_generation =>
        {
            scan.finish_source();
            record_execution_unavailable(runtime);
            retain_and_continue(runtime, scan);
            return Ok(PassOutcome::Settled);
        }
        Err(error) => {
            return Err(failure::from_syndic_read(
                &error,
                runtime.context.home_generation,
            ));
        }
    };
    let admission = runtime.context.issue_scheduled_ordinary_execution(
        source.thread_id(),
        execution.execution().clone(),
        worker,
        flight,
    );
    let lease = match admission {
        Ok(ScheduledOrdinaryAdmissionResult::Issued(lease)) => lease,
        Ok(ScheduledOrdinaryAdmissionResult::Unavailable(_)) => {
            scan.finish_source();
            record_execution_unavailable(runtime);
            retain_and_continue(runtime, scan);
            return Ok(PassOutcome::Settled);
        }
        Err(error)
            if matches!(
                failure::from_admission(&error, runtime.context.home_generation),
                SchedulerFailure::PersistentHomeFailure
            ) =>
        {
            return Err(failure::from_admission(
                &error,
                runtime.context.home_generation,
            ));
        }
        Err(error) if super::next_turn::expected_admission_drift(&error) => {
            scan.finish_source();
            record_execution_unavailable(runtime);
            retain_and_continue(runtime, scan);
            return Ok(PassOutcome::Settled);
        }
        Err(_) => return Err(SchedulerFailure::Fatal),
    };
    scan.finish_source();
    record_retained_scan(runtime, &scan);
    runtime.recovered_pending_scan = Some(scan);
    spawn_worker(runtime, source, lease).map(|()| PassOutcome::Settled)
}

fn spawn_worker(
    runtime: &mut SchedulerRuntime,
    source: RecoveredPendingSource,
    mut lease: crate::cas_projection::ScheduledOrdinaryExecutionLease,
) -> Result<(), SchedulerFailure> {
    let syndic_thread_id = source.thread_id();
    let Some(command) = failure::authorize(&runtime.context)? else {
        return Ok(());
    };
    let validator = runtime.context.lease_validator(command);
    let storage = runtime.context.storage.clone();
    let cancellation = runtime.context.ordinary_cancellation.clone();
    let signal = runtime.context.signal.clone();
    let completions = runtime.completions.clone();
    let handle = std::thread::Builder::new()
        .name("beryl-recovered-pending-execution".to_owned())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                execute_source(&validator, &storage, &cancellation, &source, &mut lease)
            }));
            let disposition = result.unwrap_or(WorkerDisposition::Fatal);
            completions.publish(WorkerCompletion {
                thread_id: std::thread::current().id(),
            });
            drop(lease);
            signal.wake(AcceptedInputWakeReason::WorkerCompleted);
            disposition
        })
        .map_err(|_| SchedulerFailure::Fatal)?;
    runtime.register_next_worker(handle, syndic_thread_id);
    Ok(())
}

fn execute_source(
    validator: &super::next_turn::LeaseValidationAuthority,
    storage: &syndic_storage::SyndicStorage,
    cancellation: &crate::cas_projection::ProjectionCancellationToken,
    source: &RecoveredPendingSource,
    lease: &mut crate::cas_projection::ScheduledOrdinaryExecutionLease,
) -> WorkerDisposition {
    if cancellation.is_cancelled() {
        return WorkerDisposition::RecoveredPendingContinue;
    }
    if let Err(error) = validator.validate(lease) {
        return if failure::is_current_health_loss_admission(&error, validator.home_generation()) {
            WorkerDisposition::PersistentHomeFailure
        } else if failure::is_cut_correlated_admission(&error, validator.home_generation()) {
            WorkerDisposition::PersistentHomeFailure
        } else if super::next_turn::expected_admission_drift(&error) {
            WorkerDisposition::RecoveredPendingContinue
        } else {
            WorkerDisposition::Fatal
        };
    }
    let selected_path = match super::next_turn::current_selected_path(
        &validator.home,
        storage,
        source.thread_id(),
        validator.home_generation(),
    ) {
        Ok(path) => path,
        Err(SchedulerFailure::PersistentHomeFailure) => {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Err(SchedulerFailure::Fatal) => return WorkerDisposition::Fatal,
    };
    if selected_path.tail() != Some(source.turn_id()) {
        return WorkerDisposition::RecoveredPendingContinue;
    }
    let observed_at = match super::next_turn::current_timestamp(source.minimum_timestamp()) {
        Ok(timestamp) => timestamp,
        Err(()) => return WorkerDisposition::Fatal,
    };
    match super::next_turn::execute_pending_turn(
        validator,
        storage,
        cancellation,
        observed_at,
        selected_path,
        lease,
    ) {
        super::next_turn::PendingTurnExecutionDisposition::Settled
        | super::next_turn::PendingTurnExecutionDisposition::ExpectedInterruption => {
            WorkerDisposition::RecoveredPendingContinue
        }
        super::next_turn::PendingTurnExecutionDisposition::PersistentHomeFailure => {
            WorkerDisposition::PersistentHomeFailure
        }
        super::next_turn::PendingTurnExecutionDisposition::ProjectionRefused => {
            WorkerDisposition::Fatal
        }
    }
}

fn retain_and_continue(runtime: &mut SchedulerRuntime, scan: RecoveredPendingScanState) {
    record_retained_scan(runtime, &scan);
    runtime.recovered_pending_scan = Some(scan);
    runtime
        .context
        .signal
        .wake(AcceptedInputWakeReason::RecoveredPendingContinue);
}

fn record_execution_unavailable(runtime: &SchedulerRuntime) {
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.recovered_pending_execution_unavailable = diagnostics
            .recovered_pending_execution_unavailable
            .saturating_add(1);
    });
}

fn record_retained_scan(runtime: &SchedulerRuntime, scan: &RecoveredPendingScanState) {
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.recovered_pending_retained_source_cursor =
            scan.cursor.is_some() || scan.current_source.is_some();
    });
}

fn clear_retained_scan(runtime: &SchedulerRuntime) {
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.recovered_pending_retained_source_cursor = false;
    });
}
