use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    ops::Range,
    time::{Duration, Instant},
};

pub(crate) const DEFAULT_MAX_ROWS_PER_DRAIN: usize = 24;
pub(crate) const DEFAULT_MAX_MEDIA_ITEMS_PER_DRAIN: usize = 32;
pub(crate) const DEFAULT_MAX_MARKDOWN_SOURCE_BYTES_PER_DRAIN: usize = 256 * 1024;
pub(crate) const DEFAULT_MAX_SOURCE_BACKED_UPLOAD_BYTES_PER_DRAIN: usize = 32 * 1024 * 1024;
pub(crate) const DEFAULT_MAX_IN_FLIGHT_LOADS: usize = 8;
pub(crate) const DEFAULT_MAX_DRAIN_TIME: Duration = Duration::from_millis(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptMediaPreloadBudget {
    pub(crate) max_rows_per_drain: usize,
    pub(crate) max_media_items_per_drain: usize,
    pub(crate) max_markdown_source_bytes_per_drain: usize,
    pub(crate) max_source_backed_upload_bytes_per_drain: usize,
    pub(crate) max_in_flight_loads: usize,
    pub(crate) max_drain_time: Duration,
}

impl Default for TranscriptMediaPreloadBudget {
    fn default() -> Self {
        Self {
            max_rows_per_drain: DEFAULT_MAX_ROWS_PER_DRAIN,
            max_media_items_per_drain: DEFAULT_MAX_MEDIA_ITEMS_PER_DRAIN,
            max_markdown_source_bytes_per_drain: DEFAULT_MAX_MARKDOWN_SOURCE_BYTES_PER_DRAIN,
            max_source_backed_upload_bytes_per_drain:
                DEFAULT_MAX_SOURCE_BACKED_UPLOAD_BYTES_PER_DRAIN,
            max_in_flight_loads: DEFAULT_MAX_IN_FLIGHT_LOADS,
            max_drain_time: DEFAULT_MAX_DRAIN_TIME,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptMediaPreloadDrainStats {
    pub(crate) generation: u64,
    pub(crate) rows_considered: usize,
    pub(crate) rows_processed: usize,
    pub(crate) rows_stale: usize,
    pub(crate) media_items_considered: usize,
    pub(crate) media_items_preloaded: usize,
    pub(crate) scheduled_loads: usize,
    pub(crate) source_backed_preloads: usize,
    pub(crate) markdown_source_bytes: usize,
    pub(crate) requested_upload_bytes: usize,
    pub(crate) segment_cache_hits: usize,
    pub(crate) segment_cache_misses: usize,
    pub(crate) coalesced_requests: u64,
    pub(crate) superseded_requests: u64,
    pub(crate) elapsed_micros: u64,
    pub(crate) budget_exhausted: bool,
}

pub(crate) struct TranscriptMediaPreloadDrainBudget {
    started_at: Instant,
    max_rows: usize,
    max_items: usize,
    max_source_bytes: usize,
    max_upload_bytes: usize,
    remaining_load_requests: usize,
    max_elapsed: Duration,
    pub(crate) rows_processed: usize,
    pub(crate) media_items_considered: usize,
    pub(crate) media_items_preloaded: usize,
    pub(crate) source_bytes: usize,
    pub(crate) requested_upload_bytes: usize,
    pub(crate) scheduled_loads: usize,
    pub(crate) source_backed_preloads: usize,
    pub(crate) segment_cache_hits: usize,
    pub(crate) segment_cache_misses: usize,
    pub(crate) budget_exhausted: bool,
}

impl TranscriptMediaPreloadDrainBudget {
    pub(crate) fn new(
        budget: TranscriptMediaPreloadBudget,
        started_at: Instant,
        remaining_load_requests: usize,
    ) -> Self {
        Self {
            started_at,
            max_rows: budget.max_rows_per_drain,
            max_items: budget.max_media_items_per_drain,
            max_source_bytes: budget.max_markdown_source_bytes_per_drain,
            max_upload_bytes: budget.max_source_backed_upload_bytes_per_drain,
            remaining_load_requests,
            max_elapsed: budget.max_drain_time,
            rows_processed: 0,
            media_items_considered: 0,
            media_items_preloaded: 0,
            source_bytes: 0,
            requested_upload_bytes: 0,
            scheduled_loads: 0,
            source_backed_preloads: 0,
            segment_cache_hits: 0,
            segment_cache_misses: 0,
            budget_exhausted: false,
        }
    }

    pub(crate) fn can_start_row(&mut self) -> bool {
        if self.rows_processed >= self.max_rows
            || self.media_items_considered >= self.max_items
            || self.source_bytes >= self.max_source_bytes
            || self.requested_upload_bytes >= self.max_upload_bytes
            || self.remaining_load_requests == 0
            || (self.rows_processed > 0 && self.started_at.elapsed() >= self.max_elapsed)
        {
            self.budget_exhausted = true;
            return false;
        }
        true
    }

    pub(crate) fn admit_markdown_source(&mut self, source_len: usize) -> bool {
        if source_len == 0 {
            return true;
        }
        if self.source_bytes.saturating_add(source_len) > self.max_source_bytes {
            self.budget_exhausted = true;
            return false;
        }
        self.source_bytes = self.source_bytes.saturating_add(source_len);
        true
    }

    pub(crate) fn remaining_upload_bytes(&self) -> usize {
        self.max_upload_bytes
            .saturating_sub(self.requested_upload_bytes)
    }

    pub(crate) fn remaining_load_requests(&self) -> usize {
        self.remaining_load_requests
    }

    pub(crate) fn note_media_run(
        &mut self,
        item_count: usize,
        scheduled_loads: usize,
        source_backed_preloads: usize,
        requested_upload_bytes: usize,
    ) {
        self.media_items_considered = self.media_items_considered.saturating_add(item_count);
        self.media_items_preloaded = self
            .media_items_preloaded
            .saturating_add(source_backed_preloads);
        self.scheduled_loads = self.scheduled_loads.saturating_add(scheduled_loads);
        self.source_backed_preloads = self
            .source_backed_preloads
            .saturating_add(source_backed_preloads);
        self.requested_upload_bytes = self
            .requested_upload_bytes
            .saturating_add(requested_upload_bytes);
        self.remaining_load_requests = self.remaining_load_requests.saturating_sub(scheduled_loads);
        if self.media_items_considered >= self.max_items
            || self.requested_upload_bytes >= self.max_upload_bytes
            || self.remaining_load_requests == 0
        {
            self.budget_exhausted = true;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptMediaRunSegmentCacheKey {
    markdown_key: String,
    source_revision: TranscriptMarkdownSourceRevision,
}

impl TranscriptMediaRunSegmentCacheKey {
    pub(crate) fn new(markdown_key: impl Into<String>, source: &str) -> Self {
        Self {
            markdown_key: markdown_key.into(),
            source_revision: TranscriptMarkdownSourceRevision::new(source),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptMarkdownSourceRevision {
    len: usize,
    hash: u64,
}

impl TranscriptMarkdownSourceRevision {
    pub(crate) fn new(source: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        Self {
            len: source.len(),
            hash: hasher.finish(),
        }
    }
}

pub(crate) fn preload_requests_can_coalesce<Workspace: PartialEq>(
    current_thread_id: &Option<String>,
    current_workspace: &Workspace,
    current_range: &Range<usize>,
    next_thread_id: &Option<String>,
    next_workspace: &Workspace,
    next_range: &Range<usize>,
) -> bool {
    current_thread_id == next_thread_id
        && current_workspace == next_workspace
        && ranges_overlap_or_touch(current_range, next_range)
}

pub(crate) fn ranges_overlap_or_touch(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start <= b.end && b.start <= a.end
}

pub(crate) fn union_range(a: Range<usize>, b: Range<usize>) -> Range<usize> {
    a.start.min(b.start)..a.end.max(b.end)
}

pub(crate) fn preload_row_distance(row_index: usize, visible: &Range<usize>) -> usize {
    if row_index < visible.start {
        visible.start.saturating_sub(row_index)
    } else if row_index >= visible.end {
        row_index.saturating_sub(visible.end.saturating_sub(1))
    } else {
        0
    }
}

pub(crate) fn duration_micros(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}
