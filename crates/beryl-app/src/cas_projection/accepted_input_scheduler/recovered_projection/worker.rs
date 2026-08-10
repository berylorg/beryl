use beryl_model::SyndicThreadId;

use super::{
    AcceptedInputWakeReason, CasProjectionCoordinator, OrdinaryTurnExecutionFailure,
    RecoveredProjectionExecution, RecoveredProjectionWorkerCommand, SchedulerFailure,
    SchedulerRuntime,
};

pub(super) fn spawn_worker_launcher(
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
                    Err(_) => super::super::WorkerDisposition::Fatal,
                }));
            let disposition = match result {
                Ok(disposition) => disposition,
                Err(_) => super::super::WorkerDisposition::Fatal,
            };
            completions.publish(super::super::WorkerCompletion {
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
) -> super::super::WorkerDisposition {
    if execution.cancellation.is_cancelled() {
        return super::disposition::restore_execution(
            execution,
            super::super::WorkerDisposition::RecoveredProjectionParked,
        );
    }
    if let Err(error) = execution.validator.validate(&mut execution.lease) {
        if super::super::failure::is_verification_pending_admission(
            &error,
            execution.validator.home_generation(),
        ) {
            let _ = execution.validator.observe_persistent_failure();
            return super::disposition::restore_execution(
                execution,
                super::super::WorkerDisposition::VerificationPending,
            );
        }
        if super::super::failure::is_cut_correlated_admission(
            &error,
            execution.validator.home_generation(),
        ) {
            return super::disposition::retain_failed_execution(execution);
        }
        return super::disposition::restore_execution(
            execution,
            if super::super::next_turn::expected_admission_drift(&error) {
                super::super::WorkerDisposition::RecoveredProjectionParked
            } else {
                super::super::WorkerDisposition::Fatal
            },
        );
    }
    let coordinator = match CasProjectionCoordinator::for_healthy_home(&execution.validator.home) {
        Ok(coordinator) => coordinator,
        Err(error)
            if super::super::failure::is_verification_pending_coordinator(
                &error,
                execution.validator.home_generation(),
            ) =>
        {
            let _ = execution.validator.observe_persistent_failure();
            return super::disposition::restore_execution(
                execution,
                super::super::WorkerDisposition::VerificationPending,
            );
        }
        Err(error)
            if super::super::failure::is_cut_correlated_coordinator(
                &error,
                execution.validator.home_generation(),
            ) =>
        {
            return super::disposition::retain_failed_execution(execution);
        }
        Err(error) if super::super::next_turn::expected_coordinator_drift(&error) => {
            return super::disposition::restore_execution(
                execution,
                super::super::WorkerDisposition::RecoveredProjectionParked,
            );
        }
        Err(_) => {
            return super::disposition::restore_execution(
                execution,
                super::super::WorkerDisposition::Fatal,
            );
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
            if super::super::next_turn::ordinary_error_verification_pending(
                &source,
                execution.validator.home_generation(),
            ) {
                execution.projection = *projection;
                let _ = execution.validator.observe_persistent_failure();
                super::disposition::restore_execution(
                    execution,
                    super::super::WorkerDisposition::VerificationPending,
                )
            } else if super::super::next_turn::ordinary_error_cut_correlated(
                &source,
                execution.validator.home_generation(),
            ) || execution.validator.observe_persistent_failure()
            {
                let _ = execution.validator.observe_persistent_failure();
                drop(execution.lease);
                execution.validator.retain_failed_projection(*projection);
                super::super::WorkerDisposition::PersistentHomeFailure
            } else {
                execution.projection = *projection;
                super::disposition::restore_execution(
                    execution,
                    super::super::WorkerDisposition::RecoveredProjectionContinue,
                )
            }
        }
        outcome => {
            let settlement =
                super::super::next_turn::settle_ordinary_outcome(&execution.validator, outcome);
            drop(execution.lease);
            match settlement {
                super::super::next_turn::OrdinaryTurnSettlement::Settled => {
                    super::super::WorkerDisposition::RecoveredProjectionContinue
                }
                super::super::next_turn::OrdinaryTurnSettlement::VerificationPending => {
                    super::super::WorkerDisposition::VerificationPending
                }
                super::super::next_turn::OrdinaryTurnSettlement::PersistentHomeFailure => {
                    super::super::WorkerDisposition::PersistentHomeFailure
                }
            }
        }
    }
}
