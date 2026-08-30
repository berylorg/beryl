use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use crate::{
    AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord, AcceptedRouteHeadProof,
    AcceptedRouteRevision, AcceptedRouteTarget, ActiveCasBinding, ActiveCasTurnRecord,
    BindingHeadRecord, BindingLifecycle, BindingRecord, BindingState, CasThreadBindingIndexRecord,
    CasThreadIndexRecord, CasTurnIndexRecord, ExecutionSnapshotRecord, InputGateRecord,
    InputGateState, PendingSteeringTargetProof, SteeringTargetProof, SyndicMutationError,
    TurnLifecycle, codec::*, domain::SyndicDomain,
};

use super::{
    super::{point, required},
    ActivateBinding, PublishActiveCasTurn,
    validation::{membership, reservation, transition_base, validate_usable_current},
};

pub(crate) struct ActivateBindingMutation {
    pub(super) request: ActivateBinding,
}

impl DomainMutation<SyndicDomain> for ActivateBindingMutation {
    type Error = SyndicMutationError;
    type Prepared = ActivateBindingRecords;

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
        reservation.reserve_records::<BindingsCodec>(1)?;
        reservation.reserve_records::<BindingHeadsCodec>(1)?;
        reservation.reserve_records::<ExecutionSnapshotsCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationHeadsCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationsCodec>(1)?;
        reservation.reserve_records::<CasThreadIndexCodec>(1)?;
        reservation.reserve_records::<CasThreadBindingIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        prepared.contribute(mutations)
    }
}

pub struct ActivateBindingRecords {
    binding: BindingRecord,
    head: BindingHeadRecord,
    snapshot: ExecutionSnapshotRecord,
    gate: InputGateRecord,
    route_head: AcceptedRouteGenerationHeadRecord,
    route_generation: AcceptedRouteGenerationRecord,
    reservation: CasThreadIndexRecord,
    membership: CasThreadBindingIndexRecord,
}

impl ActivateBindingMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<ActivateBindingRecords, SyndicMutationError> {
        let request = &self.request;
        let base = transition_base(
            reader,
            request.thread_id,
            request.expected_binding_revision,
            request.selected_path,
        )?;
        let BindingState::Valid(usable) = base.current.state() else {
            return Err(SyndicMutationError::BindingStateConflict);
        };
        validate_usable_current(reader, request.selected_path, usable)?;
        let usable = usable
            .advance_represented_source_revision(request.selected_path.thread_revision())
            .ok_or(SyndicMutationError::BindingPathConflict)?;
        if request.selected_path.tail() != Some(request.turn_id) {
            return Err(SyndicMutationError::BindingPathConflict);
        }
        let turn = required::<TurnsFamily>(reader, &request.turn_id)?;
        let turn_state = required::<TurnStatesFamily>(reader, &request.turn_id)?;
        if turn.origin_thread_id() != request.thread_id
            || turn_state.lifecycle() != TurnLifecycle::Pending
        {
            return Err(SyndicMutationError::TurnLifecycleConflict);
        }
        let current_gate = required::<InputGatesFamily>(reader, &request.thread_id)?;
        if current_gate.revision() != request.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision,
                current: current_gate.revision(),
            });
        }
        if current_gate.state() != &InputGateState::PendingTurn(request.turn_id)
            || current_gate.live_steering_count() != 0
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        if point::<ExecutionSnapshotsFamily>(reader, &request.snapshot_id)?.is_some()
            || point::<ActiveCasTurnsFamily>(reader, &request.snapshot_id)?.is_some()
        {
            return Err(SyndicMutationError::ExecutionSnapshotCollision);
        }
        if let Some(injection_generation) = usable.lineage().recovered_injection_generation()
            && request.loaded_generation.process() != injection_generation.process()
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        if let Some(completed_at) = usable.lineage().recovered_completed_at()
            && request.started_at < completed_at
        {
            return Err(SyndicMutationError::TimestampRegressed);
        }
        usable.native_turn_count().checked_next()?;
        let reservation = reservation(reader, &usable, request.thread_id, base.next_revision)?;

        let activation_gate_revision = current_gate.revision().checked_next()?;
        let active = ActiveCasBinding::new(
            usable.clone(),
            request.snapshot_id,
            request.turn_id,
            activation_gate_revision,
            request.started_at,
        );
        let binding = BindingRecord::new(
            request.thread_id,
            base.next_revision,
            request.selected_path,
            BindingState::active(active),
        );
        let head = BindingHeadRecord::new(
            request.thread_id,
            base.next_revision,
            BindingLifecycle::Active,
            request.selected_path.digest(),
        );
        let snapshot = ExecutionSnapshotRecord::new(
            request.snapshot_id,
            request.thread_id,
            base.next_revision,
            activation_gate_revision,
            request.turn_id,
            usable.cas_thread_id().clone(),
            request.selected_path,
            usable.represented_prefix(),
            usable.native_turn_count(),
            usable.tool_profile(),
            usable.lineage(),
            usable.execution().clone(),
            request.loaded_generation,
            request.started_at,
        );
        let pending = PendingSteeringTargetProof::new(
            base.next_revision,
            request.snapshot_id,
            request.turn_id,
            usable.cas_thread_id().clone(),
        );
        let route_generation_id = current_gate.next_route_generation()?;
        if point::<AcceptedRouteGenerationsFamily>(
            reader,
            &ThreadRouteKey {
                thread: request.thread_id,
                generation: route_generation_id,
            },
        )?
        .is_some()
        {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let route_proof =
            AcceptedRouteHeadProof::new(route_generation_id, AcceptedRouteRevision::FIRST);
        let route_head = AcceptedRouteGenerationHeadRecord::new(request.thread_id, route_proof);
        let route_generation = AcceptedRouteGenerationRecord::new(
            request.thread_id,
            route_generation_id,
            AcceptedRouteRevision::FIRST,
            AcceptedRouteTarget::AwaitingSteering(pending.clone()),
            None,
            None,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        )?;
        let gate = InputGateRecord::new(
            request.thread_id,
            activation_gate_revision,
            InputGateState::AwaitingSteering(request.turn_id),
            current_gate.accepted_high_water(),
            Some(route_generation_id),
            Some(route_proof),
            current_gate.live_steering_count(),
            current_gate.live_next_turn_count(),
            current_gate.live_logical_utf8_bytes(),
        )?;
        let membership = membership(
            reader,
            usable.cas_thread_id(),
            request.thread_id,
            base.next_revision,
        )?;
        Ok(ActivateBindingRecords {
            binding,
            head,
            snapshot,
            gate,
            route_head,
            route_generation,
            reservation,
            membership,
        })
    }
}

impl ActivateBindingRecords {
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
        mutations.put::<BindingHeadsCodec>(&self.binding.thread_id(), &self.head)?;
        mutations.put::<ExecutionSnapshotsCodec>(&self.snapshot.id(), &self.snapshot)?;
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        mutations.put::<AcceptedRouteGenerationHeadsCodec>(
            &self.route_head.thread_id(),
            &self.route_head,
        )?;
        mutations.put::<AcceptedRouteGenerationsCodec>(
            &ThreadRouteKey {
                thread: self.route_generation.thread_id(),
                generation: self.route_generation.generation(),
            },
            &self.route_generation,
        )?;
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
        Ok(())
    }
}

pub(crate) struct PublishActiveCasTurnMutation {
    pub(super) request: PublishActiveCasTurn,
}

impl DomainMutation<SyndicDomain> for PublishActiveCasTurnMutation {
    type Error = SyndicMutationError;
    type Prepared = PublishActiveCasTurnRecords;

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
        reservation.reserve_records::<ActiveCasTurnsCodec>(1)?;
        reservation.reserve_records::<CasTurnIndexCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationHeadsCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationsCodec>(1)?;
        reservation.reserve_records::<AcceptedReadySourcesCodec>(1)?;
        reservation.reserve_records::<AcceptedNextSourcesCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        prepared.contribute(mutations)
    }
}

pub struct PublishActiveCasTurnRecords {
    active_turn: ActiveCasTurnRecord,
    cas_turn_index: CasTurnIndexRecord,
    route_head: AcceptedRouteGenerationHeadRecord,
    route_generation: AcceptedRouteGenerationRecord,
    ready_source: Option<crate::AcceptedReadySourceRecord>,
    next_source: Option<crate::AcceptedNextSourceRecord>,
    gate: InputGateRecord,
}

impl PublishActiveCasTurnMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<PublishActiveCasTurnRecords, SyndicMutationError> {
        let request = &self.request;
        let head = required::<BindingHeadsFamily>(reader, &request.thread_id)?;
        if head.revision() != request.binding_revision {
            return Err(SyndicMutationError::BindingRevisionConflict {
                expected: request.binding_revision,
                current: head.revision(),
            });
        }
        let binding = required::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: request.thread_id,
                revision: request.binding_revision,
            },
        )?;
        let BindingState::Active(active) = binding.state() else {
            return Err(SyndicMutationError::BindingStateConflict);
        };
        if active.snapshot_id() != request.snapshot_id
            || active.usable().cas_thread_id() != &request.cas_thread_id
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        let snapshot = required::<ExecutionSnapshotsFamily>(reader, &request.snapshot_id)?;
        if snapshot.thread_id() != request.thread_id
            || snapshot.binding_revision() != request.binding_revision
            || snapshot.active_turn_id() != active.turn_id()
            || snapshot.cas_thread_id() != &request.cas_thread_id
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        if request.published_at < snapshot.started_at() {
            return Err(SyndicMutationError::TimestampRegressed);
        }

        let current_gate = required::<InputGatesFamily>(reader, &request.thread_id)?;
        if current_gate.revision() != request.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision,
                current: current_gate.revision(),
            });
        }
        let InputGateState::AwaitingSteering(gate_turn) = current_gate.state() else {
            return Err(SyndicMutationError::InputGateStateConflict);
        };
        let current_route_proof = current_gate
            .selected_route()
            .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
        let current_route = required::<AcceptedRouteGenerationsFamily>(
            reader,
            &ThreadRouteKey {
                thread: request.thread_id,
                generation: current_route_proof.generation(),
            },
        )?;
        let AcceptedRouteTarget::AwaitingSteering(pending) = current_route.target() else {
            return Err(SyndicMutationError::InputGateStateConflict);
        };
        if *gate_turn != active.turn_id()
            || pending.binding_revision() != request.binding_revision
            || pending.snapshot_id() != request.snapshot_id
            || pending.active_turn_id() != active.turn_id()
            || pending.cas_thread_id() != &request.cas_thread_id
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }

        let active_turn = ActiveCasTurnRecord::new(
            request.snapshot_id,
            request.thread_id,
            active.turn_id(),
            request.binding_revision,
            request.cas_thread_id.clone(),
            request.cas_turn_id.clone(),
            request.published_at,
        );
        if point::<ActiveCasTurnsFamily>(reader, &request.snapshot_id)?.is_some() {
            return Err(SyndicMutationError::ActiveCasTurnCollision);
        }
        let cas_turn_key =
            CasTurnKey::Record(request.cas_thread_id.clone(), request.cas_turn_id.clone());
        if point::<CasTurnIndexFamily>(reader, &cas_turn_key)?.is_some() {
            return Err(SyndicMutationError::CasTurnOwnershipConflict);
        }
        let steering_target =
            SteeringTargetProof::new(pending.clone(), request.cas_turn_id.clone());
        let current_route_head =
            required::<AcceptedRouteGenerationHeadsFamily>(reader, &request.thread_id)?;
        if current_route_head.proof() != current_route_proof
            || current_route.revision() != current_route_proof.revision()
            || current_route.target() != &AcceptedRouteTarget::AwaitingSteering(pending.clone())
        {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let route_revision = current_route.revision().checked_next()?;
        let route_generation = AcceptedRouteGenerationRecord::new(
            current_route.thread_id(),
            current_route.generation(),
            route_revision,
            AcceptedRouteTarget::Steering(steering_target.clone()),
            current_route.first_ordinal(),
            current_route.last_ordinal(),
            current_route.input_count(),
            current_route.ready_retryable_count(),
            current_route.delivering_count(),
            current_route.next_turn_count(),
            current_route.terminal_count(),
            current_route.live_logical_utf8_bytes(),
            current_route.delivering_logical_utf8_bytes(),
        )?;
        let route_proof = AcceptedRouteHeadProof::new(current_route.generation(), route_revision);
        let route_head = AcceptedRouteGenerationHeadRecord::new(request.thread_id, route_proof);
        let gate = InputGateRecord::new(
            request.thread_id,
            current_gate.revision().checked_next()?,
            InputGateState::Steerable(active.turn_id()),
            current_gate.accepted_high_water(),
            current_gate.route_generation_high_water(),
            Some(route_proof),
            current_gate.live_steering_count(),
            current_gate.live_next_turn_count(),
            current_gate.live_logical_utf8_bytes(),
        )?;
        let ready_source = (route_generation.ready_retryable_count() > 0).then(|| {
            crate::AcceptedReadySourceRecord::new(
                request.thread_id,
                gate.revision(),
                route_generation.generation(),
                route_revision,
                route_generation
                    .first_ordinal()
                    .expect("nonempty active route with ready work"),
                route_generation
                    .last_ordinal()
                    .expect("nonempty active route with ready work"),
            )
        });
        let next_source = (route_generation.next_turn_count() > 0).then(|| {
            crate::AcceptedNextSourceRecord::new(
                request.thread_id,
                route_generation.generation(),
                route_revision,
                route_generation
                    .first_ordinal()
                    .expect("nonempty active route with next work"),
                route_generation
                    .last_ordinal()
                    .expect("nonempty active route with next work"),
            )
        });
        let post_turn_native_count = active.usable().native_turn_count().checked_next()?;
        let cas_turn_index = CasTurnIndexRecord::new(
            request.cas_thread_id.clone(),
            request.cas_turn_id.clone(),
            request.thread_id,
            active.turn_id(),
            request.binding_revision,
            request.snapshot_id,
            post_turn_native_count,
        );
        Ok(PublishActiveCasTurnRecords {
            active_turn,
            cas_turn_index,
            route_head,
            route_generation,
            ready_source,
            next_source,
            gate,
        })
    }
}

impl PublishActiveCasTurnRecords {
    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<ActiveCasTurnsCodec>(&self.active_turn.snapshot_id(), &self.active_turn)?;
        mutations.put::<CasTurnIndexCodec>(
            &CasTurnKey::Record(
                self.cas_turn_index.cas_thread_id().clone(),
                self.cas_turn_index.cas_turn_id().clone(),
            ),
            &self.cas_turn_index,
        )?;
        mutations.put::<AcceptedRouteGenerationHeadsCodec>(
            &self.route_head.thread_id(),
            &self.route_head,
        )?;
        mutations.put::<AcceptedRouteGenerationsCodec>(
            &ThreadRouteKey {
                thread: self.route_generation.thread_id(),
                generation: self.route_generation.generation(),
            },
            &self.route_generation,
        )?;
        if let Some(source) = &self.ready_source {
            mutations.put::<AcceptedReadySourcesCodec>(
                &ThreadRouteKey {
                    thread: source.thread_id(),
                    generation: source.generation(),
                },
                source,
            )?;
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
