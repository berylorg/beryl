use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder};

use crate::{
    AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord, AcceptedRouteHeadProof,
    AcceptedRouteRevision, AcceptedRouteTarget, InputGateRecord, InputGateState, StopCause,
    StopDispatchClaimWitness, StopDispositionSource, StopOperationRecord, StopOperationState,
    StopSafeReopenWitness, SyndicMutationError, codec::*, domain::SyndicDomain, mutation::point,
};

use super::{
    ClaimStopDispatchMutation, JoinStopCauseMutation, SafelyReopenStopOperationMutation,
    authority::load_live_stop_authority,
};

impl DomainMutation<SyndicDomain> for JoinStopCauseMutation {
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
        mutations.put::<StopOperationsCodec>(&successor.id(), &successor)?;
        Ok(())
    }
}

impl JoinStopCauseMutation {
    fn successor(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<StopOperationRecord, SyndicMutationError> {
        let request = &self.request;
        let authority = load_live_stop_authority(
            reader,
            request.operation_id,
            &request.target,
            request.expected_gate_revision,
            request.expected_stop_revision,
        )?;
        if authority.record.causes().contains(request.cause) {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        let successor_revision = authority.record.revision().checked_next()?;
        let cause_first_revisions = authority
            .record
            .cause_first_revisions()
            .publish(request.cause, successor_revision)
            .ok_or(SyndicMutationError::InputGateStateConflict)?;
        StopOperationRecord::new(
            authority.record.id(),
            authority.record.target().clone(),
            authority.record.admission(),
            successor_revision,
            cause_first_revisions,
            authority.record.dispatch_claim(),
            authority.record.state(),
        )
        .map_err(|_| SyndicMutationError::InputGateStateConflict)
    }
}

impl DomainMutation<SyndicDomain> for ClaimStopDispatchMutation {
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
        mutations.put::<StopOperationsCodec>(&successor.id(), &successor)?;
        Ok(())
    }
}

impl ClaimStopDispatchMutation {
    fn successor(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<StopOperationRecord, SyndicMutationError> {
        let request = &self.request;
        let authority = load_live_stop_authority(
            reader,
            request.operation_id,
            &request.target,
            request.expected_gate_revision,
            request.expected_stop_revision,
        )?;
        if authority.record.state() != StopOperationState::Admitted
            || authority.record.dispatch_claim().is_some()
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        let successor_revision = authority.record.revision().checked_next()?;
        StopOperationRecord::new(
            authority.record.id(),
            authority.record.target().clone(),
            authority.record.admission(),
            successor_revision,
            authority.record.cause_first_revisions(),
            Some(StopDispatchClaimWitness::new(
                authority.record.revision(),
                request.attempt,
            )),
            StopOperationState::DispatchClaimed,
        )
        .map_err(|_| SyndicMutationError::InputGateStateConflict)
    }
}

struct SafeReopenRecords {
    route: Option<AcceptedRouteGenerationRecord>,
    route_head: Option<AcceptedRouteGenerationHeadRecord>,
    gate: InputGateRecord,
    stop: StopOperationRecord,
    compaction: Option<crate::CompactionOperationRecord>,
}

impl DomainMutation<SyndicDomain> for SafelyReopenStopOperationMutation {
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

impl SafelyReopenStopOperationMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<SafeReopenRecords, SyndicMutationError> {
        let request = &self.request;
        let authority = load_live_stop_authority(
            reader,
            request.operation_id,
            &request.target,
            request.expected_gate_revision,
            request.expected_stop_revision,
        )?;
        if authority
            .record
            .causes()
            .contains(StopCause::InterruptingApproval)
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        if authority.record.admission().is_provider_operation() {
            return provider_safe_reopen(reader, authority);
        }

        let generation = authority.gate.next_route_generation()?;
        let route_key = ThreadRouteKey {
            thread: request.target.thread_id(),
            generation,
        };
        if point::<AcceptedRouteGenerationsFamily>(reader, &route_key)?.is_some() {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let route = AcceptedRouteGenerationRecord::new(
            request.target.thread_id(),
            generation,
            AcceptedRouteRevision::FIRST,
            AcceptedRouteTarget::Steering(
                authority
                    .steering_target
                    .ok_or(SyndicMutationError::BindingStateConflict)?,
            ),
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
        let route_proof = AcceptedRouteHeadProof::new(route.generation(), route.revision());
        let route_head = AcceptedRouteGenerationHeadRecord::new(route.thread_id(), route_proof);
        let gate_revision = authority.gate.revision().checked_next()?;
        let gate = InputGateRecord::new(
            authority.gate.thread_id(),
            gate_revision,
            InputGateState::Steerable(request.target.turn_id()),
            authority.gate.accepted_high_water(),
            Some(generation),
            Some(route_proof),
            0,
            authority.gate.live_next_turn_count(),
            authority.gate.live_logical_utf8_bytes(),
        )?;
        let source =
            StopDispositionSource::new(authority.gate.revision(), authority.record.revision());
        let witness = StopSafeReopenWitness::new(source, gate_revision, route_proof);
        let stop = StopOperationRecord::new(
            authority.record.id(),
            authority.record.target().clone(),
            authority.record.admission(),
            authority.record.revision().checked_next()?,
            authority.record.cause_first_revisions(),
            authority.record.dispatch_claim(),
            StopOperationState::SafeReopened(witness),
        )
        .map_err(|_| SyndicMutationError::InputGateStateConflict)?;

        Ok(SafeReopenRecords {
            route: Some(route),
            route_head: Some(route_head),
            gate,
            stop,
            compaction: None,
        })
    }
}

impl SafeReopenRecords {
    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        if let (Some(route), Some(route_head)) = (&self.route, &self.route_head) {
            mutations.put::<AcceptedRouteGenerationsCodec>(
                &ThreadRouteKey {
                    thread: route.thread_id(),
                    generation: route.generation(),
                },
                route,
            )?;
            mutations
                .put::<AcceptedRouteGenerationHeadsCodec>(&route_head.thread_id(), route_head)?;
        }
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        mutations.put::<StopOperationsCodec>(&self.stop.id(), &self.stop)?;
        if let Some(compaction) = &self.compaction {
            mutations.put::<CompactionOperationsCodec>(&compaction.id(), compaction)?;
        }
        Ok(())
    }
}

fn provider_safe_reopen(
    reader: &DomainReader<'_, SyndicDomain>,
    authority: super::authority::LiveStopAuthority,
) -> Result<SafeReopenRecords, SyndicMutationError> {
    let operation_id = crate::CompactionOperationId::new(
        authority.record.target().thread_id(),
        crate::CompactionOperationNonce::from_bytes(
            *authority.record.target().turn_id().as_bytes(),
        ),
    );
    let current_compaction =
        crate::mutation::required::<CompactionOperationsFamily>(reader, &operation_id)?;
    let compaction = current_compaction.reopen_from_stop(authority.record.id().nonce())?;
    let gate_revision = authority.gate.revision().checked_next()?;
    let gate = InputGateRecord::new(
        authority.gate.thread_id(),
        gate_revision,
        InputGateState::compacting(authority.record.target().turn_id(), operation_id.nonce()),
        authority.gate.accepted_high_water(),
        authority.gate.route_generation_high_water(),
        authority.gate.selected_route(),
        0,
        authority.gate.live_next_turn_count(),
        authority.gate.live_logical_utf8_bytes(),
    )?;
    let source = StopDispositionSource::new(authority.gate.revision(), authority.record.revision());
    let witness = StopSafeReopenWitness::provider_operation(
        source,
        gate_revision,
        current_compaction.revision(),
        compaction.revision(),
    );
    let stop = StopOperationRecord::new(
        authority.record.id(),
        authority.record.target().clone(),
        authority.record.admission(),
        authority.record.revision().checked_next()?,
        authority.record.cause_first_revisions(),
        authority.record.dispatch_claim(),
        StopOperationState::SafeReopened(witness),
    )
    .map_err(|_| SyndicMutationError::InputGateStateConflict)?;
    Ok(SafeReopenRecords {
        route: None,
        route_head: None,
        gate,
        stop,
        compaction: Some(compaction),
    })
}
