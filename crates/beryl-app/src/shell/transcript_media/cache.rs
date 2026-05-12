use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use beryl_model::workspace::WorkspaceId;
use tracing::debug;

use super::types::{
    TranscriptMediaCacheKey, TranscriptMediaLoadCompletion, TranscriptMediaLoadCompletionResult,
    TranscriptMediaLoadOutcome, TranscriptMediaLookup, TranscriptMediaSource,
    TranscriptMediaSourceFingerprint, media_load_request,
};

pub(crate) const TRANSCRIPT_MEDIA_CACHE_MAX_COMPRESSED_IMAGE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const TRANSCRIPT_MEDIA_CACHE_MAX_DECODED_IMAGE_BYTES_ESTIMATE: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptMediaCacheStats {
    pub(crate) lookups: u64,
    pub(crate) ready_hits: u64,
    pub(crate) pending_hits: u64,
    pub(crate) misses: u64,
    pub(crate) invalidations: u64,
    pub(crate) scheduled_loads: u64,
    pub(crate) completed_loads: u64,
    pub(crate) stale_completions: u64,
    pub(crate) evictions: u64,
    pub(crate) load_micros: u64,
    pub(crate) entries: usize,
    pub(crate) pending_entries: usize,
    pub(crate) loaded_entries: usize,
    pub(crate) loaded_image_bytes: usize,
    pub(crate) decoded_image_bytes_estimate: usize,
    pub(crate) thumbnail_count: usize,
}

#[derive(Debug)]
pub(crate) struct TranscriptMediaCache {
    entries: HashMap<TranscriptMediaCacheKey, TranscriptMediaCacheEntry>,
    max_entries: usize,
    max_loaded_image_bytes: usize,
    max_decoded_image_bytes_estimate: usize,
    markdown_revalidate_after: Duration,
    access_tick: u64,
    scope_generation: u64,
    stats: TranscriptMediaCacheStats,
}

#[derive(Debug)]
struct TranscriptMediaCacheEntry {
    latest_fingerprint: TranscriptMediaSourceFingerprint,
    latest_source: TranscriptMediaSource,
    latest_execution_target: WorkspaceId,
    latest_timeout: Duration,
    last_used: u64,
    displayed: Arc<TranscriptMediaLoadOutcome>,
    displayed_fingerprint: Option<TranscriptMediaSourceFingerprint>,
    displayed_at: Option<Instant>,
    in_flight: Option<TranscriptMediaInFlightLoad>,
}

#[derive(Debug)]
struct TranscriptMediaInFlightLoad {
    fingerprint: TranscriptMediaSourceFingerprint,
}

#[derive(Clone, Copy, Debug, Default)]
struct TranscriptMediaLoadedStats {
    entries: usize,
    image_bytes: usize,
    decoded_image_bytes_estimate: usize,
}

impl TranscriptMediaCacheEntry {
    fn should_revalidate_markdown_source(
        &self,
        now: Instant,
        markdown_revalidate_after: Duration,
    ) -> bool {
        if !matches!(
            &self.latest_source,
            TranscriptMediaSource::MarkdownImage { .. }
        ) {
            return false;
        }
        let Some(displayed_at) = self.displayed_at else {
            return false;
        };
        now.saturating_duration_since(displayed_at) >= markdown_revalidate_after
    }
}

impl Default for TranscriptMediaCache {
    fn default() -> Self {
        Self::new(512)
    }
}

impl TranscriptMediaCache {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self::new_with_byte_budgets(
            max_entries,
            TRANSCRIPT_MEDIA_CACHE_MAX_COMPRESSED_IMAGE_BYTES,
            TRANSCRIPT_MEDIA_CACHE_MAX_DECODED_IMAGE_BYTES_ESTIMATE,
        )
    }

    pub(crate) fn new_with_byte_budgets(
        max_entries: usize,
        max_loaded_image_bytes: usize,
        max_decoded_image_bytes_estimate: usize,
    ) -> Self {
        Self::new_with_byte_budgets_and_markdown_revalidate_after(
            max_entries,
            max_loaded_image_bytes,
            max_decoded_image_bytes_estimate,
            Duration::from_secs(2),
        )
    }

    pub(crate) fn new_with_markdown_revalidate_after(
        max_entries: usize,
        markdown_revalidate_after: Duration,
    ) -> Self {
        Self::new_with_byte_budgets_and_markdown_revalidate_after(
            max_entries,
            TRANSCRIPT_MEDIA_CACHE_MAX_COMPRESSED_IMAGE_BYTES,
            TRANSCRIPT_MEDIA_CACHE_MAX_DECODED_IMAGE_BYTES_ESTIMATE,
            markdown_revalidate_after,
        )
    }

    pub(crate) fn new_with_byte_budgets_and_markdown_revalidate_after(
        max_entries: usize,
        max_loaded_image_bytes: usize,
        max_decoded_image_bytes_estimate: usize,
        markdown_revalidate_after: Duration,
    ) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: max_entries.max(1),
            max_loaded_image_bytes: max_loaded_image_bytes.max(1),
            max_decoded_image_bytes_estimate: max_decoded_image_bytes_estimate.max(1),
            markdown_revalidate_after,
            access_tick: 0,
            scope_generation: 0,
            stats: TranscriptMediaCacheStats::default(),
        }
    }

    pub(crate) fn clear(&mut self) -> Vec<Arc<gpui::Image>> {
        let evicted_images = self
            .entries
            .drain()
            .filter_map(|(_, entry)| entry.loaded_image_handle())
            .collect();
        self.scope_generation = self.scope_generation.saturating_add(1);
        evicted_images
    }

    pub(crate) fn lookup(
        &mut self,
        key: TranscriptMediaCacheKey,
        source: TranscriptMediaSource,
        execution_target: WorkspaceId,
        timeout: Duration,
    ) -> TranscriptMediaLookup {
        self.access_tick = self.access_tick.saturating_add(1);
        self.stats.lookups = self.stats.lookups.saturating_add(1);

        let fingerprint = TranscriptMediaSourceFingerprint::new(&source, &execution_target);
        let now = Instant::now();
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = self.access_tick;
            if entry.latest_fingerprint == fingerprint {
                if entry.in_flight.is_none()
                    && entry.should_revalidate_markdown_source(now, self.markdown_revalidate_after)
                {
                    let request = media_load_request(
                        key.clone(),
                        fingerprint,
                        self.scope_generation,
                        source,
                        execution_target,
                        timeout,
                    );
                    entry.in_flight = Some(TranscriptMediaInFlightLoad { fingerprint });
                    self.stats.scheduled_loads = self.stats.scheduled_loads.saturating_add(1);
                    self.stats.pending_hits = self.stats.pending_hits.saturating_add(1);
                    debug!(
                        source = media_source_kind(&entry.latest_source),
                        reason = "markdown_revalidate",
                        scheduled_loads = self.stats.scheduled_loads,
                        "scheduled transcript media load"
                    );
                    return TranscriptMediaLookup {
                        outcome: entry.displayed.clone(),
                        load_request: Some(request),
                        evicted_images: Vec::new(),
                    };
                }
                if entry.displayed_fingerprint == Some(fingerprint) && entry.in_flight.is_none() {
                    self.stats.ready_hits = self.stats.ready_hits.saturating_add(1);
                } else {
                    self.stats.pending_hits = self.stats.pending_hits.saturating_add(1);
                }
                return TranscriptMediaLookup {
                    outcome: entry.displayed.clone(),
                    load_request: None,
                    evicted_images: Vec::new(),
                };
            }

            self.stats.invalidations = self.stats.invalidations.saturating_add(1);
            entry.latest_fingerprint = fingerprint;
            entry.latest_source = source.clone();
            entry.latest_execution_target = execution_target.clone();
            entry.latest_timeout = timeout;
            entry.displayed = Arc::new(TranscriptMediaLoadOutcome::Pending { alt: source.alt() });
            entry.displayed_fingerprint = None;
            entry.displayed_at = None;

            let mut load_request = None;
            if entry.in_flight.is_none() {
                let request = media_load_request(
                    key.clone(),
                    fingerprint,
                    self.scope_generation,
                    source,
                    execution_target,
                    timeout,
                );
                entry.in_flight = Some(TranscriptMediaInFlightLoad { fingerprint });
                self.stats.scheduled_loads = self.stats.scheduled_loads.saturating_add(1);
                debug!(
                    source = media_source_kind(&entry.latest_source),
                    reason = "fingerprint_changed",
                    scheduled_loads = self.stats.scheduled_loads,
                    "scheduled transcript media load"
                );
                load_request = Some(request);
            } else {
                self.stats.pending_hits = self.stats.pending_hits.saturating_add(1);
            }

            let outcome = entry.displayed.clone();
            let evicted_images = self.prune_if_needed();
            return TranscriptMediaLookup {
                outcome,
                load_request,
                evicted_images,
            };
        }

        self.stats.misses = self.stats.misses.saturating_add(1);
        let pending = Arc::new(TranscriptMediaLoadOutcome::Pending { alt: source.alt() });
        self.entries.insert(
            key.clone(),
            TranscriptMediaCacheEntry {
                latest_fingerprint: fingerprint,
                latest_source: source.clone(),
                latest_execution_target: execution_target.clone(),
                latest_timeout: timeout,
                last_used: self.access_tick,
                displayed: pending.clone(),
                displayed_fingerprint: None,
                displayed_at: None,
                in_flight: Some(TranscriptMediaInFlightLoad { fingerprint }),
            },
        );
        self.stats.scheduled_loads = self.stats.scheduled_loads.saturating_add(1);
        debug!(
            source = media_source_kind(&source),
            reason = "cache_miss",
            scheduled_loads = self.stats.scheduled_loads,
            "scheduled transcript media load"
        );
        let evicted_images = self.prune_if_needed();

        TranscriptMediaLookup {
            outcome: pending,
            load_request: Some(media_load_request(
                key,
                fingerprint,
                self.scope_generation,
                source,
                execution_target,
                timeout,
            )),
            evicted_images,
        }
    }

    pub(crate) fn complete_load(
        &mut self,
        completion: TranscriptMediaLoadCompletion,
    ) -> TranscriptMediaLoadCompletionResult {
        let mut result = TranscriptMediaLoadCompletionResult::default();
        if completion.scope_generation != self.scope_generation {
            self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
            result.stale = true;
            debug!(
                reason = "scope_generation",
                load_elapsed_ms = elapsed_ms(completion.elapsed),
                stale_completions = self.stats.stale_completions,
                "ignored stale transcript media load completion"
            );
            return result;
        }

        let Some(entry) = self.entries.get_mut(&completion.key) else {
            self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
            result.stale = true;
            debug!(
                reason = "missing_entry",
                load_elapsed_ms = elapsed_ms(completion.elapsed),
                stale_completions = self.stats.stale_completions,
                "ignored stale transcript media load completion"
            );
            return result;
        };
        let Some(in_flight) = entry.in_flight.as_ref() else {
            self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
            result.stale = true;
            debug!(
                source = media_source_kind(&entry.latest_source),
                reason = "no_in_flight_load",
                load_elapsed_ms = elapsed_ms(completion.elapsed),
                stale_completions = self.stats.stale_completions,
                "ignored stale transcript media load completion"
            );
            return result;
        };
        if in_flight.fingerprint != completion.fingerprint {
            self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
            result.stale = true;
            debug!(
                source = media_source_kind(&entry.latest_source),
                reason = "fingerprint_mismatch",
                load_elapsed_ms = elapsed_ms(completion.elapsed),
                stale_completions = self.stats.stale_completions,
                "ignored stale transcript media load completion"
            );
            return result;
        }

        entry.in_flight = None;
        if entry.latest_fingerprint == completion.fingerprint {
            let outcome = media_load_outcome_label(&completion.outcome);
            let loaded_bytes = media_load_outcome_bytes(&completion.outcome);
            entry.displayed = Arc::new(completion.outcome);
            entry.displayed_fingerprint = Some(completion.fingerprint);
            entry.displayed_at = Some(Instant::now());
            entry.last_used = self.access_tick;
            result.display_changed = true;
            self.stats.completed_loads = self.stats.completed_loads.saturating_add(1);
            self.stats.load_micros = self
                .stats
                .load_micros
                .saturating_add(duration_micros(completion.elapsed));
            debug!(
                source = media_source_kind(&entry.latest_source),
                outcome,
                loaded_bytes,
                display_changed = result.display_changed,
                load_elapsed_ms = elapsed_ms(completion.elapsed),
                completed_loads = self.stats.completed_loads,
                "completed transcript media load"
            );
        } else {
            self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
            result.stale = true;
            let fingerprint = entry.latest_fingerprint;
            entry.in_flight = Some(TranscriptMediaInFlightLoad { fingerprint });
            result.follow_up_request = Some(media_load_request(
                completion.key,
                fingerprint,
                self.scope_generation,
                entry.latest_source.clone(),
                entry.latest_execution_target.clone(),
                entry.latest_timeout,
            ));
            self.stats.scheduled_loads = self.stats.scheduled_loads.saturating_add(1);
            debug!(
                source = media_source_kind(&entry.latest_source),
                reason = "latest_fingerprint_changed",
                load_elapsed_ms = elapsed_ms(completion.elapsed),
                stale_completions = self.stats.stale_completions,
                scheduled_loads = self.stats.scheduled_loads,
                "scheduled follow-up transcript media load after stale completion"
            );
        }

        result.evicted_images = self.prune_if_needed();
        result
    }

    pub(crate) fn stats(&self) -> TranscriptMediaCacheStats {
        let loaded = self.loaded_stats();
        TranscriptMediaCacheStats {
            entries: self.entries.len(),
            pending_entries: self
                .entries
                .values()
                .filter(|entry| entry.in_flight.is_some())
                .count(),
            loaded_entries: loaded.entries,
            loaded_image_bytes: loaded.image_bytes,
            decoded_image_bytes_estimate: loaded.decoded_image_bytes_estimate,
            thumbnail_count: 0,
            ..self.stats
        }
    }

    fn prune_if_needed(&mut self) -> Vec<Arc<gpui::Image>> {
        let mut evicted_images = Vec::new();
        while self.entries.len() > self.max_entries || self.loaded_budgets_exceeded() {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key)
                && let Some(image) = entry.loaded_image_handle()
            {
                evicted_images.push(image);
            }
            self.stats.evictions = self.stats.evictions.saturating_add(1);
        }
        evicted_images
    }

    fn loaded_budgets_exceeded(&self) -> bool {
        let loaded = self.loaded_stats();
        loaded.image_bytes > self.max_loaded_image_bytes
            || loaded.decoded_image_bytes_estimate > self.max_decoded_image_bytes_estimate
    }

    fn loaded_stats(&self) -> TranscriptMediaLoadedStats {
        self.entries
            .values()
            .fold(TranscriptMediaLoadedStats::default(), |mut stats, entry| {
                if let TranscriptMediaLoadOutcome::Loaded(image) = entry.displayed.as_ref() {
                    stats.entries = stats.entries.saturating_add(1);
                    stats.image_bytes = stats.image_bytes.saturating_add(image.bytes().len());
                    stats.decoded_image_bytes_estimate = stats
                        .decoded_image_bytes_estimate
                        .saturating_add(decoded_image_bytes_estimate(image));
                }
                stats
            })
    }
}

fn decoded_image_bytes_estimate(image: &super::types::TranscriptMediaLoadedImage) -> usize {
    let dimensions = image.natural_dimensions();
    (dimensions.width() as usize)
        .saturating_mul(dimensions.height() as usize)
        .saturating_mul(4)
}

impl TranscriptMediaCacheEntry {
    fn loaded_image_handle(self) -> Option<Arc<gpui::Image>> {
        match self.displayed.as_ref() {
            TranscriptMediaLoadOutcome::Loaded(image) => Some(image.image()),
            _ => None,
        }
    }
}

fn duration_micros(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn media_source_kind(source: &TranscriptMediaSource) -> &'static str {
    match source {
        TranscriptMediaSource::MarkdownImage { .. } => "markdown_image",
        TranscriptMediaSource::NativeImageGeneration { .. } => "native_generated_image",
    }
}

fn media_load_outcome_label(outcome: &TranscriptMediaLoadOutcome) -> &'static str {
    match outcome {
        TranscriptMediaLoadOutcome::Pending { .. } => "pending",
        TranscriptMediaLoadOutcome::Loaded(_) => "loaded",
        TranscriptMediaLoadOutcome::RenderNotSupported { .. } => "render_not_supported",
        TranscriptMediaLoadOutcome::TooLarge { .. } => "too_large",
        TranscriptMediaLoadOutcome::FileUnavailable { .. } => "file_unavailable",
        TranscriptMediaLoadOutcome::PathNotAllowed { .. } => "path_not_allowed",
    }
}

fn media_load_outcome_bytes(outcome: &TranscriptMediaLoadOutcome) -> Option<usize> {
    match outcome {
        TranscriptMediaLoadOutcome::Loaded(image) => Some(image.bytes().len()),
        _ => None,
    }
}
