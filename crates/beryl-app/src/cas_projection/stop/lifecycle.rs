use crate::cas_projection::context_compaction::coordinator::custody::CompactionCustodyReservation;
use beryl_backend::DynamicToolCallResponse;

use super::*;
use crate::{
    LifecycleYieldOutcome, LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::OrdinaryDynamicToolContext,
    lifecycle_attention::{LifecycleAttentionAttempt, ProcessLifecycleAttentionPool},
};

#[derive(Clone)]
pub struct ProcessLifecycleYieldHandler {
    stop: Weak<StopCoordinator>,
    attention: Weak<ProcessLifecycleAttentionPool>,
}

impl ProcessLifecycleYieldHandler {
    pub(in crate::cas_projection) fn new(
        stop: Weak<StopCoordinator>,
        attention: Weak<ProcessLifecycleAttentionPool>,
    ) -> Self {
        Self { stop, attention }
    }
}

impl LifecycleYieldRequestHandler for ProcessLifecycleYieldHandler {
    fn respond_lifecycle_yield(
        &mut self,
        context: OrdinaryDynamicToolContext,
        request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        let Some(stop) = self.stop.upgrade() else {
            return unavailable_response();
        };
        if !context.belongs_to(
            stop.home_id,
            stop.home_generation,
            stop.commands.service_generation(),
        ) {
            return unavailable_response();
        }
        match stop.accept_lifecycle_yield(
            context.thread_id(),
            context.turn_id(),
            request.outcome(),
            &self.attention,
        ) {
            Ok(true) => {
                crate::dispatch_beryl_lifecycle_dynamic_tool_request(request).into_response()
            }
            Ok(false) => DynamicToolCallResponse::failure_text(
                "Lifecycle yield was not accepted for this exact turn.",
            ),
            Err(_) => unavailable_response(),
        }
    }
}

fn unavailable_response() -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text("Lifecycle yield authority is unavailable.")
}

pub(in crate::cas_projection) struct AcceptedLifecycleYield {
    outcome: LifecycleYieldOutcome,
    continuation_pending: bool,
    compaction_turn_id: Option<SyndicTurnId>,
    attention: Weak<ProcessLifecycleAttentionPool>,
    attempt: Option<LifecycleAttentionAttempt>,
    reservation: Option<CompactionCustodyReservation>,
}

impl AcceptedLifecycleYield {
    pub(super) fn new(
        home_id: BerylHomeId,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        outcome: LifecycleYieldOutcome,
        attention: Weak<ProcessLifecycleAttentionPool>,
        reservation: Option<CompactionCustodyReservation>,
    ) -> Self {
        let attempt = attention
            .upgrade()
            .and_then(|pool| pool.track_accepted_yield(home_id, thread_id, turn_id, outcome));
        Self {
            outcome,
            continuation_pending: outcome == LifecycleYieldOutcome::PhaseContinue,
            compaction_turn_id: None,
            attention,
            attempt,
            reservation,
        }
    }

    pub(in crate::cas_projection) fn cancel_continuation(&mut self) {
        self.continuation_pending = false;
    }

    pub(super) fn is_continuation(&self) -> bool {
        self.outcome == LifecycleYieldOutcome::PhaseContinue
    }

    pub(super) fn owns_compaction_turn(&self, turn_id: SyndicTurnId) -> bool {
        self.compaction_turn_id == Some(turn_id)
    }

    pub(in crate::cas_projection) fn effective_outcome(&self) -> Option<LifecycleYieldOutcome> {
        if self.outcome == LifecycleYieldOutcome::PhaseContinue && !self.continuation_pending {
            None
        } else {
            Some(self.outcome)
        }
    }
}

impl Drop for AcceptedLifecycleYield {
    fn drop(&mut self) {
        if self.continuation_pending {
            if let (Some(pool), Some(attempt)) = (self.attention.upgrade(), self.attempt.as_ref()) {
                let _ = pool.report_continuation_failure(attempt);
            }
        }
    }
}

impl StopCoordinator {
    #[cfg(any(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn share_continuation_custody(
        &self,
        thread_id: SyndicThreadId,
        yielding_turn_id: SyndicTurnId,
        compaction_turn_id: SyndicTurnId,
    ) -> Result<CompactionCustodyReservation, StopCoordinationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let accepted = state
            .lifecycle_yields
            .get(&LifecycleYieldKey {
                thread_id,
                turn_id: yielding_turn_id,
            })
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if !accepted.is_continuation()
            || accepted
                .compaction_turn_id
                .is_some_and(|existing| existing != compaction_turn_id)
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        accepted
            .reservation
            .as_ref()
            .map(CompactionCustodyReservation::share)
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)
    }

    pub(in crate::cas_projection) fn bind_lifecycle_compaction(
        &self,
        thread_id: SyndicThreadId,
        yielding_turn_id: SyndicTurnId,
        compaction_turn_id: SyndicTurnId,
    ) -> Result<(), StopCoordinationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let accepted = state
            .lifecycle_yields
            .get_mut(&LifecycleYieldKey {
                thread_id,
                turn_id: yielding_turn_id,
            })
            .ok_or(StopCoordinationError::LocalAuthorityMismatch)?;
        if !accepted.is_continuation()
            || accepted
                .compaction_turn_id
                .is_some_and(|existing| existing != compaction_turn_id)
        {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        accepted.compaction_turn_id = Some(compaction_turn_id);
        Ok(())
    }

    pub(in crate::cas_projection) fn dynamic_tool_context(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> OrdinaryDynamicToolContext {
        OrdinaryDynamicToolContext::new(
            self.home_id,
            self.home_generation,
            self.commands.service_generation(),
            thread_id,
            turn_id,
        )
    }

    fn accept_lifecycle_yield(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        outcome: LifecycleYieldOutcome,
        attention: &Weak<ProcessLifecycleAttentionPool>,
    ) -> Result<bool, StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let key = LifecycleYieldKey { thread_id, turn_id };
        if self.lifecycle_registration_turn(&*self.current_home()?, thread_id)? != Some(turn_id)
            || state.lifecycle_yields.contains_key(&key)
        {
            return Ok(false);
        }
        if outcome == LifecycleYieldOutcome::PhaseContinue
            && (state.cancelled_continuations.get(&thread_id) == Some(&turn_id)
                || match self.read(thread_id)? {
                    StopAdmissionRead::Stopping(live) => live.target().turn_id() == turn_id,
                    StopAdmissionRead::Admissible(_) | StopAdmissionRead::Ineligible(_) => false,
                })
        {
            return Ok(false);
        }
        if !command.is_current() || state.persistent_failure.is_some() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        let reservation = if outcome == LifecycleYieldOutcome::PhaseContinue {
            let Some(reservation) = self.compaction_custody.reserve() else {
                return Ok(false);
            };
            Some(reservation)
        } else {
            None
        };
        state.lifecycle_yields.insert(
            key,
            AcceptedLifecycleYield::new(
                self.home_id,
                thread_id,
                turn_id,
                outcome,
                attention.clone(),
                reservation,
            ),
        );
        Ok(true)
    }

    #[cfg(any(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn record_lifecycle_yield(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        outcome: LifecycleYieldOutcome,
    ) -> Result<bool, StopCoordinationError> {
        self.accept_lifecycle_yield(thread_id, turn_id, outcome, &Weak::new())
    }

    pub(in crate::cas_projection) fn observe_terminal_lifecycle_yield(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let key = LifecycleYieldKey { thread_id, turn_id };
        if state
            .lifecycle_yields
            .get(&key)
            .is_some_and(|accepted| accepted.outcome == LifecycleYieldOutcome::PhaseContinue)
        {
            return;
        }
        if let Some(accepted) = state.lifecycle_yields.remove(&key) {
            if let (Some(pool), Some(attempt)) =
                (accepted.attention.upgrade(), accepted.attempt.as_ref())
            {
                let _ = pool.report_terminal(attempt);
            }
        }
    }

    pub(in crate::cas_projection) fn release_ordinary_lifecycle_yield(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) {
        let accepted = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .lifecycle_yields
            .remove(&LifecycleYieldKey { thread_id, turn_id });
        if let Some(mut accepted) = accepted {
            if self.ordinary_shutdown_won() {
                accepted.cancel_continuation();
            }
        }
    }

    pub(in crate::cas_projection) fn ordinary_shutdown_won(&self) -> bool {
        matches!(
            self.commands.status_exact(),
            Ok(super::super::persistent_failure::LiveCommandGateStatus::OrdinaryShutdown)
        )
    }

    pub(in crate::cas_projection) fn cancel_all_lifecycle_continuations(&self) {
        for accepted in self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .lifecycle_yields
            .values_mut()
        {
            accepted.cancel_continuation();
        }
    }

    pub(in crate::cas_projection) fn take_lifecycle_continuation(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Option<AcceptedLifecycleYield> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .lifecycle_yields
            .remove(&LifecycleYieldKey { thread_id, turn_id })
    }

    pub(in crate::cas_projection) fn take_terminal_lifecycle_yield(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<Option<LifecycleYieldOutcome>, StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()?;
        if !command.is_current() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        Ok(state
            .lifecycle_yields
            .remove(&LifecycleYieldKey { thread_id, turn_id })
            .and_then(|mut accepted| {
                let outcome = accepted.effective_outcome();
                accepted.cancel_continuation();
                outcome
            }))
    }

    pub(in crate::cas_projection) fn share_terminal_continuation_custody(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<Option<CompactionCustodyReservation>, StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()?;
        if !command.is_current() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        Ok(state
            .lifecycle_yields
            .get(&LifecycleYieldKey { thread_id, turn_id })
            .filter(|accepted| accepted.continuation_pending)
            .and_then(|accepted| accepted.reservation.as_ref())
            .map(CompactionCustodyReservation::share))
    }
}
