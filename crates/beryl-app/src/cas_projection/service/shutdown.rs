use super::*;

pub struct ProjectionConnectionServiceCloseFailure {
    error: ProjectionConnectionServiceCloseError,
    service: Box<ProjectionConnectionService>,
}

impl std::fmt::Debug for ProjectionConnectionServiceCloseFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectionConnectionServiceCloseFailure")
            .field("error", &self.error)
            .field("retry_error", &self.service.close_retry_error)
            .field("auxiliary_error", &self.service.close_auxiliary_error)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for ProjectionConnectionServiceCloseFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.error)
    }
}

impl std::error::Error for ProjectionConnectionServiceCloseFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl ProjectionConnectionServiceCloseFailure {
    pub fn error(&self) -> &ProjectionConnectionServiceCloseError {
        &self.error
    }

    pub fn into_parts(
        self,
    ) -> (
        ProjectionConnectionServiceCloseError,
        Box<ProjectionConnectionService>,
    ) {
        (self.error, self.service)
    }

    pub fn retry_error(&self) -> Option<&ProjectionConnectionServiceCloseError> {
        self.service.close_retry_error.as_ref()
    }

    pub fn auxiliary_error(&self) -> Option<&ProjectionConnectionServiceCloseError> {
        self.service.close_auxiliary_error.as_ref()
    }

    pub fn retry(mut self) -> Result<ProjectionConnectionServiceCloseOutcome, Self> {
        if !matches!(
            self.error,
            ProjectionConnectionServiceCloseError::RuntimeRetirement
        ) || self.service.shutdown_started
            || self.service.close_retry_error.is_some()
        {
            return Err(self);
        }
        match self.service.close_inner() {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                if !matches!(
                    error,
                    ProjectionConnectionServiceCloseError::RuntimeRetirement
                ) {
                    self.service.close_retry_error = Some(error);
                }
                Err(self)
            }
        }
    }
}

impl ProjectionConnectionService {
    /// Elects ordinary shutdown against the exact typed persistent-failure cut.
    ///
    /// Ordinary shutdown retires all workers and explicitly closes the home. A
    /// winning persistent-failure cut instead terminally disposes every retained
    /// authority and returns bounded, content-free evidence of the completed cut.
    pub fn close(
        mut self,
    ) -> Result<ProjectionConnectionServiceCloseOutcome, ProjectionConnectionServiceCloseFailure>
    {
        match self.close_inner() {
            Ok(outcome) => Ok(outcome),
            Err(error) => Err(ProjectionConnectionServiceCloseFailure {
                error,
                service: Box::new(self),
            }),
        }
    }

    pub(super) fn close_inner(
        &mut self,
    ) -> Result<ProjectionConnectionServiceCloseOutcome, ProjectionConnectionServiceCloseError>
    {
        self.connections.work_boundary().close();
        self.outage_inventory.retire();
        if self.settled {
            return Ok(ProjectionConnectionServiceCloseOutcome::Closed);
        }
        if self.shutdown_started {
            return Err(ProjectionConnectionServiceCloseError::ShutdownIncomplete);
        }
        let election = self.command_gate.close_for_shutdown();
        self.initial_start.cancel();
        let runtime_failed = self
            .runtime_interest
            .as_ref()
            .is_some_and(|owner| !owner.shutdown());
        if runtime_failed {
            return Err(ProjectionConnectionServiceCloseError::RuntimeRetirement);
        }
        self.runtime_interest = None;
        let outcome = match election {
            MasterCommandGateCloseOwner::OrdinaryShutdown => {
                self.ordinary_shutdown_inner()?;
                Ok(ProjectionConnectionServiceCloseOutcome::Closed)
            }
            MasterCommandGateCloseOwner::PersistentFailure(failure_generation) => {
                let evidence = self.persistent_failure_shutdown_inner(failure_generation)?;
                Ok(ProjectionConnectionServiceCloseOutcome::PersistentFailure(
                    evidence,
                ))
            }
        };
        self.settled = true;
        outcome
    }

    fn persistent_failure_shutdown_inner(
        &mut self,
        failure_generation: super::super::PersistentFailureGeneration,
    ) -> Result<PersistentFailureTerminalEvidence, ProjectionConnectionServiceCloseError> {
        self.shutdown_started = true;
        let persistent_failure = self
            .persistent_failure
            .take()
            .expect("unsettled service retains its persistent-failure coordinator");
        let worker_failed = persistent_failure.join().is_err();
        let drain_failed = self.command_gate.wait_until_drained().is_err();
        let snapshot = persistent_failure.snapshot();
        let completion = if !worker_failed
            && !drain_failed
            && snapshot.state() == PersistentFailureCutState::Finished
        {
            PersistentFailureCutCompletion::Finished
        } else {
            PersistentFailureCutCompletion::Incomplete
        };
        let cut = super::super::persistent_failure::PersistentFailureCutIdentity::new(
            self.home_id,
            self.home_generation,
            self.service_generation,
            failure_generation,
        );
        let disposition_failed = persistent_failure.dispose_terminal_authority(cut).is_err();

        if let Some(scheduler) = self.scheduler.as_ref() {
            scheduler.request_shutdown();
        } else {
            self.scheduler_signal.request_shutdown();
        }
        self.native_lineage_recovery.close();
        if let Some(context_compaction) = self.context_compaction.as_ref() {
            context_compaction.request_shutdown();
        }
        let (connections, mut connection_failed) = match self.connections.lock() {
            Ok(mut connections) => {
                let retained = std::mem::take(&mut *connections);
                (retained, connections.revision().is_none())
            }
            Err(poison) => (std::mem::take(&mut *poison.into_inner()), true),
        };
        for connection in connections {
            if connection.shutdown().is_err() {
                connection_failed = true;
            }
        }
        let compaction_failed = self
            .context_compaction
            .take()
            .is_some_and(|coordinator| coordinator.shutdown().is_err());
        let scheduler_failed = self.scheduler.take().is_some_and(|scheduler| {
            scheduler.join().map_or(true, |exit| {
                matches!(exit, AcceptedInputSchedulerExit::Fatal)
            })
        });
        let provider_failed = self
            .scheduled_ordinary_provider
            .take()
            .is_some_and(|provider| match Arc::try_unwrap(provider) {
                Ok(provider) => {
                    provider
                        .into_inner()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .shutdown();
                    false
                }
                Err(provider) => {
                    provider
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .shutdown();
                    true
                }
            });
        drop(self.home.take());
        drop(self.owned_home.take());

        if disposition_failed {
            return Err(ProjectionConnectionServiceCloseError::PersistentFailureDisposal);
        }
        if connection_failed {
            return Err(ProjectionConnectionServiceCloseError::ConnectionShutdown);
        }
        if scheduler_failed {
            return Err(ProjectionConnectionServiceCloseError::SchedulerShutdown);
        }
        if provider_failed {
            return Err(ProjectionConnectionServiceCloseError::ExecutionProviderShutdown);
        }
        if compaction_failed {
            return Err(ProjectionConnectionServiceCloseError::ContextCompactionShutdown);
        }
        if worker_failed {
            return Err(ProjectionConnectionServiceCloseError::PersistentFailureWorkerShutdown);
        }
        Ok(PersistentFailureTerminalEvidence::new(
            self.home_id,
            self.home_generation,
            self.service_generation,
            failure_generation,
            snapshot,
            completion,
        ))
    }

    fn ordinary_shutdown_inner(&mut self) -> Result<(), ProjectionConnectionServiceCloseError> {
        self.shutdown_started = true;
        let persistent_failure = self
            .persistent_failure
            .take()
            .expect("unsettled service retains its persistent-failure coordinator");
        persistent_failure.request_shutdown();
        if let Some(scheduler) = self.scheduler.as_ref() {
            scheduler.request_shutdown();
        } else {
            self.scheduler_signal.request_shutdown();
        }
        self.native_lineage_recovery.close();
        if let Some(context_compaction) = self.context_compaction.as_ref() {
            context_compaction.request_shutdown();
        }
        let (connections, mut connection_failed) = match self.connections.lock() {
            Ok(mut connections) => {
                let retained = std::mem::take(&mut *connections);
                (retained, connections.revision().is_none())
            }
            Err(poison) => (std::mem::take(&mut *poison.into_inner()), true),
        };
        for connection in connections {
            if connection.shutdown().is_err() {
                connection_failed = true;
            }
        }
        let compaction_failed = self
            .context_compaction
            .take()
            .is_some_and(|coordinator| coordinator.shutdown().is_err());
        let scheduler_failed = match self.scheduler.take() {
            Some(scheduler) => scheduler
                .join()
                .map_or(true, AcceptedInputSchedulerExit::failed),
            None => false,
        };
        let provider_failed = self
            .scheduled_ordinary_provider
            .take()
            .is_some_and(|provider| match Arc::try_unwrap(provider) {
                Ok(provider) => {
                    provider
                        .into_inner()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .shutdown();
                    false
                }
                Err(provider) => {
                    provider
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .shutdown();
                    true
                }
            });
        let persistent_failure_failed = persistent_failure.join().is_err();
        let drain_failed = self.command_gate.wait_until_drained().is_err();
        drop(self.home.take());
        let Some(home) = self.owned_home.take() else {
            return if connection_failed {
                Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
            } else if scheduler_failed {
                Err(ProjectionConnectionServiceCloseError::SchedulerShutdown)
            } else if provider_failed {
                Err(ProjectionConnectionServiceCloseError::ExecutionProviderShutdown)
            } else if compaction_failed {
                Err(ProjectionConnectionServiceCloseError::ContextCompactionShutdown)
            } else if persistent_failure_failed {
                Err(ProjectionConnectionServiceCloseError::PersistentFailureWorkerShutdown)
            } else if drain_failed {
                Err(ProjectionConnectionServiceCloseError::CommandDrain)
            } else {
                Ok(())
            };
        };
        let mut close_result = home
            .close()
            .map_err(ProjectionConnectionServiceCloseError::HomeClose);
        if connection_failed
            || scheduler_failed
            || provider_failed
            || compaction_failed
            || persistent_failure_failed
            || drain_failed
        {
            self.close_auxiliary_error = close_result.err();
            close_result = Ok(());
        }
        if connection_failed {
            return Err(ProjectionConnectionServiceCloseError::ConnectionShutdown);
        }
        if scheduler_failed {
            return Err(ProjectionConnectionServiceCloseError::SchedulerShutdown);
        }
        if provider_failed {
            return Err(ProjectionConnectionServiceCloseError::ExecutionProviderShutdown);
        }
        if compaction_failed {
            return Err(ProjectionConnectionServiceCloseError::ContextCompactionShutdown);
        }
        if persistent_failure_failed {
            return Err(ProjectionConnectionServiceCloseError::PersistentFailureWorkerShutdown);
        }
        if drain_failed {
            return Err(ProjectionConnectionServiceCloseError::CommandDrain);
        }
        close_result
    }
}

impl Drop for ProjectionConnectionService {
    fn drop(&mut self) {
        let _ = self.close_inner();
        self.stop_coordinator.dispose_feedback();
    }
}
