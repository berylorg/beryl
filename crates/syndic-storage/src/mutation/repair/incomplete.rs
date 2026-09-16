use super::*;
use crate::{
    HistorySummaryRecord, RepairResolution, ResolvedRepair, SyndicTimestamp, TranscriptBuildRecord,
    TranscriptViewHeadRecord, TurnEndStatus, TurnIncompleteReason, TurnStateRecord,
    TurnStateRevision,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvergeRepairIncomplete {
    thread_id: SyndicThreadId,
    expected_gate_revision: InputGateRevision,
    expected_state_revision: TurnStateRevision,
    target: RepairRequiredTarget,
    reason: TurnIncompleteReason,
    updated_at: SyndicTimestamp,
}

impl ConvergeRepairIncomplete {
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_gate_revision: InputGateRevision,
        expected_state_revision: TurnStateRevision,
        target: RepairRequiredTarget,
        reason: TurnIncompleteReason,
        updated_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            expected_gate_revision,
            expected_state_revision,
            target,
            reason,
            updated_at,
        }
    }
}

impl SyndicStorage {
    pub fn converge_repair_incomplete(
        &self,
        expected_domain_revision: DomainRevision,
        request: ConvergeRepairIncomplete,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, ConvergeMutation(request))
    }

    pub fn current_converge_repair_incomplete(
        &self,
        request: ConvergeRepairIncomplete,
    ) -> CurrentDomainCommand {
        self.handle.current_command(ConvergeMutation(request))
    }
}

struct ConvergeMutation(ConvergeRepairIncomplete);

struct Prepared {
    gate: InputGateRecord,
    state: TurnStateRecord,
    summary: HistorySummaryRecord,
    head: Option<TranscriptViewHeadRecord>,
    build: Option<TranscriptBuildRecord>,
}

impl DomainMutation<SyndicDomain> for ConvergeMutation {
    type Error = SyndicMutationError;
    type Prepared = Prepared;

    fn prepare(self, reader: &DomainReader<'_, SyndicDomain>) -> Result<Prepared, Self::Error> {
        let request = self.0;
        let gate = input_gate::required_input_gate(reader, &request.thread_id)?;
        if gate.revision() != request.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision,
                current: gate.revision(),
            });
        }
        if gate.state() != &InputGateState::RepairRequired(request.target.clone()) {
            return Err(SyndicMutationError::RepairTargetConflict);
        }
        let current = required::<TurnStatesFamily>(reader, &request.target.turn_id())?;
        if current.revision() != request.expected_state_revision {
            return Err(SyndicMutationError::TurnStateRevisionConflict {
                expected: request.expected_state_revision,
                current: current.revision(),
            });
        }
        if request.updated_at < current.updated_at() {
            return Err(SyndicMutationError::RepairTargetConflict);
        }
        let state = TurnStateRecord::with_capture_frontiers_and_issue(
            current.turn_id(),
            current.revision().checked_next()?,
            current.lifecycle(),
            current.source_event_count(),
            current.item_count(),
            current.finalized_item_count(),
            current.open_item_count(),
            current.history_blocking_item_count(),
            current.provider_observation_issue(),
            Some(TurnEndStatus::new(
                request.target.gap().status().outcome(),
                Some(request.reason),
            )?),
            request.updated_at,
            current.dispatch_provenance(),
        )?
        .with_resolved_repair(ResolvedRepair::new(
            request.target.clone(),
            RepairResolution::Incomplete(request.reason),
        ))?;
        let next_gate = InputGateRecord::new(
            gate.thread_id(),
            gate.revision().checked_next()?,
            InputGateState::FinalizingHistory(request.target.turn_id()),
            gate.accepted_high_water(),
            gate.route_generation_high_water(),
            gate.selected_route(),
            gate.live_steering_count(),
            gate.live_next_turn_count(),
            gate.live_logical_utf8_bytes(),
        )?;
        let thread = required::<ThreadsFamily>(reader, &request.thread_id)?;
        let summary = required::<HistorySummariesFamily>(reader, &request.thread_id)?;
        if summary.thread_id() != thread.id()
            || summary.thread_revision() != thread.revision()
            || summary.committed_tail() != thread.committed_tail()
            || summary.selected_path_digest() != thread.selected_path_digest()
        {
            return Err(SyndicMutationError::RepairTargetConflict);
        }
        let summary = HistorySummaryRecord::new(
            summary.thread_id(),
            summary.revision().checked_next()?,
            summary.thread_revision(),
            summary.committed_tail(),
            summary.selected_path_digest(),
            false,
            summary.last_activity_at().max(request.updated_at),
        );
        let (head, build) =
            crate::mutation::transcript::invalidate_transcript_projection(reader, &thread)?;
        Ok(Prepared {
            gate: next_gate,
            state,
            summary,
            head,
            build,
        })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reserve_input_gate(reservation)?;
        reservation.reserve_records::<TurnStatesCodec>(1)?;
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        reservation.reserve_records::<TranscriptHeadsCodec>(1)?;
        reservation.reserve_records::<TranscriptBuildsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        put_input_gate(mutations, &prepared.gate)?;
        mutations.put::<TurnStatesCodec>(&prepared.state.turn_id(), &prepared.state)?;
        mutations.put::<HistorySummariesCodec>(&prepared.summary.thread_id(), &prepared.summary)?;
        if let Some(head) = prepared.head {
            mutations.put::<TranscriptHeadsCodec>(&head.thread_id(), &head)?;
        }
        if let Some(build) = prepared.build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                &build,
            )?;
        }
        Ok(())
    }
}
