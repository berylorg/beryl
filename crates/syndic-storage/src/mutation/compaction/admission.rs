use super::*;

pub struct AdmissionRecords {
    turn: TurnRecord,
    state: TurnStateRecord,
    snapshot: ExecutionSnapshotRecord,
    operation: CompactionOperationRecord,
    gate: InputGateRecord,
}

impl DomainMutation<SyndicDomain> for AdmitMutation {
    type Error = SyndicMutationError;
    type Prepared = AdmissionRecords;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        self.records(reader)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<TurnsCodec>(1)?;
        reservation.reserve_records::<TurnStatesCodec>(1)?;
        reservation.reserve_records::<ExecutionSnapshotsCodec>(1)?;
        reservation.reserve_records::<CompactionOperationsCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = prepared;
        mutations.put::<TurnsCodec>(&records.turn.id(), &records.turn)?;
        mutations.put::<TurnStatesCodec>(&records.state.turn_id(), &records.state)?;
        mutations.put::<ExecutionSnapshotsCodec>(&records.snapshot.id(), &records.snapshot)?;
        mutations.put::<CompactionOperationsCodec>(&records.operation.id(), &records.operation)?;
        mutations.put::<InputGatesCodec>(&records.gate.thread_id(), &records.gate)?;
        Ok(())
    }
}

impl AdmitMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<AdmissionRecords, SyndicMutationError> {
        let request = &self.0;
        let target = &request.target;
        if request.operation_id.thread_id() != target.thread_id()
            || request.operation_id.provider_turn_id() != target.turn_id()
            || point::<CompactionOperationsFamily>(reader, &request.operation_id)?.is_some()
            || point::<TurnsFamily>(reader, &target.turn_id())?.is_some()
            || point::<TurnStatesFamily>(reader, &target.turn_id())?.is_some()
            || point::<ExecutionSnapshotsFamily>(reader, &target.snapshot_id())?.is_some()
            || point::<ActiveCasTurnsFamily>(reader, &target.snapshot_id())?.is_some()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }

        let thread = required::<ThreadsFamily>(reader, &target.thread_id())?;
        let gate = required::<InputGatesFamily>(reader, &target.thread_id())?;
        if gate.revision() != request.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision,
                current: gate.revision(),
            });
        }
        if gate.state() != &InputGateState::Idle
            || gate.live_count() != 0
            || gate.live_logical_utf8_bytes() != 0
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }

        let head = required::<BindingHeadsFamily>(reader, &target.thread_id())?;
        if head.revision() != target.binding_revision()
            || head.lifecycle() != BindingLifecycle::Valid
            || head.selected_path_digest() != thread.selected_path_digest()
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        let binding = required::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: target.thread_id(),
                revision: target.binding_revision(),
            },
        )?;
        let BindingState::Valid(usable) = binding.state() else {
            return Err(SyndicMutationError::BindingStateConflict);
        };
        if binding.thread_id() != target.thread_id()
            || binding.selected_path().tail() != thread.committed_tail()
            || binding.selected_path().digest() != thread.selected_path_digest()
            || binding.selected_path().thread_revision() > thread.revision()
            || usable.execution().runtime_id() != target.runtime_id()
            || usable.cas_thread_id() != target.cas_thread_id()
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        if target.snapshot_id()
            != crate::derive_compaction_snapshot_id(
                request.home_id,
                request.operation_id,
                target.turn_id(),
                request.expected_gate_revision,
                target.binding_revision(),
                usable.represented_prefix(),
                target.cas_thread_id(),
                target.loaded_generation(),
            )
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        validate_cas_reverse(
            reader,
            target.thread_id(),
            target.binding_revision(),
            target.cas_thread_id(),
        )?;

        let successor_gate_revision = gate.revision().checked_next()?;
        let snapshot = ExecutionSnapshotRecord::provider_operation(
            target.snapshot_id(),
            target.thread_id(),
            target.binding_revision(),
            gate.revision(),
            target.turn_id(),
            target.cas_thread_id().clone(),
            binding.selected_path(),
            usable.represented_prefix(),
            usable.native_turn_count(),
            usable.tool_profile(),
            usable.lineage(),
            usable.execution().clone(),
            target.loaded_generation(),
            request.started_at,
        );
        let turn = TurnRecord::new(
            target.turn_id(),
            target.thread_id(),
            TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction),
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            root_turn_chain_digest(target.turn_id()),
            request.started_at,
        );
        let state = TurnStateRecord::new(
            target.turn_id(),
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            None,
            request.started_at,
        )?;
        let operation = CompactionOperationRecord::new(
            request.operation_id,
            request.home_id,
            target.clone(),
            CompactionOperationRevision::FIRST,
            request.attempt,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            CompactionOperationState::Admitted,
        )?;
        let gate = InputGateRecord::new(
            gate.thread_id(),
            successor_gate_revision,
            InputGateState::compacting(target.turn_id(), request.operation_id.nonce()),
            gate.accepted_high_water(),
            gate.route_generation_high_water(),
            gate.selected_route(),
            0,
            0,
            0,
        )?;
        Ok(AdmissionRecords {
            turn,
            state,
            snapshot,
            operation,
            gate,
        })
    }
}

fn validate_cas_reverse(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    binding_revision: BindingRevision,
    cas_thread_id: &CasThreadId,
) -> Result<(), SyndicMutationError> {
    let owner =
        required::<CasThreadIndexFamily>(reader, &CasThreadKey::Record(cas_thread_id.clone()))?;
    let member = required::<CasThreadBindingIndexFamily>(
        reader,
        &CasThreadBindingKey::Record(cas_thread_id.clone(), binding_revision),
    )?;
    if owner.thread_id() != thread_id
        || owner.latest_binding_revision() != binding_revision
        || owner.retired_binding_revision().is_some()
        || member
            != CasThreadBindingIndexRecord::new(cas_thread_id.clone(), thread_id, binding_revision)
    {
        return Err(SyndicMutationError::CasThreadRetired);
    }
    Ok(())
}

pub(super) fn live_operation(
    reader: &DomainReader<'_, SyndicDomain>,
    operation_id: CompactionOperationId,
    expected_revision: CompactionOperationRevision,
) -> Result<(InputGateRecord, CompactionOperationRecord), SyndicMutationError> {
    let operation = required::<CompactionOperationsFamily>(reader, &operation_id)?;
    if operation.id() != operation_id || operation.revision() != expected_revision {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let gate = required::<InputGatesFamily>(reader, &operation_id.thread_id())?;
    if gate.state()
        != &InputGateState::compacting(operation.target().turn_id(), operation_id.nonce())
        || gate.live_steering_count() != 0
    {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    validate_provider_target(reader, &operation)?;
    Ok((gate, operation))
}

pub(super) fn provider_event_operation(
    reader: &DomainReader<'_, SyndicDomain>,
    operation_id: CompactionOperationId,
    expected_revision: CompactionOperationRevision,
) -> Result<(InputGateRecord, CompactionOperationRecord), SyndicMutationError> {
    let operation = required::<CompactionOperationsFamily>(reader, &operation_id)?;
    if operation.id() != operation_id
        || operation.revision() != expected_revision
        || operation.terminal().is_some()
    {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let gate = required::<InputGatesFamily>(reader, &operation_id.thread_id())?;
    let selected = match operation.state() {
        CompactionOperationState::Stopping(stop_nonce) => {
            gate.state() == &InputGateState::stopping(operation.target().turn_id(), *stop_nonce)
        }
        state if state.is_live() => {
            gate.state()
                == &InputGateState::compacting(operation.target().turn_id(), operation_id.nonce())
        }
        _ => false,
    };
    if !selected || gate.live_steering_count() != 0 {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    validate_provider_target(reader, &operation)?;
    Ok((gate, operation))
}

pub(super) fn provider_terminal_stop(
    reader: &DomainReader<'_, SyndicDomain>,
    current_gate: &InputGateRecord,
    operation: &CompactionOperationRecord,
    source_compaction_revision: crate::CompactionOperationRevision,
    successor_turn_state_revision: TurnStateRevision,
) -> Result<(Option<InputGateRecord>, Option<StopOperationRecord>), SyndicMutationError> {
    let InputGateState::Stopping {
        turn_id,
        operation_nonce,
    } = current_gate.state()
    else {
        return Ok((None, None));
    };
    if *turn_id != operation.target().turn_id() {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let stop_id = StopOperationId::new(current_gate.thread_id(), *operation_nonce);
    let current_stop = required::<StopOperationsFamily>(reader, &stop_id)?;
    let Some(stop_compaction_successor) = current_stop.admission().successor_compaction_revision()
    else {
        return Err(SyndicMutationError::InputGateStateConflict);
    };
    let target = current_stop.target();
    let cas_turn = operation
        .cas_turn()
        .ok_or(SyndicMutationError::CasTurnOwnershipConflict)?;
    if !current_stop.state().is_live()
        || !current_stop.admission().is_provider_operation()
        || operation.revision() <= stop_compaction_successor
        || target.thread_id() != operation.target().thread_id()
        || target.turn_id() != operation.target().turn_id()
        || target.turn_kind()
            != TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction)
        || target.binding_revision() != operation.target().binding_revision()
        || target.snapshot_id() != operation.target().snapshot_id()
        || target.runtime_id() != operation.target().runtime_id()
        || target.loaded_generation() != operation.target().loaded_generation()
        || target.cas_thread_id() != operation.target().cas_thread_id()
        || target.cas_turn_id() != cas_turn.cas_turn_id()
    {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let gate_revision = current_gate.revision().checked_next()?;
    let gate = InputGateRecord::new(
        current_gate.thread_id(),
        gate_revision,
        InputGateState::compacting(operation.target().turn_id(), operation.id().nonce()),
        current_gate.accepted_high_water(),
        current_gate.route_generation_high_water(),
        current_gate.selected_route(),
        0,
        current_gate.live_next_turn_count(),
        current_gate.live_logical_utf8_bytes(),
    )?;
    let witness = StopMatchingTerminalWitness::provider_operation(
        StopDispositionSource::new(current_gate.revision(), current_stop.revision()),
        gate_revision,
        successor_turn_state_revision,
        source_compaction_revision,
        operation.revision(),
    );
    let stop = StopOperationRecord::new(
        current_stop.id(),
        current_stop.target().clone(),
        current_stop.admission(),
        current_stop.revision().checked_next()?,
        current_stop.cause_first_revisions(),
        current_stop.dispatch_claim(),
        StopOperationState::MatchingTerminal(witness),
    )
    .map_err(|_| SyndicMutationError::InputGateStateConflict)?;
    Ok((Some(gate), Some(stop)))
}

fn validate_provider_target(
    reader: &DomainReader<'_, SyndicDomain>,
    operation: &CompactionOperationRecord,
) -> Result<(), SyndicMutationError> {
    let target = operation.target();
    let turn = required::<TurnsFamily>(reader, &target.turn_id())?;
    let snapshot = required::<ExecutionSnapshotsFamily>(reader, &target.snapshot_id())?;
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: target.thread_id(),
            revision: target.binding_revision(),
        },
    )?;
    let BindingState::Valid(usable) = binding.state() else {
        return Err(SyndicMutationError::BindingStateConflict);
    };
    if turn.origin_thread_id() != target.thread_id()
        || turn.kind() != TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction)
        || turn.parent() != ConversationParent::Root
        || snapshot.kind()
            != ExecutionSnapshotKind::ProviderOperation(ProviderOperationKind::ContextCompaction)
        || snapshot.thread_id() != target.thread_id()
        || snapshot.active_turn_id() != target.turn_id()
        || snapshot.binding_revision() != target.binding_revision()
        || snapshot.cas_thread_id() != target.cas_thread_id()
        || snapshot.execution().runtime_id() != target.runtime_id()
        || snapshot.loaded_generation() != target.loaded_generation()
        || usable.cas_thread_id() != target.cas_thread_id()
        || usable.execution().runtime_id() != target.runtime_id()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    validate_cas_reverse(
        reader,
        target.thread_id(),
        target.binding_revision(),
        target.cas_thread_id(),
    )
}
