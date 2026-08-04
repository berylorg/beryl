use beryl_home_store::{CursorReadLimits, ReadError};
use syndic_storage::{
    ACCEPTED_NEXT_PAGE_MAX_BYTES, AcceptedNextCandidate, AcceptedNextCandidateCursor,
    AcceptedNextSource, AcceptedNextSourceCursor, SyndicReadError,
};

use super::{
    AcceptedInputSchedulerContext, AcceptedInputWakeReason, ScanBudget, SchedulerFailure,
    SchedulerRuntime, failure,
};
use crate::cas_projection::{
    CasProjectionCoordinator, ProjectionCoordinatorError, ScheduledOrdinaryAdmissionResult,
    service_config::ProjectionWorkerPermitError,
};

mod authority;
mod worker;

pub(super) use authority::{
    LeaseValidationAuthority, expected_admission_drift, expected_coordinator_drift,
};
use worker::spawn_worker;
pub(super) use worker::{
    OrdinaryTurnSettlement, PendingTurnExecutionDisposition, current_selected_path,
    current_timestamp, execute_pending_turn, ordinary_error_cut_correlated,
    ordinary_error_verification_pending, settle_ordinary_outcome,
};

#[derive(Default)]
pub(super) struct NextScanState {
    revision: Option<beryl_model::DomainRevision>,
    source_cursor: Option<AcceptedNextSourceCursor>,
    current_source: Option<AcceptedNextSource>,
    next_source_cursor: Option<AcceptedNextSourceCursor>,
    candidate_cursor: Option<AcceptedNextCandidateCursor>,
    source_exhausted: bool,
}

enum SourceOutcome {
    Loaded,
    Exhausted,
    Stale,
    Yield,
}

enum CandidateOutcome {
    Selected(AcceptedNextCandidate),
    SourceFinished,
    Stale,
    Yield,
}

impl NextScanState {
    fn load_source(
        &mut self,
        context: &AcceptedInputSchedulerContext,
        budget: &mut ScanBudget,
    ) -> Result<SourceOutcome, SchedulerFailure> {
        if self.source_exhausted {
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
        if !budget.take_page() {
            return Ok(SourceOutcome::Yield);
        }
        context.signal.update_diagnostics(|diagnostics| {
            diagnostics.next_source_page_reads =
                diagnostics.next_source_page_reads.saturating_add(1);
        });
        let page = match context.storage.accepted_next_source_page(
            &context.home,
            revision,
            self.source_cursor,
            next_page_limits(),
        ) {
            Ok(page) => page,
            Err(
                SyndicReadError::StaleAcceptedNextSourceScan
                | SyndicReadError::ConcurrentChange { .. },
            ) => return Ok(SourceOutcome::Stale),
            Err(error) => {
                return Err(failure::from_syndic_read(&error, context.home_generation));
            }
        };
        let Some(source) = page.records().first().copied() else {
            self.source_exhausted = true;
            return Ok(SourceOutcome::Exhausted);
        };
        self.current_source = Some(source);
        self.next_source_cursor = page.next_cursor();
        self.candidate_cursor = None;
        Ok(SourceOutcome::Loaded)
    }

    fn next_candidate(
        &mut self,
        context: &AcceptedInputSchedulerContext,
        budget: &mut ScanBudget,
    ) -> Result<CandidateOutcome, SchedulerFailure> {
        let source = self
            .current_source
            .expect("accepted-next candidate scan has a current source");
        loop {
            if !budget.take_page() {
                return Ok(CandidateOutcome::Yield);
            }
            context.signal.update_diagnostics(|diagnostics| {
                diagnostics.next_candidate_page_reads =
                    diagnostics.next_candidate_page_reads.saturating_add(1);
            });
            let page = match context.storage.accepted_next_candidate_page(
                &context.home,
                source,
                self.candidate_cursor,
                next_page_limits(),
            ) {
                Ok(page) => page,
                Err(
                    SyndicReadError::StaleAcceptedNextCandidateSource
                    | SyndicReadError::ConcurrentChange { .. },
                ) => return Ok(CandidateOutcome::Stale),
                Err(error) => {
                    return Err(failure::from_syndic_read(&error, context.home_generation));
                }
            };
            self.candidate_cursor = page.next_cursor();
            if let Some(candidate) = page.into_candidate() {
                return Ok(CandidateOutcome::Selected(candidate));
            }
            if self.candidate_cursor.is_some() {
                continue;
            }
            self.finish_source();
            return Ok(CandidateOutcome::SourceFinished);
        }
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

fn next_page_limits() -> CursorReadLimits {
    CursorReadLimits::new(1, ACCEPTED_NEXT_PAGE_MAX_BYTES)
        .expect("accepted-next scheduler bounds are nonzero")
}

pub(super) fn run_pass(runtime: &mut SchedulerRuntime) -> Result<(), SchedulerFailure> {
    if runtime.context.signal.is_shutdown() || runtime.context.ordinary_cancellation.is_cancelled()
    {
        runtime.next_capacity_waiting = false;
        runtime.next_flight_waiting = false;
        runtime.next_active_worker_waiting = false;
        runtime.next_scan = None;
        runtime.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.next_retained_source_cursor = false;
            diagnostics.next_retained_candidate_cursor = false;
        });
        return Ok(());
    }
    let Some(command) = failure::authorize(&runtime.context)? else {
        return Ok(());
    };
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.next_pass_count = diagnostics.next_pass_count.saturating_add(1);
    });
    runtime.next_active_worker_waiting = false;
    let worker = match runtime
        .context
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
    {
        Ok(worker) => {
            runtime.next_capacity_waiting = false;
            worker
        }
        Err(ProjectionWorkerPermitError::CapacityFull { .. }) => {
            runtime.next_capacity_waiting = true;
            runtime.context.signal.update_diagnostics(|diagnostics| {
                diagnostics.next_capacity_waits = diagnostics.next_capacity_waits.saturating_add(1);
            });
            return Ok(());
        }
        Err(ProjectionWorkerPermitError::Poisoned) => return Err(SchedulerFailure::Fatal),
    };
    runtime.next_flight_waiting = false;
    let mut scan = runtime.next_scan.take().unwrap_or_default();
    let mut budget = ScanBudget::new(super::SCHEDULER_PASS_PAGE_BUDGET);
    loop {
        if !command.is_current() {
            clear_retained_scan(runtime);
            return Ok(());
        }
        match scan.load_source(&runtime.context, &mut budget)? {
            SourceOutcome::Loaded => {}
            SourceOutcome::Exhausted => {
                clear_retained_scan(runtime);
                return Ok(());
            }
            SourceOutcome::Stale => {
                runtime.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.next_stale_scans = diagnostics.next_stale_scans.saturating_add(1);
                    diagnostics.next_retained_source_cursor = false;
                    diagnostics.next_retained_candidate_cursor = false;
                });
                runtime.next_scan = Some(NextScanState::default());
                runtime
                    .context
                    .signal
                    .wake(AcceptedInputWakeReason::AcceptedNextReady);
                return Ok(());
            }
            SourceOutcome::Yield => {
                record_retained_scan(runtime, &scan);
                runtime.next_scan = Some(scan);
                runtime
                    .context
                    .signal
                    .wake(AcceptedInputWakeReason::AcceptedNextReady);
                return Ok(());
            }
        }
        let source = scan
            .current_source
            .expect("loaded accepted-next source remains current");
        if runtime.has_active_next_worker(source.thread_id()) {
            runtime.next_active_worker_waiting = true;
            record_retained_scan(runtime, &scan);
            runtime.next_scan = Some(scan);
            return Ok(());
        }
        let coordinator = CasProjectionCoordinator::for_healthy_home(&runtime.context.home)
            .map_err(|error| failure::from_coordinator(&error, runtime.context.home_generation))?;
        let flight = match coordinator
            .begin_scheduled_projection(source.thread_id(), &runtime.context.signal)
        {
            Ok(flight) => flight,
            Err(ProjectionCoordinatorError::ProjectionInFlight { .. }) => {
                runtime.next_flight_waiting = true;
                runtime.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.next_flight_waits = diagnostics.next_flight_waits.saturating_add(1);
                });
                record_retained_scan(runtime, &scan);
                runtime.next_scan = Some(scan);
                return Ok(());
            }
            Err(error) => {
                return Err(failure::from_coordinator(
                    &error,
                    runtime.context.home_generation,
                ));
            }
        };
        match scan.next_candidate(&runtime.context, &mut budget)? {
            CandidateOutcome::Selected(candidate) => {
                let execution = match runtime.context.storage.thread_execution(
                    &runtime.context.home,
                    candidate.thread_id(),
                    crate::cas_projection::input_replay::point_limit(),
                ) {
                    Ok(Some(execution)) => execution,
                    Ok(None) => {
                        record_execution_unavailable(runtime);
                        return Ok(());
                    }
                    Err(SyndicReadError::Read(ReadError::HealthGate(error)))
                        if error.state() == beryl_home_store::HomeHealthState::Verifying
                            && error.generation() == runtime.context.home_generation =>
                    {
                        return Err(SchedulerFailure::VerificationPending);
                    }
                    Err(SyndicReadError::Read(ReadError::HealthGate(error)))
                        if error.state() != beryl_home_store::HomeHealthState::Failed
                            || error.generation() != runtime.context.home_generation =>
                    {
                        record_execution_unavailable(runtime);
                        return Ok(());
                    }
                    Err(error) => {
                        return Err(failure::from_syndic_read(
                            &error,
                            runtime.context.home_generation,
                        ));
                    }
                };
                let admission = runtime.context.issue_scheduled_ordinary_execution(
                    candidate.thread_id(),
                    execution.execution().clone(),
                    worker,
                    flight,
                );
                let lease = match admission {
                    Ok(ScheduledOrdinaryAdmissionResult::Issued(lease)) => lease,
                    Ok(ScheduledOrdinaryAdmissionResult::Unavailable(_)) => {
                        record_execution_unavailable(runtime);
                        return Ok(());
                    }
                    Err(error)
                        if matches!(
                            failure::from_admission(&error, runtime.context.home_generation),
                            SchedulerFailure::VerificationPending
                                | SchedulerFailure::PersistentHomeFailure
                        ) =>
                    {
                        return Err(failure::from_admission(
                            &error,
                            runtime.context.home_generation,
                        ));
                    }
                    Err(error) if expected_admission_drift(&error) => {
                        record_execution_unavailable(runtime);
                        return Ok(());
                    }
                    Err(_) => return Err(SchedulerFailure::Fatal),
                };
                runtime.next_scan = Some(NextScanState::default());
                runtime.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.next_retained_source_cursor = false;
                    diagnostics.next_retained_candidate_cursor = false;
                });
                spawn_worker(runtime, candidate, lease)?;
                return Ok(());
            }
            CandidateOutcome::SourceFinished => {
                drop(flight);
            }
            CandidateOutcome::Stale => {
                runtime.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.next_stale_scans = diagnostics.next_stale_scans.saturating_add(1);
                    diagnostics.next_retained_source_cursor = false;
                    diagnostics.next_retained_candidate_cursor = false;
                });
                runtime.next_scan = Some(NextScanState::default());
                runtime
                    .context
                    .signal
                    .wake(AcceptedInputWakeReason::AcceptedNextReady);
                return Ok(());
            }
            CandidateOutcome::Yield => {
                drop(flight);
                record_retained_scan(runtime, &scan);
                runtime.next_scan = Some(scan);
                runtime
                    .context
                    .signal
                    .wake(AcceptedInputWakeReason::AcceptedNextReady);
                return Ok(());
            }
        }
    }
}

fn record_retained_scan(runtime: &SchedulerRuntime, scan: &NextScanState) {
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.next_retained_source_cursor =
            scan.source_cursor.is_some() || scan.current_source.is_some();
        diagnostics.next_retained_candidate_cursor = scan.candidate_cursor.is_some();
    });
}

fn record_execution_unavailable(runtime: &SchedulerRuntime) {
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.next_execution_unavailable =
            diagnostics.next_execution_unavailable.saturating_add(1);
    });
    clear_retained_scan(runtime);
}

fn clear_retained_scan(runtime: &SchedulerRuntime) {
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.next_retained_source_cursor = false;
        diagnostics.next_retained_candidate_cursor = false;
    });
}
