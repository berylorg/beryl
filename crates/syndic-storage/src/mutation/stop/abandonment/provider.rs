use super::*;

pub(super) struct AbandonmentRecords {
    pub(super) binding: BindingRecord,
    pub(super) binding_head: BindingHeadRecord,
    pub(super) reservation: CasThreadIndexRecord,
    pub(super) membership: CasThreadBindingIndexRecord,
    pub(super) route: AcceptedRouteGenerationRecord,
    pub(super) route_head: AcceptedRouteGenerationHeadRecord,
    pub(super) next_source: Option<AcceptedNextSourceRecord>,
    pub(super) gate: InputGateRecord,
    pub(super) stop: StopOperationRecord,
    pub(super) event: SourceEventRecord,
    pub(super) state: TurnStateRecord,
    pub(super) summary: Option<HistorySummaryRecord>,
    pub(super) transcript_head: Option<TranscriptViewHeadRecord>,
    pub(super) transcript_build: Option<TranscriptBuildRecord>,
    pub(super) activity: ActivityEffect,
}

pub(super) enum StopAbandonmentRecords {
    Ordinary(Box<AbandonmentRecords>),
    ProviderOperation(ProviderAbandonmentRecords),
}

pub(super) struct ProviderAbandonmentRecords {
    binding: BindingRecord,
    binding_head: BindingHeadRecord,
    reservation: CasThreadIndexRecord,
    membership: CasThreadBindingIndexRecord,
    gate: InputGateRecord,
    stop: StopOperationRecord,
    compaction: crate::CompactionOperationRecord,
    compaction_receipt: crate::CompactionSettlementReceiptRecord,
    event: SourceEventRecord,
    state: TurnStateRecord,
}

impl StopAbandonmentRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        match self {
            Self::Ordinary(records) => records.contribute(mutations),
            Self::ProviderOperation(records) => records.contribute(mutations),
        }
    }
}

pub(super) fn provider_abandonment_records(
    reader: &DomainReader<'_, SyndicDomain>,
    request: &AbandonStopOperation,
    authority: super::super::authority::LiveStopAuthority,
) -> Result<ProviderAbandonmentRecords, SyndicMutationError> {
    let current_gate = authority.gate;
    let current_stop = authority.record;
    let operation_id = crate::CompactionOperationId::new(
        request.target.thread_id(),
        crate::CompactionOperationNonce::from_bytes(*request.target.turn_id().as_bytes()),
    );
    let current_compaction = required::<CompactionOperationsFamily>(reader, &operation_id)?;
    let turn = required::<TurnsFamily>(reader, &request.target.turn_id())?;
    let current_state = required::<TurnStatesFamily>(reader, &request.target.turn_id())?;
    let active_turn = required::<ActiveCasTurnsFamily>(reader, &request.target.snapshot_id())?;
    if current_state.revision() != request.expected_state_revision {
        return Err(SyndicMutationError::TurnStateRevisionConflict {
            expected: request.expected_state_revision,
            current: current_state.revision(),
        });
    }
    if turn.kind()
        != crate::TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction)
        || turn.parent() != crate::ConversationParent::Root
        || current_state.turn_id() != turn.id()
        || !matches!(
            current_state.lifecycle(),
            TurnLifecycle::Pending | TurnLifecycle::Active
        )
        || current_compaction.state()
            != &crate::CompactionOperationState::Stopping(current_stop.id().nonce())
        || current_compaction
            .cas_turn()
            .is_none_or(|observation| observation.cas_turn_id() != request.target.cas_turn_id())
    {
        return Err(SyndicMutationError::LiveTurnConflict);
    }
    let current_binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: request.target.thread_id(),
            revision: request.target.binding_revision(),
        },
    )?;
    let BindingState::Valid(usable) = current_binding.state() else {
        return Err(SyndicMutationError::BindingStateConflict);
    };
    if request.stale.execution() != usable.execution()
        || request.stale.cas_thread_id() != usable.cas_thread_id()
        || request.stale.observed_tool_profile() != Some(usable.tool_profile())
        || request.stale.observed_prefix() != Some(usable.represented_prefix())
        || request.stale.observed_lineage() != Some(usable.lineage())
        || request.stale.observed_native_turn_count() != Some(usable.native_turn_count())
        || request.stale.loaded_generation() != Some(request.target.loaded_generation())
        || request.stale.observed_at() < current_state.updated_at()
        || request.stale.observed_at() < turn.submitted_at()
        || request.stale.observed_at() < active_turn.published_at()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    let binding_revision = current_binding.revision().checked_next()?;
    if point::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: request.target.thread_id(),
            revision: binding_revision,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let current_reservation = required::<CasThreadIndexFamily>(
        reader,
        &CasThreadKey::Record(request.target.cas_thread_id().clone()),
    )?;
    if current_reservation.thread_id() != request.target.thread_id()
        || current_reservation.latest_binding_revision() != current_binding.revision()
        || current_reservation.retired_binding_revision().is_some()
    {
        return Err(SyndicMutationError::CasThreadRetired);
    }
    let reservation = current_reservation.retire(binding_revision);
    let membership = membership(
        reader,
        request.target.cas_thread_id(),
        request.target.thread_id(),
        binding_revision,
    )?;
    let binding = BindingRecord::new(
        request.target.thread_id(),
        binding_revision,
        current_binding.selected_path(),
        BindingState::stale(request.stale.clone()),
    );
    let binding_head = BindingHeadRecord::new(
        request.target.thread_id(),
        binding_revision,
        BindingLifecycle::Stale,
        current_binding.selected_path().digest(),
    );

    let sequence = SourceEventSequence::new(
        current_state
            .source_event_count()
            .checked_add(1)
            .ok_or(SyndicMutationError::SourceEventFrontierExhausted)?,
    )?;
    if point::<SourceEventsFamily>(
        reader,
        &TurnEventKey {
            owner: turn.id(),
            ordinal: sequence,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::SourceEventCollision);
    }
    let status = TurnEndStatus::incomplete(TurnIncompleteReason::AuthorityLost);
    let event = SourceEventRecord::new(
        turn.id(),
        sequence,
        None,
        SourceEventPayload::TurnEnded(status),
    )?;
    let state_revision = current_state.revision().checked_next()?;
    let state = TurnStateRecord::with_capture_frontiers_and_issue(
        turn.id(),
        state_revision,
        TurnLifecycle::Incomplete,
        sequence.get(),
        current_state.item_count(),
        current_state.finalized_item_count(),
        current_state.open_item_count(),
        current_state.history_blocking_item_count(),
        current_state.provider_observation_issue(),
        Some(status),
        request.stale.observed_at(),
    )?;
    let gate_revision = current_gate.revision().checked_next()?;
    let gate = InputGateRecord::new(
        current_gate.thread_id(),
        gate_revision,
        InputGateState::Idle,
        current_gate.accepted_high_water(),
        current_gate.route_generation_high_water(),
        current_gate.selected_route(),
        0,
        current_gate.live_next_turn_count(),
        current_gate.live_logical_utf8_bytes(),
    )?;
    let compaction_reason = match request.reason {
        StopAbandonmentReason::StartupProcessGenerationLost => {
            crate::CompactionAbandonmentReason::StartupProcessGenerationLost
        }
        StopAbandonmentReason::ProviderRejectedBeforeCoreInterrupt
        | StopAbandonmentReason::TargetAuthorityLost => {
            crate::CompactionAbandonmentReason::TargetAuthorityLost
        }
    };
    if point::<CompactionSettlementReceiptsFamily>(reader, &current_compaction.id())?.is_some() {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let compaction_settlement = crate::CompactionSettlement::Abandoned(compaction_reason);
    let successor_compaction_revision = current_compaction.revision().checked_next()?;
    let compaction_receipt = crate::CompactionSettlementReceiptRecord::new(
        current_compaction.id(),
        current_compaction.revision(),
        successor_compaction_revision,
        current_gate.clone(),
        gate.clone(),
        compaction_settlement,
        None,
    )?;
    let receipt_commitment = compaction_settlement_receipt_commitment(&compaction_receipt)
        .map_err(|_| crate::SyndicRecordError::InvalidCompactionOperation)?;
    let compaction = current_compaction.abandon_from_stop(
        current_stop.id().nonce(),
        gate_revision,
        compaction_receipt.settlement().clone(),
        receipt_commitment,
    )?;
    let witness = StopAbandonmentWitness::provider_operation(
        StopDispositionSource::new(current_gate.revision(), current_stop.revision()),
        request.reason,
        gate_revision,
        binding_revision,
        state_revision,
        current_compaction.revision(),
        compaction.revision(),
    );
    let stop = StopOperationRecord::new(
        current_stop.id(),
        current_stop.target().clone(),
        current_stop.admission(),
        current_stop.revision().checked_next()?,
        current_stop.cause_first_revisions(),
        current_stop.dispatch_claim(),
        StopOperationState::Abandoned(witness),
    )
    .map_err(|_| SyndicMutationError::InputGateStateConflict)?;
    Ok(ProviderAbandonmentRecords {
        binding,
        binding_head,
        reservation,
        membership,
        gate,
        stop,
        compaction,
        compaction_receipt,
        event,
        state,
    })
}

impl ProviderAbandonmentRecords {
    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<BindingsCodec>(
            &BindingKey {
                thread: self.binding.thread_id(),
                revision: self.binding.revision(),
            },
            &self.binding,
        )?;
        mutations.put::<BindingHeadsCodec>(&self.binding_head.thread_id(), &self.binding_head)?;
        mutations.put::<CasThreadIndexCodec>(
            &CasThreadKey::Record(self.reservation.cas_thread_id().clone()),
            &self.reservation,
        )?;
        mutations.put::<CasThreadBindingIndexCodec>(
            &CasThreadBindingKey::Record(
                self.membership.cas_thread_id().clone(),
                self.membership.binding_revision(),
            ),
            &self.membership,
        )?;
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        mutations.put::<StopOperationsCodec>(&self.stop.id(), &self.stop)?;
        mutations.put::<CompactionOperationsCodec>(&self.compaction.id(), &self.compaction)?;
        mutations.put::<CompactionSettlementReceiptsCodec>(
            &self.compaction_receipt.operation_id(),
            &self.compaction_receipt,
        )?;
        mutations.put::<SourceEventsCodec>(
            &TurnEventKey {
                owner: self.event.turn_id(),
                ordinal: self.event.sequence(),
            },
            &self.event,
        )?;
        mutations.put::<TurnStatesCodec>(&self.state.turn_id(), &self.state)?;
        Ok(())
    }
}
