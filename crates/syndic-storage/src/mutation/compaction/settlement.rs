use super::*;

pub(super) struct SettlementRecords {
    pub(super) operation: CompactionOperationRecord,
    pub(super) receipt: CompactionSettlementReceiptRecord,
    pub(super) gate: InputGateRecord,
    event: Option<crate::SourceEventRecord>,
    turn_state: Option<TurnStateRecord>,
    retirement: Option<SettlementRetirement>,
}

struct SettlementRetirement {
    binding: crate::BindingRecord,
    binding_head: crate::BindingHeadRecord,
    reservation: crate::CasThreadIndexRecord,
    membership: crate::CasThreadBindingIndexRecord,
}

impl DomainMutation<SyndicDomain> for SettleMutation {
    type Error = SyndicMutationError;
    type Prepared = SettlementRecords;

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
        reservation.reserve_records::<CompactionOperationsCodec>(1)?;
        reservation.reserve_records::<CompactionSettlementReceiptsCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        reservation.reserve_records::<SourceEventsCodec>(1)?;
        reservation.reserve_records::<TurnStatesCodec>(1)?;
        reservation.reserve_records::<BindingsCodec>(1)?;
        reservation.reserve_records::<BindingHeadsCodec>(1)?;
        reservation.reserve_records::<CasThreadIndexCodec>(1)?;
        reservation.reserve_records::<CasThreadBindingIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = prepared;
        mutations.put::<CompactionOperationsCodec>(&records.operation.id(), &records.operation)?;
        mutations.put::<CompactionSettlementReceiptsCodec>(
            &records.receipt.operation_id(),
            &records.receipt,
        )?;
        mutations.put::<InputGatesCodec>(&records.gate.thread_id(), &records.gate)?;
        if let Some(event) = &records.event {
            mutations.put::<SourceEventsCodec>(
                &TurnEventKey {
                    owner: event.turn_id(),
                    ordinal: event.sequence(),
                },
                event,
            )?;
        }
        if let Some(state) = &records.turn_state {
            mutations.put::<TurnStatesCodec>(&state.turn_id(), state)?;
        }
        if let Some(retirement) = &records.retirement {
            mutations.put::<BindingsCodec>(
                &BindingKey {
                    thread: retirement.binding.thread_id(),
                    revision: retirement.binding.revision(),
                },
                &retirement.binding,
            )?;
            mutations.put::<BindingHeadsCodec>(
                &retirement.binding_head.thread_id(),
                &retirement.binding_head,
            )?;
            mutations.put::<CasThreadIndexCodec>(
                &CasThreadKey::Record(retirement.reservation.cas_thread_id().clone()),
                &retirement.reservation,
            )?;
            mutations.put::<CasThreadBindingIndexCodec>(
                &CasThreadBindingKey::Record(
                    retirement.membership.cas_thread_id().clone(),
                    retirement.membership.binding_revision(),
                ),
                &retirement.membership,
            )?;
        }
        Ok(())
    }
}

impl SettleMutation {
    pub(super) fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<SettlementRecords, SyndicMutationError> {
        let request = &self.0;
        let (gate, operation) = live_operation(
            reader,
            request.operation_id,
            request.expected_operation_revision,
        )?;
        if !settlement_is_admitted(&operation, &request.settlement, gate.live_next_turn_count()) {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        if point::<CompactionSettlementReceiptsFamily>(reader, &operation.id())?.is_some() {
            return Err(SyndicMutationError::AdmissionIdentityCollision);
        }
        let current_state = required::<TurnStatesFamily>(reader, &operation.target().turn_id())?;
        let (event, turn_state) =
            settlement_turn_effect(reader, &operation, &request.settlement, &current_state)?;
        let retirement = settlement_retires_binding(&operation, &request.settlement)
            .then(|| settlement_retirement(reader, &operation, &current_state))
            .transpose()?;
        let successor_revision = gate.revision().checked_next()?;
        let successor = InputGateRecord::new(
            gate.thread_id(),
            successor_revision,
            InputGateState::Idle,
            gate.accepted_high_water(),
            gate.route_generation_high_water(),
            gate.selected_route(),
            0,
            gate.live_next_turn_count(),
            gate.live_logical_utf8_bytes(),
        )?;
        let successor_operation_revision = operation.revision().checked_next()?;
        let receipt = CompactionSettlementReceiptRecord::new(
            operation.id(),
            operation.revision(),
            successor_operation_revision,
            gate,
            successor.clone(),
            request.settlement.clone(),
            None,
        )?;
        let receipt_commitment = compaction_settlement_receipt_commitment(&receipt)
            .map_err(|_| crate::SyndicRecordError::InvalidCompactionOperation)?;
        let consumed = operation.consume(
            successor_revision,
            request.settlement.clone(),
            receipt_commitment,
        )?;
        Ok(SettlementRecords {
            operation: consumed,
            receipt,
            gate: successor,
            event,
            turn_state,
            retirement,
        })
    }
}

fn settlement_turn_effect(
    reader: &DomainReader<'_, SyndicDomain>,
    operation: &CompactionOperationRecord,
    settlement: &CompactionSettlement,
    current: &TurnStateRecord,
) -> Result<(Option<crate::SourceEventRecord>, Option<TurnStateRecord>), SyndicMutationError> {
    let status = match settlement {
        CompactionSettlement::CancelledBeforeDispatch | CompactionSettlement::LocalNondispatch => {
            Some(TurnEndStatus::new(
                crate::TurnTerminalOutcome::Failed,
                None,
            )?)
        }
        CompactionSettlement::Abandoned(_) => Some(TurnEndStatus::incomplete(
            crate::TurnIncompleteReason::AuthorityLost,
        )),
        CompactionSettlement::ManualFailure
            if operation.terminal().is_some_and(|terminal| {
                terminal.status().outcome() == crate::TurnTerminalOutcome::Complete
            }) =>
        {
            Some(TurnEndStatus::incomplete(
                crate::TurnIncompleteReason::CompletionMismatch,
            ))
        }
        _ => None,
    };
    let Some(status) = status else {
        return Ok((None, None));
    };
    let sequence = crate::SourceEventSequence::new(
        current
            .source_event_count()
            .checked_add(1)
            .ok_or(SyndicMutationError::SourceEventFrontierExhausted)?,
    )?;
    if point::<SourceEventsFamily>(
        reader,
        &TurnEventKey {
            owner: current.turn_id(),
            ordinal: sequence,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::SourceEventCollision);
    }
    let event = crate::SourceEventRecord::new(
        current.turn_id(),
        sequence,
        None,
        crate::SourceEventPayload::TurnEnded(status),
    )?;
    let state = TurnStateRecord::with_capture_frontiers_and_issue(
        current.turn_id(),
        current.revision().checked_next()?,
        status.lifecycle(),
        sequence.get(),
        current.item_count(),
        current.finalized_item_count(),
        current.open_item_count(),
        current.history_blocking_item_count(),
        current.provider_observation_issue(),
        Some(status),
        current.updated_at(),
    )?;
    Ok((Some(event), Some(state)))
}

fn settlement_retires_binding(
    operation: &CompactionOperationRecord,
    settlement: &CompactionSettlement,
) -> bool {
    match settlement {
        CompactionSettlement::Abandoned(_) => true,
        CompactionSettlement::ManualFailure => !operation.terminal().is_some_and(|terminal| {
            terminal.status().outcome() == crate::TurnTerminalOutcome::Interrupted
                && operation.status().is_some_and(|status| {
                    status.status() == CompactionThreadStatus::Idle
                        && status.sequence() < terminal.sequence()
                })
        }),
        _ => false,
    }
}

fn settlement_retirement(
    reader: &DomainReader<'_, SyndicDomain>,
    operation: &CompactionOperationRecord,
    turn_state: &TurnStateRecord,
) -> Result<SettlementRetirement, SyndicMutationError> {
    let target = operation.target();
    let current = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: target.thread_id(),
            revision: target.binding_revision(),
        },
    )?;
    let BindingState::Valid(usable) = current.state() else {
        return Err(SyndicMutationError::BindingStateConflict);
    };
    if usable.cas_thread_id() != target.cas_thread_id()
        || usable.execution().runtime_id() != target.runtime_id()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    let revision = current.revision().checked_next()?;
    if point::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: target.thread_id(),
            revision,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let current_reservation = required::<CasThreadIndexFamily>(
        reader,
        &CasThreadKey::Record(target.cas_thread_id().clone()),
    )?;
    if current_reservation.thread_id() != target.thread_id()
        || current_reservation.latest_binding_revision() != current.revision()
        || current_reservation.retired_binding_revision().is_some()
    {
        return Err(SyndicMutationError::CasThreadRetired);
    }
    let stale = crate::StaleCasBinding::new(
        usable.execution().clone(),
        usable.cas_thread_id().clone(),
        Some(usable.tool_profile()),
        Some(usable.represented_prefix()),
        Some(usable.lineage()),
        Some(usable.native_turn_count()),
        Some(target.loaded_generation()),
        "context compaction authority retired",
        turn_state.updated_at(),
    )?;
    let binding = crate::BindingRecord::new(
        target.thread_id(),
        revision,
        current.selected_path(),
        BindingState::stale(stale),
    );
    let binding_head = crate::BindingHeadRecord::new(
        target.thread_id(),
        revision,
        BindingLifecycle::Stale,
        current.selected_path().digest(),
    );
    let reservation = current_reservation.retire(revision);
    let membership = crate::mutation::binding::membership(
        reader,
        target.cas_thread_id(),
        target.thread_id(),
        revision,
    )?;
    Ok(SettlementRetirement {
        binding,
        binding_head,
        reservation,
        membership,
    })
}

fn settlement_is_admitted(
    operation: &CompactionOperationRecord,
    settlement: &CompactionSettlement,
    next_turn_count: u64,
) -> bool {
    match settlement {
        CompactionSettlement::CancelledBeforeDispatch => {
            operation.state() == &CompactionOperationState::Admitted
        }
        CompactionSettlement::LocalNondispatch => operation.request().is_some_and(|observation| {
            observation.disposition() == CompactionRequestDisposition::ProvenLocalNondispatch
        }),
        CompactionSettlement::Abandoned(reason) => match reason {
            CompactionAbandonmentReason::ProviderRejectedBeforeCore => {
                operation.request().is_some_and(|observation| {
                    observation.disposition() == CompactionRequestDisposition::RejectedBeforeCore
                })
            }
            CompactionAbandonmentReason::CompletionUnknown => {
                operation.request().is_some_and(|observation| {
                    observation.disposition() == CompactionRequestDisposition::CompletionUnknown
                })
            }
            CompactionAbandonmentReason::TargetAuthorityLost
            | CompactionAbandonmentReason::StartupProcessGenerationLost
            | CompactionAbandonmentReason::ProviderProtocolConflict => true,
        },
        CompactionSettlement::ManualSuccess => {
            operation.terminal().is_some_and(|terminal| {
                terminal.status().outcome() == crate::TurnTerminalOutcome::Complete
            }) && operation
                .marker()
                .is_some_and(|marker| marker.lifecycle() == CompactionMarkerLifecycle::Completed)
        }
        CompactionSettlement::ManualFailure => operation.terminal().is_some(),
        CompactionSettlement::LifecycleUserWorkWon => {
            next_turn_count > 0
                && operation.terminal().is_some_and(|terminal| {
                    terminal.status().outcome() == crate::TurnTerminalOutcome::Complete
                })
                && operation.marker().is_some_and(|marker| {
                    marker.lifecycle() == CompactionMarkerLifecycle::Completed
                })
        }
        CompactionSettlement::LifecycleContinuation { .. } => false,
    }
}
