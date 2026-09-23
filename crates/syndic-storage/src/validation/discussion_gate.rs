use super::scan::{point, require, scan};
use crate::{
    DiscussionHandoffGateState, codec::*, domain::SyndicDomain, error::SyndicValidationError,
};
use beryl_home_store::DomainReader;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ThreadsFamily>(reader, |key, thread| {
        let gate = point::<DiscussionHandoffGatesFamily>(reader, key)?;
        if thread.parent_thread_id().is_some() != gate.is_some() {
            return Err(SyndicValidationError::Invariant(
                "discussion gate presence disagrees with thread lineage",
            ));
        }
        Ok(())
    })?;
    scan::<DiscussionHandoffGatesFamily>(reader, |key, gate| {
        let thread = require::<ThreadsFamily>(reader, key, "discussion gate thread is missing")?;
        if gate.thread_id() != *key
            || thread.parent_thread_id().is_none()
            || thread.context_owner_id().is_none()
        {
            return Err(SyndicValidationError::Invariant(
                "discussion gate key or ownership disagrees",
            ));
        }
        if let DiscussionHandoffGateState::Pending {
            resolving_turn_id, ..
        } = gate.state()
        {
            let turn = require::<TurnsFamily>(
                reader,
                &resolving_turn_id,
                "discussion gate resolving turn is missing",
            )?;
            if turn.origin_thread_id() != *key {
                return Err(SyndicValidationError::Invariant(
                    "discussion gate resolving turn is foreign",
                ));
            }
        }
        Ok(())
    })
}
