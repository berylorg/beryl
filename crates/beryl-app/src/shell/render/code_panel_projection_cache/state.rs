use std::sync::Arc;

use super::super::code_panel::{CodePanelDisplayProjection, CodePanelWrapMode};
use super::request::{CodePanelProjectionRequest, CodePanelSourceRevision, ProjectionFingerprint};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CodePanelProjectionCacheStats {
    pub(crate) lookups: u64,
    pub(crate) hits: u64,
    pub(crate) pending_hits: u64,
    pub(crate) misses: u64,
    pub(crate) invalidations: u64,
    pub(crate) scheduled_projections: u64,
    pub(crate) completed_projections: u64,
    pub(crate) stale_completions: u64,
    pub(crate) uncached_oversize_lookups: u64,
    pub(crate) evictions: u64,
    pub(crate) projection_micros: u64,
    pub(crate) entries: usize,
    pub(crate) pending_entries: usize,
    pub(crate) represented_source_bytes: usize,
    pub(crate) estimated_retained_bytes: usize,
    pub(crate) display_lines: usize,
}

#[derive(Debug)]
pub(crate) struct CodePanelProjectionLookup {
    pub(crate) ready: Option<CodePanelProjectionReady>,
    pub(crate) projection_request: Option<CodePanelProjectionRequest>,
}

#[derive(Clone, Debug)]
pub(crate) struct CodePanelProjectionReady {
    pub(crate) projection: Arc<CodePanelDisplayProjection>,
    pub(crate) source_revision: CodePanelSourceRevision,
}

#[derive(Debug, Default)]
pub(crate) struct CodePanelProjectionCompletionResult {
    pub(crate) display_changed: bool,
    pub(crate) follow_up_request: Option<CodePanelProjectionRequest>,
    pub(crate) stale: bool,
}

#[derive(Debug)]
pub(super) struct CodePanelProjectionCacheEntry {
    pub(super) latest_fingerprint: ProjectionFingerprint,
    pub(super) latest_revision: CodePanelSourceRevision,
    pub(super) latest_wrap_mode: CodePanelWrapMode,
    pub(super) represented_source_len: usize,
    pub(super) last_used: u64,
    pub(super) displayed: Option<Arc<CodePanelDisplayProjection>>,
    pub(super) displayed_fingerprint: Option<ProjectionFingerprint>,
    pub(super) displayed_revision: Option<CodePanelSourceRevision>,
    pub(super) in_flight: Option<CodePanelProjectionInFlight>,
}

#[derive(Debug)]
pub(super) struct CodePanelProjectionInFlight {
    pub(super) fingerprint: ProjectionFingerprint,
    pub(super) source_revision: CodePanelSourceRevision,
}

pub(super) fn projection_display_for(
    entry: &CodePanelProjectionCacheEntry,
    fingerprint: ProjectionFingerprint,
) -> Option<CodePanelProjectionReady> {
    let ready = || {
        Some(CodePanelProjectionReady {
            projection: entry.displayed.as_ref()?.clone(),
            source_revision: entry.displayed_revision.clone()?,
        })
    };
    if entry.displayed_fingerprint == Some(fingerprint) {
        ready()
    } else if entry.in_flight.is_some()
        && entry
            .displayed_fingerprint
            .is_some_and(|displayed| displayed.wrap_mode() == fingerprint.wrap_mode())
    {
        ready()
    } else {
        None
    }
}
