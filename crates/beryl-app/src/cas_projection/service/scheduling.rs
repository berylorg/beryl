use super::flight_registry::FlightRegistry;
use super::*;

impl ProjectionConnectionService {
    /// Returns bounded content-free state for the one-shot persistent-failure cut.
    #[must_use]
    pub fn persistent_failure_cut_snapshot(&self) -> PersistentFailureCutSnapshot {
        self.persistent_failure
            .as_ref()
            .expect("open service retains its persistent-failure coordinator")
            .snapshot()
    }

    /// Returns a cloneable nonblocking typed-health notification handle for process workers.
    #[must_use]
    pub fn persistent_failure_notification(&self) -> PersistentFailureNotification {
        self.persistent_failure
            .as_ref()
            .expect("open service retains its persistent-failure coordinator")
            .notification()
    }

    pub(in crate::cas_projection) fn attach_recovery_supervisor(
        &self,
        signal: std::sync::mpsc::SyncSender<()>,
    ) -> Result<(), ()> {
        self.persistent_failure_notification()
            .attach_recovery_supervisor(signal)
    }

    #[must_use]
    pub const fn initial_storage_revision(&self) -> DomainRevision {
        self.startup_storage_revision
    }

    /// Returns the immutable local limits used for every admitted candidate.
    #[must_use]
    pub const fn config(&self) -> ProjectionServiceConfig {
        self.config
    }

    /// Returns content-free worker-pool count diagnostics.
    #[must_use]
    pub fn worker_pool_diagnostics(&self) -> ProjectionWorkerPoolDiagnostics {
        self.workers.diagnostics()
    }

    /// Returns bounded content-free accepted-input scheduler diagnostics.
    #[must_use]
    pub fn accepted_input_scheduler_diagnostics(&self) -> AcceptedInputSchedulerDiagnostics {
        self.scheduler.as_ref().map_or_else(
            || self.scheduler_signal.diagnostics(),
            AcceptedInputScheduler::diagnostics,
        )
    }

    /// Installs and wakes one exact test-only scheduler-main panic.
    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn install_accepted_input_scheduler_panic_for_test(
        &self,
    ) -> super::super::test_faults::AcceptedInputSchedulerPanicController {
        let controller = super::super::test_faults::install_accepted_input_scheduler_panic(
            self.home_id,
            self.home_generation,
            self.service_generation(),
        );
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::WorkerCompleted);
        controller
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn install_blocked_scheduler_projection_worker_for_test(
        &self,
        projection: LoadedCasProjection,
        worker: super::super::service_config::ProjectionWorkerPermit,
    ) -> super::super::test_faults::AcceptedInputSchedulerWorkerController {
        let controller = super::super::test_faults::install_accepted_input_scheduler_worker(
            self.home_id,
            self.home_generation,
            self.service_generation(),
            projection,
            worker,
        );
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::WorkerCompleted);
        controller
    }

    /// Returns bounded content-free context-compaction capacity diagnostics.
    #[must_use]
    pub fn context_compaction_diagnostics(&self) -> super::super::ContextCompactionDiagnostics {
        self.context_compaction
            .as_ref()
            .expect("open service retains its context-compaction coordinator")
            .diagnostics()
    }

    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn saturate_context_compaction_capacity_for_test(
        &self,
    ) -> Result<
        super::super::ContextCompactionCapacityTestGuard,
        super::super::ContextCompactionError,
    > {
        self.context_compaction
            .as_ref()
            .ok_or(super::super::ContextCompactionError::Unavailable)?
            .saturate_capacity_for_test()
    }

    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn deny_context_compaction_capacity_probe_for_test(&self) -> bool {
        self.context_compaction
            .as_ref()
            .is_none_or(|coordinator| coordinator.deny_capacity_probe_for_test())
    }

    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn stage_context_compaction_continuation_for_test(
        &self,
    ) -> Result<syndic_storage::ContentReference, super::super::ContextCompactionError> {
        self.context_compaction
            .as_ref()
            .ok_or(super::super::ContextCompactionError::Unavailable)?
            .stage_lifecycle_content_for_test()
    }

    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn context_compaction_lifecycle_test_harness(
        &self,
    ) -> Result<
        super::super::ContextCompactionLifecycleTestHarness,
        super::super::ContextCompactionError,
    > {
        Ok(self
            .context_compaction
            .as_ref()
            .ok_or(super::super::ContextCompactionError::Unavailable)?
            .lifecycle_test_harness())
    }

    /// Wakes the shared accepted-input scheduler after process-shell execution authority changes.
    pub fn notify_scheduled_ordinary_execution_ready(&self) {
        if self.command_authorizer.is_open() {
            self.scheduler_signal
                .wake(AcceptedInputWakeReason::ExecutionReady);
        }
    }

    pub(in crate::cas_projection) fn try_acquire_scheduled_ordinary_worker(
        &self,
    ) -> Result<super::super::service_config::ProjectionWorkerPermit, ProjectionWorkerPermitError>
    {
        self.workers.try_acquire_scheduled_ordinary_or_arm()
    }

    pub(in crate::cas_projection) fn begin_scheduled_ordinary_flight(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        FlightRegistry::acquire_or_arm(
            self.home_id,
            self.home_generation,
            thread_id,
            &self.scheduler_signal,
        )
    }

    pub(in crate::cas_projection) fn issue_scheduled_ordinary_execution(
        &self,
        thread_id: SyndicThreadId,
        execution_binding: ExecutionBinding,
        worker: super::super::service_config::ProjectionWorkerPermit,
        flight: ProjectionFlight,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let Ok(command) = self.command_authorizer.authorize() else {
            return Ok(ScheduledOrdinaryAdmissionResult::Unavailable(
                ScheduledOrdinaryExecutionUnavailable::ShuttingDown,
            ));
        };
        self.ensure_current()?;
        let expected_binding = execution_binding.clone();
        let admission = ScheduledOrdinaryAdmission::new(
            self.home_id,
            self.home_generation,
            thread_id,
            execution_binding,
            worker,
            self.persistent_failure
                .as_ref()
                .expect("open service retains persistent-failure coordination")
                .terminal_disposer(self.home_id, self.home_generation),
            flight,
        );
        let mut result = self
            .scheduled_ordinary_provider
            .as_ref()
            .expect("open projection service retains its execution provider")
            .lock()
            .map_err(|_| ScheduledOrdinaryAdmissionError::ProviderPoisoned)?
            .try_issue(admission)?;
        if let ScheduledOrdinaryAdmissionResult::Issued(lease) = &result
            && (lease.home_id() != self.home_id
                || lease.home_generation() != self.home_generation
                || lease.thread_id() != thread_id
                || lease.execution_binding() != &expected_binding)
        {
            return Err(ScheduledOrdinaryAdmissionError::LeaseMismatch { thread_id });
        }
        if let ScheduledOrdinaryAdmissionResult::Issued(lease) = &mut result {
            self.validate_scheduled_ordinary_lease(lease)?;
        }
        if !command.is_current() {
            return Ok(ScheduledOrdinaryAdmissionResult::Unavailable(
                ScheduledOrdinaryExecutionUnavailable::ShuttingDown,
            ));
        }
        Ok(result)
    }

    fn validate_scheduled_ordinary_lease(
        &self,
        lease: &mut ScheduledOrdinaryExecutionLease,
    ) -> Result<(), ScheduledOrdinaryAdmissionError> {
        self.ensure_current()?;
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        lease
            .assets()
            .revision(home)
            .map_err(|source| ScheduledOrdinaryAdmissionError::AssetAuthority { source })?;

        let retained_process_generation = lease.process_generation();
        let expected_connection = Arc::clone(lease.connection());
        let (runtime_id, process_generation, session_matches_connection) = {
            let session = lease.session();
            (
                session.runtime_id(),
                session.process_generation(),
                Arc::ptr_eq(session.connection(), &expected_connection),
            )
        };
        let is_owned = self
            .connections
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionConnection,
            })?
            .iter()
            .any(|candidate| Arc::ptr_eq(candidate, &expected_connection));
        if !is_owned
            || !session_matches_connection
            || expected_connection.is_retired()
            || expected_connection.is_detached()
            || runtime_id != lease.execution_binding().runtime_id()
            || process_generation != retained_process_generation
        {
            return Err(
                ScheduledOrdinaryAdmissionError::SessionAuthorityUnavailable {
                    runtime_id,
                    process_generation,
                },
            );
        }
        self.ensure_current()?;
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn cancel_active_steering_lifecycle_for_test(&self) {
        self.scheduler
            .as_ref()
            .expect("open service retains its scheduler")
            .cancel_current_lifecycle();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn renew_active_steering_lifecycle_for_test(&self) {
        self.scheduler
            .as_ref()
            .expect("open service retains its scheduler")
            .renew_cancellation_lifecycle();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn signal_accepted_ready_for_test(&self) {
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::AcceptedReady);
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn signal_accepted_next_ready_for_test(&self) {
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::AcceptedNextReady);
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn is_accepting_for_test(&self) -> bool {
        self.command_authorizer.is_open()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn home_for_shutdown_test(&self) -> &HomeStore {
        self.home
            .as_deref()
            .expect("unsettled test service owns its opened home")
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn registered_connection_count_for_test(&self) -> usize {
        self.connections
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .len()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn acquire_steering_worker_for_test(
        &self,
    ) -> Result<super::super::service_config::ProjectionWorkerPermit, ProjectionWorkerPermitError>
    {
        self.workers.try_acquire_steering_critical()
    }
}
