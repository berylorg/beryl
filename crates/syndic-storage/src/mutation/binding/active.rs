use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainMutation, DomainReader, MutationBuilder,
};

use crate::{
    AcceptedInputDisposition, AcceptedInputOrdinal, AcceptedInputRecord, AcceptedOrderIndexRecord,
    AcceptedSteeringIndexRecord, ActiveCasBinding, ActiveCasTurnRecord, BindingHeadRecord,
    BindingLifecycle, BindingRecord, BindingState, CasThreadBindingIndexRecord,
    CasThreadIndexRecord, CasTurnIndexRecord, ExecutionSnapshotRecord, InputGateRecord,
    InputGateState, MAX_LIVE_ACCEPTED_INPUTS, PendingSteeringTargetProof, SteeringTargetProof,
    SyndicMutationError, TurnLifecycle, codec::*, domain::SyndicDomain,
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

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

struct ActivateBindingRecords {
    binding: BindingRecord,
    head: BindingHeadRecord,
    snapshot: ExecutionSnapshotRecord,
    gate: InputGateRecord,
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
        if let Some(required_generation) = usable.lineage().recovered_loaded_generation()
            && request.loaded_generation != required_generation
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        if let Some(completed_at) = usable.lineage().recovered_completed_at()
            && request.started_at < completed_at
        {
            return Err(SyndicMutationError::TimestampRegressed);
        }
        usable.native_turn_count().checked_next()?;
        let reservation = reservation(reader, usable, request.thread_id, base.next_revision)?;

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
        let gate = InputGateRecord::new(
            request.thread_id,
            activation_gate_revision,
            InputGateState::AwaitingSteering(pending),
            current_gate.accepted_high_water(),
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

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

struct RewrittenSteeringRoute {
    input: AcceptedInputRecord,
    order: AcceptedOrderIndexRecord,
    steering: AcceptedSteeringIndexRecord,
}

struct PublishActiveCasTurnRecords {
    active_turn: ActiveCasTurnRecord,
    cas_turn_index: CasTurnIndexRecord,
    routes: Vec<RewrittenSteeringRoute>,
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
        let InputGateState::AwaitingSteering(pending) = current_gate.state() else {
            return Err(SyndicMutationError::InputGateStateConflict);
        };
        if pending.binding_revision() != request.binding_revision
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
        let routes = rewrite_awaiting_routes(reader, &current_gate, pending, &steering_target)?;
        let gate = InputGateRecord::new(
            request.thread_id,
            current_gate.revision().checked_next()?,
            InputGateState::Steerable(steering_target),
            current_gate.accepted_high_water(),
            current_gate.live_steering_count(),
            current_gate.live_next_turn_count(),
            current_gate.live_logical_utf8_bytes(),
        )?;
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
            routes,
            gate,
        })
    }
}

fn rewrite_awaiting_routes(
    reader: &DomainReader<'_, SyndicDomain>,
    gate: &InputGateRecord,
    pending: &PendingSteeringTargetProof,
    target: &SteeringTargetProof,
) -> Result<Vec<RewrittenSteeringRoute>, SyndicMutationError> {
    let first = SteeringKey {
        thread: gate.thread_id(),
        turn: pending.active_turn_id(),
        ordinal: AcceptedInputOrdinal::FIRST,
    };
    let last = SteeringKey {
        thread: gate.thread_id(),
        turn: pending.active_turn_id(),
        ordinal: AcceptedInputOrdinal::new(u64::MAX)
            .expect("maximum accepted-input ordinal is nonzero"),
    };
    let page = reader.cursor::<AcceptedSteeringCodec>(
        &CursorRange::closed(first, last),
        CursorDirection::Forward,
        CursorReadLimits::new(MAX_LIVE_ACCEPTED_INPUTS as usize, 32 * 1024 * 1024)
            .expect("active-route rewrite bounds are nonzero"),
    )?;
    if page.has_more()
        || page.records().len()
            != usize::try_from(gate.live_steering_count())
                .expect("u32 live steering count fits usize")
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }

    let mut rewritten = Vec::with_capacity(page.records().len());
    for record in page.records() {
        let index = record.value();
        if record.key().thread != gate.thread_id()
            || record.key().turn != pending.active_turn_id()
            || index.thread_id() != gate.thread_id()
            || index.turn_id() != pending.active_turn_id()
            || index.ordinal() != record.key().ordinal
        {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let current = required::<AcceptedInputsFamily>(reader, &index.input_id())?;
        let expected_order = AcceptedOrderIndexRecord::new(
            current.thread_id(),
            current.ordinal(),
            current.id(),
            current.revision(),
        );
        if current.thread_id() != gate.thread_id()
            || current.ordinal() != index.ordinal()
            || current.revision() != index.input_revision()
            || current.lifecycle().is_terminal()
            || current.disposition() != &AcceptedInputDisposition::AwaitingSteering(pending.clone())
            || point::<AcceptedOrderFamily>(
                reader,
                &ThreadAcceptedKey {
                    owner: current.thread_id(),
                    ordinal: current.ordinal(),
                },
            )?
            .as_ref()
                != Some(&expected_order)
        {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let revision = current.revision().checked_next()?;
        let input = AcceptedInputRecord::new(
            current.id(),
            current.thread_id(),
            revision,
            current.ordinal(),
            current.gate_revision(),
            AcceptedInputDisposition::SteerActiveTurn(target.clone()),
            current.lifecycle(),
            current.content(),
            current.marker_count(),
            current.admitted_at(),
        );
        rewritten.push(RewrittenSteeringRoute {
            order: AcceptedOrderIndexRecord::new(
                input.thread_id(),
                input.ordinal(),
                input.id(),
                revision,
            ),
            steering: AcceptedSteeringIndexRecord::new(
                input.thread_id(),
                pending.active_turn_id(),
                input.ordinal(),
                input.id(),
                revision,
            ),
            input,
        });
    }
    Ok(rewritten)
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
        for route in &self.routes {
            mutations.put::<AcceptedInputsCodec>(&route.input.id(), &route.input)?;
            mutations.put::<AcceptedOrderCodec>(
                &ThreadAcceptedKey {
                    owner: route.input.thread_id(),
                    ordinal: route.input.ordinal(),
                },
                &route.order,
            )?;
            mutations.put::<AcceptedSteeringCodec>(
                &SteeringKey {
                    thread: route.input.thread_id(),
                    turn: route.steering.turn_id(),
                    ordinal: route.input.ordinal(),
                },
                &route.steering,
            )?;
        }
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        Ok(())
    }
}
