use super::*;
use crate::cas_projection::LiveCommandAdmissionError;
#[cfg(test)]
use beryl_model::SyndicAcceptedInputId;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../tests/unit/admission_reopening.rs"]
mod admission_reopening_tests;

pub(super) enum PreparedStop {
    Exact {
        stopping: bool,
        target: syndic_storage::StopOperationTarget,
        connection: Arc<ProjectionConnection>,
        proof: super::super::connection::StopTargetProof,
    },
    Ineligible(StopAdmissionIneligibility),
}

impl ProjectionConnectionService {
    pub(crate) fn try_reopen_shutdown_admission(
        &self,
        fence: &crate::process_admission::ProcessAdmissionFence,
    ) -> Result<(), crate::process_admission::ProcessAdmissionReopenError> {
        let permit = self.command_authorizer.authorize()?;
        let home = self
            .home
            .as_deref()
            .ok_or(LiveCommandAdmissionError::Closed)?;
        if home.home_id() != self.home_id {
            return Err(crate::process_admission::ProcessAdmissionReopenError::StaleHome);
        }
        permit.reopen_process_admission(fence, home, self.home_generation)
    }

    pub fn lifecycle_yield_handler(
        &self,
        attention: &Arc<crate::lifecycle_attention::ProcessLifecycleAttentionPool>,
    ) -> super::super::stop::ProcessLifecycleYieldHandler {
        super::super::stop::ProcessLifecycleYieldHandler::new(
            Arc::downgrade(&self.stop_coordinator),
            Arc::downgrade(attention),
        )
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
            .ok_or(ProjectionCoordinatorError::HomeServiceUnavailable)?;
        active_steering::deliver(
            home,
            self.home_id,
            self.home_generation,
            self.storage.clone(),
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
    #[cfg(any(test, feature = "test-faults"))]
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
    #[cfg(any(test, feature = "test-faults"))]
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
        self.cancel_selected_continuation_for_window_close(thread_id)?;
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

    pub fn cancel_selected_continuation_for_window_close(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<(), StopCoordinationError> {
        self.context_compaction
            .as_ref()
            .ok_or(StopCoordinationError::HomeAuthorityLost)?
            .cancel_window_close_continuation(thread_id)
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
                ..
            } => (target, connection, proof),
            PreparedStop::Ineligible(reason) => {
                return Ok((StopCoordinationOutcome::Ineligible(reason), None));
            }
        };
        let outcome = self.coordinate_prepared_stop(connection, proof, cause)?;
        Ok((outcome, Some(target)))
    }

    pub(super) fn coordinate_prepared_stop(
        &self,
        connection: Arc<ProjectionConnection>,
        proof: super::super::connection::StopTargetProof,
        cause: StopCause,
    ) -> Result<StopCoordinationOutcome, StopCoordinationError> {
        self.exact_stop_read()
            .coordinate_prepared_stop(connection, proof, cause)
    }

    pub(super) fn prepare_stop(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<PreparedStop, StopCoordinationError> {
        self.exact_stop_read().prepare_stop(thread_id)
    }

    #[cfg(feature = "test-faults")]
    pub fn has_local_stop_for_test(&self, thread: SyndicThreadId) -> bool {
        self.stop_coordinator.has_local_stop_for_test(thread)
    }

    /// Returns the registered Syndic handle paired with this owned home.
    #[must_use]
    pub fn storage(&self) -> SyndicStorage {
        self.storage.clone()
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
