mod disposition;
mod lane;
mod opening;
mod pass;
mod worker;

use super::{AcceptedInputWakeReason, SchedulerFailure, SchedulerRuntime};
use crate::cas_projection::{
    CasProjectionCoordinator, LoadedCasProjection, OrdinaryTurnExecutionFailure,
    ScheduledOrdinaryExecutionLease,
};

pub(in crate::cas_projection) use lane::{
    RecoveredProjectionLane, RecoveredProjectionLaneParts, RecoveredProjectionLaneStageError,
    RecoveredProjectionLaneStageReason,
};
use lane::{RecoveredProjectionLaneAttempt, RecoveredProjectionLaneEntry};
pub(super) use lane::{dispose_retained, retain_for_persistent_failure};
pub(super) use pass::run_pass;

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

// Recovered workers retain their exact projection before execute_ordinary_turn_in_flight, and keep
// ordinary_error_verification_pending() nonterminal through restore_execution(execution, super::WorkerDisposition::VerificationPending).
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::AcceptedInputSchedulerSignal;
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/recovered_projection_scheduler.rs"
    ));
}
