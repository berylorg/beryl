use std::time::Duration;

use super::{CodePanelDisplayProjection, CodePanelProjectionCacheEntry};

pub(super) fn duration_micros(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}

pub(super) fn code_panel_projection_entry_estimate(
    owner_id: &str,
    entry: &CodePanelProjectionCacheEntry,
) -> usize {
    owner_id
        .len()
        .saturating_add(entry.latest_revision.estimated_retained_bytes())
        .saturating_add(
            entry
                .displayed_revision
                .as_ref()
                .map_or(0, |revision| revision.estimated_retained_bytes()),
        )
        .saturating_add(entry.in_flight.as_ref().map_or(0, |in_flight| {
            in_flight.source_revision.estimated_retained_bytes()
        }))
        .saturating_add(
            entry
                .displayed
                .as_ref()
                .map_or(0, |projection| projection.estimated_retained_bytes()),
        )
}

pub(super) fn code_panel_projection_completed_entry_estimate(
    owner_id: &str,
    entry: &CodePanelProjectionCacheEntry,
    projection: &CodePanelDisplayProjection,
) -> usize {
    owner_id
        .len()
        .saturating_add(entry.latest_revision.estimated_retained_bytes())
        .saturating_add(projection.estimated_retained_bytes())
}

pub(super) fn code_panel_projection_completed_entry_estimate_for_projection(
    owner_id: &str,
    source_revision: &super::request::CodePanelSourceRevision,
    projection: &CodePanelDisplayProjection,
) -> usize {
    owner_id
        .len()
        .saturating_add(source_revision.estimated_retained_bytes())
        .saturating_add(projection.estimated_retained_bytes())
}
