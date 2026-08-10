use super::{
    RecoveredProjectionLane, RecoveredProjectionLaneEntry, SchedulerFailure, SchedulerRuntime,
    opening, worker,
};
use crate::cas_projection::{CasProjectionCoordinator, ProjectionCoordinatorError};

pub(in super::super) fn run_pass(runtime: &mut SchedulerRuntime) -> Result<(), SchedulerFailure> {
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
    let Some(command) = super::super::failure::authorize(&runtime.context)? else {
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
            opening::record_unavailable(runtime);
            opening::requeue(runtime, entry)?;
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
            if super::super::failure::is_verification_pending_coordinator(
                &error,
                runtime.context.home_generation,
            ) =>
        {
            lane.requeue(entry)?;
            return Err(SchedulerFailure::VerificationPending);
        }
        Err(error)
            if super::super::failure::is_cut_correlated_coordinator(
                &error,
                runtime.context.home_generation,
            ) =>
        {
            lane.requeue(entry)?;
            return Err(SchedulerFailure::PersistentHomeFailure);
        }
        Err(error) => {
            if super::super::next_turn::expected_coordinator_drift(&error) {
                entry.mark_attempted(pass);
                opening::record_unavailable(runtime);
                opening::requeue(runtime, entry)?;
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
    let launcher = match worker::spawn_worker_launcher(runtime, thread_id) {
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
        opening::finish_launcher(
            launcher,
            super::super::WorkerDisposition::RecoveredProjectionParked,
        )?;
        return Ok(true);
    }
    let admission = runtime.context.issue_scheduled_ordinary_execution(
        thread_id,
        projection.execution_binding().clone(),
        worker,
        flight,
    );
    opening::finish_admission(
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

fn classify_coordinator_error(
    error: &ProjectionCoordinatorError,
    home_generation: beryl_home_store::HomeGeneration,
) -> SchedulerFailure {
    super::super::failure::from_coordinator(error, home_generation)
}
