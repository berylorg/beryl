use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder};

use crate::{
    AcceptedInputDisposition, AcceptedInputLifecycle, AcceptedInputRecord,
    AcceptedNextTurnIndexRecord, AcceptedOrderIndexRecord, AcceptedSteeringIndexRecord,
    InputGateRecord, InputGateState, NextTurnReason, SyndicMutationError, SyndicRecordError,
    codec::*, domain::SyndicDomain,
};

use super::{AcceptedInputDeliveryMutation, AcceptedInputDeliveryTransitionKind};
use crate::mutation::{point, required};

struct AcceptedInputDeliveryRecords {
    prior_turn: beryl_model::SyndicTurnId,
    input: AcceptedInputRecord,
    order: AcceptedOrderIndexRecord,
    steering: Option<AcceptedSteeringIndexRecord>,
    next: Option<AcceptedNextTurnIndexRecord>,
    gate: InputGateRecord,
}

impl DomainMutation<SyndicDomain> for AcceptedInputDeliveryMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
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
        let transition = self.transition;
        let gate = required::<InputGatesFamily>(reader, &transition.thread_id)?;
        if gate.revision() != transition.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: transition.expected_gate_revision,
                current: gate.revision(),
            });
        }
        let current = required::<AcceptedInputsFamily>(reader, &transition.input_id)?;
        if current.thread_id() != transition.thread_id {
            return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
        }
        if current.revision() != transition.expected_input_revision {
            return Err(SyndicMutationError::AcceptedInputRevisionConflict {
                expected: transition.expected_input_revision,
                current: current.revision(),
            });
        }
        validate_source_lifecycle(self.kind, current.lifecycle())?;

        let AcceptedInputDisposition::SteerActiveTurn(target) = current.disposition() else {
            return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
        };
        if gate.state() != &InputGateState::Steerable(target.clone()) {
            return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
        }
        let prior_turn = target.pending().active_turn_id();
        validate_indexes(reader, &current, prior_turn)?;

        self.next_records(current, gate, prior_turn)
    }

    fn next_records(
        &self,
        current: AcceptedInputRecord,
        gate: InputGateRecord,
        prior_turn: beryl_model::SyndicTurnId,
    ) -> Result<AcceptedInputDeliveryRecords, SyndicMutationError> {
        let revision = current.revision().checked_next()?;
        let (disposition, lifecycle) = delivery_result(self.kind, current.disposition().clone());
        let input = AcceptedInputRecord::new(
            current.id(),
            current.thread_id(),
            revision,
            current.ordinal(),
            current.gate_revision(),
            disposition,
            lifecycle,
            current.content(),
            current.marker_count(),
            current.admitted_at(),
        );
        let order =
            AcceptedOrderIndexRecord::new(input.thread_id(), input.ordinal(), input.id(), revision);
        let (steering, next) = next_route(self.kind, &input, prior_turn);
        let gate = next_gate(self.kind, gate, &input)?;
        Ok(AcceptedInputDeliveryRecords {
            prior_turn,
            input,
            order,
            steering,
            next,
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
    if valid {
        Ok(())
    } else {
        Err(SyndicMutationError::AcceptedInputDeliveryConflict)
    }
}

fn validate_indexes(
    reader: &DomainReader<'_, SyndicDomain>,
    current: &AcceptedInputRecord,
    prior_turn: beryl_model::SyndicTurnId,
) -> Result<(), SyndicMutationError> {
    let order_key = ThreadAcceptedKey {
        owner: current.thread_id(),
        ordinal: current.ordinal(),
    };
    let expected_order = AcceptedOrderIndexRecord::new(
        current.thread_id(),
        current.ordinal(),
        current.id(),
        current.revision(),
    );
    let steering_key = SteeringKey {
        thread: current.thread_id(),
        turn: prior_turn,
        ordinal: current.ordinal(),
    };
    let expected_steering = AcceptedSteeringIndexRecord::new(
        current.thread_id(),
        prior_turn,
        current.ordinal(),
        current.id(),
        current.revision(),
    );
    if point::<AcceptedOrderFamily>(reader, &order_key)?.as_ref() != Some(&expected_order)
        || point::<AcceptedSteeringFamily>(reader, &steering_key)?.as_ref()
            != Some(&expected_steering)
        || point::<AcceptedNextFamily>(reader, &order_key)?.is_some()
    {
        return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
    }
    Ok(())
}

fn delivery_result(
    kind: AcceptedInputDeliveryTransitionKind,
    current: AcceptedInputDisposition,
) -> (AcceptedInputDisposition, AcceptedInputLifecycle) {
    match kind {
        AcceptedInputDeliveryTransitionKind::Begin => (current, AcceptedInputLifecycle::Delivering),
        AcceptedInputDeliveryTransitionKind::Retry => (current, AcceptedInputLifecycle::Retryable),
        AcceptedInputDeliveryTransitionKind::Complete => {
            (current, AcceptedInputLifecycle::Delivered)
        }
        AcceptedInputDeliveryTransitionKind::Rejected => (
            AcceptedInputDisposition::NextTurn(NextTurnReason::SteeringRejected),
            AcceptedInputLifecycle::Retryable,
        ),
    }
}

fn next_route(
    kind: AcceptedInputDeliveryTransitionKind,
    input: &AcceptedInputRecord,
    prior_turn: beryl_model::SyndicTurnId,
) -> (
    Option<AcceptedSteeringIndexRecord>,
    Option<AcceptedNextTurnIndexRecord>,
) {
    match kind {
        AcceptedInputDeliveryTransitionKind::Begin | AcceptedInputDeliveryTransitionKind::Retry => {
            (
                Some(AcceptedSteeringIndexRecord::new(
                    input.thread_id(),
                    prior_turn,
                    input.ordinal(),
                    input.id(),
                    input.revision(),
                )),
                None,
            )
        }
        AcceptedInputDeliveryTransitionKind::Complete => (None, None),
        AcceptedInputDeliveryTransitionKind::Rejected => (
            None,
            Some(AcceptedNextTurnIndexRecord::new(
                input.thread_id(),
                input.ordinal(),
                input.id(),
                input.revision(),
            )),
        ),
    }
}

fn next_gate(
    kind: AcceptedInputDeliveryTransitionKind,
    gate: InputGateRecord,
    input: &AcceptedInputRecord,
) -> Result<InputGateRecord, SyndicMutationError> {
    let removes_steering = matches!(
        kind,
        AcceptedInputDeliveryTransitionKind::Complete
            | AcceptedInputDeliveryTransitionKind::Rejected
    );
    let steering_count = if removes_steering {
        gate.live_steering_count()
            .checked_sub(1)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "live steering count",
            })?
    } else {
        gate.live_steering_count()
    };
    let next_count = if matches!(kind, AcceptedInputDeliveryTransitionKind::Rejected) {
        gate.live_next_turn_count()
            .checked_add(1)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "live next-turn count",
            })?
    } else {
        gate.live_next_turn_count()
    };
    let removes_live_bytes = matches!(kind, AcceptedInputDeliveryTransitionKind::Complete);
    let live_bytes = if removes_live_bytes {
        gate.live_logical_utf8_bytes()
            .checked_sub(input.content().summary().logical_utf8_bytes())
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "live accepted-input bytes",
            })?
    } else {
        gate.live_logical_utf8_bytes()
    };
    Ok(InputGateRecord::new(
        gate.thread_id(),
        gate.revision().checked_next()?,
        gate.state().clone(),
        gate.accepted_high_water(),
        steering_count,
        next_count,
        live_bytes,
    )?)
}

impl AcceptedInputDeliveryRecords {
    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        let order_key = ThreadAcceptedKey {
            owner: self.input.thread_id(),
            ordinal: self.input.ordinal(),
        };
        let steering_key = SteeringKey {
            thread: self.input.thread_id(),
            turn: self.prior_turn,
            ordinal: self.input.ordinal(),
        };
        mutations.put::<AcceptedInputsCodec>(&self.input.id(), &self.input)?;
        mutations.put::<AcceptedOrderCodec>(&order_key, &self.order)?;
        match &self.steering {
            Some(steering) => {
                mutations.put::<AcceptedSteeringCodec>(&steering_key, steering)?;
            }
            None => mutations.delete::<AcceptedSteeringCodec>(&steering_key)?,
        }
        if let Some(next) = &self.next {
            mutations.put::<AcceptedNextCodec>(&order_key, next)?;
        }
        mutations.put::<InputGatesCodec>(&self.input.thread_id(), &self.gate)?;
        Ok(())
    }
}
