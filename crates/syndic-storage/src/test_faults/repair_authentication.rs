use beryl_model::SyndicTurnId;

use crate::{RepairRequiredTarget, SourceEventRecord, SourceEventSequence};

pub fn repair_source_events_match_for_test<E>(
    target: &RepairRequiredTarget,
    read: impl FnMut(SyndicTurnId, SourceEventSequence) -> Result<Option<SourceEventRecord>, E>,
) -> Result<bool, E> {
    crate::record::repair::repair_source_events_match(target, read)
}
