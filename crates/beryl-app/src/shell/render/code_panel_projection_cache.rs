use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[path = "code_panel_projection_cache/accounting.rs"]
mod accounting;
#[path = "code_panel_projection_cache/request.rs"]
mod request;

use super::code_panel::{CodePanelDisplayProjection, CodePanelWrapMode};
use accounting::{
    code_panel_projection_completed_entry_estimate,
    code_panel_projection_completed_entry_estimate_for_projection,
    code_panel_projection_entry_estimate, duration_micros,
};
use request::ProjectionFingerprint;
pub(crate) use request::{CodePanelProjectionCompletion, CodePanelProjectionRequest};

const DEFAULT_MAX_ENTRIES: usize = 256;
const DEFAULT_MAX_SOURCE_BYTES: usize = 2_000_000;
const DEFAULT_MAX_ESTIMATED_RETAINED_BYTES: usize = 32_000_000;
const INLINE_PROJECTION_SOURCE_BYTES: usize = 16_384;

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
    pub(crate) projection: Option<Arc<CodePanelDisplayProjection>>,
    pub(crate) projection_request: Option<CodePanelProjectionRequest>,
}

#[derive(Debug, Default)]
pub(crate) struct CodePanelProjectionCompletionResult {
    pub(crate) display_changed: bool,
    pub(crate) follow_up_request: Option<CodePanelProjectionRequest>,
    pub(crate) stale: bool,
}

#[derive(Debug)]
pub(crate) struct CodePanelProjectionCache {
    entries: HashMap<String, CodePanelProjectionCacheEntry>,
    max_entries: usize,
    max_source_bytes: usize,
    max_estimated_retained_bytes: usize,
    represented_source_bytes: usize,
    access_tick: u64,
    scope_generation: u64,
    stats: CodePanelProjectionCacheStats,
}

#[derive(Debug)]
struct CodePanelProjectionCacheEntry {
    latest_fingerprint: ProjectionFingerprint,
    latest_source: String,
    latest_wrap_mode: CodePanelWrapMode,
    represented_source_len: usize,
    last_used: u64,
    displayed: Option<Arc<CodePanelDisplayProjection>>,
    displayed_fingerprint: Option<ProjectionFingerprint>,
    in_flight: Option<CodePanelProjectionInFlight>,
}

#[derive(Debug)]
struct CodePanelProjectionInFlight {
    fingerprint: ProjectionFingerprint,
    source: String,
}

impl Default for CodePanelProjectionCache {
    fn default() -> Self {
        Self::new_with_estimated_bytes(
            DEFAULT_MAX_ENTRIES,
            DEFAULT_MAX_SOURCE_BYTES,
            DEFAULT_MAX_ESTIMATED_RETAINED_BYTES,
        )
    }
}

impl CodePanelProjectionCache {
    #[allow(dead_code)]
    pub(crate) fn new(max_entries: usize, max_source_bytes: usize) -> Self {
        Self::new_with_estimated_bytes(
            max_entries,
            max_source_bytes,
            max_source_bytes
                .saturating_mul(16)
                .max(DEFAULT_MAX_ESTIMATED_RETAINED_BYTES.min(max_source_bytes.max(1))),
        )
    }

    pub(crate) fn new_with_estimated_bytes(
        max_entries: usize,
        max_source_bytes: usize,
        max_estimated_retained_bytes: usize,
    ) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: max_entries.max(1),
            max_source_bytes: max_source_bytes.max(1),
            max_estimated_retained_bytes: max_estimated_retained_bytes.max(1),
            represented_source_bytes: 0,
            access_tick: 0,
            scope_generation: 0,
            stats: CodePanelProjectionCacheStats::default(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.represented_source_bytes = 0;
        self.scope_generation = self.scope_generation.saturating_add(1);
    }

    pub(crate) fn lookup(
        &mut self,
        owner_id: &str,
        source: &str,
        wrap_mode: CodePanelWrapMode,
    ) -> CodePanelProjectionLookup {
        self.access_tick = self.access_tick.saturating_add(1);
        self.stats.lookups = self.stats.lookups.saturating_add(1);

        if source.len() > self.max_source_bytes {
            self.release_owner(owner_id);
            self.stats.uncached_oversize_lookups =
                self.stats.uncached_oversize_lookups.saturating_add(1);
            return CodePanelProjectionLookup {
                projection: None,
                projection_request: None,
            };
        }

        let fingerprint = ProjectionFingerprint::new(source, wrap_mode);
        let mut lookup = if self.entries.contains_key(owner_id) {
            self.lookup_existing(owner_id, fingerprint, source, wrap_mode)
        } else {
            self.lookup_missing(owner_id, fingerprint, source, wrap_mode)
        };
        self.prune_if_needed();
        if lookup.projection_request.is_some() && !self.entries.contains_key(owner_id) {
            lookup.projection_request = None;
            self.stats.scheduled_projections = self.stats.scheduled_projections.saturating_sub(1);
            self.stats.uncached_oversize_lookups =
                self.stats.uncached_oversize_lookups.saturating_add(1);
        }

        lookup
    }

    pub(crate) fn complete_projection(
        &mut self,
        completion: CodePanelProjectionCompletion,
    ) -> CodePanelProjectionCompletionResult {
        let mut result = CodePanelProjectionCompletionResult::default();

        if completion.scope_generation != self.scope_generation {
            self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
            result.stale = true;
            return result;
        }

        let owner_id = completion.owner_id.clone();
        let mut remove_completed_entry = false;
        {
            let Some(entry) = self.entries.get_mut(owner_id.as_str()) else {
                self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
                result.stale = true;
                return result;
            };
            let Some(in_flight) = entry.in_flight.as_ref() else {
                self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
                result.stale = true;
                return result;
            };
            if in_flight.fingerprint != completion.fingerprint {
                self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
                result.stale = true;
                return result;
            }

            entry.in_flight = None;
            let completion_is_latest = entry.latest_fingerprint == completion.fingerprint;
            if completion_is_latest {
                let projection = Arc::new(completion.projection);
                if code_panel_projection_completed_entry_estimate(
                    owner_id.as_str(),
                    entry,
                    &projection,
                ) > self.max_estimated_retained_bytes
                {
                    self.stats.uncached_oversize_lookups =
                        self.stats.uncached_oversize_lookups.saturating_add(1);
                    remove_completed_entry = true;
                } else {
                    entry.displayed = Some(projection);
                    entry.displayed_fingerprint = Some(completion.fingerprint);
                    entry.last_used = self.access_tick;
                    self.stats.completed_projections =
                        self.stats.completed_projections.saturating_add(1);
                    self.stats.projection_micros = self
                        .stats
                        .projection_micros
                        .saturating_add(duration_micros(completion.elapsed));
                    result.display_changed = true;
                }
            } else {
                self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
                result.stale = true;
            }

            if !completion_is_latest
                && entry.displayed_fingerprint != Some(entry.latest_fingerprint)
            {
                let source = entry.latest_source.clone();
                let fingerprint = entry.latest_fingerprint;
                let wrap_mode = entry.latest_wrap_mode;
                entry.in_flight = Some(CodePanelProjectionInFlight {
                    fingerprint,
                    source: source.clone(),
                });
                self.stats.scheduled_projections =
                    self.stats.scheduled_projections.saturating_add(1);
                result.follow_up_request = Some(projection_request_for(
                    owner_id.clone(),
                    fingerprint,
                    self.scope_generation,
                    source,
                    wrap_mode,
                ));
            }
        }

        if remove_completed_entry {
            self.remove_entry(owner_id.as_str());
        }
        self.prune_if_needed();
        result
    }

    pub(crate) fn retain_owners(&mut self, retained_owner_ids: &HashSet<String>) {
        let released = self
            .entries
            .keys()
            .filter(|owner_id| !retained_owner_ids.contains(*owner_id))
            .cloned()
            .collect::<Vec<_>>();
        self.remove_keys(released);
    }

    pub(crate) fn release_owners_matching(&mut self, mut should_release: impl FnMut(&str) -> bool) {
        let released = self
            .entries
            .keys()
            .filter(|owner_id| should_release(owner_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        self.remove_keys(released);
    }

    pub(crate) fn stats(&self) -> CodePanelProjectionCacheStats {
        let mut stats = self.stats;
        stats.entries = self.entries.len();
        stats.pending_entries = self
            .entries
            .values()
            .filter(|entry| entry.in_flight.is_some())
            .count();
        stats.represented_source_bytes = self.represented_source_bytes;
        stats.estimated_retained_bytes = self.estimated_retained_bytes();
        stats.display_lines = self
            .entries
            .values()
            .filter_map(|entry| entry.displayed.as_ref())
            .map(|projection| projection.display_line_count())
            .sum();
        stats
    }

    fn lookup_missing(
        &mut self,
        owner_id: &str,
        fingerprint: ProjectionFingerprint,
        source: &str,
        wrap_mode: CodePanelWrapMode,
    ) -> CodePanelProjectionLookup {
        self.stats.misses = self.stats.misses.saturating_add(1);
        if source.len() <= INLINE_PROJECTION_SOURCE_BYTES {
            let projection = Arc::new(CodePanelDisplayProjection::new(source, wrap_mode));
            if code_panel_projection_completed_entry_estimate_for_projection(
                owner_id,
                source,
                &projection,
            ) > self.max_estimated_retained_bytes
            {
                self.stats.uncached_oversize_lookups =
                    self.stats.uncached_oversize_lookups.saturating_add(1);
                return CodePanelProjectionLookup {
                    projection: None,
                    projection_request: None,
                };
            }
            self.entries.insert(
                owner_id.to_string(),
                CodePanelProjectionCacheEntry {
                    latest_fingerprint: fingerprint,
                    latest_source: source.to_string(),
                    latest_wrap_mode: wrap_mode,
                    represented_source_len: fingerprint.len,
                    last_used: self.access_tick,
                    displayed: Some(projection.clone()),
                    displayed_fingerprint: Some(fingerprint),
                    in_flight: None,
                },
            );
            self.represented_source_bytes = self
                .represented_source_bytes
                .saturating_add(fingerprint.len);
            self.stats.completed_projections = self.stats.completed_projections.saturating_add(1);
            return CodePanelProjectionLookup {
                projection: Some(projection),
                projection_request: None,
            };
        }

        let source = source.to_string();
        let request = projection_request_for(
            owner_id.to_string(),
            fingerprint,
            self.scope_generation,
            source.clone(),
            wrap_mode,
        );
        self.entries.insert(
            owner_id.to_string(),
            CodePanelProjectionCacheEntry {
                latest_fingerprint: fingerprint,
                latest_source: source.clone(),
                latest_wrap_mode: wrap_mode,
                represented_source_len: fingerprint.len,
                last_used: self.access_tick,
                displayed: None,
                displayed_fingerprint: None,
                in_flight: Some(CodePanelProjectionInFlight {
                    fingerprint,
                    source,
                }),
            },
        );
        self.represented_source_bytes = self
            .represented_source_bytes
            .saturating_add(fingerprint.len);
        self.stats.scheduled_projections = self.stats.scheduled_projections.saturating_add(1);

        CodePanelProjectionLookup {
            projection: None,
            projection_request: Some(request),
        }
    }

    fn lookup_existing(
        &mut self,
        owner_id: &str,
        fingerprint: ProjectionFingerprint,
        source: &str,
        wrap_mode: CodePanelWrapMode,
    ) -> CodePanelProjectionLookup {
        let entry = self
            .entries
            .get_mut(owner_id)
            .expect("existing code panel projection entry should be present");
        entry.last_used = self.access_tick;

        if entry.latest_fingerprint == fingerprint {
            if entry.in_flight.is_none() && entry.displayed_fingerprint == Some(fingerprint) {
                self.stats.hits = self.stats.hits.saturating_add(1);
            } else {
                self.stats.pending_hits = self.stats.pending_hits.saturating_add(1);
            }
            return CodePanelProjectionLookup {
                projection: projection_display_for(entry, fingerprint),
                projection_request: None,
            };
        }

        self.stats.invalidations = self.stats.invalidations.saturating_add(1);
        self.represented_source_bytes = self
            .represented_source_bytes
            .saturating_sub(entry.represented_source_len)
            .saturating_add(fingerprint.len);
        entry.latest_fingerprint = fingerprint;
        entry.latest_source = source.to_string();
        entry.latest_wrap_mode = wrap_mode;
        entry.represented_source_len = fingerprint.len;

        let mut projection_request = None;
        if entry.displayed_fingerprint != Some(fingerprint) {
            if source.len() <= INLINE_PROJECTION_SOURCE_BYTES {
                let projection = Arc::new(CodePanelDisplayProjection::new(source, wrap_mode));
                if code_panel_projection_completed_entry_estimate_for_projection(
                    owner_id,
                    source,
                    &projection,
                ) > self.max_estimated_retained_bytes
                {
                    entry.displayed = None;
                    entry.displayed_fingerprint = None;
                    entry.in_flight = None;
                    self.stats.uncached_oversize_lookups =
                        self.stats.uncached_oversize_lookups.saturating_add(1);
                    return CodePanelProjectionLookup {
                        projection: None,
                        projection_request: None,
                    };
                }
                entry.displayed = Some(projection.clone());
                entry.displayed_fingerprint = Some(fingerprint);
                entry.in_flight = None;
                self.stats.completed_projections =
                    self.stats.completed_projections.saturating_add(1);
                return CodePanelProjectionLookup {
                    projection: Some(projection),
                    projection_request: None,
                };
            }
            if entry.in_flight.is_none() {
                let source = source.to_string();
                entry.in_flight = Some(CodePanelProjectionInFlight {
                    fingerprint,
                    source: source.clone(),
                });
                self.stats.scheduled_projections =
                    self.stats.scheduled_projections.saturating_add(1);
                projection_request = Some(projection_request_for(
                    owner_id.to_string(),
                    fingerprint,
                    self.scope_generation,
                    source,
                    wrap_mode,
                ));
            } else {
                self.stats.pending_hits = self.stats.pending_hits.saturating_add(1);
            }
        }

        CodePanelProjectionLookup {
            projection: projection_display_for(entry, fingerprint),
            projection_request,
        }
    }

    fn remove_keys(&mut self, owner_ids: Vec<String>) {
        for owner_id in owner_ids {
            self.remove_entry(owner_id.as_str());
        }
    }

    fn remove_entry(&mut self, owner_id: &str) {
        if let Some(entry) = self.entries.remove(owner_id) {
            self.represented_source_bytes = self
                .represented_source_bytes
                .saturating_sub(entry.represented_source_len);
        }
    }

    fn prune_if_needed(&mut self) {
        while self.entries.len() > self.max_entries
            || self.represented_source_bytes > self.max_source_bytes
            || self.estimated_retained_bytes() > self.max_estimated_retained_bytes
        {
            let Some(owner_id) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(owner_id, _)| owner_id.clone())
            else {
                break;
            };
            self.remove_entry(owner_id.as_str());
            self.stats.evictions = self.stats.evictions.saturating_add(1);
        }
    }

    fn estimated_retained_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|(owner_id, entry)| code_panel_projection_entry_estimate(owner_id, entry))
            .sum()
    }

    fn release_owner(&mut self, owner_id: &str) {
        self.remove_entry(owner_id);
    }
}

fn projection_request_for(
    owner_id: String,
    fingerprint: ProjectionFingerprint,
    scope_generation: u64,
    source: String,
    wrap_mode: CodePanelWrapMode,
) -> CodePanelProjectionRequest {
    CodePanelProjectionRequest {
        owner_id,
        fingerprint,
        scope_generation,
        source,
        wrap_mode,
    }
}

fn projection_display_for(
    entry: &CodePanelProjectionCacheEntry,
    fingerprint: ProjectionFingerprint,
) -> Option<Arc<CodePanelDisplayProjection>> {
    if entry.displayed_fingerprint == Some(fingerprint) {
        entry.displayed.clone()
    } else {
        None
    }
}
