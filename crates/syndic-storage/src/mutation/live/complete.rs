use beryl_home_store::{DomainReader, MutationBuilder};

use super::*;
use crate::mutation::required;
use crate::{InputGateRecord, InputGateState};

pub(super) struct TerminalHistoryCompletionRecords {
    gate: InputGateRecord,
}

impl CompleteTerminalHistoryMutation {
    pub(super) fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<TerminalHistoryCompletionRecords, SyndicMutationError> {
        let request = &self.request;
        let thread = required::<ThreadsFamily>(reader, &request.thread_id)?;
        let turn = required::<TurnsFamily>(reader, &request.turn_id)?;
        let state = required::<TurnStatesFamily>(reader, &request.turn_id)?;
        let gate = required::<InputGatesFamily>(reader, &request.thread_id)?;
        if turn.origin_thread_id() != thread.id()
            || thread.committed_tail() != Some(request.turn_id)
            || state.turn_id() != request.turn_id
            || !state.lifecycle().is_proven_terminal()
        {
            return conflict();
        }
        if state.revision() != request.expected_state_revision {
            return Err(SyndicMutationError::TurnStateRevisionConflict {
                expected: request.expected_state_revision,
                current: state.revision(),
            });
        }
        if request.observed_gate.thread_id() != request.thread_id
            || !gate.is_compatible_finalizing_history_descendant_of(
                &request.observed_gate,
                request.turn_id,
            )
        {
            return conflict();
        }

        let expected_transcript = crate::terminal_history::ExpectedTerminalTranscript::new(
            request.expected_transcript_generation,
            request.expected_transcript_revision,
        );
        if !crate::terminal_history::is_complete(
            reader,
            &thread,
            &state,
            Some(expected_transcript),
        )? {
            return conflict();
        }

        let gate = InputGateRecord::new(
            gate.thread_id(),
            gate.revision().checked_next()?,
            InputGateState::Idle,
            gate.accepted_high_water(),
            gate.route_generation_high_water(),
            gate.selected_route(),
            gate.live_steering_count(),
            gate.live_next_turn_count(),
            gate.live_logical_utf8_bytes(),
        )?;
        Ok(TerminalHistoryCompletionRecords { gate })
    }
}

impl TerminalHistoryCompletionRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        Ok(())
    }
}

fn conflict<T>() -> Result<T, SyndicMutationError> {
    Err(SyndicMutationError::TerminalHistoryCompletionConflict)
}
