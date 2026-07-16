use super::*;
use crate::mutation::{point, required};
use crate::{
    BindingHeadRecord, BindingRecord, BindingState, CasThreadBindingIndexRecord,
    CasThreadIndexRecord, HistorySummaryRecord, InputGateRecord, TranscriptViewHeadRecord,
    TurnLifecycle, TurnStateRecord,
};

mod item;

use super::terminal::{terminal_gate, terminal_valid_binding};
use item::{ItemEffect, append_item, complete_item, start_item};

pub(super) struct EventRecords {
    event: SourceEventRecord,
    state: TurnStateRecord,
    gate: Option<InputGateRecord>,
    summary: HistorySummaryRecord,
    transcript_head: Option<TranscriptViewHeadRecord>,
    transcript_build: Option<crate::TranscriptBuildRecord>,
    effect: Option<ItemEffect>,
    terminal_binding: Option<(
        BindingRecord,
        BindingHeadRecord,
        CasThreadIndexRecord,
        CasThreadBindingIndexRecord,
    )>,
}

impl LiveSourceEventMutation {
    pub(super) fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<EventRecords, SyndicMutationError> {
        let request = &self.event;
        let thread = required::<ThreadsFamily>(reader, &request.thread_id)?;
        let turn = required::<TurnsFamily>(reader, &request.turn_id)?;
        let current = required::<TurnStatesFamily>(reader, &request.turn_id)?;
        let gate = required::<InputGatesFamily>(reader, &request.thread_id)?;
        let summary = required::<HistorySummariesFamily>(reader, &request.thread_id)?;

        if turn.origin_thread_id() != thread.id()
            || thread.committed_tail() != Some(turn.id())
            || current.turn_id() != turn.id()
        {
            return Err(SyndicMutationError::LiveTurnConflict);
        }
        let event = SourceEventRecord::new(
            request.turn_id,
            request.sequence,
            request.source.clone(),
            request.payload.clone(),
        )?;
        let event_key = TurnEventKey {
            owner: request.turn_id,
            ordinal: request.sequence,
        };
        if let Some(existing) = point::<SourceEventsFamily>(reader, &event_key)? {
            return if existing == event {
                Err(SyndicMutationError::SourceEventAlreadyAdmitted)
            } else {
                Err(SyndicMutationError::SourceEventCollision)
            };
        }
        if current.revision() != request.expected_state_revision {
            return Err(SyndicMutationError::TurnStateRevisionConflict {
                expected: request.expected_state_revision,
                current: current.revision(),
            });
        }
        if gate.revision() != request.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision,
                current: gate.revision(),
            });
        }
        if request.observed_at < current.updated_at()
            || request.observed_at < summary.last_activity_at()
        {
            return Err(SyndicMutationError::TimestampRegressed);
        }
        if current.lifecycle().is_proven_terminal() {
            return Err(SyndicMutationError::TerminalTurnClosed);
        }

        let expected_sequence = next_source_sequence(current.source_event_count())?;
        if request.sequence != expected_sequence {
            return Err(SyndicMutationError::SourceEventSequenceConflict {
                expected: expected_sequence,
                actual: request.sequence,
            });
        }
        validate_turn_source(
            reader,
            &thread,
            &turn,
            request.source.as_ref(),
            &request.payload,
        )?;

        let mut lifecycle = current.lifecycle();
        let mut item_count = current.item_count();
        let finalized_item_count = current.finalized_item_count();
        let mut open_item_count = current.open_item_count();
        let mut history_blocking_item_count = current.history_blocking_item_count();
        let mut end_status = current.end_status();
        let mut next_gate = None;
        let (effect, transcript_dirty) = match &request.payload {
            SourceEventPayload::TurnActivated => {
                if !matches!(
                    lifecycle,
                    TurnLifecycle::Pending | TurnLifecycle::UnknownTerminal
                ) {
                    return Err(SyndicMutationError::TurnLifecycleConflict);
                }
                lifecycle = TurnLifecycle::Active;
                end_status = None;
                (None, false)
            }
            SourceEventPayload::ItemStarted {
                item,
                assistant_phase,
            } => {
                require_live_capture(lifecycle)?;
                let new_ordinal = item_count.saturating_add(1);
                let started = start_item(reader, &event, new_ordinal, item, *assistant_phase)?;
                if started.added_item {
                    item_count = item_count
                        .checked_add(1)
                        .ok_or(SyndicMutationError::CanonicalItemConflict)?;
                    open_item_count = open_item_count
                        .checked_add(1)
                        .ok_or(SyndicMutationError::CanonicalItemConflict)?;
                    if item.disposition().is_history_blocking() {
                        history_blocking_item_count = history_blocking_item_count
                            .checked_add(1)
                            .ok_or(SyndicMutationError::CanonicalItemConflict)?;
                    }
                }
                (Some(started.effect), started.transcript_dirty)
            }
            SourceEventPayload::ItemDelta {
                item_id,
                cas_item_id,
                expected_kind,
                text,
            } => {
                require_live_capture(lifecycle)?;
                let (effect, visible) =
                    append_item(reader, &event, *item_id, cas_item_id, *expected_kind, text)?;
                (Some(effect), visible)
            }
            SourceEventPayload::ItemCompleted {
                item,
                assistant_phase,
            } => {
                require_live_capture(lifecycle)?;
                let completed = complete_item(
                    reader,
                    &event,
                    item_count.saturating_add(1),
                    item,
                    *assistant_phase,
                )?;
                if completed.added_item {
                    item_count = item_count
                        .checked_add(1)
                        .ok_or(SyndicMutationError::CanonicalItemConflict)?;
                    if item.disposition().is_history_blocking() {
                        history_blocking_item_count = history_blocking_item_count
                            .checked_add(1)
                            .ok_or(SyndicMutationError::CanonicalItemConflict)?;
                    }
                } else {
                    open_item_count = open_item_count
                        .checked_sub(1)
                        .ok_or(SyndicMutationError::ProviderItemLifecycleConflict)?;
                }
                (Some(completed.effect), completed.transcript_dirty)
            }
            SourceEventPayload::TurnEnded(status) => {
                if status.incomplete_reason().is_none()
                    && (open_item_count != 0 || history_blocking_item_count != 0)
                {
                    return Err(SyndicMutationError::TerminalItemAuditConflict);
                }
                lifecycle = status.lifecycle();
                end_status = Some(*status);
                next_gate = Some(terminal_gate(&gate, turn.id(), lifecycle)?);
                (None, true)
            }
        };

        let state = TurnStateRecord::with_capture_frontiers(
            turn.id(),
            current.revision().checked_next()?,
            lifecycle,
            request.sequence.get(),
            item_count,
            finalized_item_count,
            open_item_count,
            history_blocking_item_count,
            end_status,
            request.observed_at,
        )?;
        let (transcript_head, transcript_build) = if transcript_dirty {
            crate::mutation::transcript::invalidate_transcript_projection(reader, &thread)?
        } else {
            (None, None)
        };
        let summary = HistorySummaryRecord::new(
            summary.thread_id(),
            summary.thread_revision(),
            summary.committed_tail(),
            summary.selected_path_digest(),
            false,
            request.observed_at,
        );
        let terminal_binding = if lifecycle.is_proven_terminal() {
            terminal_valid_binding(reader, &thread, turn.id(), request.source.as_ref())?
        } else {
            None
        };
        Ok(EventRecords {
            event,
            state,
            gate: next_gate,
            summary,
            transcript_head,
            transcript_build,
            effect,
            terminal_binding,
        })
    }
}

impl EventRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<SourceEventsCodec>(
            &TurnEventKey {
                owner: self.event.turn_id(),
                ordinal: self.event.sequence(),
            },
            &self.event,
        )?;
        mutations.put::<TurnStatesCodec>(&self.state.turn_id(), &self.state)?;
        if let Some(gate) = &self.gate {
            mutations.put::<InputGatesCodec>(&gate.thread_id(), gate)?;
        }
        mutations.put::<HistorySummariesCodec>(&self.summary.thread_id(), &self.summary)?;
        if let Some(head) = &self.transcript_head {
            mutations.put::<TranscriptHeadsCodec>(&head.thread_id(), head)?;
        }
        if let Some(build) = &self.transcript_build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        if let Some(effect) = self.effect {
            effect.contribute(mutations)?;
        }
        if let Some((binding, head, reservation, membership)) = &self.terminal_binding {
            mutations.put::<BindingsCodec>(
                &BindingKey {
                    thread: binding.thread_id(),
                    revision: binding.revision(),
                },
                binding,
            )?;
            mutations.put::<BindingHeadsCodec>(&head.thread_id(), head)?;
            mutations.put::<CasThreadIndexCodec>(
                &CasThreadKey::Record(reservation.cas_thread_id().clone()),
                reservation,
            )?;
            mutations.put::<CasThreadBindingIndexCodec>(
                &CasThreadBindingKey::Record(
                    membership.cas_thread_id().clone(),
                    membership.binding_revision(),
                ),
                membership,
            )?;
        }
        Ok(())
    }
}

fn next_source_sequence(count: u64) -> Result<SourceEventSequence, SyndicMutationError> {
    let next = count
        .checked_add(1)
        .ok_or(SyndicMutationError::SourceEventFrontierExhausted)?;
    Ok(SourceEventSequence::new(next)?)
}

fn require_live_capture(lifecycle: TurnLifecycle) -> Result<(), SyndicMutationError> {
    if matches!(
        lifecycle,
        TurnLifecycle::Active | TurnLifecycle::UnknownTerminal
    ) {
        Ok(())
    } else {
        Err(SyndicMutationError::TurnLifecycleConflict)
    }
}

fn validate_turn_source(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    turn: &crate::TurnRecord,
    source: Option<&CasTurnSource>,
    payload: &SourceEventPayload,
) -> Result<(), SyndicMutationError> {
    let head = required::<BindingHeadsFamily>(reader, &thread.id())?;
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread.id(),
            revision: head.revision(),
        },
    )?;
    let Some(source) = source else {
        return if source_less_local_terminal(payload)
            && matches!(
                binding.state(),
                BindingState::Stale(_) | BindingState::Unbound { .. }
            ) {
            Ok(())
        } else {
            Err(SyndicMutationError::SourceIdentityConflict)
        };
    };
    let BindingState::Active(active) = binding.state() else {
        return Err(SyndicMutationError::SourceIdentityConflict);
    };
    if active.turn_id() != turn.id() || active.usable().cas_thread_id() != source.thread_id() {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    let active_turn = required::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())?;
    if active_turn.thread_id() != thread.id()
        || active_turn.turn_id() != turn.id()
        || active_turn.binding_revision() != head.revision()
        || active_turn.cas_thread_id() != source.thread_id()
        || active_turn.cas_turn_id() != source.turn_id()
    {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    let index = required::<CasTurnIndexFamily>(
        reader,
        &CasTurnKey::Record(source.thread_id().clone(), source.turn_id().clone()),
    )?;
    if index.thread_id() != thread.id()
        || index.turn_id() != turn.id()
        || index.binding_revision() != head.revision()
        || index.snapshot_id() != active.snapshot_id()
    {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    Ok(())
}

fn source_less_local_terminal(payload: &SourceEventPayload) -> bool {
    matches!(
        payload,
        SourceEventPayload::TurnEnded(status)
            if status.outcome() != crate::TurnTerminalOutcome::Complete
    )
}
