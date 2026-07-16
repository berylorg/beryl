use std::collections::BTreeSet;

use beryl_home_store::DomainReader;

use crate::{
    AcceptedInputDisposition, AcceptedInputLifecycle, AcceptedNextTurnIndexRecord,
    AcceptedOrderIndexRecord, AcceptedSteeringIndexRecord, BindingState, InputGateState,
    MAX_LIVE_ACCEPTED_INPUTS, MAX_LIVE_ACCEPTED_UTF8_BYTES, PendingSteeringTargetProof,
    SteeringTargetProof, codec::*, domain::SyndicDomain, error::SyndicValidationError,
};

use super::scan::{point, require, scan, scan_range};

mod delivery;

use delivery::validate_delivery_unknown_proof;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    validate_primary(reader)?;
    validate_order(reader)?;
    validate_steering(reader)?;
    validate_next_turn(reader)?;
    validate_gates(reader)
}

fn validate_primary(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<AcceptedInputsFamily>(reader, |key, input| {
        if *key != input.id() {
            return invariant("accepted-input key and identity disagree");
        }
        if point::<DraftsFamily>(
            reader,
            &beryl_model::SyndicDraftId::from_bytes(*input.id().as_bytes()),
        )?
        .is_some()
            || point::<TurnsFamily>(
                reader,
                &beryl_model::SyndicTurnId::from_bytes(*input.id().as_bytes()),
            )?
            .is_some()
        {
            return invariant("accepted input raw identity was consumed more than once");
        }
        require::<ThreadsFamily>(
            reader,
            &input.thread_id(),
            "accepted input owner thread is missing",
        )?;
        let order_key = ThreadAcceptedKey {
            owner: input.thread_id(),
            ordinal: input.ordinal(),
        };
        let expected = AcceptedOrderIndexRecord::new(
            input.thread_id(),
            input.ordinal(),
            input.id(),
            input.revision(),
        );
        if require::<AcceptedOrderFamily>(
            reader,
            &order_key,
            "accepted-input order index is missing",
        )? != expected
        {
            return invariant("accepted-input order index disagrees");
        }
        if input.lifecycle() == AcceptedInputLifecycle::Delivering
            && !matches!(
                input.disposition(),
                AcceptedInputDisposition::SteerActiveTurn(_)
            )
        {
            return invariant("delivering accepted input lacks an exact steering target");
        }
        if input.lifecycle() == AcceptedInputLifecycle::DeliveryUnknown {
            let AcceptedInputDisposition::SteerActiveTurn(target) = input.disposition() else {
                return invariant("delivery-unknown accepted input lacks dispatch provenance");
            };
            validate_delivery_unknown_proof(reader, input, target)?;
        }
        let steering_turn = match input.disposition() {
            AcceptedInputDisposition::AwaitingSteering(target) => Some(target.active_turn_id()),
            AcceptedInputDisposition::SteerActiveTurn(target) => {
                Some(target.pending().active_turn_id())
            }
            AcceptedInputDisposition::NextTurn(_) => None,
        };
        let order_key = ThreadAcceptedKey {
            owner: input.thread_id(),
            ordinal: input.ordinal(),
        };
        if input.lifecycle().is_terminal() {
            if let Some(turn) = steering_turn {
                let key = SteeringKey {
                    thread: input.thread_id(),
                    turn,
                    ordinal: input.ordinal(),
                };
                if point::<AcceptedSteeringFamily>(reader, &key)?.is_some() {
                    return invariant("terminal accepted input remains in steering index");
                }
            }
            if point::<AcceptedNextFamily>(reader, &order_key)?.is_some() {
                return invariant("terminal accepted input remains in next-turn index");
            }
            return Ok(());
        }
        match input.disposition() {
            AcceptedInputDisposition::AwaitingSteering(target) => {
                let turn = target.active_turn_id();
                let target =
                    require::<TurnsFamily>(reader, &turn, "steering target turn is missing")?;
                if target.origin_thread_id() != input.thread_id() {
                    return invariant("steering target turn belongs to another thread");
                }
                let steering_key = SteeringKey {
                    thread: input.thread_id(),
                    turn,
                    ordinal: input.ordinal(),
                };
                let expected = AcceptedSteeringIndexRecord::new(
                    input.thread_id(),
                    turn,
                    input.ordinal(),
                    input.id(),
                    input.revision(),
                );
                if require::<AcceptedSteeringFamily>(
                    reader,
                    &steering_key,
                    "accepted steering index is missing",
                )? != expected
                {
                    return invariant("accepted steering index disagrees");
                }
                if point::<AcceptedNextFamily>(reader, &order_key)?.is_some() {
                    return invariant("steering input also appears in next-turn index");
                }
            }
            AcceptedInputDisposition::SteerActiveTurn(proof) => {
                let turn = proof.pending().active_turn_id();
                let target =
                    require::<TurnsFamily>(reader, &turn, "steering target turn is missing")?;
                if target.origin_thread_id() != input.thread_id() {
                    return invariant("steering target turn belongs to another thread");
                }
                let steering_key = SteeringKey {
                    thread: input.thread_id(),
                    turn,
                    ordinal: input.ordinal(),
                };
                let expected = AcceptedSteeringIndexRecord::new(
                    input.thread_id(),
                    turn,
                    input.ordinal(),
                    input.id(),
                    input.revision(),
                );
                if require::<AcceptedSteeringFamily>(
                    reader,
                    &steering_key,
                    "accepted steering index is missing",
                )? != expected
                {
                    return invariant("accepted steering index disagrees");
                }
                if point::<AcceptedNextFamily>(reader, &order_key)?.is_some() {
                    return invariant("steering input also appears in next-turn index");
                }
            }
            AcceptedInputDisposition::NextTurn(_) => {
                let expected = AcceptedNextTurnIndexRecord::new(
                    input.thread_id(),
                    input.ordinal(),
                    input.id(),
                    input.revision(),
                );
                if require::<AcceptedNextFamily>(
                    reader,
                    &order_key,
                    "accepted next-turn index is missing",
                )? != expected
                {
                    return invariant("accepted next-turn index disagrees");
                }
                if let Some(turn) = steering_turn {
                    let key = SteeringKey {
                        thread: input.thread_id(),
                        turn,
                        ordinal: input.ordinal(),
                    };
                    if point::<AcceptedSteeringFamily>(reader, &key)?.is_some() {
                        return invariant("next-turn input also appears in steering index");
                    }
                }
            }
        }
        Ok(())
    })
}

fn validate_order(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    let mut current_thread = None;
    let mut expected = 1_u64;
    scan::<AcceptedOrderFamily>(reader, |key, index| {
        if key.owner != index.thread_id() || key.ordinal != index.ordinal() {
            return invariant("accepted-order key disagrees");
        }
        if current_thread != Some(index.thread_id()) {
            current_thread = Some(index.thread_id());
            expected = 1;
        }
        if index.ordinal().get() != expected {
            return invariant("accepted-input order is not contiguous");
        }
        expected = expected
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "accepted-input order exhausted",
            ))?;
        let input = require::<AcceptedInputsFamily>(
            reader,
            &index.input_id(),
            "accepted-order target is missing",
        )?;
        if input.thread_id() != index.thread_id()
            || input.ordinal() != index.ordinal()
            || input.revision() != index.input_revision()
        {
            return invariant("accepted-order target disagrees");
        }
        Ok(())
    })
}

fn validate_steering(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<AcceptedSteeringFamily>(reader, |key, index| {
        if key.thread != index.thread_id()
            || key.turn != index.turn_id()
            || key.ordinal != index.ordinal()
        {
            return invariant("accepted-steering key disagrees");
        }
        let input = require::<AcceptedInputsFamily>(
            reader,
            &index.input_id(),
            "accepted-steering target is missing",
        )?;
        let turn = require::<TurnsFamily>(
            reader,
            &index.turn_id(),
            "accepted-steering turn is missing",
        )?;
        let disposition_turn = match input.disposition() {
            AcceptedInputDisposition::AwaitingSteering(target) => target.active_turn_id(),
            AcceptedInputDisposition::SteerActiveTurn(target) => target.pending().active_turn_id(),
            AcceptedInputDisposition::NextTurn(_) => {
                return invariant("accepted-steering target has next-turn disposition");
            }
        };
        if input.thread_id() != index.thread_id()
            || input.ordinal() != index.ordinal()
            || input.revision() != index.input_revision()
            || input.lifecycle().is_terminal()
            || disposition_turn != index.turn_id()
            || turn.origin_thread_id() != index.thread_id()
        {
            return invariant("accepted-steering target disagrees");
        }
        Ok(())
    })
}

fn validate_next_turn(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<AcceptedNextFamily>(reader, |key, index| {
        if key.owner != index.thread_id() || key.ordinal != index.ordinal() {
            return invariant("accepted-next-turn key disagrees");
        }
        let input = require::<AcceptedInputsFamily>(
            reader,
            &index.input_id(),
            "accepted-next-turn target is missing",
        )?;
        if input.thread_id() != index.thread_id()
            || input.ordinal() != index.ordinal()
            || input.revision() != index.input_revision()
            || input.lifecycle().is_terminal()
            || !matches!(input.disposition(), AcceptedInputDisposition::NextTurn(_))
        {
            return invariant("accepted-next-turn target disagrees");
        }
        Ok(())
    })
}

fn validate_gates(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<ThreadsFamily>(reader, |_, thread| {
        let gate =
            require::<InputGatesFamily>(reader, &thread.id(), "thread input gate is missing")?;
        if gate.thread_id() != thread.id() {
            return invariant("input-gate key and thread disagree");
        }

        let mut high_water = 0_u64;
        scan_range::<AcceptedOrderFamily>(
            reader,
            ThreadAcceptedKey::first_for_thread(thread.id()),
            ThreadAcceptedKey::last_for_thread(thread.id()),
            |_, index| {
                high_water = index.ordinal().get();
                Ok(())
            },
        )?;

        let mut steering_count = 0_u32;
        let mut next_count = 0_u32;
        let mut live_bytes = 0_u64;
        let mut live_ids = BTreeSet::new();
        scan_range::<AcceptedSteeringFamily>(
            reader,
            SteeringKey::first_for_thread(thread.id()),
            SteeringKey::last_for_thread(thread.id()),
            |_, index| {
                steering_count =
                    steering_count
                        .checked_add(1)
                        .ok_or(SyndicValidationError::Invariant(
                            "live steering count overflowed",
                        ))?;
                let input = require::<AcceptedInputsFamily>(
                    reader,
                    &index.input_id(),
                    "live steering input is missing",
                )?;
                if !live_ids.insert(input.id()) {
                    return invariant("accepted input repeats in live routes");
                }
                live_bytes = live_bytes
                    .checked_add(input.content().summary().logical_utf8_bytes())
                    .ok_or(SyndicValidationError::Invariant(
                        "live accepted-input bytes overflowed",
                    ))?;
                validate_live_steering_disposition(&gate, &input)
            },
        )?;
        scan_range::<AcceptedNextFamily>(
            reader,
            ThreadAcceptedKey::first_for_thread(thread.id()),
            ThreadAcceptedKey::last_for_thread(thread.id()),
            |_, index| {
                next_count = next_count
                    .checked_add(1)
                    .ok_or(SyndicValidationError::Invariant(
                        "live next-turn count overflowed",
                    ))?;
                let input = require::<AcceptedInputsFamily>(
                    reader,
                    &index.input_id(),
                    "live next-turn input is missing",
                )?;
                if !live_ids.insert(input.id()) {
                    return invariant("accepted input repeats in live routes");
                }
                live_bytes = live_bytes
                    .checked_add(input.content().summary().logical_utf8_bytes())
                    .ok_or(SyndicValidationError::Invariant(
                        "live accepted-input bytes overflowed",
                    ))?;
                Ok(())
            },
        )?;

        if high_water != gate.accepted_high_water()
            || steering_count != gate.live_steering_count()
            || next_count != gate.live_next_turn_count()
            || live_bytes != gate.live_logical_utf8_bytes()
            || gate.live_count() > MAX_LIVE_ACCEPTED_INPUTS
            || live_bytes > MAX_LIVE_ACCEPTED_UTF8_BYTES
        {
            return invariant("input-gate retained order or live-route accounting disagrees");
        }
        validate_gate_state(reader, thread, &gate)
    })?;
    scan::<InputGatesFamily>(reader, |key, gate| {
        if *key != gate.thread_id() || point::<ThreadsFamily>(reader, key)?.is_none() {
            return invariant("input gate has no matching thread");
        }
        Ok(())
    })
}

fn validate_live_steering_disposition(
    gate: &crate::InputGateRecord,
    input: &crate::AcceptedInputRecord,
) -> Result<(), SyndicValidationError> {
    let agrees = match (gate.state(), input.disposition()) {
        (
            InputGateState::AwaitingSteering(gate_target),
            AcceptedInputDisposition::AwaitingSteering(input_target),
        ) => gate_target == input_target,
        (
            InputGateState::Steerable(gate_target),
            AcceptedInputDisposition::SteerActiveTurn(input_target),
        ) => gate_target == input_target,
        _ => false,
    };
    if !agrees {
        return invariant("live steering route disagrees with current input gate");
    }
    Ok(())
}

fn validate_gate_state(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    gate: &crate::InputGateRecord,
) -> Result<(), SyndicValidationError> {
    match gate.state() {
        InputGateState::Idle => {
            if gate.live_steering_count() != 0 {
                return invariant("idle input gate retains live steering");
            }
            if let Some(tail) = thread.committed_tail()
                && require::<TurnStatesFamily>(
                    reader,
                    &tail,
                    "committed-tail turn state is missing",
                )?
                .lifecycle()
                .blocks_same_thread_start()
            {
                return invariant("idle input gate leaves committed turn blocking");
            }
        }
        InputGateState::PendingTurn(turn) => {
            validate_blocking_turn(reader, thread, *turn, None)?;
            if gate.live_steering_count() != 0 {
                return invariant("pending-turn input gate retains live steering");
            }
        }
        InputGateState::AwaitingSteering(target) => {
            validate_steering_target(reader, thread, target, None)?;
        }
        InputGateState::Steerable(target) => {
            validate_steering_target(reader, thread, target.pending(), Some(target))?;
        }
        InputGateState::Compacting(turn) => {
            validate_blocking_turn(
                reader,
                thread,
                *turn,
                Some(crate::ProviderOperationKind::ContextCompaction),
            )?;
            if gate.live_steering_count() != 0 {
                return invariant("compaction input gate retains live steering");
            }
        }
        InputGateState::Stopping(target) => {
            validate_steering_target(reader, thread, target.pending(), Some(target))?;
            if gate.live_steering_count() != 0 {
                return invariant("stopping input gate retains live steering");
            }
        }
    }
    Ok(())
}

fn validate_blocking_turn(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    turn_id: beryl_model::SyndicTurnId,
    operation: Option<crate::ProviderOperationKind>,
) -> Result<(), SyndicValidationError> {
    if thread.committed_tail() != Some(turn_id) {
        return invariant("input-gate blocking turn is not the committed tail");
    }
    let turn = require::<TurnsFamily>(reader, &turn_id, "input-gate turn is missing")?;
    let state = require::<TurnStatesFamily>(reader, &turn_id, "input-gate turn state is missing")?;
    if turn.origin_thread_id() != thread.id() || !state.lifecycle().blocks_same_thread_start() {
        return invariant("input-gate turn does not block the owning thread");
    }
    match (operation, turn.kind()) {
        (None, crate::TurnKind::OrdinaryUser)
        | (
            Some(crate::ProviderOperationKind::ContextCompaction),
            crate::TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction),
        ) => Ok(()),
        _ => invariant("input-gate turn kind disagrees with gate state"),
    }
}

fn validate_steering_target(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    pending: &PendingSteeringTargetProof,
    exact: Option<&SteeringTargetProof>,
) -> Result<(), SyndicValidationError> {
    validate_blocking_turn(reader, thread, pending.active_turn_id(), None)?;
    let head = require::<BindingHeadsFamily>(
        reader,
        &thread.id(),
        "steering input gate binding head is missing",
    )?;
    if head.revision() != pending.binding_revision() {
        return invariant("steering input gate binding revision is stale");
    }
    let binding = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread.id(),
            revision: pending.binding_revision(),
        },
        "steering input gate binding is missing",
    )?;
    let BindingState::Active(active) = binding.state() else {
        return invariant("steering input gate binding is not active");
    };
    if active.snapshot_id() != pending.snapshot_id()
        || active.turn_id() != pending.active_turn_id()
        || active.usable().cas_thread_id() != pending.cas_thread_id()
    {
        return invariant("steering input gate and active binding disagree");
    }
    let snapshot = require::<ExecutionSnapshotsFamily>(
        reader,
        &pending.snapshot_id(),
        "steering input gate execution snapshot is missing",
    )?;
    if snapshot.thread_id() != thread.id()
        || snapshot.binding_revision() != pending.binding_revision()
        || snapshot.active_turn_id() != pending.active_turn_id()
        || snapshot.cas_thread_id() != pending.cas_thread_id()
    {
        return invariant("steering input gate and execution snapshot disagree");
    }
    match (
        exact,
        point::<ActiveCasTurnsFamily>(reader, &pending.snapshot_id())?,
    ) {
        (None, None) => {}
        (Some(exact), Some(active_turn))
            if active_turn.thread_id() == thread.id()
                && active_turn.turn_id() == pending.active_turn_id()
                && active_turn.binding_revision() == pending.binding_revision()
                && active_turn.cas_thread_id() == pending.cas_thread_id()
                && active_turn.cas_turn_id() == exact.cas_turn_id() => {}
        (None, Some(_)) => {
            return invariant("awaiting-steering gate has a published active CAS turn");
        }
        (Some(_), None) => return invariant("steerable gate active CAS turn is missing"),
        (Some(_), Some(_)) => return invariant("steering gate active CAS turn disagrees"),
    }
    Ok(())
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
