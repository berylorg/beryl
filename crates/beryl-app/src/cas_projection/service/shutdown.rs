use super::*;

impl ProjectionConnectionService {
    /// Elects ordinary shutdown against the exact typed persistent-failure cut.
    ///
    /// Ordinary shutdown retires all workers and explicitly closes the home. A
    /// winning persistent-failure cut instead terminally disposes every retained
    /// authority and returns bounded, content-free evidence of the completed cut.
    pub fn close(
        mut self,
    ) -> Result<ProjectionConnectionServiceCloseOutcome, ProjectionConnectionServiceCloseError>
    {
        self.close_inner()
    }

    fn close_inner(
        &mut self,
    ) -> Result<ProjectionConnectionServiceCloseOutcome, ProjectionConnectionServiceCloseError>
    {
        if self.settled {
            return Ok(ProjectionConnectionServiceCloseOutcome::Closed);
        }
        match self.command_gate.close_for_shutdown() {
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
        }
    }

    fn persistent_failure_shutdown_inner(
        &mut self,
        failure_generation: super::super::PersistentFailureGeneration,
    ) -> Result<PersistentFailureTerminalEvidence, ProjectionConnectionServiceCloseError> {
        self.settled = true;
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
        if let Some(context_compaction) = self.context_compaction.as_ref() {
            context_compaction.request_shutdown();
        }
        let connections = match self.connections.lock() {
            Ok(mut connections) => std::mem::take(&mut *connections),
            Err(poison) => std::mem::take(&mut *poison.into_inner()),
        };
        let mut connection_failed = false;
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
        self.settled = true;
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
        if let Some(context_compaction) = self.context_compaction.as_ref() {
            context_compaction.request_shutdown();
        }
        self.connections.reap_finished_ordinary_retirements();
        let connections = match self.connections.lock() {
            Ok(mut connections) => std::mem::take(&mut *connections),
            Err(poison) => std::mem::take(&mut *poison.into_inner()),
        };
        let mut connection_failed = false;
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
        let Some(home) = self.home.take() else {
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
            } else {
                Ok(())
            };
        };
        let home = Arc::try_unwrap(home)
            .map_err(|_| ProjectionConnectionServiceCloseError::HomeOwnershipLeaked)?;
        let close_result = home
            .close()
            .map_err(ProjectionConnectionServiceCloseError::HomeClose);
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
        close_result
    }

    fn request_implicit_ordinary_shutdown(&mut self) {
        self.settled = true;
        if let Some(persistent_failure) = self.persistent_failure.as_ref() {
            persistent_failure.request_shutdown();
        }
        if let Some(scheduler) = self.scheduler.as_ref() {
            scheduler.request_shutdown();
        } else {
            self.scheduler_signal.request_shutdown();
        }
        if let Some(context_compaction) = self.context_compaction.as_ref() {
            context_compaction.request_shutdown();
        }
        let connections = match self.connections.lock() {
            Ok(mut connections) => std::mem::take(&mut *connections),
            Err(poison) => std::mem::take(&mut *poison.into_inner()),
        };
        for connection in connections {
            connection.request_ordinary_retirement_after_service_shutdown();
        }
    }
}

impl Drop for ProjectionConnectionService {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        match self.command_gate.close_for_shutdown() {
            MasterCommandGateCloseOwner::OrdinaryShutdown => {
                self.request_implicit_ordinary_shutdown();
            }
            MasterCommandGateCloseOwner::PersistentFailure(failure_generation) => {
                let _ = self.persistent_failure_shutdown_inner(failure_generation);
            }
        }
    }
}
