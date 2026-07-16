use std::{sync::atomic::Ordering, time::Duration};

use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse, TurnStartOptions, UserInput};
use beryl_model::{CasLoadedSessionGeneration, CasThreadId, CasTurnId, SyndicThreadId};

use super::super::{ProjectionConnection, TargetTurnStartOutcome, turn_start_allows_not_started};
use super::{
    LiveEventPoll, LiveEventTarget, LiveEventTargetCloseReason, LiveEventTargetError,
    LiveEventTargetHandoffError, TargetHandoffRequirement,
};
use crate::cas_projection::{LoadedCasProjection, ProjectionExecutionError};

impl LiveEventTarget {
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

    pub(in crate::cas_projection) fn start_turn(
        &self,
        input: Vec<UserInput>,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> Result<TargetTurnStartOutcome, ProjectionExecutionError> {
        self.connection
            .start_target_turn(self.registration(), input, options, timeout)
    }

    pub(in crate::cas_projection) fn respond_dynamic_tool_call(
        &self,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), ProjectionExecutionError> {
        self.connection
            .respond_target_dynamic_tool_call(self.registration(), request, response)
    }

    pub(in crate::cas_projection) fn into_not_started_projection(
        self,
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
        self,
    ) -> Result<LoadedCasProjection, LiveEventTargetHandoffError> {
        self.into_projection(TargetHandoffRequirement::ProvenTerminal)
    }

    /// Confirms the one-way CAS-turn binding observed from a request response.
    ///
    /// A prior matching `turn/started` event makes this idempotent. A different
    /// identity closes the target and revokes its projection authority.
    pub fn confirm_turn(&self, turn_id: CasTurnId) -> Result<(), LiveEventTargetError> {
        let registration = self.registration();
        self.connection.confirm_target_turn(registration, turn_id)
    }

    /// Waits up to `timeout` for one routed event or a terminal close reason.
    ///
    /// [`LiveEventPoll::Quiet`] is an active state and never retires the target.
    #[must_use]
    pub fn poll(&self, timeout: Duration) -> LiveEventPoll {
        let registration = self.registration();
        match registration.receiver.recv_timeout(timeout) {
            Ok(queued) => {
                registration.queued_count.fetch_sub(1, Ordering::AcqRel);
                registration
                    .queued_bytes
                    .fetch_sub(queued.retained_bytes, Ordering::AcqRel);
                LiveEventPoll::Event(queued.event)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => LiveEventPoll::Quiet,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => LiveEventPoll::Closed(
                registration
                    .terminal
                    .lock()
                    .map(|terminal| terminal.unwrap_or(LiveEventTargetCloseReason::WorkerStopped))
                    .unwrap_or(LiveEventTargetCloseReason::WorkerStopped),
            ),
        }
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
        mut self,
        requirement: TargetHandoffRequirement,
    ) -> Result<LoadedCasProjection, LiveEventTargetHandoffError> {
        self.connection
            .handoff_target(self.registration(), requirement)?;
        self.registration.take();
        Ok(self
            .projection
            .take()
            .expect("live-event target retains its projection until handoff"))
    }
}

impl Drop for LiveEventTarget {
    fn drop(&mut self) {
        if let Some(registration) = self.registration.take() {
            self.connection.abandon_target(&registration);
        }
        self.projection.take();
    }
}
