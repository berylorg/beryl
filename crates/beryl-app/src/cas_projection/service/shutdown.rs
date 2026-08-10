use super::*;

impl ProjectionConnectionService {
    /// Elects ordinary shutdown against the exact typed persistent-failure cut.
    ///
    /// Ordinary shutdown retires all workers and explicitly closes the home. A
    /// winning persistent-failure cut instead returns its non-cloneable retained
    /// handoff without stopping providers, retiring connections, or closing the home.
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
                Ok(ProjectionConnectionServiceCloseOutcome::PersistentFailure(
                    self.retain_persistent_failure(failure_generation),
                ))
            }
        }
    }

    fn ordinary_shutdown_inner(&mut self) -> Result<(), ProjectionConnectionServiceCloseError> {
        self.settled = true;
        drop(
            self.persistent_failure_escrow
                .take()
                .expect("unsettled service retains its failure-escrow reservation"),
        );
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

    /// Explicitly joins a never-published service after its connection epochs became inert.
    ///
    /// The service may still be dormant or may already have converged its recovered startup state;
    /// in both cases its process-publication gate remains unpublished and the caller cancels that
    /// gate before disposal. Adoption retains the exact opened home outside this service, so
    /// terminal disposition settles all service-owned workers and the execution-provider fence
    /// without closing that home.
    pub(in crate::cas_projection) fn dispose_unpublished_inert(
        &mut self,
    ) -> Result<(), ProjectionConnectionServiceCloseError> {
        if self.settled {
            return Ok(());
        }
        self.settled = true;
        self.command_gate.close_for_local_failure();
        drop(self.persistent_failure_escrow.take());

        let persistent_failure = self.persistent_failure.take();
        if let Some(persistent_failure) = persistent_failure.as_ref() {
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

        self.connections
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clear();
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
        let persistent_failure_failed =
            persistent_failure.is_some_and(|coordinator| coordinator.join().is_err());

        if scheduler_failed {
            Err(ProjectionConnectionServiceCloseError::SchedulerShutdown)
        } else if provider_failed {
            Err(ProjectionConnectionServiceCloseError::ExecutionProviderShutdown)
        } else if compaction_failed {
            Err(ProjectionConnectionServiceCloseError::ContextCompactionShutdown)
        } else if persistent_failure_failed {
            Err(ProjectionConnectionServiceCloseError::PersistentFailureWorkerShutdown)
        } else {
            Ok(())
        }
    }

    fn request_implicit_ordinary_shutdown(&mut self) {
        self.settled = true;
        drop(self.persistent_failure_escrow.take());
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

    fn take_persistent_failure_retained_service(
        &mut self,
        failure_generation: super::super::PersistentFailureGeneration,
        persistent_failure: PersistentFailureCoordinator,
        mut completion: PersistentFailureCutCompletion,
    ) -> PersistentFailureRetainedService {
        let retained_connections = match self.connections.lock() {
            Ok(connections) => connections.clone(),
            Err(poison) => {
                completion = PersistentFailureCutCompletion::Incomplete;
                poison.into_inner().clone()
            }
        };
        PersistentFailureRetainedService::new(
            self.home
                .take()
                .expect("unsettled service retains its exact opened home"),
            self.home_id,
            self.home_generation,
            self.storage,
            self.startup.storage_revision(),
            self.config,
            self.workers.clone(),
            self.service_generation,
            failure_generation,
            self.command_gate.clone(),
            self.command_authorizer.clone(),
            persistent_failure,
            Arc::clone(&self.connections),
            retained_connections,
            Arc::clone(&self.stop_coordinator),
            self.context_compaction.take(),
            self.scheduler.take(),
            self.scheduler_signal.clone(),
            self.scheduled_ordinary_provider.take(),
            completion,
        )
    }

    fn retain_persistent_failure(
        &mut self,
        failure_generation: super::super::PersistentFailureGeneration,
    ) -> PersistentFailureCutHandoff {
        self.settled = true;
        let escrow = self
            .persistent_failure_escrow
            .take()
            .expect("unsettled service retains its failure-escrow reservation");
        let persistent_failure = self
            .persistent_failure
            .take()
            .expect("unsettled service retains its persistent-failure coordinator");
        let worker_join_failed = persistent_failure.join().is_err();
        let drain_failed = self.command_gate.wait_until_drained().is_err();
        let snapshot = persistent_failure.snapshot();
        let completion = if !worker_join_failed
            && !drain_failed
            && snapshot.state() == PersistentFailureCutState::Finished
        {
            PersistentFailureCutCompletion::Finished
        } else {
            PersistentFailureCutCompletion::Incomplete
        };
        self.take_persistent_failure_retained_service(
            failure_generation,
            persistent_failure,
            completion,
        )
        .escrow(escrow)
    }

    fn escrow_implicit_persistent_failure(
        &mut self,
        failure_generation: super::super::PersistentFailureGeneration,
    ) {
        self.settled = true;
        let escrow = self
            .persistent_failure_escrow
            .take()
            .expect("unsettled service retains its failure-escrow reservation");
        let persistent_failure = self
            .persistent_failure
            .take()
            .expect("unsettled service retains its persistent-failure coordinator");
        drop(
            self.take_persistent_failure_retained_service(
                failure_generation,
                persistent_failure,
                PersistentFailureCutCompletion::Incomplete,
            )
            .escrow(escrow),
        );
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
                self.escrow_implicit_persistent_failure(failure_generation);
            }
        }
    }
}
