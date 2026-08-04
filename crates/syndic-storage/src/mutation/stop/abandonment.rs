use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder};
use beryl_model::InputGateRevision;

use crate::{
    AcceptedNextSourceRecord, AcceptedRouteAbandonmentKind, AcceptedRouteAbandonmentProof,
    AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord, AcceptedRouteHeadProof,
    AcceptedRouteLostTarget, AcceptedRouteProjectionLostProof, AcceptedRouteTarget,
    BindingHeadRecord, BindingLifecycle, BindingRecord, BindingState, CasThreadBindingIndexRecord,
    CasThreadIndexRecord, HistorySummaryRecord, InputGateRecord, InputGateState,
    SourceEventPayload, SourceEventRecord, SourceEventSequence, StaleCasBinding,
    StopAbandonmentReason, StopAbandonmentWitness, StopDispositionSource, StopOperationId,
    StopOperationRecord, StopOperationRevision, StopOperationState, StopOperationTarget,
    SyndicMutationError, TranscriptBuildRecord, TranscriptViewHeadRecord, TurnEndStatus,
    TurnIncompleteReason, TurnLifecycle, TurnStateRecord, TurnStateRevision,
    codec::*,
    domain::SyndicDomain,
    mutation::{
        binding::membership,
        live::{ActivityEffect, activity_advance},
        point, required,
    },
};

use super::authority::load_live_stop_authority;

mod provider;

use provider::{AbandonmentRecords, StopAbandonmentRecords, provider_abandonment_records};

/// Exact classified authority loss that atomically consumes one live stop operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AbandonStopOperation {
    operation_id: StopOperationId,
    target: StopOperationTarget,
    expected_gate_revision: InputGateRevision,
    expected_stop_revision: StopOperationRevision,
    expected_state_revision: TurnStateRevision,
    reason: StopAbandonmentReason,
    stale: StaleCasBinding,
}

impl AbandonStopOperation {
    /// Captures the complete stop, turn-state, and stale-binding facts to consume atomically.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        operation_id: StopOperationId,
        target: StopOperationTarget,
        expected_gate_revision: InputGateRevision,
        expected_stop_revision: StopOperationRevision,
        expected_state_revision: TurnStateRevision,
        reason: StopAbandonmentReason,
        stale: StaleCasBinding,
    ) -> Self {
        Self {
            operation_id,
            target,
            expected_gate_revision,
            expected_stop_revision,
            expected_state_revision,
            reason,
            stale,
        }
    }

    #[must_use]
    pub const fn operation_id(&self) -> StopOperationId {
        self.operation_id
    }

    #[must_use]
    pub const fn target(&self) -> &StopOperationTarget {
        &self.target
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn expected_stop_revision(&self) -> StopOperationRevision {
        self.expected_stop_revision
    }

    #[must_use]
    pub const fn expected_state_revision(&self) -> TurnStateRevision {
        self.expected_state_revision
    }

    #[must_use]
    pub const fn reason(&self) -> StopAbandonmentReason {
        self.reason
    }

    #[must_use]
    pub const fn stale(&self) -> &StaleCasBinding {
        &self.stale
    }
}

pub(super) struct AbandonStopOperationMutation {
    pub(super) request: AbandonStopOperation,
}

impl DomainMutation<SyndicDomain> for AbandonStopOperationMutation {
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

impl AbandonStopOperationMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<StopAbandonmentRecords, SyndicMutationError> {
        let request = &self.request;
        let authority = load_live_stop_authority(
            reader,
            request.operation_id,
            &request.target,
            request.expected_gate_revision,
            request.expected_stop_revision,
        )?;
        if authority.record.admission().successor_gate_revision() > authority.gate.revision()
            || authority.gate.live_steering_count() != 0
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        if authority.record.admission().is_provider_operation() {
            return provider_abandonment_records(reader, request, authority)
                .map(StopAbandonmentRecords::ProviderOperation);
        }

        let thread = required::<ThreadsFamily>(reader, &request.target.thread_id())?;
        let turn = required::<TurnsFamily>(reader, &request.target.turn_id())?;
        let current_state = required::<TurnStatesFamily>(reader, &request.target.turn_id())?;
        let current_summary =
            required::<HistorySummariesFamily>(reader, &request.target.thread_id())?;
        if current_state.revision() != request.expected_state_revision {
            return Err(SyndicMutationError::TurnStateRevisionConflict {
                expected: request.expected_state_revision,
                current: current_state.revision(),
            });
        }
        if thread.committed_tail() != Some(request.target.turn_id())
            || thread.selected_path().tail() != Some(request.target.turn_id())
            || turn.origin_thread_id() != thread.id()
            || current_state.turn_id() != turn.id()
            || !matches!(
                current_state.lifecycle(),
                TurnLifecycle::Pending | TurnLifecycle::Active
            )
            || current_summary.thread_id() != thread.id()
            || current_summary.thread_revision() != thread.revision()
            || current_summary.committed_tail() != thread.committed_tail()
            || current_summary.selected_path_digest() != thread.selected_path_digest()
            || current_summary.complete()
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
        let BindingState::Active(active) = current_binding.state() else {
            return Err(SyndicMutationError::BindingStateConflict);
        };
        let snapshot = required::<ExecutionSnapshotsFamily>(reader, &request.target.snapshot_id())?;
        let active_turn = required::<ActiveCasTurnsFamily>(reader, &request.target.snapshot_id())?;
        if request.stale.execution() != active.usable().execution()
            || request.stale.cas_thread_id() != active.usable().cas_thread_id()
            || request.stale.observed_tool_profile() != Some(active.usable().tool_profile())
            || request.stale.observed_prefix() != Some(active.usable().represented_prefix())
            || request.stale.observed_lineage() != Some(active.usable().lineage())
            || request.stale.observed_native_turn_count()
                != Some(active.usable().native_turn_count())
            || request.stale.loaded_generation() != Some(snapshot.loaded_generation())
        {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        if request.stale.observed_at() < current_state.updated_at()
            || request.stale.observed_at() < current_summary.last_activity_at()
            || request.stale.observed_at() < turn.submitted_at()
            || request.stale.observed_at() < active.started_at()
            || request.stale.observed_at() < active_turn.published_at()
        {
            return Err(SyndicMutationError::TimestampRegressed);
        }

        let binding_revision = request.target.binding_revision().checked_next()?;
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
            || current_reservation.latest_binding_revision() != request.target.binding_revision()
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
            thread.selected_path(),
            BindingState::stale(request.stale.clone()),
        );
        let binding_head = BindingHeadRecord::new(
            request.target.thread_id(),
            binding_revision,
            BindingLifecycle::Stale,
            thread.selected_path_digest(),
        );

        let source_route = authority
            .gate
            .selected_route()
            .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
        let route_key = ThreadRouteKey {
            thread: request.target.thread_id(),
            generation: source_route.generation(),
        };
        let current_route = required::<AcceptedRouteGenerationsFamily>(reader, &route_key)?;
        if point::<AcceptedReadySourcesFamily>(reader, &route_key)?.is_some() {
            return Err(SyndicMutationError::ActiveSteeringRouteConflict);
        }
        let current_next = point::<AcceptedNextSourcesFamily>(reader, &route_key)?;
        match (current_route.next_turn_count(), current_next) {
            (0, None) => {}
            (count, Some(source))
                if count > 0
                    && source.thread_id() == current_route.thread_id()
                    && source.generation() == current_route.generation()
                    && source.generation_revision() == current_route.revision()
                    && Some(source.first_ordinal()) == current_route.first_ordinal()
                    && Some(source.last_ordinal()) == current_route.last_ordinal() => {}
            _ => return Err(SyndicMutationError::ActiveSteeringRouteConflict),
        }
        let route_revision = current_route.revision().checked_next()?;
        let route = AcceptedRouteGenerationRecord::new(
            current_route.thread_id(),
            current_route.generation(),
            route_revision,
            AcceptedRouteTarget::ProjectionLost(AcceptedRouteProjectionLostProof::new(
                AcceptedRouteLostTarget::Steering(
                    authority
                        .steering_target
                        .ok_or(SyndicMutationError::BindingStateConflict)?,
                ),
                AcceptedRouteAbandonmentProof::new(
                    request.target.binding_revision(),
                    authority.gate.revision(),
                    source_route,
                    AcceptedRouteAbandonmentKind::Generic,
                ),
                binding_revision,
                request.target.snapshot_id(),
                request.target.cas_thread_id().clone(),
            )),
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
        let route_proof = AcceptedRouteHeadProof::new(route.generation(), route.revision());
        let route_head =
            AcceptedRouteGenerationHeadRecord::new(request.target.thread_id(), route_proof);
        let next_source = (route.next_turn_count() > 0).then(|| {
            AcceptedNextSourceRecord::new(
                route.thread_id(),
                route.generation(),
                route.revision(),
                route
                    .first_ordinal()
                    .expect("a stopped route with next work has a first ordinal"),
                route
                    .last_ordinal()
                    .expect("a stopped route with next work has a last ordinal"),
            )
        });

        let current_gate = authority.gate;
        let current_stop = authority.record;
        let sequence = SourceEventSequence::new(
            current_state
                .source_event_count()
                .checked_add(1)
                .ok_or(SyndicMutationError::SourceEventFrontierExhausted)?,
        )?;
        let event_key = TurnEventKey {
            owner: request.target.turn_id(),
            ordinal: sequence,
        };
        if point::<SourceEventsFamily>(reader, &event_key)?.is_some() {
            return Err(SyndicMutationError::SourceEventCollision);
        }
        let status = TurnEndStatus::incomplete(TurnIncompleteReason::AuthorityLost);
        let event = SourceEventRecord::new(
            request.target.turn_id(),
            sequence,
            None,
            SourceEventPayload::TurnEnded(status),
        )?;
        let state_revision = current_state.revision().checked_next()?;
        let state = TurnStateRecord::with_capture_frontiers_and_issue(
            current_state.turn_id(),
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
            InputGateState::FinalizingHistory(request.target.turn_id()),
            current_gate.accepted_high_water(),
            current_gate.route_generation_high_water(),
            Some(route_proof),
            current_gate.live_steering_count(),
            current_gate.live_next_turn_count(),
            current_gate.live_logical_utf8_bytes(),
        )?;
        let summary = if current_summary.complete()
            || current_summary.last_activity_at() != request.stale.observed_at()
        {
            Some(HistorySummaryRecord::new(
                current_summary.thread_id(),
                current_summary.revision().checked_next()?,
                current_summary.thread_revision(),
                current_summary.committed_tail(),
                current_summary.selected_path_digest(),
                false,
                request.stale.observed_at(),
            ))
        } else {
            None
        };
        let (transcript_head, transcript_build) =
            crate::mutation::transcript::invalidate_transcript_projection(reader, &thread)?;
        let activity = activity_advance(
            reader,
            request.target.thread_id(),
            request.target.turn_id(),
            sequence,
            true,
            None,
        )?;
        let witness = StopAbandonmentWitness::new(
            StopDispositionSource::new(current_gate.revision(), current_stop.revision()),
            request.reason,
            gate_revision,
            binding.revision(),
            state_revision,
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
        Ok(StopAbandonmentRecords::Ordinary(Box::new(
            AbandonmentRecords {
                binding,
                binding_head,
                reservation,
                membership,
                route,
                route_head,
                next_source,
                gate,
                stop,
                event,
                state,
                summary,
                transcript_head,
                transcript_build,
                activity,
            },
        )))
    }
}

impl AbandonmentRecords {
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
        let route_key = ThreadRouteKey {
            thread: self.route.thread_id(),
            generation: self.route.generation(),
        };
        mutations.put::<AcceptedRouteGenerationsCodec>(&route_key, &self.route)?;
        mutations.put::<AcceptedRouteGenerationHeadsCodec>(
            &self.route_head.thread_id(),
            &self.route_head,
        )?;
        if let Some(source) = &self.next_source {
            mutations.put::<AcceptedNextSourcesCodec>(&route_key, source)?;
        }
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        mutations.put::<StopOperationsCodec>(&self.stop.id(), &self.stop)?;
        mutations.put::<SourceEventsCodec>(
            &TurnEventKey {
                owner: self.event.turn_id(),
                ordinal: self.event.sequence(),
            },
            &self.event,
        )?;
        mutations.put::<TurnStatesCodec>(&self.state.turn_id(), &self.state)?;
        if let Some(summary) = &self.summary {
            mutations.put::<HistorySummariesCodec>(&summary.thread_id(), summary)?;
        }
        if let Some(head) = &self.transcript_head {
            mutations.put::<TranscriptHeadsCodec>(&head.thread_id(), head)?;
        }
        if let Some(build) = &self.transcript_build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        self.activity.contribute(mutations)
    }
}
