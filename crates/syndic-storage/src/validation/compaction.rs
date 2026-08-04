use beryl_home_store::DomainReader;

use crate::{
    BindingState, CompactionOperationId, CompactionOperationState, ConversationParent,
    ExecutionSnapshotKind, InputGateState, ProviderOperationKind, TurnKind, TurnLifecycle,
    TurnStateRecord, codec::*, domain::SyndicDomain, error::SyndicValidationError,
};

use super::scan::{point, require, scan};

mod continuation;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<CompactionOperationsFamily>(reader, |key, operation| {
        if *key != operation.id()
            || operation.id().thread_id() != operation.target().thread_id()
            || operation.id().provider_turn_id() != operation.target().turn_id()
        {
            return invariant("compaction-operation key, identity, and target disagree");
        }
        let target = operation.target();
        let turn = require::<TurnsFamily>(
            reader,
            &target.turn_id(),
            "compaction provider turn is missing",
        )?;
        let state = require::<TurnStatesFamily>(
            reader,
            &target.turn_id(),
            "compaction provider turn state is missing",
        )?;
        let snapshot = require::<ExecutionSnapshotsFamily>(
            reader,
            &target.snapshot_id(),
            "compaction execution snapshot is missing",
        )?;
        if turn.origin_thread_id() != target.thread_id()
            || turn.kind() != TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction)
            || turn.parent() != ConversationParent::Root
            || snapshot.kind()
                != ExecutionSnapshotKind::ProviderOperation(
                    ProviderOperationKind::ContextCompaction,
                )
            || snapshot.thread_id() != target.thread_id()
            || snapshot.active_turn_id() != target.turn_id()
            || snapshot.binding_revision() != target.binding_revision()
            || snapshot.cas_thread_id() != target.cas_thread_id()
            || snapshot.execution().runtime_id() != target.runtime_id()
            || snapshot.loaded_generation() != target.loaded_generation()
        {
            return invariant("compaction turn, snapshot, and immutable target disagree");
        }
        let binding = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: target.thread_id(),
                revision: target.binding_revision(),
            },
            "compaction binding is missing",
        )?;
        if snapshot.selected_path() != binding.selected_path() {
            return invariant("compaction snapshot and binding path disagree");
        }
        let BindingState::Valid(usable) = binding.state() else {
            return invariant("compaction does not retain its admitted valid binding");
        };
        if usable.cas_thread_id() != target.cas_thread_id()
            || usable.execution().runtime_id() != target.runtime_id()
        {
            return invariant("compaction and admitted valid binding disagree");
        }
        if operation.state().is_live() {
            let gate = require::<InputGatesFamily>(
                reader,
                &target.thread_id(),
                "live compaction input gate is missing",
            )?;
            if gate.state() != &InputGateState::compacting(target.turn_id(), operation.id().nonce())
            {
                return invariant("live compaction is not selected by its gate");
            }
        }
        match operation.cas_turn() {
            Some(observed) => {
                let active = require::<ActiveCasTurnsFamily>(
                    reader,
                    &target.snapshot_id(),
                    "compaction CAS-turn publication is missing",
                )?;
                if active.thread_id() != target.thread_id()
                    || active.turn_id() != target.turn_id()
                    || active.binding_revision() != target.binding_revision()
                    || active.cas_thread_id() != target.cas_thread_id()
                    || active.cas_turn_id() != observed.cas_turn_id()
                {
                    return invariant("compaction CAS-turn publication disagrees");
                }
            }
            None if point::<ActiveCasTurnsFamily>(reader, &target.snapshot_id())?.is_some() => {
                return invariant("compaction record omits its CAS-turn publication");
            }
            None => {}
        }
        if operation.terminal().is_some_and(|terminal| {
            terminal.turn_state_revision() > state.revision()
                || (terminal.turn_state_revision() == state.revision()
                    && state.end_status() != Some(terminal.status()))
        }) {
            return invariant("compaction terminal and provider turn state disagree");
        }
        if let CompactionOperationState::Consumed(_) = operation.state() {
            let receipt = require::<CompactionSettlementReceiptsFamily>(
                reader,
                &operation.id(),
                "consumed compaction settlement receipt is missing",
            )?;
            let gate = require::<InputGatesFamily>(
                reader,
                &target.thread_id(),
                "consumed compaction successor gate is missing",
            )?;
            if !operation.consumed_receipt_is_exact(&receipt)
                || !receipt.current_gate_is_descendant(&gate)
                || !stop_source_is_exact(reader, operation, &receipt)?
            {
                return invariant("consumed compaction witness revision disagrees");
            }
            validate_consumed_settlement(reader, operation, &receipt, &state)?;
        } else if point::<CompactionSettlementReceiptsFamily>(reader, &operation.id())?.is_some() {
            return invariant("live compaction has a consumed settlement receipt");
        }
        Ok(())
    })?;

    scan::<CompactionSettlementReceiptsFamily>(reader, |key, receipt| {
        let operation = require::<CompactionOperationsFamily>(
            reader,
            key,
            "compaction settlement receipt operation is missing",
        )?;
        if *key != receipt.operation_id()
            || !matches!(operation.state(), CompactionOperationState::Consumed(_))
            || !operation.consumed_receipt_is_exact(receipt)
            || !stop_source_is_exact(reader, &operation, receipt)?
        {
            return invariant("compaction settlement receipt and operation disagree");
        }
        Ok(())
    })?;

    scan::<InputGatesFamily>(reader, |thread, gate| {
        let InputGateState::Compacting {
            turn_id,
            operation_nonce,
        } = gate.state()
        else {
            return Ok(());
        };
        let operation = require::<CompactionOperationsFamily>(
            reader,
            &CompactionOperationId::new(*thread, *operation_nonce),
            "compacting gate operation is missing",
        )?;
        if operation.target().turn_id() != *turn_id || !operation.state().is_live() {
            return invariant("compacting gate selects incompatible operation state");
        }
        Ok(())
    })
}

fn stop_source_is_exact(
    reader: &DomainReader<'_, SyndicDomain>,
    operation: &crate::CompactionOperationRecord,
    receipt: &crate::CompactionSettlementReceiptRecord,
) -> Result<bool, SyndicValidationError> {
    let InputGateState::Stopping {
        operation_nonce, ..
    } = receipt.source_gate().state()
    else {
        return Ok(true);
    };
    let stop = require::<StopOperationsFamily>(
        reader,
        &crate::StopOperationId::new(operation.target().thread_id(), *operation_nonce),
        "stopping compaction settlement source stop is missing",
    )?;
    Ok(stop.provider_abandonment_authenticates(operation, receipt))
}

fn validate_consumed_settlement(
    reader: &DomainReader<'_, SyndicDomain>,
    operation: &crate::CompactionOperationRecord,
    receipt: &crate::CompactionSettlementReceiptRecord,
    state: &TurnStateRecord,
) -> Result<(), SyndicValidationError> {
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        return invariant("compaction settlement validator received a live operation");
    };
    match witness.settlement() {
        crate::CompactionSettlement::CancelledBeforeDispatch
        | crate::CompactionSettlement::LocalNondispatch => {
            if state.lifecycle() != TurnLifecycle::Failed {
                return invariant("consumed compaction settlement and provider turn disagree");
            }
        }
        crate::CompactionSettlement::Abandoned(_) => {
            if state.lifecycle() != TurnLifecycle::Incomplete
                || state.incomplete_reason() != Some(crate::TurnIncompleteReason::AuthorityLost)
            {
                return invariant("consumed compaction settlement and provider turn disagree");
            }
            validate_retired_successor(reader, operation)?;
        }
        crate::CompactionSettlement::ManualSuccess
        | crate::CompactionSettlement::LifecycleContinuation { .. } => {
            if state.lifecycle() != TurnLifecycle::Complete {
                return invariant("consumed compaction settlement and provider turn disagree");
            }
        }
        crate::CompactionSettlement::LifecycleUserWorkWon => {
            if state.lifecycle() != TurnLifecycle::Complete {
                return invariant("consumed compaction settlement and provider turn disagree");
            }
            validate_accepted_work_successor(reader, operation, receipt)?;
        }
        crate::CompactionSettlement::ManualFailure => {
            let terminal = operation
                .terminal()
                .ok_or(SyndicValidationError::Invariant(
                    "manual compaction failure has no terminal witness",
                ))?;
            if terminal.status().outcome() == crate::TurnTerminalOutcome::Complete {
                if state.lifecycle() != TurnLifecycle::Incomplete
                    || state.incomplete_reason()
                        != Some(crate::TurnIncompleteReason::CompletionMismatch)
                {
                    return invariant("consumed compaction settlement and provider turn disagree");
                }
            } else if state.end_status() != Some(terminal.status()) {
                return invariant("consumed compaction settlement and provider turn disagree");
            }
            let preserves_binding = terminal.status().outcome()
                == crate::TurnTerminalOutcome::Interrupted
                && operation.status().is_some_and(|status| {
                    status.status() == crate::CompactionThreadStatus::Idle
                        && status.sequence() < terminal.sequence()
                });
            if !preserves_binding {
                validate_retired_successor(reader, operation)?;
            }
        }
    }
    if let crate::CompactionSettlement::LifecycleContinuation {
        turn_id,
        item_id,
        content_id,
    } = witness.settlement()
    {
        continuation::validate(reader, operation, receipt, *turn_id, *item_id, *content_id)?;
    }
    Ok(())
}

fn validate_accepted_work_successor(
    reader: &DomainReader<'_, SyndicDomain>,
    operation: &crate::CompactionOperationRecord,
    receipt: &crate::CompactionSettlementReceiptRecord,
) -> Result<(), SyndicValidationError> {
    let high_water = receipt.source_gate().accepted_high_water();
    let ordinal = crate::AcceptedInputOrdinal::new(high_water).map_err(|_| {
        SyndicValidationError::Invariant("compaction accepted-work witness has no input ordinal")
    })?;
    if receipt.source_gate().live_next_turn_count() == 0 {
        return invariant("compaction accepted-work witness has no next-turn input");
    }
    let order = require::<AcceptedOrderFamily>(
        reader,
        &ThreadAcceptedKey {
            owner: operation.target().thread_id(),
            ordinal,
        },
        "compaction accepted-work order witness is missing",
    )?;
    let input = require::<AcceptedInputsFamily>(
        reader,
        &order.input_id(),
        "compaction accepted-work input witness is missing",
    )?;
    if order.thread_id() != operation.target().thread_id()
        || order.ordinal() != ordinal
        || input.thread_id() != operation.target().thread_id()
        || input.ordinal() != ordinal
        || input.route_generation() != order.route_generation()
        || input.admission_gate_revision() > receipt.source_gate().revision()
    {
        return invariant("compaction accepted-work successor disagrees");
    }
    Ok(())
}

fn validate_retired_successor(
    reader: &DomainReader<'_, SyndicDomain>,
    operation: &crate::CompactionOperationRecord,
) -> Result<(), SyndicValidationError> {
    let target = operation.target();
    let revision = target
        .binding_revision()
        .checked_next()
        .map_err(|_| SyndicValidationError::Invariant("compaction binding revision exhausted"))?;
    let binding = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: target.thread_id(),
            revision,
        },
        "compaction retired successor binding is missing",
    )?;
    let reservation = require::<CasThreadIndexFamily>(
        reader,
        &CasThreadKey::Record(target.cas_thread_id().clone()),
        "compaction retired successor reservation is missing",
    )?;
    if !matches!(binding.state(), BindingState::Stale(_))
        || reservation.thread_id() != target.thread_id()
        || reservation.retired_binding_revision() != Some(revision)
    {
        return invariant("compaction retired settlement successor disagrees");
    }
    Ok(())
}

pub(super) fn validate_complete_terminal_authority(
    reader: &DomainReader<'_, SyndicDomain>,
    state: &TurnStateRecord,
) -> Result<bool, SyndicValidationError> {
    if state.lifecycle() != TurnLifecycle::Complete {
        return Ok(false);
    }
    let turn = require::<TurnsFamily>(reader, &state.turn_id(), "complete turn record is missing")?;
    if turn.kind() != TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction) {
        return Ok(false);
    }
    if state.source_event_count() != 0
        || point::<SourceEventsFamily>(
            reader,
            &TurnEventKey {
                owner: state.turn_id(),
                ordinal: crate::SourceEventSequence::FIRST,
            },
        )?
        .is_some()
    {
        return invariant("complete compaction turn has ordinary terminal source authority");
    }

    let operation_id = CompactionOperationId::new(
        turn.origin_thread_id(),
        crate::CompactionOperationNonce::from_bytes(*state.turn_id().as_bytes()),
    );
    let operation = require::<CompactionOperationsFamily>(
        reader,
        &operation_id,
        "complete compaction turn is missing its terminal operation authority",
    )?;
    let Some(terminal) = operation.terminal() else {
        return invariant("complete compaction turn is missing its terminal operation authority");
    };
    if operation.target().thread_id() != turn.origin_thread_id()
        || operation.target().turn_id() != state.turn_id()
        || Some(terminal.status()) != state.end_status()
        || terminal.turn_state_revision() != state.revision()
    {
        return invariant("compaction terminal authority and complete turn state disagree");
    }
    Ok(true)
}

pub(super) fn reject_ordinary_complete_terminal(
    reader: &DomainReader<'_, SyndicDomain>,
    turn_id: beryl_model::SyndicTurnId,
) -> Result<(), SyndicValidationError> {
    let turn = require::<TurnsFamily>(
        reader,
        &turn_id,
        "complete terminal event owner turn is missing",
    )?;
    if turn.kind() == TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction) {
        return invariant("complete compaction turn has ordinary terminal source authority");
    }
    Ok(())
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
