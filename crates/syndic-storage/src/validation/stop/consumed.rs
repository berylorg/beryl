use super::*;

fn validate_consumed_gate(
    reader: &Reader<'_>,
    record: &StopOperationRecord,
    source: crate::StopDispositionSource,
    successor_revision: beryl_model::InputGateRevision,
) -> Result<InputGateRecord, SyndicValidationError> {
    if source.gate_revision() < record.admission().successor_gate_revision() {
        return invariant("stop disposition predates stop admission");
    }
    let gate = require::<InputGatesFamily>(
        reader,
        &record.target().thread_id(),
        "consumed stop operation input gate is missing",
    )?;
    if gate.revision() < successor_revision {
        return invariant("consumed stop operation successor gate is missing");
    }
    Ok(gate)
}

fn expected_steering_target(target: &StopOperationTarget) -> SteeringTargetProof {
    SteeringTargetProof::new(
        crate::PendingSteeringTargetProof::new(
            target.binding_revision(),
            target.snapshot_id(),
            target.turn_id(),
            target.cas_thread_id().clone(),
        ),
        target.cas_turn_id().clone(),
    )
}

pub(super) fn validate_safe_reopen(
    reader: &Reader<'_>,
    record: &StopOperationRecord,
    witness: crate::StopSafeReopenWitness,
) -> Result<(), SyndicValidationError> {
    if is_provider_operation(record) {
        let crate::StopSafeReopenWitness::ProviderOperation {
            source_compaction_revision,
            successor_compaction_revision,
            ..
        } = witness
        else {
            return invariant("provider safe-reopen witness carries ordinary route authority");
        };
        if record.causes().contains(StopCause::InterruptingApproval) {
            return invariant("provider safe-reopen cannot consume interrupting approval");
        }
        let gate = validate_consumed_gate(
            reader,
            record,
            witness.source(),
            witness.successor_gate_revision(),
        )?;
        let operation_id = crate::CompactionOperationId::new(
            record.target().thread_id(),
            crate::CompactionOperationNonce::from_bytes(*record.target().turn_id().as_bytes()),
        );
        let operation = require::<CompactionOperationsFamily>(
            reader,
            &operation_id,
            "provider safe-reopen compaction successor is missing",
        )?;
        let (Some(admission_source), Some(admission_successor)) = (
            record.admission().source_compaction_revision(),
            record.admission().successor_compaction_revision(),
        ) else {
            return invariant("provider safe-reopen admission omits compaction revisions");
        };
        let receipt = if matches!(
            operation.state(),
            crate::CompactionOperationState::Consumed(_)
        ) {
            Some(require::<CompactionSettlementReceiptsFamily>(
                reader,
                &operation_id,
                "provider safe-reopen consumed descendant receipt is missing",
            )?)
        } else {
            None
        };
        if !operation.safe_reopen_descendant_is_exact(
            admission_source,
            admission_successor,
            source_compaction_revision,
            successor_compaction_revision,
            receipt.as_ref(),
        ) || (gate.revision() == witness.successor_gate_revision()
            && gate.state()
                != &InputGateState::compacting(record.target().turn_id(), operation_id.nonce()))
        {
            return invariant("provider safe-reopen successor authority disagrees");
        }
        return Ok(());
    }
    if record.causes().contains(StopCause::InterruptingApproval)
        || witness.successor_route().revision() != crate::AcceptedRouteRevision::FIRST
        || witness.successor_route().generation()
            <= record.admission().successor_stopped_route().generation()
    {
        return invariant("safe-reopened stop operation has an invalid successor witness");
    }
    let gate = validate_consumed_gate(
        reader,
        record,
        witness.source(),
        witness.successor_gate_revision(),
    )?;
    let route = require::<AcceptedRouteGenerationsFamily>(
        reader,
        &ThreadRouteKey {
            thread: record.target().thread_id(),
            generation: witness.successor_route().generation(),
        },
        "safe-reopened stop successor route is missing",
    )?;
    if route.revision() < witness.successor_route().revision()
        || gate
            .route_generation_high_water()
            .is_none_or(|high_water| high_water < witness.successor_route().generation())
    {
        return invariant("safe-reopened stop route descendant disagrees");
    }
    if route.revision() == witness.successor_route().revision() && route.input_count() != 0 {
        return invariant("safe-reopened stop immediate route is not fresh and empty");
    }
    validate_reopened_route_descendant(record, witness.successor_route(), &route)?;

    if gate.revision() == witness.successor_gate_revision()
        && (!matches!(
            gate.state(),
            InputGateState::Steerable(turn) if *turn == record.target().turn_id()
        ) || gate.selected_route() != Some(witness.successor_route())
            || gate.route_generation_high_water() != Some(witness.successor_route().generation())
            || gate.live_steering_count() != 0)
    {
        return invariant("safe-reopened stop immediate gate successor disagrees");
    }
    Ok(())
}

fn validate_reopened_route_descendant(
    record: &StopOperationRecord,
    successor: AcceptedRouteHeadProof,
    route: &AcceptedRouteGenerationRecord,
) -> Result<(), SyndicValidationError> {
    let expected = expected_steering_target(record.target());
    match route.target() {
        AcceptedRouteTarget::Steering(current) if current == &expected => Ok(()),
        AcceptedRouteTarget::ProjectionLost(lost)
            if matches!(
                lost.prior_target(),
                AcceptedRouteLostTarget::Steering(current) if current == &expected
            ) && lost.abandonment().expected_route().generation() == successor.generation()
                && lost.abandonment().expected_route().revision() >= successor.revision() =>
        {
            Ok(())
        }
        AcceptedRouteTarget::NextTurn(NextTurnReason::Stop)
            if route.revision() > successor.revision() =>
        {
            Ok(())
        }
        _ => invariant("safe-reopened stop route has no authenticated target descendant"),
    }
}

fn validate_finalizing_gate(
    reader: &Reader<'_>,
    record: &StopOperationRecord,
    source: crate::StopDispositionSource,
    successor: beryl_model::InputGateRevision,
) -> Result<(), SyndicValidationError> {
    let gate = validate_consumed_gate(reader, record, source, successor)?;
    if gate.revision() == successor
        && !matches!(
            gate.state(),
            InputGateState::FinalizingHistory(turn) if *turn == record.target().turn_id()
        )
    {
        return invariant("consumed stop immediate finalizing gate disagrees");
    }
    Ok(())
}

fn validate_terminal_descendant(
    reader: &Reader<'_>,
    record: &StopOperationRecord,
    successor_revision: crate::TurnStateRevision,
    authority_lost: bool,
) -> Result<(), SyndicValidationError> {
    let state = require::<TurnStatesFamily>(
        reader,
        &record.target().turn_id(),
        "consumed stop turn state is missing",
    )?;
    let lifecycle_matches = if authority_lost {
        state.lifecycle() == TurnLifecycle::Incomplete
            && state.incomplete_reason() == Some(TurnIncompleteReason::AuthorityLost)
    } else {
        state.lifecycle().is_proven_terminal()
    };
    if state.revision() < successor_revision
        || !lifecycle_matches
        || state.source_event_count() == 0
    {
        return invariant("consumed stop turn-state descendant disagrees");
    }
    let sequence = SourceEventSequence::new(state.source_event_count())
        .map_err(|_| SyndicValidationError::Invariant("stop source-event frontier is invalid"))?;
    let event = require::<SourceEventsFamily>(
        reader,
        &TurnEventKey {
            owner: record.target().turn_id(),
            ordinal: sequence,
        },
        "consumed stop terminal event is missing",
    )?;
    let terminal_matches = matches!(
        event.payload(),
        SourceEventPayload::TurnEnded(status) if state.end_status() == Some(*status)
    );
    let source_matches = if authority_lost {
        event.source().is_none()
            && state.incomplete_reason() == Some(TurnIncompleteReason::AuthorityLost)
    } else {
        event.source().is_some_and(|source| {
            source.thread_id() == record.target().cas_thread_id()
                && source.turn_id() == record.target().cas_turn_id()
        })
    };
    if !terminal_matches || !source_matches {
        return invariant("consumed stop terminal authority disagrees");
    }
    Ok(())
}

pub(super) fn validate_matching_terminal(
    reader: &Reader<'_>,
    record: &StopOperationRecord,
    witness: crate::StopMatchingTerminalWitness,
) -> Result<(), SyndicValidationError> {
    if is_provider_operation(record) {
        let crate::StopMatchingTerminalWitness::ProviderOperation {
            source_compaction_revision,
            successor_compaction_revision,
            ..
        } = witness
        else {
            return invariant("provider matching-terminal witness carries ordinary authority");
        };
        let gate = validate_consumed_gate(
            reader,
            record,
            witness.source(),
            witness.successor_gate_revision(),
        )?;
        let state = require::<TurnStatesFamily>(
            reader,
            &record.target().turn_id(),
            "provider matching-terminal state is missing",
        )?;
        let operation_id = crate::CompactionOperationId::new(
            record.target().thread_id(),
            crate::CompactionOperationNonce::from_bytes(*record.target().turn_id().as_bytes()),
        );
        let operation = require::<CompactionOperationsFamily>(
            reader,
            &operation_id,
            "provider matching-terminal compaction successor is missing",
        )?;
        let (Some(admission_source), Some(admission_successor)) = (
            record.admission().source_compaction_revision(),
            record.admission().successor_compaction_revision(),
        ) else {
            return invariant("provider matching-terminal admission omits compaction revisions");
        };
        let receipt = if matches!(
            operation.state(),
            crate::CompactionOperationState::Consumed(_)
        ) {
            Some(require::<CompactionSettlementReceiptsFamily>(
                reader,
                &operation_id,
                "provider matching-terminal consumed descendant receipt is missing",
            )?)
        } else {
            None
        };
        let binding = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: record.target().thread_id(),
                revision: record.target().binding_revision(),
            },
            "provider matching-terminal valid binding is missing",
        )?;
        if gate.revision() == witness.successor_gate_revision()
            && gate.state()
                != &InputGateState::compacting(record.target().turn_id(), operation_id.nonce())
            || state.revision() < witness.successor_turn_state_revision()
            || !state.lifecycle().is_proven_terminal()
            || !operation.matching_terminal_descendant_is_exact(
                admission_source,
                admission_successor,
                source_compaction_revision,
                successor_compaction_revision,
                witness.successor_turn_state_revision(),
                receipt.as_ref(),
            )
            || !matches!(binding.state(), BindingState::Valid(_))
        {
            return invariant("provider matching-terminal successor authority disagrees");
        }
        return Ok(());
    }
    validate_finalizing_gate(
        reader,
        record,
        witness.source(),
        witness.successor_gate_revision(),
    )?;
    validate_terminal_descendant(
        reader,
        record,
        witness.successor_turn_state_revision(),
        false,
    )?;
    let successor_revision = successor_binding_revision(record.target())?;
    let successor = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: record.target().thread_id(),
            revision: successor_revision,
        },
        "matching-terminal stop binding successor is missing",
    )?;
    let BindingState::Valid(_) = successor.state() else {
        return invariant("matching-terminal stop binding successor is not valid");
    };
    Ok(())
}

pub(super) fn validate_abandoned(
    reader: &Reader<'_>,
    record: &StopOperationRecord,
    witness: crate::StopAbandonmentWitness,
) -> Result<(), SyndicValidationError> {
    if is_provider_operation(record) {
        let crate::StopAbandonmentWitness::ProviderOperation {
            source_compaction_revision,
            successor_compaction_revision,
            ..
        } = witness
        else {
            return invariant("provider abandonment witness carries ordinary authority");
        };
        let gate = validate_consumed_gate(
            reader,
            record,
            witness.source(),
            witness.successor_gate_revision(),
        )?;
        let operation_id = crate::CompactionOperationId::new(
            record.target().thread_id(),
            crate::CompactionOperationNonce::from_bytes(*record.target().turn_id().as_bytes()),
        );
        let operation = require::<CompactionOperationsFamily>(
            reader,
            &operation_id,
            "abandoned provider stop compaction successor is missing",
        )?;
        let receipt = require::<CompactionSettlementReceiptsFamily>(
            reader,
            &operation_id,
            "abandoned provider stop compaction settlement receipt is missing",
        )?;
        let (Some(admission_source), Some(admission_successor)) = (
            record.admission().source_compaction_revision(),
            record.admission().successor_compaction_revision(),
        ) else {
            return invariant("abandoned provider stop admission omits compaction revisions");
        };
        let expected_compaction_reason = match witness.reason() {
            crate::StopAbandonmentReason::StartupProcessGenerationLost => {
                crate::CompactionAbandonmentReason::StartupProcessGenerationLost
            }
            crate::StopAbandonmentReason::ProviderRejectedBeforeCoreInterrupt
            | crate::StopAbandonmentReason::TargetAuthorityLost => {
                crate::CompactionAbandonmentReason::TargetAuthorityLost
            }
        };
        if gate.revision() == witness.successor_gate_revision()
            && gate.state() != &InputGateState::Idle
            || !matches!(
                operation.state(),
                crate::CompactionOperationState::Consumed(_)
            )
            || !operation.stop_abandonment_successor_is_exact(
                admission_source,
                admission_successor,
                source_compaction_revision,
                successor_compaction_revision,
                &receipt,
            )
            || receipt.source_gate().revision() != witness.source().gate_revision()
            || receipt.successor_gate().revision() != witness.successor_gate_revision()
            || receipt.source_gate().state()
                != &InputGateState::stopping(record.target().turn_id(), record.id().nonce())
            || receipt.settlement()
                != &crate::CompactionSettlement::Abandoned(expected_compaction_reason)
        {
            return invariant("abandoned provider stop gate or compaction successor disagrees");
        }
        let expected_retirement = successor_binding_revision(record.target())?;
        if witness.retired_binding_revision() != expected_retirement {
            return invariant("abandoned provider stop retirement revision disagrees");
        }
        let retired = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: record.target().thread_id(),
                revision: witness.retired_binding_revision(),
            },
            "abandoned provider stop stale binding successor is missing",
        )?;
        if !matches!(retired.state(), BindingState::Stale(_)) {
            return invariant("abandoned provider stop binding successor is not stale");
        }
        return validate_terminal_descendant(
            reader,
            record,
            witness.successor_turn_state_revision(),
            true,
        );
    }
    validate_finalizing_gate(
        reader,
        record,
        witness.source(),
        witness.successor_gate_revision(),
    )?;
    let expected_retirement = successor_binding_revision(record.target())?;
    if witness.retired_binding_revision() != expected_retirement {
        return invariant("abandoned stop retirement revision disagrees");
    }
    let retired = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: record.target().thread_id(),
            revision: witness.retired_binding_revision(),
        },
        "abandoned stop stale binding successor is missing",
    )?;
    let BindingState::Stale(_) = retired.state() else {
        return invariant("abandoned stop binding successor is not stale");
    };
    validate_terminal_descendant(
        reader,
        record,
        witness.successor_turn_state_revision(),
        true,
    )
}

pub(super) fn validate_abandoned_route(
    record: &StopOperationRecord,
    witness: crate::StopAbandonmentWitness,
    route: &AcceptedRouteGenerationRecord,
) -> Result<(), SyndicValidationError> {
    let AcceptedRouteTarget::ProjectionLost(lost) = route.target() else {
        return invariant("abandoned stop route is not projection-lost");
    };
    let stopped = record.admission().successor_stopped_route();
    let expected_target = expected_steering_target(record.target());
    let abandonment = lost.abandonment();
    if !matches!(
        lost.prior_target(),
        AcceptedRouteLostTarget::Steering(target) if target == &expected_target
    ) || abandonment.expected_binding_revision() != record.target().binding_revision()
        || abandonment.expected_gate_revision() != witness.source().gate_revision()
        || abandonment.expected_route() != stopped
        || abandonment.kind() != AcceptedRouteAbandonmentKind::Generic
        || lost.retirement_binding_revision() != witness.retired_binding_revision()
        || lost.snapshot_id() != record.target().snapshot_id()
        || lost.cas_thread_id() != record.target().cas_thread_id()
        || route.revision() <= stopped.revision()
    {
        return invariant("abandoned stop projection-loss witness disagrees");
    }
    Ok(())
}
