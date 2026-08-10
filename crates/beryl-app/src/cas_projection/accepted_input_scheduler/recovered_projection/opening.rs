use std::sync::Arc;

use super::{
    RecoveredProjectionExecution, RecoveredProjectionLaneAttempt, RecoveredProjectionLaneEntry,
    RecoveredProjectionWorkerCommand, SchedulerFailure, SchedulerRuntime,
};
use crate::cas_projection::{
    LoadedCasProjection, ScheduledOrdinaryAdmissionResult, connection::ProjectionConnection,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn finish_admission(
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
                    super::super::WorkerDisposition::RecoveredProjectionParked,
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
                super::super::WorkerDisposition::RecoveredProjectionParked,
            )
        }
        Err(error)
            if super::super::failure::is_verification_pending_admission(
                &error,
                runtime.context.home_generation,
            ) =>
        {
            let entry = RecoveredProjectionLaneEntry::from_materialized(projection, prior_attempt);
            requeue(runtime, entry)?;
            finish_launcher(
                launcher,
                super::super::WorkerDisposition::VerificationPending,
            )
        }
        Err(error)
            if super::super::failure::is_cut_correlated_admission(
                &error,
                runtime.context.home_generation,
            ) =>
        {
            runtime.context.projection_retainer.retain(projection);
            finish_launcher(
                launcher,
                super::super::WorkerDisposition::PersistentHomeFailure,
            )
        }
        Err(error) => {
            let expected = super::super::next_turn::expected_admission_drift(&error);
            let mut entry =
                RecoveredProjectionLaneEntry::from_materialized(projection, prior_attempt);
            entry.mark_attempted(pass);
            record_unavailable(runtime);
            requeue(runtime, entry)?;
            finish_launcher(
                launcher,
                if expected {
                    super::super::WorkerDisposition::RecoveredProjectionParked
                } else {
                    super::super::WorkerDisposition::Fatal
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

pub(super) fn finish_launcher(
    launcher: std::sync::mpsc::SyncSender<RecoveredProjectionWorkerCommand>,
    disposition: super::super::WorkerDisposition,
) -> Result<(), SchedulerFailure> {
    launcher
        .send(RecoveredProjectionWorkerCommand::Finish(disposition))
        .map_err(|_| SchedulerFailure::Fatal)
}

pub(super) fn record_unavailable(runtime: &SchedulerRuntime) {
    runtime.context.signal.update_diagnostics(|diagnostics| {
        diagnostics.recovered_projection_execution_unavailable = diagnostics
            .recovered_projection_execution_unavailable
            .saturating_add(1);
    });
}

pub(super) fn requeue(
    runtime: &SchedulerRuntime,
    entry: RecoveredProjectionLaneEntry,
) -> Result<(), SchedulerFailure> {
    runtime.context.recovered_projection_lane.requeue(entry)
}
