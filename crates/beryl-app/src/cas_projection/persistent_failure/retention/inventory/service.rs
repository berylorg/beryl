use super::*;

impl PersistentFailureRetainedService {
    pub(super) fn shutdown_old_service_epoch(
        &self,
    ) -> Result<(), PersistentFailureOldServiceEpochRetirementReason> {
        let mut first_failure = None;
        match self.context_compaction.as_ref() {
            Some(context_compaction) => {
                if context_compaction.shutdown().is_err() {
                    first_failure = Some(
                        PersistentFailureOldServiceEpochRetirementReason::ContextCompactionShutdown,
                    );
                }
            }
            None => {
                first_failure = Some(
                    PersistentFailureOldServiceEpochRetirementReason::ContextCompactionUnavailable,
                );
            }
        }

        match self.scheduled_ordinary_provider.as_ref() {
            Some(provider) => {
                if Arc::strong_count(provider) != 1 && first_failure.is_none() {
                    first_failure = Some(
                        PersistentFailureOldServiceEpochRetirementReason::ExecutionProviderAliased,
                    );
                }
                match provider.lock() {
                    Ok(mut provider) => provider.shutdown(),
                    Err(poison) => {
                        poison.into_inner().shutdown();
                        if first_failure.is_none() {
                            first_failure = Some(
                                PersistentFailureOldServiceEpochRetirementReason::ExecutionProviderPoisoned,
                            );
                        }
                    }
                }
            }
            None => {
                if first_failure.is_none() {
                    first_failure = Some(
                        PersistentFailureOldServiceEpochRetirementReason::ExecutionProviderUnavailable,
                    );
                }
            }
        }

        first_failure.map_or(Ok(()), Err)
    }

    pub(super) fn quiesce_scheduler(&self) -> SchedulerQuiescence {
        let (scheduler, owner_poisoned) = match self.scheduler.lock() {
            Ok(mut scheduler) => (scheduler.take(), false),
            Err(poison) => (poison.into_inner().take(), true),
        };
        let Some(scheduler) = scheduler else {
            self.scheduler_signal.request_shutdown();
            return if owner_poisoned {
                SchedulerQuiescence::Poisoned
            } else {
                SchedulerQuiescence::Unavailable
            };
        };
        scheduler.request_shutdown();
        let exit = match scheduler.join() {
            Err(()) => return SchedulerQuiescence::Panicked,
            Ok(exit) => exit,
        };
        if owner_poisoned {
            return SchedulerQuiescence::Poisoned;
        }
        if exit == AcceptedInputSchedulerExit::Fatal {
            return SchedulerQuiescence::Fatal;
        }
        match self.command_authorizer.status_exact() {
            Ok(LiveCommandGateStatus::PersistentFailure) => SchedulerQuiescence::Clean,
            Ok(
                LiveCommandGateStatus::Open
                | LiveCommandGateStatus::OrdinaryShutdown
                | LiveCommandGateStatus::LocalFailure,
            ) => SchedulerQuiescence::Fatal,
            Err(_) => SchedulerQuiescence::CommandGatePoisoned,
        }
    }
}

impl Drop for PersistentFailureRecoveryInventory {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.escrow
            .retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(Arc::clone(&self.retained));
    }
}
