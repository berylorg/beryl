use beryl_home_store::DomainReader;

use super::super::scan::{point, require};
use crate::{
    TurnDispatchProvenance, TurnKind, TurnLifecycle, TurnRecord, TurnStateRecord, codec::*,
    domain::SyndicDomain, error::SyndicValidationError,
};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
    turn: &TurnRecord,
    state: &TurnStateRecord,
) -> Result<(), SyndicValidationError> {
    let provenance = state.dispatch_provenance();
    if matches!(turn.kind(), TurnKind::ProviderOperation(_)) {
        return if provenance == TurnDispatchProvenance::ProviderOperation {
            Ok(())
        } else {
            super::invariant("provider-operation dispatch provenance disagrees")
        };
    }
    let (anchor, cancelled) = match provenance {
        TurnDispatchProvenance::Unattempted => {
            if state.lifecycle() == TurnLifecycle::Active
                || (state.lifecycle() == TurnLifecycle::Pending && state.source_event_count() != 0)
            {
                return super::invariant("unattempted turn has active capture authority");
            }
            return Ok(());
        }
        TurnDispatchProvenance::Activated(anchor) => (anchor, false),
        TurnDispatchProvenance::Cancelled(anchor) => (anchor, true),
        TurnDispatchProvenance::ProviderOperation => {
            return super::invariant("ordinary turn carries provider-operation provenance");
        }
    };
    let binding = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: turn.origin_thread_id(),
            revision: anchor.binding_revision(),
        },
        "dispatch provenance active binding is missing",
    )?;
    let snapshot = require::<ExecutionSnapshotsFamily>(
        reader,
        &anchor.snapshot_id(),
        "dispatch provenance snapshot is missing",
    )?;
    if !crate::dispatch_provenance::activation_matches(
        turn.origin_thread_id(),
        turn.id(),
        anchor,
        &snapshot,
        &binding,
    ) {
        return super::invariant("dispatch provenance activation disagrees");
    }
    if cancelled {
        let revision = anchor.binding_revision().checked_next().map_err(|_| {
            SyndicValidationError::Invariant("dispatch provenance binding revision is exhausted")
        })?;
        let successor = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: turn.origin_thread_id(),
                revision,
            },
            "cancelled dispatch successor is missing",
        )?;
        if !crate::dispatch_provenance::cancelled_successor_matches(&binding, &successor)
            || point::<ActiveCasTurnsFamily>(reader, &anchor.snapshot_id())?.is_some()
            || state.lifecycle() == TurnLifecycle::Active
            || (state.lifecycle() == TurnLifecycle::Pending && state.source_event_count() != 0)
        {
            return super::invariant("cancelled dispatch provenance disagrees");
        }
    }
    Ok(())
}
