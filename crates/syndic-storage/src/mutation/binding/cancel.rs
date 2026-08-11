use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use crate::{
    AcceptedRouteTarget, BindingHeadRecord, BindingLifecycle, BindingRecord, BindingState,
    CasThreadBindingIndexRecord, CasThreadIndexRecord, InputGateRecord, InputGateState,
    SyndicMutationError, TurnLifecycle, codec::*, domain::SyndicDomain,
};

use super::{
    CancelBindingActivation,
    validation::{advance_reservation, membership, transition_base},
};
use crate::mutation::{point, required};

pub(crate) struct CancelBindingActivationMutation {
    pub(super) request: CancelBindingActivation,
}

struct CancelBindingActivationRecords {
    binding: BindingRecord,
    head: BindingHeadRecord,
    gate: InputGateRecord,
    reservation: CasThreadIndexRecord,
    membership: CasThreadBindingIndexRecord,
}

impl DomainMutation<SyndicDomain> for CancelBindingActivationMutation {
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
        reservation.reserve_records::<InputGatesCodec>(1)?;
        reservation.reserve_records::<CasThreadIndexCodec>(1)?;
        reservation.reserve_records::<CasThreadBindingIndexCodec>(1)?;
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

impl CancelBindingActivationMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<CancelBindingActivationRecords, SyndicMutationError> {
        let request = &self.request;
        let base = transition_base(
            reader,
            request.thread_id(),
            request.expected_binding_revision(),
            request.selected_path(),
        )?;
        let BindingState::Active(active) = base.current.state() else {
            return Err(SyndicMutationError::BindingStateConflict);
        };
        if active.snapshot_id() != request.snapshot_id()
            || active.turn_id() != request.turn_id()
            || point::<ActiveCasTurnsFamily>(reader, &request.snapshot_id())?.is_some()
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }

        let snapshot = required::<ExecutionSnapshotsFamily>(reader, &request.snapshot_id())?;
        if snapshot.thread_id() != request.thread_id()
            || snapshot.binding_revision() != base.current.revision()
            || snapshot.activation_gate_revision() != active.activation_gate_revision()
            || snapshot.active_turn_id() != active.turn_id()
            || snapshot.cas_thread_id() != active.usable().cas_thread_id()
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        let state = required::<TurnStatesFamily>(reader, &active.turn_id())?;
        if state.lifecycle() != TurnLifecycle::Pending || state.source_event_count() != 0 {
            return Err(SyndicMutationError::TurnLifecycleConflict);
        }

        let current_gate = required::<InputGatesFamily>(reader, &request.thread_id())?;
        if current_gate.revision() != request.expected_gate_revision() {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision(),
                current: current_gate.revision(),
            });
        }
        let InputGateState::AwaitingSteering(gate_turn) = current_gate.state() else {
            return Err(SyndicMutationError::InputGateStateConflict);
        };
        let route_proof = current_gate
            .selected_route()
            .ok_or(SyndicMutationError::InputGateStateConflict)?;
        let route = required::<AcceptedRouteGenerationsFamily>(
            reader,
            &ThreadRouteKey {
                thread: request.thread_id(),
                generation: route_proof.generation(),
            },
        )?;
        let AcceptedRouteTarget::AwaitingSteering(pending) = route.target() else {
            return Err(SyndicMutationError::InputGateStateConflict);
        };
        if *gate_turn != active.turn_id()
            || route.revision() != route_proof.revision()
            || pending.binding_revision() != base.current.revision()
            || pending.snapshot_id() != active.snapshot_id()
            || pending.active_turn_id() != active.turn_id()
            || pending.cas_thread_id() != active.usable().cas_thread_id()
            || current_gate.live_count() != 0
            || current_gate.live_logical_utf8_bytes() != 0
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }

        let binding = BindingRecord::new(
            request.thread_id(),
            base.next_revision,
            request.selected_path(),
            BindingState::valid(active.usable().clone()),
        );
        let head = BindingHeadRecord::new(
            request.thread_id(),
            base.next_revision,
            BindingLifecycle::Valid,
            request.selected_path().digest(),
        );
        let gate = InputGateRecord::new(
            request.thread_id(),
            current_gate.revision().checked_next()?,
            InputGateState::PendingTurn(active.turn_id()),
            current_gate.accepted_high_water(),
            current_gate.route_generation_high_water(),
            None,
            0,
            0,
            0,
        )?;
        let reservation = advance_reservation(
            reader,
            active.usable().cas_thread_id(),
            request.thread_id(),
            base.current.revision(),
            base.next_revision,
        )?;
        let membership = membership(
            reader,
            active.usable().cas_thread_id(),
            request.thread_id(),
            base.next_revision,
        )?;
        Ok(CancelBindingActivationRecords {
            binding,
            head,
            gate,
            reservation,
            membership,
        })
    }
}

impl CancelBindingActivationRecords {
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
