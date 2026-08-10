use super::{RecoveredProjectionExecution, RecoveredProjectionLaneEntry};

pub(super) fn restore_execution(
    execution: RecoveredProjectionExecution,
    disposition: super::super::WorkerDisposition,
) -> super::super::WorkerDisposition {
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
        return super::super::WorkerDisposition::Fatal;
    }
    disposition
}

pub(super) fn retain_failed_execution(
    execution: RecoveredProjectionExecution,
) -> super::super::WorkerDisposition {
    let RecoveredProjectionExecution {
        validator,
        projection,
        lease,
        ..
    } = execution;
    drop(lease);
    validator.retain_failed_projection(projection);
    super::super::WorkerDisposition::PersistentHomeFailure
}
