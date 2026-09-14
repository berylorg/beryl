use crate::mutation::input_gate::*;

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

pub struct CancelBindingActivationRecords {
    state: crate::TurnStateRecord,
    transcript_path: Option<crate::TranscriptPathTurnRecord>,
    binding: BindingRecord,
    head: BindingHeadRecord,
    gate: InputGateRecord,
    reservation: CasThreadIndexRecord,
    membership: CasThreadBindingIndexRecord,
}

impl DomainMutation<SyndicDomain> for CancelBindingActivationMutation {
    type Error = SyndicMutationError;
    type Prepared = CancelBindingActivationRecords;

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
        reservation.reserve_records::<TurnStatesCodec>(1)?;
        reservation.reserve_records::<TranscriptPathTurnsCodec>(1)?;
        reserve_input_gate(reservation)?;
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
        let anchor = crate::TurnDispatchAnchor::new(request.snapshot_id(), base.current.revision());
        if !crate::dispatch_provenance::activation_matches(
            request.thread_id(),
            request.turn_id(),
            anchor,
            &snapshot,
            &base.current,
        ) {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        let state = required::<TurnStatesFamily>(reader, &active.turn_id())?;
        if state.revision() != request.expected_state_revision() {
            return Err(SyndicMutationError::TurnStateRevisionConflict {
                expected: request.expected_state_revision(),
                current: state.revision(),
            });
        }
        if state.lifecycle() != TurnLifecycle::Pending || state.source_event_count() != 0 {
            return Err(SyndicMutationError::TurnLifecycleConflict);
        }
        if state.dispatch_provenance() != crate::TurnDispatchProvenance::Activated(anchor) {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        let state =
            state.advance_dispatch_provenance(crate::TurnDispatchProvenance::Cancelled(anchor))?;

        let current_gate = required_input_gate(reader, &request.thread_id())?;
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
        let thread = required::<ThreadsFamily>(reader, &request.thread_id())?;
        let turn = required::<TurnsFamily>(reader, &active.turn_id())?;
        let transcript_path = crate::mutation::transcript::refresh_current_path_state(
            reader, &thread, &turn, &state,
        )?;
        Ok(CancelBindingActivationRecords {
            state,
            transcript_path,
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
        mutations.put::<TurnStatesCodec>(&self.state.turn_id(), &self.state)?;
        if let Some(path) = &self.transcript_path {
            mutations.put::<TranscriptPathTurnsCodec>(
                &ThreadTranscriptPathKey {
                    thread: path.thread_id(),
                    generation: path.generation(),
                    depth: path.depth(),
                },
                path,
            )?;
        }
        mutations.put::<BindingsCodec>(
            &BindingKey {
                thread: self.binding.thread_id(),
                revision: self.binding.revision(),
            },
            &self.binding,
        )?;
        mutations.put::<BindingHeadsCodec>(&self.head.thread_id(), &self.head)?;
        put_input_gate(mutations, &self.gate)?;
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
