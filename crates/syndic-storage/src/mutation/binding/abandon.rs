use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainMutation, DomainReader, MutationBuilder,
};

use crate::{
    AcceptedInputDisposition, AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedInputRecord,
    AcceptedNextTurnIndexRecord, AcceptedOrderIndexRecord, BindingHeadRecord, BindingLifecycle,
    BindingRecord, BindingState, CasThreadBindingIndexRecord, CasThreadIndexRecord,
    InputGateRecord, InputGateState, MAX_LIVE_ACCEPTED_INPUTS, NextTurnReason, SyndicMutationError,
    SyndicRecordError, codec::*, domain::SyndicDomain,
};

use super::{
    AbandonActiveBinding,
    validation::{membership, retirement, transition_base, validate_stale},
};
use crate::mutation::{point, required};

pub(crate) struct AbandonActiveBindingMutation {
    pub(super) request: AbandonActiveBinding,
}

struct AbandonedInput {
    prior_turn: beryl_model::SyndicTurnId,
    input: AcceptedInputRecord,
    order: AcceptedOrderIndexRecord,
    next: Option<AcceptedNextTurnIndexRecord>,
}

struct AbandonActiveBindingRecords {
    binding: BindingRecord,
    head: BindingHeadRecord,
    reservation: CasThreadIndexRecord,
    membership: CasThreadBindingIndexRecord,
    routes: Vec<AbandonedInput>,
    gate: InputGateRecord,
}

impl DomainMutation<SyndicDomain> for AbandonActiveBindingMutation {
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
        if gate.revision() != request.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision,
                current: gate.revision(),
            });
        }
        validate_active_gate(&gate, base.current.revision(), active)?;
        let routes = abandon_steering(reader, &gate, active.turn_id())?;
        let rerouted_count =
            u32::try_from(routes.iter().filter(|route| route.next.is_some()).count())
                .expect("bounded live steering route count fits u32");
        let terminalized_bytes = routes
            .iter()
            .filter(|route| route.next.is_none())
            .try_fold(0_u64, |total, route| {
                total
                    .checked_add(route.input.content().summary().logical_utf8_bytes())
                    .ok_or(SyndicRecordError::LengthOverflow {
                        kind: "terminalized accepted-input bytes",
                    })
            })?;
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
            routes,
            gate,
        })
    }
}

fn validate_active_gate(
    gate: &InputGateRecord,
    binding_revision: beryl_model::BindingRevision,
    active: &crate::ActiveCasBinding,
) -> Result<(), SyndicMutationError> {
    let pending = match gate.state() {
        InputGateState::AwaitingSteering(pending) => pending,
        InputGateState::Steerable(target) | InputGateState::Stopping(target) => target.pending(),
        InputGateState::Idle | InputGateState::PendingTurn(_) | InputGateState::Compacting(_) => {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
    };
    if pending.binding_revision() != binding_revision
        || pending.snapshot_id() != active.snapshot_id()
        || pending.active_turn_id() != active.turn_id()
        || pending.cas_thread_id() != active.usable().cas_thread_id()
    {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    Ok(())
}

fn abandon_steering(
    reader: &DomainReader<'_, SyndicDomain>,
    gate: &InputGateRecord,
    turn_id: beryl_model::SyndicTurnId,
) -> Result<Vec<AbandonedInput>, SyndicMutationError> {
    let expected_disposition = match gate.state() {
        InputGateState::AwaitingSteering(pending) => {
            AcceptedInputDisposition::AwaitingSteering(pending.clone())
        }
        InputGateState::Steerable(target) | InputGateState::Stopping(target) => {
            AcceptedInputDisposition::SteerActiveTurn(target.clone())
        }
        InputGateState::Idle | InputGateState::PendingTurn(_) | InputGateState::Compacting(_) => {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
    };
    let first = SteeringKey {
        thread: gate.thread_id(),
        turn: turn_id,
        ordinal: AcceptedInputOrdinal::FIRST,
    };
    let last = SteeringKey {
        thread: gate.thread_id(),
        turn: turn_id,
        ordinal: AcceptedInputOrdinal::new(u64::MAX)
            .expect("maximum accepted-input ordinal is nonzero"),
    };
    let page = reader.cursor::<AcceptedSteeringCodec>(
        &CursorRange::closed(first, last),
        CursorDirection::Forward,
        CursorReadLimits::new(MAX_LIVE_ACCEPTED_INPUTS as usize, 32 * 1024 * 1024)
            .expect("abandoned-route rewrite bounds are nonzero"),
    )?;
    if page.has_more()
        || page.records().len()
            != usize::try_from(gate.live_steering_count())
                .expect("u32 live steering count fits usize")
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }

    let mut abandoned = Vec::with_capacity(page.records().len());
    for record in page.records() {
        let index = record.value();
        if record.key().thread != gate.thread_id()
            || record.key().turn != turn_id
            || index.thread_id() != gate.thread_id()
            || index.turn_id() != turn_id
            || index.ordinal() != record.key().ordinal
        {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let current = required::<AcceptedInputsFamily>(reader, &index.input_id())?;
        let order_key = ThreadAcceptedKey {
            owner: current.thread_id(),
            ordinal: current.ordinal(),
        };
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
            || current.disposition() != &expected_disposition
            || point::<AcceptedOrderFamily>(reader, &order_key)?.as_ref() != Some(&expected_order)
            || point::<AcceptedNextFamily>(reader, &order_key)?.is_some()
        {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let revision = current.revision().checked_next()?;
        let (disposition, lifecycle, remains_live) = match current.lifecycle() {
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable => (
                AcceptedInputDisposition::NextTurn(NextTurnReason::ProjectionLost),
                current.lifecycle(),
                true,
            ),
            AcceptedInputLifecycle::Delivering
                if matches!(
                    current.disposition(),
                    AcceptedInputDisposition::SteerActiveTurn(_)
                ) =>
            {
                (
                    current.disposition().clone(),
                    AcceptedInputLifecycle::DeliveryUnknown,
                    false,
                )
            }
            AcceptedInputLifecycle::Delivering
            | AcceptedInputLifecycle::Delivered
            | AcceptedInputLifecycle::Failed
            | AcceptedInputLifecycle::DeliveryUnknown => {
                return Err(SyndicMutationError::ActiveSteeringRouteConflict);
            }
        };
        let input = AcceptedInputRecord::new(
            current.id(),
            current.thread_id(),
            revision,
            current.ordinal(),
            current.gate_revision(),
            disposition,
            lifecycle,
            current.content(),
            current.marker_count(),
            current.admitted_at(),
        );
        abandoned.push(AbandonedInput {
            prior_turn: turn_id,
            order: AcceptedOrderIndexRecord::new(
                input.thread_id(),
                input.ordinal(),
                input.id(),
                revision,
            ),
            next: remains_live.then(|| {
                AcceptedNextTurnIndexRecord::new(
                    input.thread_id(),
                    input.ordinal(),
                    input.id(),
                    revision,
                )
            }),
            input,
        });
    }
    Ok(abandoned)
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
        for route in &self.routes {
            let order_key = ThreadAcceptedKey {
                owner: route.input.thread_id(),
                ordinal: route.input.ordinal(),
            };
            mutations.put::<AcceptedInputsCodec>(&route.input.id(), &route.input)?;
            mutations.put::<AcceptedOrderCodec>(&order_key, &route.order)?;
            if let Some(next) = &route.next {
                mutations.put::<AcceptedNextCodec>(&order_key, next)?;
            }
            mutations.delete::<AcceptedSteeringCodec>(&SteeringKey {
                thread: route.input.thread_id(),
                turn: route.prior_turn,
                ordinal: route.input.ordinal(),
            })?;
        }
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        Ok(())
    }
}
