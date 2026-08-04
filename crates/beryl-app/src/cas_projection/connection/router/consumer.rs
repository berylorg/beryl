use std::{sync::atomic::Ordering, time::Duration};

use beryl_backend::{
    ClientUserMessageId, CompactionAttemptCorrelation, DynamicToolCallResponse, TurnStartOptions,
    TurnSteerOutcome,
};
use beryl_model::{CasLoadedSessionGeneration, CasThreadId, SyndicThreadId};
use syndic_storage::{SyndicDeliveringSteeringInput, SyndicReadySteeringInput};

use super::super::{
    ConnectionCommandOutcome, ExactContextCompactionDispatch, ProjectionConnection,
    StreamedInputBrokerService, TargetTurnStartOutcome,
    provider_broker::{
        ActiveBindingLossDisposition, CheckedSteeringLifecycleArmError,
        CheckedSteeringLifecycleOwner, ProviderBrokerLossError, ProviderBrokerLossOutcome,
    },
    turn_start_allows_not_started,
};
#[cfg(test)]
use super::TargetRegistrationProof;
use super::{
    ActiveSteeringAttemptAcquireError, ActiveSteeringAttemptPermit, LiveEventPoll, LiveEventTarget,
    LiveEventTargetCloseReason, LiveEventTargetHandoffError, LiveEventTargetLossError,
    LiveEventTargetLossOutcome, RoutedDynamicToolResponse, RoutedTargetOperation,
    TargetAuthorizationFailure, TargetHandoffRequirement, TargetRegistration, TargetTerminalSignal,
};
use crate::cas_projection::{
    LoadedCasProjection, ProjectionCoordinatorError, ProjectionExecutionError,
};

#[cfg(test)]
pub(in crate::cas_projection) struct ActiveSteeringRaceProbe {
    connection: std::sync::Arc<ProjectionConnection>,
    registration: TargetRegistrationProof,
}

#[cfg(test)]
impl ActiveSteeringRaceProbe {
    pub(in crate::cas_projection) fn close_checked_steering_lifecycles(&self) {
        self.connection
            .close_checked_steering_lifecycles_for_test()
            .expect("steering race probe retains an attached connection");
    }

    pub(in crate::cas_projection) fn converge_target_loss(
        &self,
    ) -> Result<bool, ProviderBrokerLossError> {
        Ok(matches!(
            self.connection.converge_settled_active_steering_loss(
                &self.registration,
                syndic_storage::TurnIncompleteReason::AuthorityLost,
            )?,
            ProviderBrokerLossOutcome::Incomplete
        ))
    }

    pub(in crate::cas_projection) fn target_loss_requested(&self) -> bool {
        self.connection
            .current_router()
            .expect("test target retains an attached service epoch")
            .target_loss_requested_for_test(&self.registration)
    }
}

impl LiveEventTarget {
    pub(in crate::cas_projection) fn dispatch_context_compaction(
        &self,
        attempt: CompactionAttemptCorrelation,
        timeout: Duration,
    ) -> Result<
        Result<
            ConnectionCommandOutcome<ExactContextCompactionDispatch>,
            TargetAuthorizationFailure,
        >,
        ProjectionCoordinatorError,
    > {
        self.connection
            .compact_target(&self.registration().proof(), attempt, timeout)
    }

    pub(in crate::cas_projection) fn new(
        projection: LoadedCasProjection,
        connection: std::sync::Arc<ProjectionConnection>,
        registration: super::TargetRegistration,
    ) -> Self {
        Self {
            projection: Some(projection),
            connection,
            registration: Some(registration),
        }
    }

    /// Returns the durable Syndic thread owning this target.
    #[must_use]
    pub fn syndic_thread_id(&self) -> SyndicThreadId {
        self.projection().syndic_thread_id()
    }

    /// Returns the exact CAS thread accepted by this target.
    #[must_use]
    pub fn cas_thread_id(&self) -> &CasThreadId {
        self.projection().cas_thread_id()
    }

    /// Returns the exact managed-process and loaded-thread generation pair.
    #[must_use]
    pub fn loaded_session_generation(&self) -> CasLoadedSessionGeneration {
        self.projection().loaded_session_generation()
    }

    pub(in crate::cas_projection) fn home_id(&self) -> beryl_model::BerylHomeId {
        self.projection().home_id()
    }

    pub(in crate::cas_projection) fn home_generation(&self) -> beryl_home_store::HomeGeneration {
        self.projection().home_generation()
    }

    pub(in crate::cas_projection) fn stop_coordinator(
        &self,
    ) -> Result<
        std::sync::Arc<crate::cas_projection::stop::StopCoordinator>,
        ProjectionCoordinatorError,
    > {
        self.connection.stop_coordinator()
    }

    pub(in crate::cas_projection) fn context_compaction_coordinator(
        &self,
    ) -> Result<
        std::sync::Arc<crate::cas_projection::context_compaction::ContextCompactionCoordinator>,
        ProjectionCoordinatorError,
    > {
        self.connection.context_compaction_coordinator()
    }

    pub(in crate::cas_projection) fn accepted_next_ready_notifier(
        &self,
    ) -> super::AcceptedNextReadyNotifier {
        self.connection
            .current_router()
            .expect("a live event target retains an attached service epoch")
            .accepted_next_ready_notifier()
    }

    pub(in crate::cas_projection) fn start_streamed_turn(
        &self,
        options: TurnStartOptions,
        timeout: Duration,
        service: impl StreamedInputBrokerService,
    ) -> Result<TargetTurnStartOutcome, ProjectionExecutionError> {
        self.connection
            .start_target_streamed_turn(self.registration(), options, timeout, service)
    }

    pub(in crate::cas_projection) fn acquire_active_steering_attempt(
        &self,
        ready: &SyndicReadySteeringInput,
    ) -> Result<ActiveSteeringAttemptPermit, ActiveSteeringAttemptAcquireError> {
        self.connection
            .current_router()
            .map_err(|_| ActiveSteeringAttemptAcquireError::Router)?
            .acquire_active_steering_attempt(
                &self.registration().proof(),
                ready.target(),
                ready.loaded_generation(),
            )
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn active_steering_capability(
        &self,
        ready: &SyndicReadySteeringInput,
    ) -> Result<
        crate::cas_projection::active_steering::ActiveSteeringTarget,
        super::ActiveSteeringTargetLookupError,
    > {
        crate::cas_projection::active_steering::ActiveSteeringTarget::lookup_exact_registration(
            std::sync::Arc::clone(&self.connection),
            ready,
            self.registration().registration(),
        )
    }

    pub(in crate::cas_projection) fn arm_checked_steering_lifecycle(
        &self,
        attempt: &ActiveSteeringAttemptPermit,
        route: &SyndicDeliveringSteeringInput,
        correlation: &ClientUserMessageId,
    ) -> Result<CheckedSteeringLifecycleOwner, CheckedSteeringLifecycleArmError> {
        self.connection.arm_active_steering_lifecycle(
            attempt,
            route,
            self.home_generation(),
            correlation,
        )
    }

    pub(in crate::cas_projection) fn steer_streamed_input(
        &self,
        attempt: &ActiveSteeringAttemptPermit,
        correlation: ClientUserMessageId,
        timeout: Duration,
        service: impl StreamedInputBrokerService,
    ) -> Result<
        Result<ConnectionCommandOutcome<TurnSteerOutcome>, TargetAuthorizationFailure>,
        ProjectionCoordinatorError,
    > {
        let registration = self.registration().proof();
        self.connection.steer_target_streamed_input(
            &registration,
            attempt,
            correlation,
            timeout,
            service,
        )
    }

    pub(in crate::cas_projection) fn converge_active_steering_loss(
        &self,
        attempt: ActiveSteeringAttemptPermit,
        owner: CheckedSteeringLifecycleOwner,
        disposition: ActiveBindingLossDisposition,
        cause: syndic_storage::TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.connection
            .converge_owned_active_steering_loss(attempt, owner, disposition, cause)
    }

    pub(in crate::cas_projection) fn converge_settled_active_steering_loss(
        &self,
        cause: syndic_storage::TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.connection
            .converge_settled_active_steering_loss(&self.registration().proof(), cause)
    }

    pub(in crate::cas_projection) fn converge_unarmed_active_steering_loss(
        &self,
        attempt: ActiveSteeringAttemptPermit,
        disposition: ActiveBindingLossDisposition,
        cause: syndic_storage::TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.connection
            .converge_unarmed_active_steering_loss(attempt, disposition, cause)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn active_steering_race_probe_for_test(
        &self,
    ) -> ActiveSteeringRaceProbe {
        ActiveSteeringRaceProbe {
            connection: std::sync::Arc::clone(&self.connection),
            registration: self.registration().proof(),
        }
    }

    pub(in crate::cas_projection) fn respond_dynamic_tool_call(
        &self,
        call: RoutedDynamicToolResponse,
        response: DynamicToolCallResponse,
    ) -> Result<(), ProjectionExecutionError> {
        self.connection
            .respond_target_dynamic_tool_call(self.registration(), call, response)
    }

    pub(in crate::cas_projection) fn converge_source_loss(
        self,
        cause: syndic_storage::TurnIncompleteReason,
    ) -> Result<LiveEventTargetLossOutcome, LiveEventTargetLossError> {
        let outcome = self
            .connection
            .converge_target_loss(self.registration(), cause)
            .map_err(|source| LiveEventTargetLossError::Broker {
                source: Box::new(source),
            })?;
        Ok(match outcome {
            super::super::provider_broker::ProviderBrokerLossOutcome::Incomplete => {
                LiveEventTargetLossOutcome::Incomplete
            }
            super::super::provider_broker::ProviderBrokerLossOutcome::ProvenTerminal(outcome) => {
                LiveEventTargetLossOutcome::ProvenTerminal {
                    target: self,
                    outcome,
                }
            }
        })
    }

    pub(in crate::cas_projection) fn into_not_started_projection(
        &mut self,
        start: &TargetTurnStartOutcome,
    ) -> Result<LoadedCasProjection, LiveEventTargetHandoffError> {
        if !start.belongs_to(
            self.connection.authority.generation,
            self.registration().registration(),
        ) {
            return Err(LiveEventTargetHandoffError::TurnStartOutcomeTargetMismatch);
        }
        if !turn_start_allows_not_started(start.outcome()) {
            return Err(LiveEventTargetHandoffError::TurnStartOutcomeNotReusable);
        }
        self.into_projection(TargetHandoffRequirement::NotStarted)
    }

    pub(in crate::cas_projection) fn into_proven_terminal_projection(
        &mut self,
    ) -> Result<LoadedCasProjection, LiveEventTargetHandoffError> {
        self.into_projection(TargetHandoffRequirement::ProvenTerminal)
    }

    pub(in crate::cas_projection) fn into_context_compaction_nondispatch_projection(
        &mut self,
    ) -> Result<LoadedCasProjection, LiveEventTargetHandoffError> {
        self.into_projection(TargetHandoffRequirement::CompactionNotDispatched)
    }

    pub(in crate::cas_projection) fn retire_context_compaction_connection(self) {
        self.connection.retire();
        drop(self);
    }

    /// Waits for one feature-owned operation, durable terminal outcome, or close reason.
    ///
    /// [`LiveEventPoll::Quiet`] is an active state and never retires the target.
    #[must_use]
    pub fn poll(&self, timeout: Duration) -> LiveEventPoll {
        self.registration().poll(timeout)
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn abandon_receiver_for_test(&mut self) {
        let (_, replacement) = std::sync::mpsc::sync_channel(1);
        self.registration
            .as_mut()
            .expect("live-event target retains its registration until drop")
            .receiver = replacement;
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn provider_test_key(
        &self,
    ) -> crate::cas_projection::test_faults::ProviderTestKey {
        self.connection.provider_test_key()
    }

    fn projection(&self) -> &LoadedCasProjection {
        self.projection
            .as_ref()
            .expect("live-event target retains its projection until drop")
    }

    fn registration(&self) -> &super::TargetRegistration {
        self.registration
            .as_ref()
            .expect("live-event target retains its registration until drop")
    }

    fn into_projection(
        &mut self,
        requirement: TargetHandoffRequirement,
    ) -> Result<LoadedCasProjection, LiveEventTargetHandoffError> {
        let surrender = self
            .projection()
            .prepare_preactivation_surrender()
            .map_err(
                |source| LiveEventTargetHandoffError::PreactivationSurrenderUnavailable { source },
            )?;
        self.connection
            .handoff_target(self.registration(), requirement)?;
        self.registration.take();
        let mut projection = self
            .projection
            .take()
            .expect("live-event target retains its projection until handoff");
        projection.install_preactivation_surrender(surrender);
        Ok(projection)
    }
}

impl TargetRegistration {
    #[must_use]
    pub(super) fn poll(&self, timeout: Duration) -> LiveEventPoll {
        match self.receiver.recv_timeout(timeout) {
            Ok(queued) => {
                self.queued_operations.fetch_sub(1, Ordering::AcqRel);
                match queued.operation {
                    RoutedTargetOperation::Approval(approval) => LiveEventPoll::Approval(approval),
                    RoutedTargetOperation::DynamicTool(call) => LiveEventPoll::DynamicTool(call),
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => LiveEventPoll::Quiet,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => match self.terminal.lock() {
                Ok(terminal) => match *terminal {
                    TargetTerminalSignal::Proven(outcome) => LiveEventPoll::ProvenTerminal(outcome),
                    TargetTerminalSignal::Closed(reason) => LiveEventPoll::Closed(reason),
                    TargetTerminalSignal::Open => {
                        LiveEventPoll::Closed(LiveEventTargetCloseReason::WorkerStopped)
                    }
                },
                Err(_) => LiveEventPoll::Closed(LiveEventTargetCloseReason::WorkerStopped),
            },
        }
    }
}

impl Drop for LiveEventTarget {
    fn drop(&mut self) {
        if let (Some(registration), Some(projection)) =
            (self.registration.take(), self.projection.take())
        {
            self.connection
                .settle_abandoned_target_projection(&registration, projection);
        }
    }
}
