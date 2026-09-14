use beryl_home_store::DomainReader;
use beryl_model::{SyndicThreadId, SyndicTurnId};

use crate::{
    SyndicMutationError, TurnDispatchProvenance,
    codec::*,
    domain::SyndicDomain,
    mutation::{point, required},
};

pub(super) fn authenticate_pending(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    provenance: TurnDispatchProvenance,
) -> Result<(), SyndicMutationError> {
    let anchor = match provenance {
        TurnDispatchProvenance::Unattempted => return Ok(()),
        TurnDispatchProvenance::Cancelled(anchor) => anchor,
        TurnDispatchProvenance::Activated(_) | TurnDispatchProvenance::ProviderOperation => {
            return Err(SyndicMutationError::TurnLifecycleConflict);
        }
    };
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread_id,
            revision: anchor.binding_revision(),
        },
    )?;
    let successor = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread_id,
            revision: anchor.binding_revision().checked_next()?,
        },
    )?;
    let snapshot = required::<ExecutionSnapshotsFamily>(reader, &anchor.snapshot_id())?;
    if !crate::dispatch_provenance::activation_matches(
        thread_id, turn_id, anchor, &snapshot, &binding,
    ) || !crate::dispatch_provenance::cancelled_successor_matches(&binding, &successor)
        || point::<ActiveCasTurnsFamily>(reader, &anchor.snapshot_id())?.is_some()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    Ok(())
}
