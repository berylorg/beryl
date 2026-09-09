use super::scan::{point, scan};
use super::*;
use crate::{codec::*, record::non_idle_gate_source_matches};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<InputGatesFamily>(reader, |thread_id, gate| {
        let source = point::<NonIdleGateSourcesFamily>(reader, thread_id)?;
        if !non_idle_gate_source_matches(*thread_id, Some(gate), source.as_ref()) {
            return Err(SyndicValidationError::Invariant(
                "input gate and non-idle source disagree",
            ));
        }
        Ok(())
    })?;
    scan::<NonIdleGateSourcesFamily>(reader, |thread_id, source| {
        let gate = point::<InputGatesFamily>(reader, thread_id)?;
        if !non_idle_gate_source_matches(*thread_id, gate.as_ref(), Some(source)) {
            return Err(SyndicValidationError::Invariant(
                "non-idle source has no matching current gate",
            ));
        }
        Ok(())
    })
}
