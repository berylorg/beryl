use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[path = "cache/accounting.rs"]
mod accounting;
#[path = "cache/request.rs"]
mod request;

use accounting::{
    duration_micros, syntax_highlight_completed_entry_estimate, syntax_highlight_entry_estimate,
};
use request::SourceFingerprint;
pub(crate) use request::{
    SyntaxHighlightCacheKey, SyntaxHighlightCompletion, SyntaxHighlightRequest,
};

use super::{
    model::{SyntaxHighlight, SyntaxLanguage},
    normalize_syntax_language,
};

const DEFAULT_MAX_ENTRIES: usize = 512;
const DEFAULT_MAX_SOURCE_BYTES: usize = 1_000_000;
const DEFAULT_MAX_ESTIMATED_RETAINED_BYTES: usize = 2_000_000;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SyntaxHighlightCacheStats {
    pub(crate) lookups: u64,
    pub(crate) hits: u64,
    pub(crate) pending_hits: u64,
    pub(crate) misses: u64,
    pub(crate) invalidations: u64,
    pub(crate) scheduled_highlights: u64,
    pub(crate) completed_highlights: u64,
    pub(crate) stale_completions: u64,
    pub(crate) uncached_plain_lookups: u64,
    pub(crate) uncached_oversize_lookups: u64,
    pub(crate) evictions: u64,
    pub(crate) highlight_micros: u64,
    pub(crate) entries: usize,
    pub(crate) pending_entries: usize,
    pub(crate) represented_source_bytes: usize,
    pub(crate) estimated_retained_bytes: usize,
    pub(crate) tokens: usize,
}

#[derive(Debug)]
pub(crate) struct SyntaxHighlightLookup {
    pub(crate) highlight: Arc<SyntaxHighlight>,
    pub(crate) highlight_request: Option<SyntaxHighlightRequest>,
}

#[derive(Debug, Default)]
pub(crate) struct SyntaxHighlightCompletionResult {
    pub(crate) display_changed: bool,
    pub(crate) follow_up_request: Option<SyntaxHighlightRequest>,
    pub(crate) stale: bool,
}

#[derive(Debug)]
pub(crate) struct SyntaxHighlightCache {
    entries: HashMap<SyntaxHighlightCacheKey, SyntaxHighlightCacheEntry>,
    max_entries: usize,
    max_source_bytes: usize,
    max_estimated_retained_bytes: usize,
    represented_source_bytes: usize,
    access_tick: u64,
    scope_generation: u64,
    stats: SyntaxHighlightCacheStats,
}

#[derive(Debug)]
struct SyntaxHighlightCacheEntry {
    latest_fingerprint: SourceFingerprint,
    latest_source: String,
    represented_source_len: usize,
    last_used: u64,
    displayed: Arc<SyntaxHighlight>,
    displayed_fingerprint: Option<SourceFingerprint>,
    in_flight: Option<SyntaxHighlightInFlight>,
}

#[derive(Debug)]
struct SyntaxHighlightInFlight {
    fingerprint: SourceFingerprint,
    source: String,
}

impl Default for SyntaxHighlightCache {
    fn default() -> Self {
        Self::new_with_estimated_bytes(
            DEFAULT_MAX_ENTRIES,
            DEFAULT_MAX_SOURCE_BYTES,
            DEFAULT_MAX_ESTIMATED_RETAINED_BYTES,
        )
    }
}

impl SyntaxHighlightCache {
    pub(crate) fn new(max_entries: usize, max_source_bytes: usize) -> Self {
        Self::new_with_estimated_bytes(
            max_entries,
            max_source_bytes,
            max_source_bytes
                .saturating_mul(2)
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
            stats: SyntaxHighlightCacheStats::default(),
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
        syntax_label: Option<&str>,
    ) -> SyntaxHighlightLookup {
        self.access_tick = self.access_tick.saturating_add(1);
        self.stats.lookups = self.stats.lookups.saturating_add(1);

        let Some(language) = normalize_syntax_language(syntax_label) else {
            self.release_owner(owner_id);
            self.stats.uncached_plain_lookups = self.stats.uncached_plain_lookups.saturating_add(1);
            return SyntaxHighlightLookup {
                highlight: Arc::new(SyntaxHighlight::plain()),
                highlight_request: None,
            };
        };

        let key = SyntaxHighlightCacheKey::new(owner_id, language);
        self.release_other_languages(owner_id, language);

        if source.len() > self.max_source_bytes {
            self.remove_entry(&key);
            self.stats.uncached_oversize_lookups =
                self.stats.uncached_oversize_lookups.saturating_add(1);
            return SyntaxHighlightLookup {
                highlight: Arc::new(SyntaxHighlight::plain()),
                highlight_request: None,
            };
        }

        let fingerprint = SourceFingerprint::new(source);
        let mut lookup = if self.entries.contains_key(&key) {
            self.lookup_existing(key.clone(), fingerprint, source)
        } else {
            self.lookup_missing(key.clone(), fingerprint, source)
        };
        self.prune_if_needed();
        if lookup.highlight_request.is_some() && !self.entries.contains_key(&key) {
            lookup.highlight = Arc::new(SyntaxHighlight::plain());
            lookup.highlight_request = None;
            self.stats.scheduled_highlights = self.stats.scheduled_highlights.saturating_sub(1);
            self.stats.uncached_oversize_lookups =
                self.stats.uncached_oversize_lookups.saturating_add(1);
        }

        lookup
    }

    pub(crate) fn complete_highlight(
        &mut self,
        completion: SyntaxHighlightCompletion,
    ) -> SyntaxHighlightCompletionResult {
        let mut result = SyntaxHighlightCompletionResult::default();

        if completion.scope_generation != self.scope_generation {
            self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
            result.stale = true;
            return result;
        }

        let key = completion.key.clone();
        let mut remove_completed_entry = false;
        {
            let Some(entry) = self.entries.get_mut(&key) else {
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
                let highlight = Arc::new(completion.highlight);
                if syntax_highlight_completed_entry_estimate(&key, entry, &highlight)
                    > self.max_estimated_retained_bytes
                {
                    self.stats.uncached_oversize_lookups =
                        self.stats.uncached_oversize_lookups.saturating_add(1);
                    remove_completed_entry = true;
                } else {
                    entry.displayed = highlight;
                    entry.displayed_fingerprint = Some(completion.fingerprint);
                    entry.last_used = self.access_tick;
                    self.stats.completed_highlights =
                        self.stats.completed_highlights.saturating_add(1);
                    self.stats.highlight_micros = self
                        .stats
                        .highlight_micros
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
                entry.in_flight = Some(SyntaxHighlightInFlight {
                    fingerprint,
                    source: source.clone(),
                });
                self.stats.scheduled_highlights = self.stats.scheduled_highlights.saturating_add(1);
                result.follow_up_request = Some(highlight_request_for(
                    key.clone(),
                    fingerprint,
                    self.scope_generation,
                    source,
                ));
            }
        }

        if remove_completed_entry {
            self.remove_entry(&key);
        }
        self.prune_if_needed();
        result
    }

    pub(crate) fn retain_owners(&mut self, retained_owner_ids: &HashSet<String>) {
        let released = self
            .entries
            .keys()
            .filter(|key| !retained_owner_ids.contains(key.owner_id()))
            .cloned()
            .collect::<Vec<_>>();
        self.remove_keys(released);
    }

    pub(crate) fn release_owners_matching(&mut self, mut should_release: impl FnMut(&str) -> bool) {
        let released = self
            .entries
            .keys()
            .filter(|key| should_release(key.owner_id()))
            .cloned()
            .collect::<Vec<_>>();
        self.remove_keys(released);
    }

    pub(crate) fn stats(&self) -> SyntaxHighlightCacheStats {
        let mut stats = self.stats;
        stats.entries = self.entries.len();
        stats.pending_entries = self
            .entries
            .values()
            .filter(|entry| entry.in_flight.is_some())
            .count();
        stats.represented_source_bytes = self.represented_source_bytes;
        stats.estimated_retained_bytes = self.estimated_retained_bytes();
        stats.tokens = self
            .entries
            .values()
            .map(|entry| entry.displayed.tokens().len())
            .sum();
        stats
    }

    fn lookup_missing(
        &mut self,
        key: SyntaxHighlightCacheKey,
        fingerprint: SourceFingerprint,
        source: &str,
    ) -> SyntaxHighlightLookup {
        self.stats.misses = self.stats.misses.saturating_add(1);

        let source = source.to_string();
        let request = highlight_request_for(
            key.clone(),
            fingerprint,
            self.scope_generation,
            source.clone(),
        );
        self.entries.insert(
            key,
            SyntaxHighlightCacheEntry {
                latest_fingerprint: fingerprint,
                latest_source: source.clone(),
                represented_source_len: fingerprint.len,
                last_used: self.access_tick,
                displayed: Arc::new(SyntaxHighlight::plain()),
                displayed_fingerprint: None,
                in_flight: Some(SyntaxHighlightInFlight {
                    fingerprint,
                    source,
                }),
            },
        );
        self.represented_source_bytes = self
            .represented_source_bytes
            .saturating_add(fingerprint.len);
        self.stats.scheduled_highlights = self.stats.scheduled_highlights.saturating_add(1);

        SyntaxHighlightLookup {
            highlight: Arc::new(SyntaxHighlight::plain()),
            highlight_request: Some(request),
        }
    }

    fn lookup_existing(
        &mut self,
        key: SyntaxHighlightCacheKey,
        fingerprint: SourceFingerprint,
        source: &str,
    ) -> SyntaxHighlightLookup {
        let entry = self
            .entries
            .get_mut(&key)
            .expect("existing syntax highlight entry should be present");
        entry.last_used = self.access_tick;

        if entry.latest_fingerprint == fingerprint {
            if entry.in_flight.is_none() && entry.displayed_fingerprint == Some(fingerprint) {
                self.stats.hits = self.stats.hits.saturating_add(1);
            } else {
                self.stats.pending_hits = self.stats.pending_hits.saturating_add(1);
            }
            return SyntaxHighlightLookup {
                highlight: highlight_display_for(entry, fingerprint),
                highlight_request: None,
            };
        }

        self.stats.invalidations = self.stats.invalidations.saturating_add(1);
        self.represented_source_bytes = self
            .represented_source_bytes
            .saturating_sub(entry.represented_source_len)
            .saturating_add(fingerprint.len);
        entry.latest_fingerprint = fingerprint;
        entry.latest_source = source.to_string();
        entry.represented_source_len = fingerprint.len;

        let mut highlight_request = None;
        if entry.displayed_fingerprint != Some(fingerprint) {
            if entry.in_flight.is_none() {
                let source = source.to_string();
                entry.in_flight = Some(SyntaxHighlightInFlight {
                    fingerprint,
                    source: source.clone(),
                });
                self.stats.scheduled_highlights = self.stats.scheduled_highlights.saturating_add(1);
                highlight_request = Some(highlight_request_for(
                    key,
                    fingerprint,
                    self.scope_generation,
                    source,
                ));
            } else {
                self.stats.pending_hits = self.stats.pending_hits.saturating_add(1);
            }
        }

        SyntaxHighlightLookup {
            highlight: highlight_display_for(entry, fingerprint),
            highlight_request,
        }
    }

    fn remove_keys(&mut self, keys: Vec<SyntaxHighlightCacheKey>) {
        for key in keys {
            self.remove_entry(&key);
        }
    }

    fn remove_entry(&mut self, key: &SyntaxHighlightCacheKey) {
        if let Some(entry) = self.entries.remove(key) {
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
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.remove_entry(&key);
            self.stats.evictions = self.stats.evictions.saturating_add(1);
        }
    }

    fn estimated_retained_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|(key, entry)| syntax_highlight_entry_estimate(key, entry))
            .sum()
    }

    fn release_owner(&mut self, owner_id: &str) {
        let released = self
            .entries
            .keys()
            .filter(|key| key.owner_id() == owner_id)
            .cloned()
            .collect::<Vec<_>>();
        self.remove_keys(released);
    }

    fn release_other_languages(&mut self, owner_id: &str, language: SyntaxLanguage) {
        let released = self
            .entries
            .keys()
            .filter(|key| key.owner_id() == owner_id && key.language != language)
            .cloned()
            .collect::<Vec<_>>();
        self.remove_keys(released);
    }
}

fn highlight_request_for(
    key: SyntaxHighlightCacheKey,
    fingerprint: SourceFingerprint,
    scope_generation: u64,
    source: String,
) -> SyntaxHighlightRequest {
    SyntaxHighlightRequest {
        key,
        fingerprint,
        scope_generation,
        source,
    }
}

fn highlight_display_for(
    entry: &SyntaxHighlightCacheEntry,
    fingerprint: SourceFingerprint,
) -> Arc<SyntaxHighlight> {
    if entry.displayed_fingerprint == Some(fingerprint) {
        entry.displayed.clone()
    } else {
        Arc::new(SyntaxHighlight::plain())
    }
}
