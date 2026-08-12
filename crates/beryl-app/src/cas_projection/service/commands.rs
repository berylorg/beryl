use super::*;

enum PreparedStop {
    Exact {
        target: syndic_storage::StopOperationTarget,
        connection: Arc<ProjectionConnection>,
        proof: super::super::connection::StopTargetProof,
    },
    Ineligible(StopAdmissionIneligibility),
}

impl ProjectionConnectionService {
    /// Builds, admits, and executes one direct idle new-turn submission.
    ///
    /// The final reserve query is synchronous and immediately precedes writer admission. Any
    /// non-sufficient outcome leaves the caller's draft and editor projection untouched.
    pub fn execute_idle_submission(
        &self,
        assets: beryl_state::AssetState,
        submission: syndic_storage::IdleSubmission,
    ) -> Result<(), IdleSubmissionExecutionError> {
        let command = self
            .command_authorizer
            .authorize()
            .map_err(|_| IdleSubmissionExecutionError::ServiceClosed)?;
        self.ensure_current()?;
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        let prepared = crate::input_admission::idle_submission_command(
            home,
            self.storage,
            assets,
            submission,
        )?;
        if !command.is_current() {
            return Err(IdleSubmissionExecutionError::ServiceClosed);
        }
        let free_space = home.query_free_space(self.config.turn_start_admission_requirement());
        if !matches!(
            free_space,
            beryl_home_store::FreeSpaceOutcome::Sufficient { .. }
        ) {
            return Err(IdleSubmissionExecutionError::FreeSpace(free_space));
        }
        match home.execute(prepared) {
            beryl_home_store::CommandOutcome::NotCommitted { evidence } => {
                Err(IdleSubmissionExecutionError::Command(evidence))
            }
            beryl_home_store::CommandOutcome::Committed {
                receipt: _,
                later_failure: None,
            } => Ok(()),
            beryl_home_store::CommandOutcome::Committed {
                receipt,
                later_failure: Some(later_failure),
            } => Err(IdleSubmissionExecutionError::CommandCommitted {
                receipt,
                later_failure,
            }),
            beryl_home_store::CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                reconciliation.install();
                Err(IdleSubmissionExecutionError::CommandIndeterminate { failure })
            }
        }
    }

    /// Executes and reconciles one exact accepted-input publication.
    ///
    /// Exact durable acceptance wakes automatic steering only after receipt
    /// reconciliation has completed.
    pub fn execute_accepted_input_admission(
        &self,
        prepared: PreparedAcceptedInputAdmission,
    ) -> Result<(), AcceptedInputAdmissionExecutionError> {
        let command = self
            .command_authorizer
            .authorize()
            .map_err(|_| AcceptedInputAdmissionExecutionError::ServiceClosed)?;
        self.ensure_current()?;
        if prepared.home_id != self.home_id {
            return Err(ProjectionCoordinatorError::HomeIdentityMismatch {
                expected: self.home_id,
                actual: prepared.home_id,
            }
            .into());
        }
        if prepared.home_generation != self.home_generation {
            return Err(ProjectionCoordinatorError::HomeGenerationMismatch {
                expected: self.home_generation,
                actual: Some(prepared.home_generation),
                state: HomeHealthState::Healthy,
            }
            .into());
        }
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        if !command.is_current() {
            return Err(AcceptedInputAdmissionExecutionError::ServiceClosed);
        }
        self.ensure_current()?;
        let dispatch = home.execute(prepared.command);
        #[cfg(test)]
        self.pause_admission_reconciliation_if_requested(prepared.admission.accepted_input_id());
        match dispatch {
            beryl_home_store::CommandOutcome::NotCommitted { evidence } => {
                Err(AcceptedInputAdmissionExecutionError::Command(evidence))
            }
            beryl_home_store::CommandOutcome::Committed {
                receipt: _,
                later_failure: None,
            } => {
                self.scheduler_signal
                    .wake(AcceptedInputWakeReason::AcceptedReady);
                self.scheduler_signal
                    .wake(AcceptedInputWakeReason::AcceptedNextReady);
                Ok(())
            }
            beryl_home_store::CommandOutcome::Committed {
                receipt,
                later_failure: Some(later_failure),
            } => Err(AcceptedInputAdmissionExecutionError::CommandCommitted {
                receipt,
                later_failure,
            }),
            beryl_home_store::CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                reconciliation.install();
                Err(AcceptedInputAdmissionExecutionError::CommandIndeterminate { failure })
            }
        }
    }

    /// Runs one bounded exact steering-delivery attempt on the caller's non-GPUI worker.
    #[cfg(test)]
    pub(in crate::cas_projection) fn deliver_active_steering_input(
        &self,
        target: &LiveEventTarget,
        input_id: SyndicAcceptedInputId,
        cancellation: &ProjectionCancellationToken,
        request_timeout: Duration,
    ) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
        let _command = self
            .command_authorizer
            .authorize()
            .map_err(|_| ActiveSteeringDeliveryError::ServiceClosed)?;
        self.ensure_current()?;
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        active_steering::deliver(
            home,
            self.home_id,
            self.home_generation,
            self.storage,
            &self.workers,
            target,
            input_id,
            cancellation,
            request_timeout,
        )
    }

    /// Records the first lifecycle-yield outcome for one exact executing Syndic turn.
    ///
    /// The state is owned by the healthy-home process service rather than a window. A phase-
    /// continuation outcome is refused when the same exact turn is already durably stopping.
    pub fn record_lifecycle_yield_outcome(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        outcome: crate::LifecycleYieldOutcome,
    ) -> Result<bool, StopCoordinationError> {
        self.stop_coordinator
            .record_lifecycle_yield(thread_id, turn_id, outcome)
    }

    /// Admits or joins one exact durable context-compaction operation.
    ///
    /// The caller must be a non-GPUI worker. Expiring the shared deadline returns
    /// `StillRunning` while the process coordinator continues exact convergence.
    pub fn compact_thread(
        &self,
        request: super::super::ContextCompactionRequest,
    ) -> Result<super::super::ContextCompactionOutcome, super::super::ContextCompactionError> {
        let _command = self
            .command_authorizer
            .authorize()
            .map_err(|_| super::super::ContextCompactionError::Unavailable)?;
        self.context_compaction
            .as_ref()
            .ok_or(super::super::ContextCompactionError::Unavailable)?
            .compact_thread(request)
    }

    /// Consumes one process-owned lifecycle-yield outcome after exact terminal observation.
    ///
    /// Stop admission removes only a matching automatic phase continuation. Other terminal
    /// notification outcomes remain available to the later GUI integration phase.
    pub fn take_terminal_lifecycle_yield_outcome(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<Option<crate::LifecycleYieldOutcome>, StopCoordinationError> {
        self.stop_coordinator
            .take_terminal_lifecycle_yield(thread_id, turn_id)
    }

    /// Admits or joins deliberate control of one exact selected provider operation.
    ///
    /// This synchronous boundary performs storage and transport waits and must run on a non-GPUI
    /// worker. Matching acceptance remains [`StopCoordinationOutcome::Stopping`] until ordinary
    /// terminal or authority-loss convergence consumes the durable stop.
    pub fn stop_selected_operation(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<StopCoordinationOutcome, StopCoordinationError> {
        self.coordinate_stop(thread_id, StopCause::SelectedOperationControl)
            .map(|(outcome, _)| outcome)
    }

    /// Admits or joins the diagnostic cause for one exact selected provider operation.
    pub fn stop_selected_operation_for_diagnostics(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<StopCoordinationOutcome, StopCoordinationError> {
        self.coordinate_stop(thread_id, StopCause::DiagnosticControl)
            .map(|(outcome, _)| outcome)
    }

    /// Admits or joins healthy-home window-close ownership of one exact selected operation.
    ///
    /// A waiting result owns an exact non-cloneable barrier. The non-GUI caller must retain its
    /// thread claim until that barrier reports terminal-history or authority-loss convergence.
    pub fn stop_selected_operation_for_window_close(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<WindowCloseStopOutcome, StopCoordinationError> {
        let (outcome, target) =
            self.coordinate_stop(thread_id, StopCause::HealthyHomeWindowClose)?;
        Ok(match outcome {
            StopCoordinationOutcome::Stopping {
                operation_id,
                primary_owner,
            } => WindowCloseStopOutcome::Waiting(WindowCloseStopBarrier::new(
                Arc::clone(&self.stop_coordinator),
                operation_id,
                target
                    .as_ref()
                    .expect("a stopping outcome retains its exact target")
                    .turn_id(),
                primary_owner,
            )),
            StopCoordinationOutcome::Abandoned { operation_id } => {
                WindowCloseStopOutcome::Waiting(WindowCloseStopBarrier::new(
                    Arc::clone(&self.stop_coordinator),
                    operation_id,
                    target
                        .as_ref()
                        .expect("an abandoned stop outcome retains its exact target")
                        .turn_id(),
                    true,
                ))
            }
            StopCoordinationOutcome::SafelyReopened { operation_id } => {
                WindowCloseStopOutcome::SafelyReopened { operation_id }
            }
            StopCoordinationOutcome::Ineligible(reason) => {
                WindowCloseStopOutcome::Ineligible(reason)
            }
        })
    }

    fn coordinate_stop(
        &self,
        thread_id: SyndicThreadId,
        cause: StopCause,
    ) -> Result<
        (
            StopCoordinationOutcome,
            Option<syndic_storage::StopOperationTarget>,
        ),
        StopCoordinationError,
    > {
        let (target, connection, proof) = match self.prepare_stop(thread_id)? {
            PreparedStop::Exact {
                target,
                connection,
                proof,
            } => (target, connection, proof),
            PreparedStop::Ineligible(reason) => {
                return Ok((StopCoordinationOutcome::Ineligible(reason), None));
            }
        };
        let outcome = match connection.coordinate_stop(&self.stop_coordinator, proof, cause)? {
            StopOwnership::Primary(owner) => connection.dispatch_exact_stop(owner),
            StopOwnership::Joined {
                operation_id,
                interruption: _,
            } => Ok(StopCoordinationOutcome::Stopping {
                operation_id,
                primary_owner: false,
            }),
        }?;
        Ok((outcome, Some(target)))
    }

    fn prepare_stop(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<PreparedStop, StopCoordinationError> {
        let command = self
            .command_authorizer
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        let home = self
            .home
            .as_deref()
            .ok_or(StopCoordinationError::HomeAuthorityLost)?;
        let read = self.storage.stop_admission_read(
            home,
            thread_id,
            SyndicPointReadLimit::new(1_000_000)
                .expect("stop coordination point-read bound is nonzero"),
        )?;
        let target = match read {
            StopAdmissionRead::Admissible(candidate) => candidate.target().clone(),
            StopAdmissionRead::Stopping(live) => live.target().clone(),
            StopAdmissionRead::Ineligible(reason) => {
                return Ok(PreparedStop::Ineligible(reason));
            }
        };
        let (connection, proof) = self.stop_connection(&target)?;
        if !command.is_current() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        Ok(PreparedStop::Exact {
            target,
            connection,
            proof,
        })
    }

    fn stop_connection(
        &self,
        target: &syndic_storage::StopOperationTarget,
    ) -> Result<
        (
            Arc<ProjectionConnection>,
            super::super::connection::StopTargetProof,
        ),
        StopCoordinationError,
    > {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| StopCoordinationError::TargetUnavailable)?;
        let mut found = None;
        let mut duplicate = false;
        connections.retain(|connection| {
            if connection.is_detached() {
                return false;
            }
            if connection.runtime_id() == target.runtime_id()
                && connection.process_generation() == target.loaded_generation().process()
                && let Ok(proof) = connection.stop_target(target)
            {
                if found.is_some() {
                    duplicate = true;
                    return true;
                }

                found = Some((Arc::clone(connection), proof));
            }
            true
        });
        if duplicate {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        found.ok_or(StopCoordinationError::TargetUnavailable)
    }

    /// Returns the registered Syndic handle paired with this owned home.
    #[must_use]
    pub const fn storage(&self) -> SyndicStorage {
        self.storage
    }

    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    /// Returns the process-local incarnation of this projection service.
    #[must_use]
    pub const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    /// Supplies the process shell with the same master gate used by projection workers.
    ///
    /// Draft, input-admission, catalog, and other store-dependent workers must retain one scoped
    /// permit through their complete preparation, execution, and publication boundary.
    #[must_use]
    pub(in crate::cas_projection) fn live_command_authorizer(&self) -> LiveCommandAuthorizer {
        self.command_authorizer.clone()
    }

    /// Admits one scoped store-dependent process-shell command.
    ///
    /// This is the only public path from the projection service to its owned
    /// `HomeStore`. The returned borrow cannot outlive the master-gate permit.
    pub fn live_home_command(
        &self,
    ) -> Result<LiveHomeCommand<'_>, super::super::persistent_failure::LiveCommandAdmissionError>
    {
        let permit = self.command_authorizer.authorize()?;
        let home = self
            .home
            .as_deref()
            .ok_or(super::super::persistent_failure::LiveCommandAdmissionError::Closed)?;
        Ok(LiveHomeCommand {
            home,
            _permit: permit,
        })
    }
}
