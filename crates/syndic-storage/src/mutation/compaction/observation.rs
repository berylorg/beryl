use super::*;

impl DomainMutation<SyndicDomain> for ClaimMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.successor(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let successor = self.successor(reader)?;
        mutations.put::<CompactionOperationsCodec>(&successor.id(), &successor)?;
        Ok(())
    }
}

impl ClaimMutation {
    fn successor(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<CompactionOperationRecord, SyndicMutationError> {
        let request = self.0;
        let (_, operation) = live_operation(
            reader,
            request.operation_id,
            request.expected_operation_revision,
        )?;
        operation
            .claim_dispatch(request.attempt)
            .map_err(SyndicMutationError::from)
    }
}

impl DomainMutation<SyndicDomain> for RequestMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.successor(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let successor = self.successor(reader)?;
        mutations.put::<CompactionOperationsCodec>(&successor.id(), &successor)?;
        Ok(())
    }
}

impl RequestMutation {
    fn successor(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<CompactionOperationRecord, SyndicMutationError> {
        let request = self.0;
        let (_, operation) = live_operation(
            reader,
            request.operation_id,
            request.expected_operation_revision,
        )?;
        if operation.attempt() != request.attempt {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        operation
            .observe_request(request.disposition)
            .map_err(SyndicMutationError::from)
    }
}

struct ProviderRecords {
    operation: CompactionOperationRecord,
    turn_state: Option<TurnStateRecord>,
    active_cas_turn: Option<ActiveCasTurnRecord>,
    cas_turn_index: Option<CasTurnIndexRecord>,
    gate: Option<InputGateRecord>,
    stop: Option<StopOperationRecord>,
}

impl DomainMutation<SyndicDomain> for ProviderMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = self.records(reader)?;
        mutations.put::<CompactionOperationsCodec>(&records.operation.id(), &records.operation)?;
        if let Some(state) = records.turn_state {
            mutations.put::<TurnStatesCodec>(&state.turn_id(), &state)?;
        }
        if let Some(active) = records.active_cas_turn {
            mutations.put::<ActiveCasTurnsCodec>(&active.snapshot_id(), &active)?;
        }
        if let Some(index) = records.cas_turn_index {
            mutations.put::<CasTurnIndexCodec>(
                &CasTurnKey::Record(index.cas_thread_id().clone(), index.cas_turn_id().clone()),
                &index,
            )?;
        }
        if let Some(gate) = records.gate {
            mutations.put::<InputGatesCodec>(&gate.thread_id(), &gate)?;
        }
        if let Some(stop) = records.stop {
            mutations.put::<StopOperationsCodec>(&stop.id(), &stop)?;
        }
        Ok(())
    }
}

impl ProviderMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<ProviderRecords, SyndicMutationError> {
        let request = &self.0;
        let (gate, operation) = provider_event_operation(
            reader,
            request.operation_id,
            request.expected_operation_revision,
        )?;
        let target = operation.target();
        let current_state = required::<TurnStatesFamily>(reader, &target.turn_id())?;
        if current_state.turn_id() != target.turn_id()
            || current_state.lifecycle().is_proven_terminal()
            || current_state.lifecycle() == TurnLifecycle::UnknownTerminal
        {
            return Err(SyndicMutationError::TurnLifecycleConflict);
        }

        match &request.event {
            CompactionProviderEvent::ThreadStatus(status) => Ok(ProviderRecords {
                operation: operation.observe_status(request.sequence, *status)?,
                turn_state: None,
                active_cas_turn: None,
                cas_turn_index: None,
                gate: None,
                stop: None,
            }),
            CompactionProviderEvent::TurnStarted(cas_turn_id) => {
                if point::<ActiveCasTurnsFamily>(reader, &target.snapshot_id())?.is_some()
                    || point::<CasTurnIndexFamily>(
                        reader,
                        &CasTurnKey::Record(target.cas_thread_id().clone(), cas_turn_id.clone()),
                    )?
                    .is_some()
                {
                    return Err(SyndicMutationError::CasTurnOwnershipConflict);
                }
                let state_revision = current_state.revision().checked_next()?;
                let state = TurnStateRecord::with_capture_frontiers_and_issue(
                    target.turn_id(),
                    state_revision,
                    TurnLifecycle::Active,
                    current_state.source_event_count(),
                    current_state.item_count(),
                    current_state.finalized_item_count(),
                    current_state.open_item_count(),
                    current_state.history_blocking_item_count(),
                    current_state.provider_observation_issue(),
                    None,
                    request.observed_at,
                )?;
                let snapshot = required::<ExecutionSnapshotsFamily>(reader, &target.snapshot_id())?;
                let active = ActiveCasTurnRecord::new(
                    target.snapshot_id(),
                    target.thread_id(),
                    target.turn_id(),
                    target.binding_revision(),
                    target.cas_thread_id().clone(),
                    cas_turn_id.clone(),
                    request.observed_at,
                );
                let index = CasTurnIndexRecord::new(
                    target.cas_thread_id().clone(),
                    cas_turn_id.clone(),
                    target.thread_id(),
                    target.turn_id(),
                    target.binding_revision(),
                    target.snapshot_id(),
                    snapshot.represented_base_native_turn_count(),
                );
                Ok(ProviderRecords {
                    operation: operation.observe_cas_turn(request.sequence, cas_turn_id.clone())?,
                    turn_state: Some(state),
                    active_cas_turn: Some(active),
                    cas_turn_index: Some(index),
                    gate: None,
                    stop: None,
                })
            }
            CompactionProviderEvent::Marker { item_id, lifecycle } => Ok(ProviderRecords {
                operation: operation.observe_marker(CompactionMarkerObservation::new(
                    request.sequence,
                    *item_id,
                    *lifecycle,
                ))?,
                turn_state: None,
                active_cas_turn: None,
                cas_turn_index: None,
                gate: None,
                stop: None,
            }),
            CompactionProviderEvent::Terminal(status) => {
                let revision = current_state.revision().checked_next()?;
                let state = TurnStateRecord::with_capture_frontiers_and_issue(
                    target.turn_id(),
                    revision,
                    status.lifecycle(),
                    current_state.source_event_count(),
                    current_state.item_count(),
                    current_state.finalized_item_count(),
                    current_state.open_item_count(),
                    current_state.history_blocking_item_count(),
                    current_state.provider_observation_issue(),
                    Some(*status),
                    request.observed_at,
                )?;
                let source_compaction_revision = operation.revision();
                let operation = operation.observe_terminal(request.sequence, *status, revision)?;
                let (successor_gate, stop) = provider_terminal_stop(
                    reader,
                    &gate,
                    &operation,
                    source_compaction_revision,
                    revision,
                )?;
                Ok(ProviderRecords {
                    operation,
                    turn_state: Some(state),
                    active_cas_turn: None,
                    cas_turn_index: None,
                    gate: successor_gate,
                    stop,
                })
            }
        }
    }
}
