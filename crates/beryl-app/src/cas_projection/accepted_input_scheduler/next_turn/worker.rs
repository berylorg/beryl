use beryl_home_store::CommandOutcome;
use syndic_storage::{AcceptedInputPromotionStatus, AcceptedNextCandidate, PromoteAcceptedInput};

use super::{
    super::{
        AcceptedInputSchedulerSignal, AcceptedInputWakeReason, SchedulerFailure, SchedulerRuntime,
        WorkerCompletion, WorkerDisposition, failure,
    },
    authority::{
        LeaseValidationAuthority, expected_admission_drift, obsolete_admission_generation,
    },
};
use crate::{
    cas_projection::{
        ProjectionCancellationToken, ScheduledOrdinaryExecutionLease,
        connection::ConnectionPromotionReleaseOutcome,
    },
    input_admission::accepted_input_promotion_command,
};

mod execution;
mod preparation;
mod settlement;

pub(in crate::cas_projection::accepted_input_scheduler) use execution::{
    PendingTurnExecutionDisposition, execute_pending_turn,
};
use preparation::{
    classify_unbuilt_promotion, fresh_item_id, fresh_turn_id, pause_obsolete_generation,
    reconcile_promotion,
};
pub(in crate::cas_projection::accepted_input_scheduler) use preparation::{
    current_selected_path, current_timestamp,
};
pub(in crate::cas_projection::accepted_input_scheduler) use settlement::{
    OrdinaryTurnSettlement, ordinary_error_cut_correlated, settle_ordinary_outcome,
};

pub(super) fn spawn_worker(
    runtime: &mut SchedulerRuntime,
    candidate: AcceptedNextCandidate,
    mut lease: ScheduledOrdinaryExecutionLease,
) -> Result<(), SchedulerFailure> {
    let syndic_thread_id = candidate.thread_id();
    let Some(command) = failure::authorize(&runtime.context)? else {
        return Ok(());
    };
    let validator = runtime.context.lease_validator(command);
    let storage = runtime.context.storage;
    let cancellation = runtime.context.ordinary_cancellation.clone();
    let signal = runtime.context.signal.clone();
    let completions = runtime.completions.clone();
    let handle = std::thread::Builder::new()
        .name("beryl-scheduled-ordinary-execution".to_owned())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                execute_candidate(
                    &validator,
                    storage,
                    &signal,
                    &cancellation,
                    candidate,
                    &mut lease,
                )
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

fn execute_candidate(
    validator: &LeaseValidationAuthority,
    storage: syndic_storage::SyndicStorage,
    signal: &AcceptedInputSchedulerSignal,
    cancellation: &ProjectionCancellationToken,
    candidate: AcceptedNextCandidate,
    lease: &mut ScheduledOrdinaryExecutionLease,
) -> WorkerDisposition {
    if cancellation.is_cancelled() {
        return WorkerDisposition::NextParked;
    }
    if let Err(error) = validator.validate(lease) {
        pause_obsolete_generation(candidate.thread_id(), obsolete_admission_generation(&error));
        return if failure::is_current_health_loss_admission(&error, validator.home_generation()) {
            WorkerDisposition::PersistentHomeFailure
        } else if failure::is_cut_correlated_admission(&error, validator.home_generation()) {
            WorkerDisposition::PersistentHomeFailure
        } else if expected_admission_drift(&error) {
            WorkerDisposition::NextParked
        } else {
            WorkerDisposition::Fatal
        };
    }
    let promoted_at = match current_timestamp(candidate.minimum_promotion_timestamp()) {
        Ok(timestamp) => timestamp,
        Err(()) => return WorkerDisposition::Fatal,
    };
    let successor_turn_id = match fresh_turn_id() {
        Ok(id) => id,
        Err(()) => return WorkerDisposition::Fatal,
    };
    let successor_item_id = match fresh_item_id() {
        Ok(id) => id,
        Err(()) => return WorkerDisposition::Fatal,
    };
    let promotion =
        PromoteAcceptedInput::new(candidate, successor_turn_id, successor_item_id, promoted_at);
    let command = match accepted_input_promotion_command(
        &validator.home,
        storage,
        lease.assets(),
        promotion.clone(),
    ) {
        Ok(command) => command,
        Err(_) => {
            return classify_unbuilt_promotion(validator, storage, &promotion);
        }
    };
    if cancellation.is_cancelled() {
        return WorkerDisposition::NextParked;
    }
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_scheduled_promotion_reservation(
        promotion.thread_id(),
    );
    let reservation = match validator.reserve_promotion(lease) {
        Ok(Some(reservation)) => reservation,
        Ok(None) => return WorkerDisposition::NextParked,
        Err(error) if failure::is_cut_correlated_admission(&error, validator.home_generation()) => {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Err(error)
            if failure::is_current_health_loss_admission(&error, validator.home_generation()) =>
        {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Err(error) if expected_admission_drift(&error) => {
            return WorkerDisposition::NextParked;
        }
        Err(error) => {
            pause_obsolete_generation(promotion.thread_id(), obsolete_admission_generation(&error));
            return WorkerDisposition::Fatal;
        }
    };
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_scheduled_promotion(promotion.thread_id());
    let free_space = validator
        .home
        .query_free_space(validator.turn_start_admission_requirement());
    if !matches!(
        free_space,
        beryl_home_store::FreeSpaceOutcome::Sufficient { .. }
    ) {
        return match reservation.release() {
            Ok(ConnectionPromotionReleaseOutcome::Ordinary)
            | Ok(ConnectionPromotionReleaseOutcome::Closed) => WorkerDisposition::NextParked,
            Ok(ConnectionPromotionReleaseOutcome::PersistentFailure) => {
                WorkerDisposition::PersistentHomeFailure
            }
            Err(_) => WorkerDisposition::Fatal,
        };
    }
    let dispatch = validator.home.execute(command);
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_scheduled_promotion_reconciliation(
        promotion.thread_id(),
    );
    let command_failure = match dispatch {
        CommandOutcome::NotCommitted { evidence } => {
            Some(WorkerDisposition::CommandNotCommitted(evidence))
        }
        CommandOutcome::Committed {
            receipt: _,
            later_failure: None,
        } => None,
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
        } => Some(WorkerDisposition::CommandCommitted {
            receipt,
            later_failure,
        }),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            Some(WorkerDisposition::CommandIndeterminate { failure })
        }
    };
    match reservation.release() {
        Ok(ConnectionPromotionReleaseOutcome::Ordinary) => {}
        Ok(ConnectionPromotionReleaseOutcome::PersistentFailure) => {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Ok(ConnectionPromotionReleaseOutcome::Closed) => {
            return WorkerDisposition::NextParked;
        }
        Err(_) => return WorkerDisposition::Fatal,
    }
    if let Some(failure) = command_failure {
        return failure;
    }
    let promotion_result = reconcile_promotion(validator, storage, lease.assets(), &promotion)
        .map(|status| Some((true, status)));
    let Some((dispatch_succeeded, status)) = (match promotion_result {
        Ok(result) => result,
        Err(SchedulerFailure::PersistentHomeFailure) => {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Err(SchedulerFailure::Fatal) => return WorkerDisposition::Fatal,
    }) else {
        signal.wake(AcceptedInputWakeReason::AcceptedNextReady);
        return WorkerDisposition::NextContinue;
    };
    match (dispatch_succeeded, status) {
        (_, AcceptedInputPromotionStatus::Exact) => {}
        (false, AcceptedInputPromotionStatus::Prior) => {
            return WorkerDisposition::NextParked;
        }
        (true, AcceptedInputPromotionStatus::Prior)
        | (_, AcceptedInputPromotionStatus::Collision) => return WorkerDisposition::Fatal,
    }

    signal.wake(AcceptedInputWakeReason::AcceptedNextReady);
    let selected_path = match current_selected_path(
        &validator.home,
        storage,
        promotion.thread_id(),
        validator.home_generation(),
    ) {
        Ok(path) => path,
        Err(SchedulerFailure::PersistentHomeFailure) => {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Err(SchedulerFailure::Fatal) => return WorkerDisposition::Fatal,
    };
    match execute_pending_turn(
        validator,
        storage,
        cancellation,
        promoted_at,
        selected_path,
        lease,
    ) {
        PendingTurnExecutionDisposition::Settled => WorkerDisposition::NextContinue,
        PendingTurnExecutionDisposition::ExpectedInterruption => WorkerDisposition::NextParked,
        PendingTurnExecutionDisposition::PersistentHomeFailure => {
            WorkerDisposition::PersistentHomeFailure
        }
        PendingTurnExecutionDisposition::ProjectionRefused => WorkerDisposition::Fatal,
    }
}
