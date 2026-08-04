mod lane;

use std::sync::Arc;

use beryl_model::SyndicThreadId;

use super::{
    AcceptedInputSchedulerSignal, AcceptedInputWakeReason, SchedulerFailure, SchedulerRuntime,
};
use crate::cas_projection::{
    CasProjectionCoordinator, LoadedCasProjection, OrdinaryTurnExecutionFailure,
    ProjectionCoordinatorError, ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionLease,
    connection::ProjectionConnection,
};

pub(in crate::cas_projection) use lane::{
    RecoveredProjectionLane, RecoveredProjectionLaneParts, RecoveredProjectionLaneStageError,
    RecoveredProjectionLaneStageReason,
};
use lane::{RecoveredProjectionLaneAttempt, RecoveredProjectionLaneEntry};
pub(super) use lane::{dispose_retained, retain_for_persistent_failure};

enum RecoveredProjectionWorkerCommand {
    Execute(RecoveredProjectionExecution),
    Finish(super::WorkerDisposition),
}

struct RecoveredProjectionExecution {
    validator: super::next_turn::LeaseValidationAuthority,
    storage: syndic_storage::SyndicStorage,
    cancellation: crate::cas_projection::ProjectionCancellationToken,
    lane: RecoveredProjectionLane,
    projection: LoadedCasProjection,
    lease: ScheduledOrdinaryExecutionLease,
    attempt: RecoveredProjectionLaneAttempt,
}

pub(super) fn run_pass(runtime: &mut SchedulerRuntime) -> Result<(), SchedulerFailure> {
    if runtime.context.signal.is_shutdown() || runtime.context.ordinary_cancellation.is_cancelled()
    {
        return Ok(());
    }
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.recovered_projection_pass_count = diagnostics
            .recovered_projection_pass_count
            .saturating_add(1);
    });
    runtime.recovered_projection_flight_waiting = false;
    runtime.recovered_projection_worker_waiting = false;
    runtime.recovered_projection_scan = runtime.recovered_projection_scan.saturating_add(1).max(1);
    let lane = runtime.context.recovered_projection_lane.clone();
    let pass = runtime.recovered_projection_pass;
    let scan = runtime.recovered_projection_scan;
    let queued = lane.queued_len()?;
    for _ in 0..queued {
        if !run_one(runtime, &lane, pass, scan)? {
            break;
        }
    }
    Ok(())
}

fn run_one(
    runtime: &mut SchedulerRuntime,
    lane: &RecoveredProjectionLane,
    pass: u64,
    scan: u64,
) -> Result<bool, SchedulerFailure> {
    if runtime.context.signal.is_shutdown() || runtime.context.ordinary_cancellation.is_cancelled()
    {
        return Ok(false);
    }
    let Some(command) = super::failure::authorize(&runtime.context)? else {
        return Ok(false);
    };
    let (entry, worker_waiting) = lane.pop_eligible(pass, scan, |thread_id| {
        !runtime.has_active_next_worker(thread_id)
    })?;
    runtime.recovered_projection_worker_waiting |= worker_waiting;
    let Some(mut entry) = entry else {
        return Ok(false);
    };
    let thread_id = entry.thread_id();
    match entry.owner.authenticate_live_exact() {
        Ok(true) => {}
        Ok(false) => {
            entry.mark_attempted(pass);
            record_unavailable(runtime);
            requeue(runtime, entry)?;
            return Ok(true);
        }
        Err(error) => {
            lane.requeue(entry)?;
            return Err(classify_coordinator_error(
                &error,
                runtime.context.home_generation,
            ));
        }
    }
    let coordinator = match CasProjectionCoordinator::for_healthy_home(&runtime.context.home) {
        Ok(coordinator) => coordinator,
        Err(error)
            if super::failure::is_verification_pending_coordinator(
                &error,
                runtime.context.home_generation,
            ) =>
        {
            lane.requeue(entry)?;
            return Err(SchedulerFailure::VerificationPending);
        }
        Err(error)
            if super::failure::is_cut_correlated_coordinator(
                &error,
                runtime.context.home_generation,
            ) =>
        {
            lane.requeue(entry)?;
            return Err(SchedulerFailure::PersistentHomeFailure);
        }
        Err(error) => {
            if super::next_turn::expected_coordinator_drift(&error) {
                entry.mark_attempted(pass);
                record_unavailable(runtime);
                requeue(runtime, entry)?;
                return Ok(true);
            }
            lane.requeue(entry)?;
            return Err(classify_coordinator_error(
                &error,
                runtime.context.home_generation,
            ));
        }
    };
    let flight = match coordinator.begin_scheduled_projection(thread_id, &runtime.context.signal) {
        Ok(flight) => flight,
        Err(ProjectionCoordinatorError::ProjectionInFlight { .. }) => {
            runtime.recovered_projection_flight_waiting = true;
            runtime.context.signal.update_diagnostics(|diagnostics| {
                diagnostics.recovered_projection_flight_waits = diagnostics
                    .recovered_projection_flight_waits
                    .saturating_add(1);
            });
            lane.requeue(entry)?;
            return Ok(true);
        }
        Err(error) => {
            lane.requeue(entry)?;
            return Err(classify_coordinator_error(
                &error,
                runtime.context.home_generation,
            ));
        }
    };
    let launcher = match spawn_worker_launcher(runtime, thread_id) {
        Ok(launcher) => launcher,
        Err(failure) => {
            drop(flight);
            lane.requeue(entry)?;
            return Err(failure);
        }
    };
    let expected_connection = entry.expected_connection();
    let (projection, worker, prior_attempt) =
        entry.materialize(runtime.context.projection_retainer.clone());
    if runtime.context.ordinary_cancellation.is_cancelled() {
        let mut entry = RecoveredProjectionLaneEntry::from_materialized_with_worker(
            projection,
            worker,
            prior_attempt,
        );
        entry.mark_attempted(pass);
        lane.requeue(entry)?;
        finish_launcher(
            launcher,
            super::WorkerDisposition::RecoveredProjectionParked,
        )?;
        return Ok(true);
    }
    let admission = runtime.context.issue_scheduled_ordinary_execution(
        thread_id,
        projection.execution_binding().clone(),
        worker,
        flight,
    );
    finish_admission(
        runtime,
        command,
        launcher,
        projection,
        prior_attempt,
        pass,
        expected_connection,
        admission,
    )?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn finish_admission(
    runtime: &mut SchedulerRuntime,
    command: crate::cas_projection::LiveCommandPermit,
    launcher: std::sync::mpsc::SyncSender<RecoveredProjectionWorkerCommand>,
    projection: LoadedCasProjection,
    prior_attempt: RecoveredProjectionLaneAttempt,
    pass: u64,
    expected_connection: Arc<ProjectionConnection>,
    admission: Result<
        ScheduledOrdinaryAdmissionResult,
        crate::cas_projection::ScheduledOrdinaryAdmissionError,
    >,
) -> Result<(), SchedulerFailure> {
    match admission {
        Ok(ScheduledOrdinaryAdmissionResult::Issued(lease)) => {
            if !Arc::ptr_eq(lease.connection(), &expected_connection) {
                drop(lease);
                let mut entry =
                    RecoveredProjectionLaneEntry::from_materialized(projection, prior_attempt);
                entry.mark_attempted(pass);
                record_unavailable(runtime);
                requeue(runtime, entry)?;
                return finish_launcher(
                    launcher,
                    super::WorkerDisposition::RecoveredProjectionParked,
                );
            }
            let mut attempt = prior_attempt;
            attempt.last_attempt = pass;
            let execution = RecoveredProjectionExecution {
                validator: runtime.context.lease_validator(command),
                storage: runtime.context.storage,
                cancellation: runtime.context.ordinary_cancellation.clone(),
                lane: runtime.context.recovered_projection_lane.clone(),
                projection,
                lease,
                attempt,
            };
            match launcher.send(RecoveredProjectionWorkerCommand::Execute(execution)) {
                Ok(()) => Ok(()),
                Err(error) => {
                    if let RecoveredProjectionWorkerCommand::Execute(execution) = error.0 {
                        restore_unsent_execution(execution)?;
                    }
                    Err(SchedulerFailure::Fatal)
                }
            }
        }
        Ok(ScheduledOrdinaryAdmissionResult::Unavailable(_)) => {
            let mut entry =
                RecoveredProjectionLaneEntry::from_materialized(projection, prior_attempt);
            entry.mark_attempted(pass);
            record_unavailable(runtime);
            requeue(runtime, entry)?;
            finish_launcher(
                launcher,
                super::WorkerDisposition::RecoveredProjectionParked,
            )
        }
        Err(error)
            if super::failure::is_verification_pending_admission(
                &error,
                runtime.context.home_generation,
            ) =>
        {
            let entry = RecoveredProjectionLaneEntry::from_materialized(projection, prior_attempt);
            requeue(runtime, entry)?;
            finish_launcher(launcher, super::WorkerDisposition::VerificationPending)
        }
        Err(error)
            if super::failure::is_cut_correlated_admission(
                &error,
                runtime.context.home_generation,
            ) =>
        {
            runtime.context.projection_retainer.retain(projection);
            finish_launcher(launcher, super::WorkerDisposition::PersistentHomeFailure)
        }
        Err(error) => {
            let expected = super::next_turn::expected_admission_drift(&error);
            let mut entry =
                RecoveredProjectionLaneEntry::from_materialized(projection, prior_attempt);
            entry.mark_attempted(pass);
            record_unavailable(runtime);
            requeue(runtime, entry)?;
            finish_launcher(
                launcher,
                if expected {
                    super::WorkerDisposition::RecoveredProjectionParked
                } else {
                    super::WorkerDisposition::Fatal
                },
            )
        }
    }
}

fn restore_unsent_execution(
    execution: RecoveredProjectionExecution,
) -> Result<(), SchedulerFailure> {
    let RecoveredProjectionExecution {
        lane,
        projection,
        lease,
        attempt,
        ..
    } = execution;
    drop(lease);
    let entry = RecoveredProjectionLaneEntry::from_materialized(projection, attempt);
    lane.requeue(entry)
}

fn finish_launcher(
    launcher: std::sync::mpsc::SyncSender<RecoveredProjectionWorkerCommand>,
    disposition: super::WorkerDisposition,
) -> Result<(), SchedulerFailure> {
    launcher
        .send(RecoveredProjectionWorkerCommand::Finish(disposition))
        .map_err(|_| SchedulerFailure::Fatal)
}

fn classify_coordinator_error(
    error: &ProjectionCoordinatorError,
    home_generation: beryl_home_store::HomeGeneration,
) -> SchedulerFailure {
    super::failure::from_coordinator(error, home_generation)
}

fn record_unavailable(runtime: &SchedulerRuntime) {
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.recovered_projection_execution_unavailable = diagnostics
            .recovered_projection_execution_unavailable
            .saturating_add(1);
    });
}

fn requeue(
    runtime: &SchedulerRuntime,
    entry: RecoveredProjectionLaneEntry,
) -> Result<(), SchedulerFailure> {
    runtime.context.recovered_projection_lane.requeue(entry)
}

fn spawn_worker_launcher(
    runtime: &mut SchedulerRuntime,
    thread_id: SyndicThreadId,
) -> Result<std::sync::mpsc::SyncSender<RecoveredProjectionWorkerCommand>, SchedulerFailure> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let completions = runtime.completions.clone();
    let signal = runtime.context.signal.clone();
    let worker_signal = signal.clone();
    let handle = std::thread::Builder::new()
        .name("beryl-recovered-projection-execution".to_owned())
        .spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match receiver.recv() {
                    Ok(RecoveredProjectionWorkerCommand::Execute(execution)) => {
                        execute_recovered_projection(execution)
                    }
                    Ok(RecoveredProjectionWorkerCommand::Finish(disposition)) => disposition,
                    Err(_) => super::WorkerDisposition::Fatal,
                }));
            let disposition = match result {
                Ok(disposition) => disposition,
                Err(_) => super::WorkerDisposition::Fatal,
            };
            completions.publish(super::WorkerCompletion {
                thread_id: std::thread::current().id(),
                disposition,
            });
            worker_signal.wake(AcceptedInputWakeReason::WorkerCompleted);
            disposition
        })
        .map_err(|_| SchedulerFailure::Fatal)?;
    runtime.register_recovered_projection_worker(handle, thread_id);
    Ok(sender)
}

fn execute_recovered_projection(
    mut execution: RecoveredProjectionExecution,
) -> super::WorkerDisposition {
    if execution.cancellation.is_cancelled() {
        return restore_execution(
            execution,
            super::WorkerDisposition::RecoveredProjectionParked,
        );
    }
    if let Err(error) = execution.validator.validate(&mut execution.lease) {
        if super::failure::is_verification_pending_admission(
            &error,
            execution.validator.home_generation(),
        ) {
            let _ = execution.validator.observe_persistent_failure();
            return restore_execution(execution, super::WorkerDisposition::VerificationPending);
        }
        if super::failure::is_cut_correlated_admission(
            &error,
            execution.validator.home_generation(),
        ) {
            return retain_failed_execution(execution);
        }
        return restore_execution(
            execution,
            if super::next_turn::expected_admission_drift(&error) {
                super::WorkerDisposition::RecoveredProjectionParked
            } else {
                super::WorkerDisposition::Fatal
            },
        );
    }
    let coordinator = match CasProjectionCoordinator::for_healthy_home(&execution.validator.home) {
        Ok(coordinator) => coordinator,
        Err(error)
            if super::failure::is_verification_pending_coordinator(
                &error,
                execution.validator.home_generation(),
            ) =>
        {
            let _ = execution.validator.observe_persistent_failure();
            return restore_execution(execution, super::WorkerDisposition::VerificationPending);
        }
        Err(error)
            if super::failure::is_cut_correlated_coordinator(
                &error,
                execution.validator.home_generation(),
            ) =>
        {
            return retain_failed_execution(execution);
        }
        Err(error) if super::next_turn::expected_coordinator_drift(&error) => {
            return restore_execution(
                execution,
                super::WorkerDisposition::RecoveredProjectionParked,
            );
        }
        Err(_) => {
            return restore_execution(execution, super::WorkerDisposition::Fatal);
        }
    };
    let projection = execution.projection;
    let cancellation = execution.cancellation.clone();
    let storage = execution.storage;
    let outcome =
        execution
            .lease
            .with_execution_authority(|_session, policy, assets, tools, flight| {
                coordinator.execute_ordinary_turn_in_flight(
                    &execution.validator.home,
                    storage,
                    assets,
                    projection,
                    &cancellation,
                    policy.turn(),
                    tools,
                    flight,
                )
            });
    match outcome {
        Err(OrdinaryTurnExecutionFailure::PreActivation { projection, source }) => {
            if super::next_turn::ordinary_error_verification_pending(
                &source,
                execution.validator.home_generation(),
            ) {
                execution.projection = *projection;
                let _ = execution.validator.observe_persistent_failure();
                restore_execution(execution, super::WorkerDisposition::VerificationPending)
            } else if super::next_turn::ordinary_error_cut_correlated(
                &source,
                execution.validator.home_generation(),
            ) || execution.validator.observe_persistent_failure()
            {
                let _ = execution.validator.observe_persistent_failure();
                drop(execution.lease);
                execution.validator.retain_failed_projection(*projection);
                super::WorkerDisposition::PersistentHomeFailure
            } else {
                execution.projection = *projection;
                restore_execution(
                    execution,
                    super::WorkerDisposition::RecoveredProjectionContinue,
                )
            }
        }
        outcome => {
            let settlement =
                super::next_turn::settle_ordinary_outcome(&execution.validator, outcome);
            drop(execution.lease);
            match settlement {
                super::next_turn::OrdinaryTurnSettlement::Settled => {
                    super::WorkerDisposition::RecoveredProjectionContinue
                }
                super::next_turn::OrdinaryTurnSettlement::VerificationPending => {
                    super::WorkerDisposition::VerificationPending
                }
                super::next_turn::OrdinaryTurnSettlement::PersistentHomeFailure => {
                    super::WorkerDisposition::PersistentHomeFailure
                }
            }
        }
    }
}

fn restore_execution(
    execution: RecoveredProjectionExecution,
    disposition: super::WorkerDisposition,
) -> super::WorkerDisposition {
    let RecoveredProjectionExecution {
        lane,
        projection,
        lease,
        attempt,
        ..
    } = execution;
    drop(lease);
    let entry = RecoveredProjectionLaneEntry::from_materialized(projection, attempt);
    if lane.requeue(entry).is_err() {
        return super::WorkerDisposition::Fatal;
    }
    disposition
}

fn retain_failed_execution(execution: RecoveredProjectionExecution) -> super::WorkerDisposition {
    let RecoveredProjectionExecution {
        validator,
        projection,
        lease,
        ..
    } = execution;
    drop(lease);
    validator.retain_failed_projection(projection);
    super::WorkerDisposition::PersistentHomeFailure
}

#[cfg(test)]
mod tests {
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/recovered_projection_scheduler.rs"
    ));
}
