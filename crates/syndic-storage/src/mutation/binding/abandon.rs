use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use crate::{
    AcceptedInputLifecycle, AcceptedNextSourceRecord, AcceptedRouteGenerationHeadRecord,
    AcceptedRouteGenerationRecord, AcceptedRouteHeadProof, AcceptedRouteLeafRecord,
    AcceptedRouteLeafState, AcceptedRouteLeafTransitionKind, AcceptedRouteLeafTransitionProof,
    AcceptedRouteLostTarget, AcceptedRouteProjectionLostProof, AcceptedRouteTarget,
    BindingHeadRecord, BindingLifecycle, BindingRecord, BindingState, CasThreadBindingIndexRecord,
    CasThreadIndexRecord, InputGateRecord, InputGateState, NextTurnReason, SyndicMutationError,
    SyndicRecordError, codec::*, domain::SyndicDomain,
};

use super::{
    AbandonActiveBinding,
    validation::{membership, retirement, transition_base, validate_stale},
};
use crate::mutation::required;

pub(crate) struct AbandonActiveBindingMutation {
    pub(super) request: AbandonActiveBinding,
}

struct AbandonActiveBindingRecords {
    binding: BindingRecord,
    head: BindingHeadRecord,
    reservation: CasThreadIndexRecord,
    membership: CasThreadBindingIndexRecord,
    route_head: AcceptedRouteGenerationHeadRecord,
    route_generation: AcceptedRouteGenerationRecord,
    next_source: Option<AcceptedNextSourceRecord>,
    exact_rejected_leaf: Option<AcceptedRouteLeafRecord>,
    gate: InputGateRecord,
}

impl DomainMutation<SyndicDomain> for AbandonActiveBindingMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<BindingsCodec>(1)?;
        reservation.reserve_records::<BindingHeadsCodec>(1)?;
        reservation.reserve_records::<CasThreadIndexCodec>(1)?;
        reservation.reserve_records::<CasThreadBindingIndexCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationHeadsCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationsCodec>(1)?;
        reservation.reserve_records::<AcceptedReadySourcesCodec>(1)?;
        reservation.reserve_records::<AcceptedNextSourcesCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteLeavesCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

impl AbandonActiveBindingMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<AbandonActiveBindingRecords, SyndicMutationError> {
        let request = &self.request;
        let base = transition_base(
            reader,
            request.thread_id,
            request.expected_binding_revision,
            request.selected_path,
        )?;
        let BindingState::Active(active) = base.current.state() else {
            return Err(SyndicMutationError::BindingStateConflict);
        };
        let snapshot = required::<ExecutionSnapshotsFamily>(reader, &active.snapshot_id())?;
        if snapshot.thread_id() != request.thread_id
            || snapshot.binding_revision() != base.current.revision()
            || snapshot.active_turn_id() != active.turn_id()
            || snapshot.cas_thread_id() != active.usable().cas_thread_id()
            || snapshot.loaded_generation()
                != request
                    .stale
                    .loaded_generation()
                    .ok_or(SyndicMutationError::BindingStateConflict)?
            || request.stale.execution() != active.usable().execution()
            || request.stale.cas_thread_id() != active.usable().cas_thread_id()
            || request.stale.observed_prefix() != Some(active.usable().represented_prefix())
            || request.stale.observed_lineage() != Some(active.usable().lineage())
            || request.stale.observed_native_turn_count()
                != Some(active.usable().native_turn_count())
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        if request.stale.observed_at() < active.started_at() {
            return Err(SyndicMutationError::TimestampRegressed);
        }
        validate_stale(reader, request.selected_path, &request.stale)?;

        let gate = required::<InputGatesFamily>(reader, &request.thread_id)?;
        validate_active_gate(&gate, base.current.revision(), active)?;
        let route_proof = gate
            .selected_route()
            .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
        if route_proof.generation() != request.route_generation {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let current_route_head =
            required::<AcceptedRouteGenerationHeadsFamily>(reader, &request.thread_id)?;
        let current_route = required::<AcceptedRouteGenerationsFamily>(
            reader,
            &ThreadRouteKey {
                thread: request.thread_id,
                generation: request.route_generation,
            },
        )?;
        if current_route_head.proof() != route_proof
            || current_route.revision() != route_proof.revision()
            || !route_target_matches(current_route.target(), request.target())
        {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        validate_loss_target(reader, request, active, &snapshot, &gate)?;
        let exact_rejected_leaf = exact_rejected_leaf(
            reader,
            request,
            &current_route,
            gate.revision(),
            route_proof,
        )?;
        let preserved_count = u64::from(exact_rejected_leaf.is_some());
        let preserved_bytes = exact_rejected_leaf
            .as_ref()
            .map_or(0, |(_, logical_bytes)| *logical_bytes);
        let rerouted_count = current_route
            .ready_retryable_count()
            .checked_add(preserved_count)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "accepted-route rerouted count",
            })?;
        let terminalized_count = current_route
            .delivering_count()
            .checked_sub(preserved_count)
            .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
        let terminalized_bytes = current_route
            .delivering_logical_utf8_bytes()
            .checked_sub(preserved_bytes)
            .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
        let route_next_count = current_route
            .next_turn_count()
            .checked_add(rerouted_count)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "accepted-route next-turn count",
            })?;
        let route_terminal_count = current_route
            .terminal_count()
            .checked_add(terminalized_count)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "accepted-route terminal count",
            })?;
        let route_live_bytes = current_route
            .live_logical_utf8_bytes()
            .checked_sub(terminalized_bytes)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "accepted-route live bytes",
            })?;
        let route_revision = current_route.revision().checked_next()?;
        let lost_proof = AcceptedRouteProjectionLostProof::new(
            request.target.clone(),
            request.route_abandonment_proof(gate.revision(), route_proof),
            base.next_revision,
            active.snapshot_id(),
            active.usable().cas_thread_id().clone(),
        );
        let route_generation = AcceptedRouteGenerationRecord::new(
            current_route.thread_id(),
            current_route.generation(),
            route_revision,
            AcceptedRouteTarget::ProjectionLost(lost_proof),
            current_route.first_ordinal(),
            current_route.last_ordinal(),
            current_route.input_count(),
            0,
            0,
            route_next_count,
            route_terminal_count,
            route_live_bytes,
            0,
        )?;
        let next_route_proof =
            AcceptedRouteHeadProof::new(current_route.generation(), route_revision);
        let route_head =
            AcceptedRouteGenerationHeadRecord::new(request.thread_id, next_route_proof);
        let next_source = (route_next_count > 0).then(|| {
            AcceptedNextSourceRecord::new(
                request.thread_id,
                current_route.generation(),
                route_revision,
                current_route
                    .first_ordinal()
                    .expect("nonempty lost route with next work"),
                current_route
                    .last_ordinal()
                    .expect("nonempty lost route with next work"),
            )
        });
        let next_count = gate
            .live_next_turn_count()
            .checked_add(rerouted_count)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "live next-turn count",
            })?;
        let live_bytes = gate
            .live_logical_utf8_bytes()
            .checked_sub(terminalized_bytes)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "live accepted-input bytes",
            })?;
        let gate = InputGateRecord::new(
            request.thread_id,
            gate.revision().checked_next()?,
            InputGateState::PendingTurn(active.turn_id()),
            gate.accepted_high_water(),
            gate.route_generation_high_water(),
            Some(next_route_proof),
            0,
            next_count,
            live_bytes,
        )?;
        let reservation = retirement(
            reader,
            &request.stale,
            request.thread_id,
            base.next_revision,
        )?
        .ok_or(SyndicMutationError::CasThreadRetired)?;
        let binding = BindingRecord::new(
            request.thread_id,
            base.next_revision,
            request.selected_path,
            BindingState::stale(request.stale.clone()),
        );
        let head = BindingHeadRecord::new(
            request.thread_id,
            base.next_revision,
            BindingLifecycle::Stale,
            request.selected_path.digest(),
        );
        let membership = membership(
            reader,
            request.stale.cas_thread_id(),
            request.thread_id,
            base.next_revision,
        )?;
        Ok(AbandonActiveBindingRecords {
            binding,
            head,
            reservation,
            membership,
            route_head,
            route_generation,
            next_source,
            exact_rejected_leaf: exact_rejected_leaf.map(|(leaf, _)| leaf),
            gate,
        })
    }
}

fn exact_rejected_leaf(
    reader: &DomainReader<'_, SyndicDomain>,
    request: &AbandonActiveBinding,
    route: &AcceptedRouteGenerationRecord,
    source_gate_revision: beryl_model::InputGateRevision,
    source_route: AcceptedRouteHeadProof,
) -> Result<Option<(AcceptedRouteLeafRecord, u64)>, SyndicMutationError> {
    let Some(rejected) = request.exact_rejected_delivery else {
        return Ok(None);
    };
    if !matches!(request.target(), AcceptedRouteLostTarget::Steering(_))
        || !route_target_matches(route.target(), request.target())
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }

    let input = required::<AcceptedInputsFamily>(reader, &rejected.input_id())?;
    let leaf = required::<AcceptedRouteLeavesFamily>(reader, &rejected.input_id())?;
    if input.id() != rejected.input_id()
        || input.thread_id() != request.thread_id
        || input.route_generation() != request.route_generation
        || leaf.input_id() != input.id()
        || leaf.thread_id() != input.thread_id()
        || leaf.generation() != input.route_generation()
        || leaf.ordinal() != input.ordinal()
        || leaf.revision() != rejected.expected_input_revision()
        || leaf.state() != AcceptedRouteLeafState::Routed
        || leaf.lifecycle() != AcceptedInputLifecycle::Delivering
    {
        return Err(SyndicMutationError::AcceptedInputDeliveryConflict);
    }
    let logical_bytes = input.content().summary().logical_utf8_bytes();
    Ok(Some((
        AcceptedRouteLeafRecord::new(
            leaf.input_id(),
            leaf.thread_id(),
            leaf.generation(),
            leaf.ordinal(),
            leaf.revision().checked_next()?,
            AcceptedRouteLeafState::NextTurn(NextTurnReason::ProjectionLost),
            AcceptedInputLifecycle::Retryable,
        )
        .with_transition_proof(AcceptedRouteLeafTransitionProof::new(
            source_gate_revision,
            source_route,
            rejected.expected_input_revision(),
            AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection,
        )),
        logical_bytes,
    )))
}

fn route_target_matches(current: &AcceptedRouteTarget, expected: &AcceptedRouteLostTarget) -> bool {
    match (current, expected) {
        (
            AcceptedRouteTarget::AwaitingSteering(current),
            AcceptedRouteLostTarget::AwaitingSteering(expected),
        ) => current == expected,
        (AcceptedRouteTarget::Steering(current), AcceptedRouteLostTarget::Steering(expected)) => {
            current == expected
        }
        (
            AcceptedRouteTarget::AwaitingTerminal(current),
            AcceptedRouteLostTarget::AwaitingTerminal(expected),
        ) => current == expected,
        (
            AcceptedRouteTarget::AwaitingSteering(_)
            | AcceptedRouteTarget::Steering(_)
            | AcceptedRouteTarget::AwaitingTerminal(_)
            | AcceptedRouteTarget::NextTurn(_)
            | AcceptedRouteTarget::ProjectionLost(_),
            AcceptedRouteLostTarget::AwaitingSteering(_)
            | AcceptedRouteLostTarget::Steering(_)
            | AcceptedRouteLostTarget::AwaitingTerminal(_),
        ) => false,
    }
}

fn validate_loss_target(
    reader: &DomainReader<'_, SyndicDomain>,
    request: &AbandonActiveBinding,
    active: &crate::ActiveCasBinding,
    snapshot: &crate::ExecutionSnapshotRecord,
    gate: &InputGateRecord,
) -> Result<(), SyndicMutationError> {
    let pending = request.target().pending();
    if pending.binding_revision() != request.expected_binding_revision
        || pending.snapshot_id() != active.snapshot_id()
        || pending.snapshot_id() != snapshot.id()
        || pending.active_turn_id() != active.turn_id()
        || pending.cas_thread_id() != active.usable().cas_thread_id()
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }
    match request.target() {
        AcceptedRouteLostTarget::AwaitingSteering(_) => {
            if request.exact_rejected_delivery.is_some()
                || !matches!(
                    gate.state(),
                    InputGateState::AwaitingSteering(turn) if *turn == active.turn_id()
                )
            {
                return Err(SyndicMutationError::ActiveSteeringRouteConflict);
            }
        }
        AcceptedRouteLostTarget::Steering(target) => {
            if !matches!(
                gate.state(),
                InputGateState::Steerable(turn) if *turn == active.turn_id()
            ) {
                return Err(SyndicMutationError::ActiveSteeringRouteConflict);
            }
            let published_turn = required::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())?;
            if published_turn.snapshot_id() != active.snapshot_id()
                || published_turn.thread_id() != request.thread_id
                || published_turn.turn_id() != active.turn_id()
                || published_turn.binding_revision() != request.expected_binding_revision
                || published_turn.cas_thread_id() != active.usable().cas_thread_id()
                || published_turn.cas_turn_id() != target.cas_turn_id()
            {
                return Err(SyndicMutationError::ActiveSteeringRouteConflict);
            }
        }
        AcceptedRouteLostTarget::AwaitingTerminal(target) => {
            if request.exact_rejected_delivery.is_some()
                || !matches!(
                    gate.state(),
                    InputGateState::AwaitingTerminal(turn) if *turn == active.turn_id()
                )
            {
                return Err(SyndicMutationError::ActiveSteeringRouteConflict);
            }
            let published_turn = required::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())?;
            if published_turn.snapshot_id() != active.snapshot_id()
                || published_turn.thread_id() != request.thread_id
                || published_turn.turn_id() != active.turn_id()
                || published_turn.binding_revision() != request.expected_binding_revision
                || published_turn.cas_thread_id() != active.usable().cas_thread_id()
                || published_turn.cas_turn_id() != target.cas_turn_id()
            {
                return Err(SyndicMutationError::ActiveSteeringRouteConflict);
            }
        }
    }
    Ok(())
}

fn validate_active_gate(
    gate: &InputGateRecord,
    _binding_revision: beryl_model::BindingRevision,
    active: &crate::ActiveCasBinding,
) -> Result<(), SyndicMutationError> {
    let turn = match gate.state() {
        InputGateState::AwaitingSteering(turn)
        | InputGateState::Steerable(turn)
        | InputGateState::AwaitingTerminal(turn) => *turn,
        InputGateState::Idle
        | InputGateState::PendingTurn(_)
        | InputGateState::Compacting { .. }
        | InputGateState::Stopping { .. }
        | InputGateState::FinalizingHistory(_) => {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
    };
    if turn != active.turn_id() || gate.selected_route().is_none() {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    Ok(())
}

impl AbandonActiveBindingRecords {
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
        mutations.put::<BindingHeadsCodec>(&self.head.thread_id(), &self.head)?;
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
        mutations.delete::<AcceptedReadySourcesCodec>(&ThreadRouteKey {
            thread: self.route_generation.thread_id(),
            generation: self.route_generation.generation(),
        })?;
        if let Some(source) = &self.next_source {
            mutations.put::<AcceptedNextSourcesCodec>(
                &ThreadRouteKey {
                    thread: source.thread_id(),
                    generation: source.generation(),
                },
                source,
            )?;
        }
        if let Some(leaf) = &self.exact_rejected_leaf {
            mutations.put::<AcceptedRouteLeavesCodec>(&leaf.input_id(), leaf)?;
        }
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        Ok(())
    }
}
