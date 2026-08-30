use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use crate::{
    AcceptedNextSourceRecord, AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord,
    AcceptedRouteHeadProof, AcceptedRouteTarget, InputGateRecord, InputGateState, NextTurnReason,
    StopAdmissionWitness, StopOperationRecord, SyndicMutationError, SyndicRecordError,
    codec::*,
    domain::SyndicDomain,
    mutation::{point, required},
};

use super::{AdmitStopOperationMutation, authority::validate_execution_target};

pub struct AdmissionRecords {
    route: Option<AcceptedRouteGenerationRecord>,
    route_head: Option<AcceptedRouteGenerationHeadRecord>,
    next_source: Option<AcceptedNextSourceRecord>,
    gate: InputGateRecord,
    stop: StopOperationRecord,
    compaction: Option<crate::CompactionOperationRecord>,
}

impl DomainMutation<SyndicDomain> for AdmitStopOperationMutation {
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
        reservation.reserve_records::<AcceptedRouteGenerationsCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationHeadsCodec>(1)?;
        reservation.reserve_records::<AcceptedReadySourcesCodec>(1)?;
        reservation.reserve_records::<AcceptedNextSourcesCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        reservation.reserve_records::<StopOperationsCodec>(1)?;
        reservation.reserve_records::<CompactionOperationsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        prepared.contribute(mutations)
    }
}

impl AdmitStopOperationMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<AdmissionRecords, SyndicMutationError> {
        let request = &self.request;
        if request.operation_id.thread_id() != request.target.thread_id() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if point::<StopOperationsFamily>(reader, &request.operation_id)?.is_some() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if matches!(
            request.target.turn_kind(),
            crate::TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction)
        ) {
            return self.provider_records(reader);
        }
        let expected_route = request
            .expected_route
            .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
        let steering_target = validate_execution_target(reader, &request.target)?;
        let current_gate = required::<InputGatesFamily>(reader, &request.target.thread_id())?;
        if current_gate.revision() != request.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision,
                current: current_gate.revision(),
            });
        }
        if current_gate.state() != &InputGateState::Steerable(request.target.turn_id())
            || current_gate.selected_route() != Some(expected_route)
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }

        let current_head =
            required::<AcceptedRouteGenerationHeadsFamily>(reader, &request.target.thread_id())?;
        let current_route = required::<AcceptedRouteGenerationsFamily>(
            reader,
            &ThreadRouteKey {
                thread: request.target.thread_id(),
                generation: expected_route.generation(),
            },
        )?;
        if current_head.proof() != expected_route
            || current_route.thread_id() != request.target.thread_id()
            || current_route.generation() != expected_route.generation()
            || current_route.revision() != expected_route.revision()
            || current_route.target() != &AcceptedRouteTarget::Steering(steering_target)
            || current_route.delivering_count() != 0
            || current_route.delivering_logical_utf8_bytes() != 0
            || current_gate.live_steering_count() != current_route.ready_retryable_count()
        {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        validate_sources(reader, &current_gate, &current_route)?;

        self.successors(current_gate, current_route, expected_route)
    }

    fn successors(
        &self,
        current_gate: InputGateRecord,
        current_route: AcceptedRouteGenerationRecord,
        expected_route: AcceptedRouteHeadProof,
    ) -> Result<AdmissionRecords, SyndicMutationError> {
        let request = &self.request;
        let next_count = current_route
            .next_turn_count()
            .checked_add(current_route.ready_retryable_count())
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "accepted-route next-turn count",
            })?;
        let route_revision = current_route.revision().checked_next()?;
        let route = AcceptedRouteGenerationRecord::new(
            current_route.thread_id(),
            current_route.generation(),
            route_revision,
            AcceptedRouteTarget::NextTurn(NextTurnReason::Stop),
            current_route.first_ordinal(),
            current_route.last_ordinal(),
            current_route.input_count(),
            0,
            0,
            next_count,
            current_route.terminal_count(),
            current_route.live_logical_utf8_bytes(),
            0,
        )?;
        let route_proof = AcceptedRouteHeadProof::new(route.generation(), route.revision());
        let route_head = AcceptedRouteGenerationHeadRecord::new(route.thread_id(), route_proof);
        let gate_revision = current_gate.revision().checked_next()?;
        let gate_next_count = current_gate
            .live_next_turn_count()
            .checked_add(current_gate.live_steering_count())
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "live next-turn count",
            })?;
        let gate = InputGateRecord::new(
            current_gate.thread_id(),
            gate_revision,
            InputGateState::stopping(request.target.turn_id(), request.operation_id.nonce()),
            current_gate.accepted_high_water(),
            current_gate.route_generation_high_water(),
            Some(route_proof),
            0,
            gate_next_count,
            current_gate.live_logical_utf8_bytes(),
        )?;
        let next_source = (route.next_turn_count() > 0).then(|| {
            AcceptedNextSourceRecord::new(
                route.thread_id(),
                route.generation(),
                route.revision(),
                route
                    .first_ordinal()
                    .expect("a route containing next-turn work has a first ordinal"),
                route
                    .last_ordinal()
                    .expect("a route containing next-turn work has a last ordinal"),
            )
        });
        let admission = StopAdmissionWitness::new(
            current_gate.revision(),
            expected_route,
            gate_revision,
            route_proof,
        );
        let stop = StopOperationRecord::admitted(
            request.operation_id,
            request.target.clone(),
            admission,
            request.causes,
        )
        .map_err(|_| SyndicMutationError::InputGateStateConflict)?;

        Ok(AdmissionRecords {
            route: Some(route),
            route_head: Some(route_head),
            next_source,
            gate,
            stop,
            compaction: None,
        })
    }

    fn provider_records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<AdmissionRecords, SyndicMutationError> {
        let request = &self.request;
        if request.expected_route.is_some() {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let operation_id = crate::CompactionOperationId::new(
            request.target.thread_id(),
            crate::CompactionOperationNonce::from_bytes(*request.target.turn_id().as_bytes()),
        );
        let operation = required::<CompactionOperationsFamily>(reader, &operation_id)?;
        let gate = required::<InputGatesFamily>(reader, &request.target.thread_id())?;
        if gate.revision() != request.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision,
                current: gate.revision(),
            });
        }
        if gate.state()
            != &InputGateState::compacting(request.target.turn_id(), operation_id.nonce())
            || operation.target().thread_id() != request.target.thread_id()
            || operation.target().turn_id() != request.target.turn_id()
            || operation.target().snapshot_id() != request.target.snapshot_id()
            || operation.target().binding_revision() != request.target.binding_revision()
            || operation.target().runtime_id() != request.target.runtime_id()
            || operation.target().loaded_generation() != request.target.loaded_generation()
            || operation.target().cas_thread_id() != request.target.cas_thread_id()
            || operation
                .cas_turn()
                .is_none_or(|turn| turn.cas_turn_id() != request.target.cas_turn_id())
            || !matches!(
                operation.state(),
                crate::CompactionOperationState::DispatchClaimed
                    | crate::CompactionOperationState::Live
            )
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        let snapshot = required::<ExecutionSnapshotsFamily>(reader, &request.target.snapshot_id())?;
        let binding = required::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: request.target.thread_id(),
                revision: request.target.binding_revision(),
            },
        )?;
        let crate::BindingState::Valid(usable) = binding.state() else {
            return Err(SyndicMutationError::BindingStateConflict);
        };
        let active = required::<ActiveCasTurnsFamily>(reader, &request.target.snapshot_id())?;
        if snapshot.kind()
            != crate::ExecutionSnapshotKind::ProviderOperation(
                crate::ProviderOperationKind::ContextCompaction,
            )
            || snapshot.active_turn_id() != request.target.turn_id()
            || snapshot.binding_revision() != request.target.binding_revision()
            || snapshot.cas_thread_id() != request.target.cas_thread_id()
            || usable.cas_thread_id() != request.target.cas_thread_id()
            || active.cas_turn_id() != request.target.cas_turn_id()
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        let successor_gate_revision = gate.revision().checked_next()?;
        let successor_gate = InputGateRecord::new(
            gate.thread_id(),
            successor_gate_revision,
            InputGateState::stopping(request.target.turn_id(), request.operation_id.nonce()),
            gate.accepted_high_water(),
            gate.route_generation_high_water(),
            gate.selected_route(),
            0,
            gate.live_next_turn_count(),
            gate.live_logical_utf8_bytes(),
        )?;
        let compaction = operation.handoff_to_stop(request.operation_id.nonce())?;
        let admission = StopAdmissionWitness::provider_operation(
            gate.revision(),
            successor_gate_revision,
            operation.revision(),
            compaction.revision(),
        );
        let stop = StopOperationRecord::admitted(
            request.operation_id,
            request.target.clone(),
            admission,
            request.causes,
        )
        .map_err(|_| SyndicMutationError::InputGateStateConflict)?;
        Ok(AdmissionRecords {
            route: None,
            route_head: None,
            next_source: None,
            gate: successor_gate,
            stop,
            compaction: Some(compaction),
        })
    }
}

fn validate_sources(
    reader: &DomainReader<'_, SyndicDomain>,
    gate: &InputGateRecord,
    route: &AcceptedRouteGenerationRecord,
) -> Result<(), SyndicMutationError> {
    let key = ThreadRouteKey {
        thread: route.thread_id(),
        generation: route.generation(),
    };
    let ready = point::<AcceptedReadySourcesFamily>(reader, &key)?;
    match (route.ready_retryable_count(), ready) {
        (0, None) => {}
        (count, Some(source))
            if count > 0
                && source.thread_id() == route.thread_id()
                && source.gate_revision() == gate.revision()
                && source.generation() == route.generation()
                && source.generation_revision() == route.revision()
                && Some(source.first_ordinal()) == route.first_ordinal()
                && Some(source.last_ordinal()) == route.last_ordinal() => {}
        _ => return Err(SyndicMutationError::ActiveSteeringRouteConflict),
    }

    let next = point::<AcceptedNextSourcesFamily>(reader, &key)?;
    match (route.next_turn_count(), next) {
        (0, None) => {}
        (count, Some(source))
            if count > 0
                && source.thread_id() == route.thread_id()
                && source.generation() == route.generation()
                && source.generation_revision() == route.revision()
                && Some(source.first_ordinal()) == route.first_ordinal()
                && Some(source.last_ordinal()) == route.last_ordinal() => {}
        _ => return Err(SyndicMutationError::ActiveSteeringRouteConflict),
    }
    Ok(())
}

impl AdmissionRecords {
    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        if let (Some(route), Some(route_head)) = (&self.route, &self.route_head) {
            let route_key = ThreadRouteKey {
                thread: route.thread_id(),
                generation: route.generation(),
            };
            mutations.put::<AcceptedRouteGenerationsCodec>(&route_key, route)?;
            mutations
                .put::<AcceptedRouteGenerationHeadsCodec>(&route_head.thread_id(), route_head)?;
            mutations.delete::<AcceptedReadySourcesCodec>(&route_key)?;
            match &self.next_source {
                Some(source) => {
                    mutations.put::<AcceptedNextSourcesCodec>(&route_key, source)?;
                }
                None => {
                    mutations.delete::<AcceptedNextSourcesCodec>(&route_key)?;
                }
            }
        }
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        mutations.put::<StopOperationsCodec>(&self.stop.id(), &self.stop)?;
        if let Some(compaction) = &self.compaction {
            mutations.put::<CompactionOperationsCodec>(&compaction.id(), compaction)?;
        }
        Ok(())
    }
}
