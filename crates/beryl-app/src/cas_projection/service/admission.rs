use super::*;

impl ProjectionConnectionService {
    /// Creates, initializes, and release-admits one exact foreground candidate.
    ///
    /// The service selects the candidate's home, healthy generation, registered
    /// storage, process identity, immutable foreground configuration, and worker
    /// permit pair before the connector performs the authenticated WebSocket handshake.
    pub fn admit(
        &self,
        connector: &ManagedBackendClientConnector,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<AdmittedProjectionSession, ProjectionSessionAdmissionError> {
        if connector.launch_identity().is_some_and(|identity| {
            identity.runtime_id() != runtime_id
                || identity.process_generation() != process_generation
                || config_cwd.to_str() != Some(identity.working_directory().as_str())
        }) {
            return Err(ProjectionSessionAdmissionError::release_admission(
                runtime_id,
                process_generation,
                ManagedBackendError::ManagedLaunchIdentityMismatch,
            ));
        }
        let prepared = self.prepare_session_admission(runtime_id, process_generation)?;
        let mut backend = self.connect_and_initialize_candidate(
            connector,
            runtime_id,
            process_generation,
            timeout,
        )?;
        let release_admission = backend
            .admit_release(config_cwd, timeout)
            .map_err(|source| {
                ProjectionSessionAdmissionError::release_admission(
                    runtime_id,
                    process_generation,
                    source,
                )
            })?;
        if connector.launch_identity() != Some(release_admission.launch_identity()) {
            return Err(ProjectionSessionAdmissionError::release_admission(
                runtime_id,
                process_generation,
                ManagedBackendError::ManagedLaunchIdentityMismatch,
            ));
        }
        let connection =
            self.finish_session_admission(backend, runtime_id, process_generation, prepared)?;
        Ok(AdmittedProjectionSession::from_admitted_connection(
            connection,
        ))
    }

    /// Admits one lifecycle fixture without manufacturing managed-launch authority.
    #[cfg(any(test, feature = "test-faults"))]
    #[doc(hidden)]
    pub fn admit_lifecycle_test_candidate(
        &self,
        connector: &ManagedBackendClientConnector,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<AdmittedProjectionSession, ProjectionSessionAdmissionError> {
        if connector.launch_identity().is_some() {
            return Err(ProjectionSessionAdmissionError::release_admission(
                runtime_id,
                process_generation,
                ManagedBackendError::ManagedLaunchIdentityMismatch,
            ));
        }
        let prepared = self.prepare_session_admission(runtime_id, process_generation)?;
        let mut backend = self.connect_and_initialize_candidate(
            connector,
            runtime_id,
            process_generation,
            timeout,
        )?;
        backend
            .admit_release_non_authorizing_for_lifecycle_test(config_cwd, timeout)
            .map_err(|source| {
                ProjectionSessionAdmissionError::release_admission(
                    runtime_id,
                    process_generation,
                    source,
                )
            })?;
        let connection =
            self.finish_session_admission(backend, runtime_id, process_generation, prepared)?;
        Ok(AdmittedProjectionSession::from_admitted_connection(
            connection,
        ))
    }

    fn prepare_session_admission(
        &self,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Result<PreparedProjectionSessionAdmission, ProjectionSessionAdmissionError> {
        let command = self.command_authorizer.authorize().map_err(|_| {
            ProjectionSessionAdmissionError::service_closed(runtime_id, process_generation)
        })?;
        self.ensure_current().map_err(|source| {
            ProjectionSessionAdmissionError::connection_ownership(
                runtime_id,
                process_generation,
                source,
            )
        })?;
        self.connections.reap_finished_ordinary_retirements();
        let worker_permits = self.workers.try_acquire_pair().map_err(|error| {
            let source = match error {
                ProjectionWorkerPermitError::CapacityFull { available } => {
                    ProjectionCoordinatorError::ProjectionWorkerCapacityFull { available }
                }
                ProjectionWorkerPermitError::Poisoned => {
                    ProjectionCoordinatorError::ProjectionWorkerPoolPoisoned
                }
            };
            ProjectionSessionAdmissionError::connection_ownership(
                runtime_id,
                process_generation,
                source,
            )
        })?;
        Ok(PreparedProjectionSessionAdmission {
            command,
            home: Arc::clone(self.home.as_ref().expect("open service owns its home")),
            worker_permits,
        })
    }

    fn connect_and_initialize_candidate(
        &self,
        connector: &ManagedBackendClientConnector,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ProjectionSessionAdmissionError> {
        let mut backend = connector
            .connect_foreground_candidate(self.config.foreground(), timeout)
            .map_err(|source| {
                ProjectionSessionAdmissionError::candidate_connection(
                    runtime_id,
                    process_generation,
                    source,
                )
            })?;
        backend.initialize_foreground(timeout).map_err(|source| {
            ProjectionSessionAdmissionError::initialization(runtime_id, process_generation, source)
        })?;
        Ok(backend)
    }

    fn finish_session_admission(
        &self,
        backend: ManagedBackendSession,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        prepared: PreparedProjectionSessionAdmission,
    ) -> Result<Arc<ProjectionConnection>, ProjectionSessionAdmissionError> {
        let connection = ProjectionConnection::new(
            backend,
            runtime_id,
            process_generation,
            prepared.home,
            self.home_id,
            self.home_generation,
            self.storage,
            prepared.worker_permits,
            self.scheduler_signal.clone(),
            Arc::clone(&self.stop_coordinator),
            Arc::clone(
                self.context_compaction
                    .as_ref()
                    .expect("open service retains its context-compaction coordinator"),
            ),
            self.command_authorizer.clone(),
            self.persistent_failure
                .as_ref()
                .expect("open service retains its persistent-failure coordinator")
                .notification(),
            self.persistent_failure
                .as_ref()
                .expect("open service retains its persistent-failure coordinator")
                .terminal_disposer(self.home_id, self.home_generation),
        )
        .map_err(|source| {
            ProjectionSessionAdmissionError::connection_ownership(
                runtime_id,
                process_generation,
                source,
            )
        })?;
        self.register_connection(&connection);
        if !prepared.command.is_current() {
            connection.retire();
            return Err(ProjectionSessionAdmissionError::service_closed(
                runtime_id,
                process_generation,
            ));
        }
        Ok(connection)
    }

    pub(super) fn ensure_current(&self) -> Result<(), ProjectionCoordinatorError> {
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        if home.home_id() != self.home_id {
            return Err(ProjectionCoordinatorError::HomeIdentityMismatch {
                expected: self.home_id,
                actual: home.home_id(),
            });
        }
        let health = home.health();
        if health.state() != HomeHealthState::Healthy
            || health.generation() != Some(self.home_generation)
        {
            return Err(ProjectionCoordinatorError::HomeGenerationMismatch {
                expected: self.home_generation,
                actual: health.generation(),
                state: health.state(),
            });
        }
        self.storage
            .revision(home)
            .map_err(|source| ProjectionCoordinatorError::SyndicRevisionUnavailable { source })?;
        Ok(())
    }

    fn register_connection(&self, connection: &Arc<ProjectionConnection>) {
        self.connections.reap_finished_ordinary_retirements();
        let mut active = self
            .connections
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !active
            .iter()
            .any(|candidate| Arc::ptr_eq(candidate, connection))
        {
            active.push(Arc::clone(connection));
        }
    }
}
