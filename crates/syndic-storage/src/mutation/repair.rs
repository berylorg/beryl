use beryl_home_store::{CurrentDomainCommand, ReconciliationReservation};
use beryl_model::InputGateRevision;

use super::*;
use crate::{InputGateState, RepairRequestDisposition, RepairRequiredTarget};

mod incomplete;
pub use incomplete::ConvergeRepairIncomplete;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequireTerminalRepair {
    thread_id: SyndicThreadId,
    expected_gate_revision: InputGateRevision,
    target: RepairRequiredTarget,
}

impl RequireTerminalRepair {
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_gate_revision: InputGateRevision,
        target: RepairRequiredTarget,
    ) -> Self {
        Self {
            thread_id,
            expected_gate_revision,
            target,
        }
    }
}

impl SyndicStorage {
    pub fn require_terminal_repair(
        &self,
        expected_domain_revision: DomainRevision,
        request: RequireTerminalRepair,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            RequireTerminalRepairMutation(request),
        )
    }

    pub fn current_require_terminal_repair(
        &self,
        request: RequireTerminalRepair,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(RequireTerminalRepairMutation(request))
    }
}

struct RequireTerminalRepairMutation(RequireTerminalRepair);

impl DomainMutation<SyndicDomain> for RequireTerminalRepairMutation {
    type Error = SyndicMutationError;
    type Prepared = InputGateRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let request = self.0;
        let gate = input_gate::required_input_gate(reader, &request.thread_id)?;
        if gate.revision() != request.expected_gate_revision {
            return Err(SyndicMutationError::InputGateRevisionConflict {
                expected: request.expected_gate_revision,
                current: gate.revision(),
            });
        }
        if gate.state() != &InputGateState::FinalizingHistory(request.target.turn_id())
            || gate.live_steering_count() != 0
            || request.target.request() != RepairRequestDisposition::Available
            || !crate::record::repair::retained_repair_target_matches(
                reader,
                request.thread_id,
                &request.target,
            )?
        {
            return Err(SyndicMutationError::RepairTargetConflict);
        }
        Ok(InputGateRecord::new(
            gate.thread_id(),
            gate.revision().checked_next()?,
            InputGateState::RepairRequired(request.target),
            gate.accepted_high_water(),
            gate.route_generation_high_water(),
            gate.selected_route(),
            gate.live_steering_count(),
            gate.live_next_turn_count(),
            gate.live_logical_utf8_bytes(),
        )?)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reserve_input_gate(reservation)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        put_input_gate(mutations, &prepared)?;
        Ok(())
    }
}

pub(super) fn exclude_repair(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    target: Option<SyndicTurnId>,
) -> Result<(), SyndicMutationError> {
    if let Some(gate) = current_input_gate(reader, &thread_id)?
        && let InputGateState::RepairRequired(repair) = gate.state()
        && target.is_none_or(|turn| turn == repair.turn_id())
    {
        return Err(SyndicMutationError::RepairTargetConflict);
    }
    Ok(())
}
