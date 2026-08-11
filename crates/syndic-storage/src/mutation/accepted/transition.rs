use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use crate::{
    AcceptedInputLifecycle, AcceptedNextSourceRecord, AcceptedReadySourceRecord,
    AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord, AcceptedRouteHeadProof,
    AcceptedRouteLeafRecord, AcceptedRouteLeafState, AcceptedRouteLeafTransitionProof,
    AcceptedRouteTarget, InputGateRecord, InputGateState, NextTurnReason, SyndicMutationError,
    SyndicRecordError, codec::*, domain::SyndicDomain,
};

use super::{AcceptedInputDeliveryMutation, AcceptedInputDeliveryTransitionKind};
use crate::mutation::required;

struct AcceptedInputDeliveryRecords {
    leaf: AcceptedRouteLeafRecord,
    generation: AcceptedRouteGenerationRecord,
    head: AcceptedRouteGenerationHeadRecord,
    ready_source: Option<AcceptedReadySourceRecord>,
    next_source: Option<AcceptedNextSourceRecord>,
    gate: InputGateRecord,
}

impl DomainMutation<SyndicDomain> for AcceptedInputDeliveryMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AcceptedRouteLeavesCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationHeadsCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationsCodec>(1)?;
        reservation.reserve_records::<AcceptedReadySourcesCodec>(1)?;
        reservation.reserve_records::<AcceptedNextSourcesCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

impl AcceptedInputDeliveryMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<AcceptedInputDeliveryRecords, SyndicMutationError> {
        let transition = &self.transition;
        let gate = required::<InputGatesFamily>(reader, &transition.thread_id)?;
        let input = required::<AcceptedInputsFamily>(reader, &transition.input_id)?;
        let leaf = required::<AcceptedRouteLeavesFamily>(reader, &transition.input_id)?;
        if input.thread_id() != transition.thread_id
            || leaf.thread_id() != transition.thread_id
            || leaf.input_id() != input.id()
            || leaf.ordinal() != input.ordinal()
            || leaf.generation() != input.route_generation()
        {
            return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
        }
        if leaf.revision() != transition.expected_input_revision {
            return Err(SyndicMutationError::AcceptedInputRevisionConflict {
                expected: transition.expected_input_revision,
                current: leaf.revision(),
            });
        }
        validate_source_lifecycle(self.kind, leaf.lifecycle())?;
        if leaf.state() != AcceptedRouteLeafState::Routed {
            return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
        }

        let proof = gate
            .selected_route()
            .ok_or(SyndicMutationError::AcceptedInputDeliveryConflict)?;
        let head = required::<AcceptedRouteGenerationHeadsFamily>(reader, &transition.thread_id)?;
        let generation = required::<AcceptedRouteGenerationsFamily>(
            reader,
            &ThreadRouteKey {
                thread: transition.thread_id,
                generation: leaf.generation(),
            },
        )?;
        let AcceptedRouteTarget::Steering(target) = generation.target() else {
            return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
        };
        let InputGateState::Steerable(gate_turn) = gate.state() else {
            return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
        };
        if proof.generation() != leaf.generation()
            || proof != head.proof()
            || proof.revision() != generation.revision()
            || target != &transition.target
            || *gate_turn != transition.target.pending().active_turn_id()
        {
            return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
        }

        self.next_records(input, leaf, generation, gate)
    }

    fn next_records(
        &self,
        input: crate::AcceptedInputRecord,
        current_leaf: AcceptedRouteLeafRecord,
        current_generation: AcceptedRouteGenerationRecord,
        current_gate: InputGateRecord,
    ) -> Result<AcceptedInputDeliveryRecords, SyndicMutationError> {
        let bytes = input.content().summary().logical_utf8_bytes();
        let mut ready = current_generation.ready_retryable_count();
        let mut delivering = current_generation.delivering_count();
        let mut next = current_generation.next_turn_count();
        let mut terminal = current_generation.terminal_count();
        let mut live_bytes = current_generation.live_logical_utf8_bytes();
        let mut delivering_bytes = current_generation.delivering_logical_utf8_bytes();

        match self.kind {
            AcceptedInputDeliveryTransitionKind::Begin => {
                ready = checked_sub(ready, 1, "accepted-route ready count")?;
                delivering = checked_add(delivering, 1, "accepted-route delivering count")?;
                delivering_bytes =
                    checked_add(delivering_bytes, bytes, "accepted-route delivering bytes")?;
            }
            AcceptedInputDeliveryTransitionKind::Retry => {
                delivering = checked_sub(delivering, 1, "accepted-route delivering count")?;
                ready = checked_add(ready, 1, "accepted-route ready count")?;
                delivering_bytes =
                    checked_sub(delivering_bytes, bytes, "accepted-route delivering bytes")?;
            }
            AcceptedInputDeliveryTransitionKind::Complete => {
                delivering = checked_sub(delivering, 1, "accepted-route delivering count")?;
                terminal = checked_add(terminal, 1, "accepted-route terminal count")?;
                delivering_bytes =
                    checked_sub(delivering_bytes, bytes, "accepted-route delivering bytes")?;
                live_bytes = checked_sub(live_bytes, bytes, "accepted-route live bytes")?;
            }
            AcceptedInputDeliveryTransitionKind::Rejected => {
                delivering = checked_sub(delivering, 1, "accepted-route delivering count")?;
                next = checked_add(next, 1, "accepted-route next-turn count")?;
                delivering_bytes =
                    checked_sub(delivering_bytes, bytes, "accepted-route delivering bytes")?;
            }
        }

        let leaf_revision = current_leaf.revision().checked_next()?;
        let leaf = AcceptedRouteLeafRecord::new(
            current_leaf.input_id(),
            current_leaf.thread_id(),
            current_leaf.generation(),
            current_leaf.ordinal(),
            leaf_revision,
            match self.kind {
                AcceptedInputDeliveryTransitionKind::Rejected => {
                    AcceptedRouteLeafState::NextTurn(NextTurnReason::SteeringRejected)
                }
                AcceptedInputDeliveryTransitionKind::Begin
                | AcceptedInputDeliveryTransitionKind::Retry
                | AcceptedInputDeliveryTransitionKind::Complete => AcceptedRouteLeafState::Routed,
            },
            match self.kind {
                AcceptedInputDeliveryTransitionKind::Begin => AcceptedInputLifecycle::Delivering,
                AcceptedInputDeliveryTransitionKind::Retry
                | AcceptedInputDeliveryTransitionKind::Rejected => {
                    AcceptedInputLifecycle::Retryable
                }
                AcceptedInputDeliveryTransitionKind::Complete => AcceptedInputLifecycle::Delivered,
            },
        )
        .with_transition_proof(AcceptedRouteLeafTransitionProof::new(
            current_gate.revision(),
            AcceptedRouteHeadProof::new(
                current_generation.generation(),
                current_generation.revision(),
            ),
            self.transition.expected_input_revision,
            self.kind.persisted(),
        ));
        let generation_revision = current_generation.revision().checked_next()?;
        let generation = AcceptedRouteGenerationRecord::new(
            current_generation.thread_id(),
            current_generation.generation(),
            generation_revision,
            current_generation.target().clone(),
            current_generation.first_ordinal(),
            current_generation.last_ordinal(),
            current_generation.input_count(),
            ready,
            delivering,
            next,
            terminal,
            live_bytes,
            delivering_bytes,
        )?;
        let proof = AcceptedRouteHeadProof::new(generation.generation(), generation_revision);
        let head = AcceptedRouteGenerationHeadRecord::new(generation.thread_id(), proof);
        let next_source = (generation.next_turn_count() > 0).then(|| {
            AcceptedNextSourceRecord::new(
                generation.thread_id(),
                generation.generation(),
                generation_revision,
                generation
                    .first_ordinal()
                    .expect("nonempty generation owns transitioned leaf"),
                generation
                    .last_ordinal()
                    .expect("nonempty generation owns transitioned leaf"),
            )
        });

        let removes_steering = matches!(
            self.kind,
            AcceptedInputDeliveryTransitionKind::Complete
                | AcceptedInputDeliveryTransitionKind::Rejected
        );
        let steering_count = if removes_steering {
            checked_sub(current_gate.live_steering_count(), 1, "live steering count")?
        } else {
            current_gate.live_steering_count()
        };
        let next_count = if matches!(self.kind, AcceptedInputDeliveryTransitionKind::Rejected) {
            checked_add(
                current_gate.live_next_turn_count(),
                1,
                "live next-turn count",
            )?
        } else {
            current_gate.live_next_turn_count()
        };
        let gate_live_bytes = if matches!(self.kind, AcceptedInputDeliveryTransitionKind::Complete)
        {
            checked_sub(
                current_gate.live_logical_utf8_bytes(),
                bytes,
                "live accepted-input bytes",
            )?
        } else {
            current_gate.live_logical_utf8_bytes()
        };
        let gate = InputGateRecord::new(
            current_gate.thread_id(),
            current_gate.revision().checked_next()?,
            current_gate.state().clone(),
            current_gate.accepted_high_water(),
            current_gate.route_generation_high_water(),
            Some(proof),
            steering_count,
            next_count,
            gate_live_bytes,
        )?;
        let ready_source = (generation.ready_retryable_count() > 0).then(|| {
            AcceptedReadySourceRecord::new(
                generation.thread_id(),
                gate.revision(),
                generation.generation(),
                generation_revision,
                generation
                    .first_ordinal()
                    .expect("nonempty generation owns ready work"),
                generation
                    .last_ordinal()
                    .expect("nonempty generation owns ready work"),
            )
        });

        Ok(AcceptedInputDeliveryRecords {
            leaf,
            generation,
            head,
            ready_source,
            next_source,
            gate,
        })
    }
}

fn validate_source_lifecycle(
    kind: AcceptedInputDeliveryTransitionKind,
    lifecycle: AcceptedInputLifecycle,
) -> Result<(), SyndicMutationError> {
    let valid = match kind {
        AcceptedInputDeliveryTransitionKind::Begin => matches!(
            lifecycle,
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable
        ),
        AcceptedInputDeliveryTransitionKind::Retry
        | AcceptedInputDeliveryTransitionKind::Complete
        | AcceptedInputDeliveryTransitionKind::Rejected => {
            lifecycle == AcceptedInputLifecycle::Delivering
        }
    };
    valid
        .then_some(())
        .ok_or(SyndicMutationError::AcceptedInputDeliveryConflict)
}

fn checked_add(value: u64, amount: u64, kind: &'static str) -> Result<u64, SyndicRecordError> {
    value
        .checked_add(amount)
        .ok_or(SyndicRecordError::LengthOverflow { kind })
}

fn checked_sub(value: u64, amount: u64, kind: &'static str) -> Result<u64, SyndicRecordError> {
    value
        .checked_sub(amount)
        .ok_or(SyndicRecordError::LengthOverflow { kind })
}

impl AcceptedInputDeliveryRecords {
    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<AcceptedRouteLeavesCodec>(&self.leaf.input_id(), &self.leaf)?;
        mutations.put::<AcceptedRouteGenerationHeadsCodec>(&self.head.thread_id(), &self.head)?;
        mutations.put::<AcceptedRouteGenerationsCodec>(
            &ThreadRouteKey {
                thread: self.generation.thread_id(),
                generation: self.generation.generation(),
            },
            &self.generation,
        )?;
        let source_key = ThreadRouteKey {
            thread: self.generation.thread_id(),
            generation: self.generation.generation(),
        };
        if let Some(source) = &self.ready_source {
            mutations.put::<AcceptedReadySourcesCodec>(&source_key, source)?;
        } else {
            mutations.delete::<AcceptedReadySourcesCodec>(&source_key)?;
        }
        if let Some(source) = &self.next_source {
            mutations.put::<AcceptedNextSourcesCodec>(
                &ThreadRouteKey {
                    thread: source.thread_id(),
                    generation: source.generation(),
                },
                source,
            )?;
        }
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        Ok(())
    }
}
