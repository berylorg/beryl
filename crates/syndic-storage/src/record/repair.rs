use beryl_model::SyndicTurnId;

use crate::{RepairRequiredTarget, SourceEventPayload, SourceEventRecord, SourceEventSequence};

mod retained;
pub(crate) use retained::retained_repair_target_matches;

pub(crate) fn repair_source_events_match<E>(
    target: &RepairRequiredTarget,
    mut read: impl FnMut(SyndicTurnId, SourceEventSequence) -> Result<Option<SourceEventRecord>, E>,
) -> Result<bool, E> {
    let gap = target.gap();
    let Some(terminal) = read(target.turn_id(), gap.terminal().sequence())? else {
        return Ok(false);
    };
    if terminal.turn_id() != target.turn_id()
        || terminal.sequence() != gap.terminal().sequence()
        || terminal.source() != Some(target.source())
        || !matches!(terminal.payload(), SourceEventPayload::TurnEnded(status) if *status == gap.status())
        || terminal.repair_witness() != gap.terminal()
    {
        return Ok(false);
    }
    let Some(witness) = gap.issue() else {
        return Ok(true);
    };
    let Some(event) = read(target.turn_id(), witness.sequence())? else {
        return Ok(false);
    };
    Ok(event.turn_id() == target.turn_id()
        && event.sequence() == witness.sequence()
        && event.source() == Some(target.source())
        && matches!(event.payload(), SourceEventPayload::ProviderObservationIssue(issue) if issue.source() == target.source())
        && event.repair_witness() == witness)
}
