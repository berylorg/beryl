use super::*;

const CONTINUATION_CANCELLATION_CAPACITY: usize = 256;

impl StopCoordinator {
    pub(in crate::cas_projection) fn cancel_window_close_continuation(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<(), StopCoordinationError> {
        self.cancel_window_close_continuation_with_capacity(
            thread_id,
            CONTINUATION_CANCELLATION_CAPACITY,
        )
    }

    pub(in crate::cas_projection) fn cancel_window_close_continuation_with_capacity(
        &self,
        thread_id: SyndicThreadId,
        capacity: usize,
    ) -> Result<(), StopCoordinationError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        let home = self.current_home()?;
        let mut retired = Vec::new();
        for (&cancelled_thread, &cancelled_turn) in &state.cancelled_continuations {
            if self.lifecycle_registration_turn(&home, cancelled_thread)? != Some(cancelled_turn) {
                retired.push(cancelled_thread);
            }
        }
        let current_turn = self.lifecycle_registration_turn(&home, thread_id)?;
        if current_turn.is_some()
            && !state.cancelled_continuations.contains_key(&thread_id)
            && state.cancelled_continuations.len() - retired.len() >= capacity
        {
            return Err(StopCoordinationError::ContinuationCancellationCapacity);
        }
        self.ensure_current()?;
        if !command.is_current() || state.persistent_failure.is_some() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        for retired_thread in retired {
            state.cancelled_continuations.remove(&retired_thread);
        }
        if let Some(turn_id) = current_turn {
            state.cancelled_continuations.insert(thread_id, turn_id);
        }
        for (key, accepted) in &mut state.lifecycle_yields {
            if key.thread_id == thread_id {
                accepted.cancel_continuation();
            }
        }
        Ok(())
    }

    pub(super) fn lifecycle_registration_turn(
        &self,
        home: &HomeStore,
        thread_id: SyndicThreadId,
    ) -> Result<Option<SyndicTurnId>, StopCoordinationError> {
        let gate = self.storage.input_gate(home, thread_id, point_limit())?;
        Ok(gate.and_then(|gate| match gate.state() {
            syndic_storage::InputGateState::Idle
            | syndic_storage::InputGateState::Compacting { .. } => None,
            state => state.blocking_turn_id(),
        }))
    }
}
