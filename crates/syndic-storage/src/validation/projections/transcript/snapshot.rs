use beryl_home_store::DomainReader;

use crate::{TranscriptBuildPhase, codec::*, domain::SyndicDomain, error::SyndicValidationError};

use crate::validation::scan::require;

use super::invariant;

pub(super) fn path_snapshot_must_match_current(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &crate::TranscriptBuildRecord,
) -> Result<bool, SyndicValidationError> {
    match build.phase() {
        TranscriptBuildPhase::Collecting { .. } | TranscriptBuildPhase::Publishing { .. } => {
            Ok(true)
        }
        TranscriptBuildPhase::Complete => {
            let head = require::<TranscriptHeadsFamily>(
                reader,
                &build.thread_id(),
                "completed transcript path head is missing",
            )?;
            Ok(head.generation() == build.generation()
                && head.lifecycle() == crate::ProjectionLifecycle::Current)
        }
        TranscriptBuildPhase::Superseded => Ok(false),
    }
}

pub(super) fn validate_path_state_snapshot(
    reader: &DomainReader<'_, SyndicDomain>,
    path: &crate::TranscriptPathTurnRecord,
    turn: &crate::TurnRecord,
    current: &crate::TurnStateRecord,
    require_current: bool,
) -> Result<(), SyndicValidationError> {
    if current.turn_id() != path.turn_id()
        || path.state_revision() > current.revision()
        || path.finalized_item_count() > path.item_count()
        || path.item_count() > current.item_count()
        || path.finalized_item_count() > current.finalized_item_count()
        || path.source_event_count() > current.source_event_count()
        || path.updated_at() < turn.submitted_at()
        || path.updated_at() > current.updated_at()
    {
        return invariant("transcript path state snapshot frontier is invalid");
    }

    let exact = path.state_revision() == current.revision();
    if (require_current || exact) && !path_state_snapshot_equals(path, current) {
        return invariant("current transcript path state snapshot is stale");
    }
    if !exact && !snapshot_lifecycle_may_precede(path.lifecycle(), current.lifecycle()) {
        return invariant("historical transcript path lifecycle is impossible");
    }

    validate_snapshot_source_frontier(reader, path, current)?;
    validate_snapshot_item_frontier(reader, path)?;
    Ok(())
}

pub(super) fn path_state_snapshot_equals(
    path: &crate::TranscriptPathTurnRecord,
    state: &crate::TurnStateRecord,
) -> bool {
    path.state_revision() == state.revision()
        && path.lifecycle() == state.lifecycle()
        && path.source_event_count() == state.source_event_count()
        && path.item_count() == state.item_count()
        && path.finalized_item_count() == state.finalized_item_count()
        && path.updated_at() == state.updated_at()
}

fn snapshot_lifecycle_may_precede(
    snapshot: crate::TurnLifecycle,
    current: crate::TurnLifecycle,
) -> bool {
    match snapshot {
        crate::TurnLifecycle::Pending => true,
        crate::TurnLifecycle::Active => current != crate::TurnLifecycle::Pending,
        crate::TurnLifecycle::UnknownTerminal => !matches!(current, crate::TurnLifecycle::Pending),
        crate::TurnLifecycle::Complete
        | crate::TurnLifecycle::Interrupted
        | crate::TurnLifecycle::Failed
        | crate::TurnLifecycle::Incomplete => snapshot == current,
    }
}

fn validate_snapshot_source_frontier(
    reader: &DomainReader<'_, SyndicDomain>,
    path: &crate::TranscriptPathTurnRecord,
    state: &crate::TurnStateRecord,
) -> Result<(), SyndicValidationError> {
    if path.source_event_count() == 0 {
        if path.lifecycle() != crate::TurnLifecycle::Pending {
            return invariant("eventless transcript path snapshot is not pending");
        }
        return Ok(());
    }
    if path.lifecycle() == crate::TurnLifecycle::Pending {
        return invariant("pending transcript path snapshot has source events");
    }
    let last_sequence =
        crate::SourceEventSequence::new(path.source_event_count()).map_err(|_| {
            SyndicValidationError::Invariant("transcript path event frontier is invalid")
        })?;
    let last = require::<SourceEventsFamily>(
        reader,
        &TurnEventKey {
            owner: path.turn_id(),
            ordinal: last_sequence,
        },
        "transcript path source frontier event is missing",
    )?;
    if path.lifecycle().is_proven_terminal()
        && !matches!(
            last.payload(),
            crate::SourceEventPayload::TurnEnded(status)
                if state.end_status() == Some(*status)
        )
    {
        return invariant("terminal transcript path snapshot source frontier disagrees");
    }
    Ok(())
}

fn validate_snapshot_item_frontier(
    reader: &DomainReader<'_, SyndicDomain>,
    path: &crate::TranscriptPathTurnRecord,
) -> Result<(), SyndicValidationError> {
    for count in [path.item_count(), path.finalized_item_count()] {
        if count == 0 {
            continue;
        }
        let ordinal = crate::TurnItemOrdinal::new(count).map_err(|_| {
            SyndicValidationError::Invariant("transcript path item frontier is invalid")
        })?;
        require::<TurnItemsFamily>(
            reader,
            &TurnItemKey {
                owner: path.turn_id(),
                ordinal,
            },
            "transcript path item frontier is missing",
        )?;
    }
    Ok(())
}
