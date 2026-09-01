use super::{
    AcceptedInputWakeReason, SchedulerFailure, SchedulerRuntime, WorkerCompletion,
    WorkerDisposition, failure,
    next_turn::{
        LeaseValidationAuthority, OrdinaryTurnSettlement, PendingTurnExecutionDisposition,
        classify_projection_error, classify_projection_error_ref, expected_admission_drift,
        expected_coordinator_drift, settle_ordinary_outcome,
    },
};
use crate::cas_projection::{
    CasProjectionCoordinator, NativeLineageRecoveryCommand, ProjectionExecutionError,
    native_lineage_recovery::NativeLineageRecoveryWork,
    service_config::ProjectionWorkerPermitError,
};

pub(super) fn run_pass(runtime: &mut SchedulerRuntime) -> Result<(), SchedulerFailure> {
    if runtime.context.signal.is_shutdown()
        || runtime.context.ordinary_cancellation.is_cancelled()
        || !runtime.context.native_lineage_recovery.has_ready_work()
    {
        runtime.native_lineage_capacity_waiting = false;
        return Ok(());
    }
    let Some(command) = failure::authorize(&runtime.context)? else {
        return Ok(());
    };
    let worker = match runtime
        .context
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
    {
        Ok(worker) => worker,
        Err(ProjectionWorkerPermitError::CapacityFull { .. }) => {
            runtime.native_lineage_capacity_waiting = true;
            return Ok(());
        }
        Err(ProjectionWorkerPermitError::Poisoned) => return Err(SchedulerFailure::Fatal),
    };
    let Some(work) = runtime
        .context
        .native_lineage_recovery
        .take_ready_work(worker)
    else {
        runtime.native_lineage_capacity_waiting = false;
        return Ok(());
    };
    runtime.native_lineage_capacity_waiting = false;
    spawn_worker(runtime, runtime.context.lease_validator(command), work)?;
    if runtime.context.native_lineage_recovery.has_ready_work() {
        runtime
            .context
            .signal
            .wake(AcceptedInputWakeReason::NativeLineageReady);
    }
    Ok(())
}

fn spawn_worker(
    runtime: &mut SchedulerRuntime,
    validator: LeaseValidationAuthority,
    work: NativeLineageRecoveryWork,
) -> Result<(), SchedulerFailure> {
    let syndic_thread_id = work.thread_id();
    let storage = runtime.context.storage.clone();
    let signal = runtime.context.signal.clone();
    let completions = runtime.completions.clone();
    let handle = std::thread::Builder::new()
        .name("beryl-native-lineage-recovery-execution".to_owned())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                execute_work(&validator, &storage, work)
            }));
            let disposition = result.unwrap_or(WorkerDisposition::Fatal);
            completions.publish(WorkerCompletion {
                thread_id: std::thread::current().id(),
            });
            signal.wake(AcceptedInputWakeReason::WorkerCompleted);
            disposition
        })
        .map_err(|_| SchedulerFailure::Fatal)?;
    runtime.register_next_worker(handle, syndic_thread_id);
    Ok(())
}

fn execute_work(
    validator: &LeaseValidationAuthority,
    storage: &syndic_storage::SyndicStorage,
    work: NativeLineageRecoveryWork,
) -> WorkerDisposition {
    let (attempt, command, mut decision, mut lease) = work.into_parts();
    let cancellation = attempt.cancellation().clone();
    if cancellation.is_cancelled() {
        return WorkerDisposition::NextContinue;
    }
    if let Err(error) = validator.validate(&mut lease) {
        return if failure::is_current_health_loss_admission(&error, validator.home_generation())
            || failure::is_cut_correlated_admission(&error, validator.home_generation())
        {
            WorkerDisposition::PersistentHomeFailure
        } else if expected_admission_drift(&error) {
            WorkerDisposition::NextContinue
        } else {
            WorkerDisposition::Fatal
        };
    }
    let coordinator = match CasProjectionCoordinator::for_healthy_home(&validator.home) {
        Ok(coordinator) => coordinator,
        Err(error)
            if failure::is_cut_correlated_coordinator(&error, validator.home_generation()) =>
        {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Err(error) if expected_coordinator_drift(&error) => return WorkerDisposition::NextContinue,
        Err(_) => return WorkerDisposition::Fatal,
    };
    let projection = lease.with_execution_authority(|session, _, _, _, _| match command {
        NativeLineageRecoveryCommand::Retry => coordinator.retry_native_lineage_retained_in_flight(
            &validator.home,
            storage,
            session,
            &decision,
            &cancellation,
        ),
        NativeLineageRecoveryCommand::RecoverFromSyndic => coordinator
            .recover_native_lineage_from_syndic_retained(
                &validator.home,
                storage,
                session,
                &decision,
                &cancellation,
            ),
    });
    let projection = match projection {
        Ok(projection) => projection,
        Err(error) => {
            let disposition = classify_projection_error_ref(&error, validator.home_generation());
            match disposition {
                PendingTurnExecutionDisposition::PersistentHomeFailure => {
                    return WorkerDisposition::PersistentHomeFailure;
                }
                PendingTurnExecutionDisposition::ExpectedInterruption => {
                    return WorkerDisposition::NextContinue;
                }
                PendingTurnExecutionDisposition::ProjectionRefused => {}
                PendingTurnExecutionDisposition::Settled
                | PendingTurnExecutionDisposition::NativeLineageCapacityBlocked
                | PendingTurnExecutionDisposition::ParkNativeLineage { .. } => {
                    return WorkerDisposition::Fatal;
                }
            }
            if let ProjectionExecutionError::NativeLineageRecoveryRequired {
                decision: successor,
            } = error
            {
                decision = successor;
            }
            let retry_validation = lease.with_execution_authority(|session, _, _, _, _| {
                coordinator.validate_native_lineage_retry_in_flight(
                    &validator.home,
                    storage,
                    session,
                    &decision,
                    &cancellation,
                )
            });
            if let Err(error) = retry_validation {
                return match classify_projection_error(error, validator.home_generation()) {
                    PendingTurnExecutionDisposition::PersistentHomeFailure => {
                        WorkerDisposition::PersistentHomeFailure
                    }
                    PendingTurnExecutionDisposition::ExpectedInterruption
                    | PendingTurnExecutionDisposition::ProjectionRefused => {
                        WorkerDisposition::NextContinue
                    }
                    PendingTurnExecutionDisposition::Settled
                    | PendingTurnExecutionDisposition::NativeLineageCapacityBlocked
                    | PendingTurnExecutionDisposition::ParkNativeLineage { .. } => {
                        WorkerDisposition::Fatal
                    }
                };
            }
            let recovery_available = lease.with_execution_authority(|session, _, _, _, _| {
                coordinator
                    .validate_native_lineage_recovery_in_flight(
                        &validator.home,
                        storage,
                        session,
                        &decision,
                        &cancellation,
                    )
                    .is_ok()
            });
            attempt.failed(decision, lease, command, recovery_available);
            return WorkerDisposition::NextContinue;
        }
    };
    attempt.leaving();
    let outcome = lease.with_execution_authority(|_, policy, assets, tools, flight| {
        coordinator.execute_ordinary_turn_in_flight(
            &validator.home,
            storage,
            assets,
            projection,
            &cancellation,
            policy.turn(),
            tools,
            flight,
        )
    });
    match settle_ordinary_outcome(validator, outcome) {
        OrdinaryTurnSettlement::Settled => WorkerDisposition::NextContinue,
        OrdinaryTurnSettlement::PersistentHomeFailure => WorkerDisposition::PersistentHomeFailure,
    }
}
