use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

use super::{
    Block, BlockRenderPlan, Document, Inline, MarkdownImageRequest,
    block_render_plan_with_copy_source, parse,
};

const DEFAULT_MAX_ENTRIES: usize = 512;
const DEFAULT_MAX_SOURCE_BYTES: usize = 1_000_000;
const DEFAULT_MAX_ESTIMATED_RETAINED_BYTES: usize = 4_000_000;
const ESTIMATED_BLOCK_BYTES: usize = 128;
const ESTIMATED_INLINE_BYTES: usize = 96;
const ESTIMATED_MEDIA_REQUEST_BYTES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptMarkdownCacheKey(String);

impl TranscriptMarkdownCacheKey {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl ParsedTranscriptMarkdown {
    fn from_source(source: &str) -> Self {
        parse(source)
            .map(|document| Self::from_document(document, false, source))
            .unwrap_or_else(|_| Self::plain_fallback(source))
    }

    fn plain_fallback(source: &str) -> Self {
        Self::from_document(
            Document::new(vec![Block::paragraph(vec![Inline::text(source)])]),
            true,
            source,
        )
    }

    fn from_document(document: Document, parser_fallback: bool, markdown_source: &str) -> Self {
        let render_plan = block_render_plan_with_copy_source(&document, markdown_source);
        let media_requests = document.image_requests();
        Self {
            source: markdown_source.to_string(),
            document,
            render_plan,
            media_requests,
            parser_fallback,
        }
    }
}

impl TranscriptMarkdownSourceFingerprint {
    fn new(source: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        Self {
            len: source.len(),
            hash: hasher.finish(),
        }
    }
}

impl TranscriptMarkdownCacheStats {
    pub(crate) fn counter_delta_since(self, earlier: Self) -> Self {
        Self {
            lookups: self.lookups.saturating_sub(earlier.lookups),
            ready_hits: self.ready_hits.saturating_sub(earlier.ready_hits),
            pending_hits: self.pending_hits.saturating_sub(earlier.pending_hits),
            misses: self.misses.saturating_sub(earlier.misses),
            invalidations: self.invalidations.saturating_sub(earlier.invalidations),
            scheduled_parses: self
                .scheduled_parses
                .saturating_sub(earlier.scheduled_parses),
            completed_parses: self
                .completed_parses
                .saturating_sub(earlier.completed_parses),
            stale_completions: self
                .stale_completions
                .saturating_sub(earlier.stale_completions),
            evictions: self.evictions.saturating_sub(earlier.evictions),
            parse_micros: self.parse_micros.saturating_sub(earlier.parse_micros),
            entries: self.entries,
            pending_entries: self.pending_entries,
            source_bytes: self.source_bytes,
            estimated_retained_bytes: self.estimated_retained_bytes,
            in_flight_source_bytes: self.in_flight_source_bytes,
            displayed_source_bytes: self.displayed_source_bytes,
            parsed_source_bytes: self.parsed_source_bytes,
            markdown_estimated_structure_bytes: self.markdown_estimated_structure_bytes,
            markdown_blocks: self.markdown_blocks,
            markdown_inlines: self.markdown_inlines,
            markdown_media_requests: self.markdown_media_requests,
        }
    }
}

fn duration_micros(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedTranscriptMarkdown {
    source: String,
    document: Document,
    render_plan: BlockRenderPlan,
    media_requests: Vec<MarkdownImageRequest>,
    parser_fallback: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MarkdownStructureCounts {
    pub(crate) blocks: usize,
    pub(crate) inlines: usize,
    pub(crate) media_requests: usize,
}

impl ParsedTranscriptMarkdown {
    #[cfg(test)]
    pub(crate) fn from_test_document(document: Document, markdown_source: &str) -> Self {
        Self::from_document(document, false, markdown_source)
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn render_plan(&self) -> &BlockRenderPlan {
        &self.render_plan
    }

    pub(crate) fn used_parser_fallback(&self) -> bool {
        self.parser_fallback
    }

    pub(crate) fn media_requests(&self) -> &[MarkdownImageRequest] {
        &self.media_requests
    }

    pub(crate) fn structure_counts(&self) -> MarkdownStructureCounts {
        let mut counts = document_structure_counts(&self.document);
        counts.media_requests = self.media_requests.len();
        counts
    }
}

fn document_structure_counts(document: &Document) -> MarkdownStructureCounts {
    let mut counts = MarkdownStructureCounts::default();
    count_blocks(document.blocks(), &mut counts);
    counts
}

fn count_blocks(blocks: &[Block], counts: &mut MarkdownStructureCounts) {
    for block in blocks {
        counts.blocks = counts.blocks.saturating_add(1);
        match block {
            Block::Paragraph(inlines) => count_inlines(inlines, counts),
            Block::Heading(heading) => count_inlines(heading.children(), counts),
            Block::List(list) => {
                for item in list.items() {
                    count_blocks(item.blocks(), counts);
                }
            }
            Block::BlockQuote(blocks) => count_blocks(blocks, counts),
            Block::Code(_) | Block::Math(_) | Block::ThematicBreak | Block::Unsupported(_) => {}
        }
    }
}

fn count_inlines(inlines: &[Inline], counts: &mut MarkdownStructureCounts) {
    for inline in inlines {
        counts.inlines = counts.inlines.saturating_add(1);
        match inline {
            Inline::Emphasis(children) | Inline::Strong(children) => {
                count_inlines(children, counts);
            }
            Inline::Link(link) => count_inlines(link.children(), counts),
            Inline::Text(_)
            | Inline::Code(_)
            | Inline::Image(_)
            | Inline::Math(_)
            | Inline::SoftBreak
            | Inline::HardBreak
            | Inline::Unsupported(_) => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptMarkdownCacheStats {
    pub(crate) lookups: u64,
    pub(crate) ready_hits: u64,
    pub(crate) pending_hits: u64,
    pub(crate) misses: u64,
    pub(crate) invalidations: u64,
    pub(crate) scheduled_parses: u64,
    pub(crate) completed_parses: u64,
    pub(crate) stale_completions: u64,
    pub(crate) evictions: u64,
    pub(crate) parse_micros: u64,
    pub(crate) entries: usize,
    pub(crate) pending_entries: usize,
    pub(crate) source_bytes: usize,
    pub(crate) estimated_retained_bytes: usize,
    pub(crate) in_flight_source_bytes: usize,
    pub(crate) displayed_source_bytes: usize,
    pub(crate) parsed_source_bytes: usize,
    pub(crate) markdown_estimated_structure_bytes: usize,
    pub(crate) markdown_blocks: usize,
    pub(crate) markdown_inlines: usize,
    pub(crate) markdown_media_requests: usize,
}

#[derive(Debug)]
pub(crate) struct TranscriptMarkdownLookup {
    pub(crate) markdown: Arc<ParsedTranscriptMarkdown>,
    pub(crate) parse_request: Option<TranscriptMarkdownParseRequest>,
}

#[derive(Clone, Debug)]
pub(crate) struct TranscriptMarkdownParseRequest {
    key: TranscriptMarkdownCacheKey,
    fingerprint: TranscriptMarkdownSourceFingerprint,
    scope_generation: u64,
    source: String,
}

impl TranscriptMarkdownParseRequest {
    pub(crate) fn parse(self) -> TranscriptMarkdownParseCompletion {
        let started_at = Instant::now();
        let markdown = ParsedTranscriptMarkdown::from_source(self.source.as_str());

        TranscriptMarkdownParseCompletion {
            key: self.key,
            fingerprint: self.fingerprint,
            scope_generation: self.scope_generation,
            markdown,
            elapsed: started_at.elapsed(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TranscriptMarkdownParseCompletion {
    key: TranscriptMarkdownCacheKey,
    fingerprint: TranscriptMarkdownSourceFingerprint,
    scope_generation: u64,
    markdown: ParsedTranscriptMarkdown,
    elapsed: Duration,
}

#[derive(Debug, Default)]
pub(crate) struct TranscriptMarkdownParseCompletionResult {
    pub(crate) display_changed: bool,
    pub(crate) follow_up_request: Option<TranscriptMarkdownParseRequest>,
    pub(crate) stale: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TranscriptMarkdownSourceFingerprint {
    len: usize,
    hash: u64,
}

#[derive(Debug)]
pub(crate) struct TranscriptMarkdownCache {
    entries: HashMap<TranscriptMarkdownCacheKey, TranscriptMarkdownCacheEntry>,
    max_entries: usize,
    max_source_bytes: usize,
    max_estimated_retained_bytes: usize,
    source_bytes: usize,
    access_tick: u64,
    scope_generation: u64,
    stats: TranscriptMarkdownCacheStats,
}

#[derive(Debug)]
struct TranscriptMarkdownCacheEntry {
    latest_fingerprint: TranscriptMarkdownSourceFingerprint,
    latest_source: String,
    source_len: usize,
    last_used: u64,
    displayed: Arc<ParsedTranscriptMarkdown>,
    displayed_fingerprint: Option<TranscriptMarkdownSourceFingerprint>,
    displayed_source: Option<String>,
    in_flight: Option<TranscriptMarkdownInFlightParse>,
}

#[derive(Debug)]
struct TranscriptMarkdownInFlightParse {
    fingerprint: TranscriptMarkdownSourceFingerprint,
    source: String,
}

impl Default for TranscriptMarkdownCache {
    fn default() -> Self {
        Self::new_with_estimated_bytes(
            DEFAULT_MAX_ENTRIES,
            DEFAULT_MAX_SOURCE_BYTES,
            DEFAULT_MAX_ESTIMATED_RETAINED_BYTES,
        )
    }
}

impl TranscriptMarkdownCacheEntry {
    fn is_ready_for(&self, fingerprint: TranscriptMarkdownSourceFingerprint) -> bool {
        self.in_flight.is_none() && self.displayed_fingerprint == Some(fingerprint)
    }
}

fn parse_request_for(
    key: TranscriptMarkdownCacheKey,
    fingerprint: TranscriptMarkdownSourceFingerprint,
    scope_generation: u64,
    source: String,
) -> TranscriptMarkdownParseRequest {
    TranscriptMarkdownParseRequest {
        key,
        fingerprint,
        scope_generation,
        source,
    }
}

impl TranscriptMarkdownCache {
    pub(crate) fn new(max_entries: usize, max_source_bytes: usize) -> Self {
        Self::new_with_estimated_bytes(
            max_entries,
            max_source_bytes,
            max_source_bytes
                .saturating_mul(4)
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
            source_bytes: 0,
            access_tick: 0,
            scope_generation: 0,
            stats: TranscriptMarkdownCacheStats::default(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.source_bytes = 0;
        self.scope_generation = self.scope_generation.saturating_add(1);
    }

    pub(crate) fn lookup(
        &mut self,
        key: TranscriptMarkdownCacheKey,
        source: &str,
    ) -> TranscriptMarkdownLookup {
        self.access_tick = self.access_tick.saturating_add(1);
        self.stats.lookups = self.stats.lookups.saturating_add(1);

        let fingerprint = TranscriptMarkdownSourceFingerprint::new(source);
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = self.access_tick;

            if entry.latest_fingerprint == fingerprint {
                if entry.is_ready_for(fingerprint) {
                    self.stats.ready_hits = self.stats.ready_hits.saturating_add(1);
                } else {
                    self.stats.pending_hits = self.stats.pending_hits.saturating_add(1);
                }

                return TranscriptMarkdownLookup {
                    markdown: entry.displayed.clone(),
                    parse_request: None,
                };
            }

            self.stats.invalidations = self.stats.invalidations.saturating_add(1);
            self.source_bytes = self
                .source_bytes
                .saturating_sub(entry.source_len)
                .saturating_add(fingerprint.len);
            entry.latest_fingerprint = fingerprint;
            entry.latest_source = source.to_string();
            entry.source_len = fingerprint.len;

            let mut parse_request = None;
            if entry.in_flight.is_none() {
                let request = parse_request_for(
                    key.clone(),
                    fingerprint,
                    self.scope_generation,
                    source.to_string(),
                );
                entry.in_flight = Some(TranscriptMarkdownInFlightParse {
                    fingerprint,
                    source: source.to_string(),
                });
                self.stats.scheduled_parses = self.stats.scheduled_parses.saturating_add(1);
                parse_request = Some(request);
            } else {
                self.stats.pending_hits = self.stats.pending_hits.saturating_add(1);
            }

            let markdown = entry.displayed.clone();
            self.prune_if_needed();

            return TranscriptMarkdownLookup {
                markdown,
                parse_request,
            };
        }

        self.stats.misses = self.stats.misses.saturating_add(1);

        let fallback = Arc::new(ParsedTranscriptMarkdown::plain_fallback(source));
        let source_len = fingerprint.len;
        let source = source.to_string();
        self.source_bytes = self.source_bytes.saturating_add(source_len);
        self.entries.insert(
            key.clone(),
            TranscriptMarkdownCacheEntry {
                latest_fingerprint: fingerprint,
                latest_source: source.clone(),
                source_len,
                last_used: self.access_tick,
                displayed: fallback.clone(),
                displayed_fingerprint: None,
                displayed_source: None,
                in_flight: Some(TranscriptMarkdownInFlightParse {
                    fingerprint,
                    source: source.clone(),
                }),
            },
        );
        self.stats.scheduled_parses = self.stats.scheduled_parses.saturating_add(1);
        self.prune_if_needed();

        TranscriptMarkdownLookup {
            markdown: fallback,
            parse_request: Some(parse_request_for(
                key,
                fingerprint,
                self.scope_generation,
                source,
            )),
        }
    }

    pub(crate) fn complete_parse(
        &mut self,
        completion: TranscriptMarkdownParseCompletion,
    ) -> TranscriptMarkdownParseCompletionResult {
        let mut result = TranscriptMarkdownParseCompletionResult::default();

        if completion.scope_generation != self.scope_generation {
            self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
            result.stale = true;
            return result;
        }

        let Some(entry) = self.entries.get_mut(&completion.key) else {
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

        let in_flight = entry
            .in_flight
            .take()
            .expect("in-flight parse exists after fingerprint match");
        let completion_is_latest = entry.latest_fingerprint == completion.fingerprint;
        let completion_is_append_prefix = entry.latest_source.starts_with(&in_flight.source);
        let completion_extends_displayed =
            entry.displayed_source.as_ref().is_none_or(|displayed| {
                in_flight.source.starts_with(displayed) && in_flight.source.len() >= displayed.len()
            });
        let should_promote =
            completion_is_latest || completion_is_append_prefix && completion_extends_displayed;

        if should_promote {
            entry.displayed = Arc::new(completion.markdown);
            entry.displayed_fingerprint = Some(completion.fingerprint);
            entry.displayed_source = Some(in_flight.source);
            result.display_changed = true;
            self.stats.completed_parses = self.stats.completed_parses.saturating_add(1);
            self.stats.parse_micros = self
                .stats
                .parse_micros
                .saturating_add(duration_micros(completion.elapsed));
        } else {
            self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
            result.stale = true;
        }

        entry.last_used = self.access_tick;
        if !completion_is_latest {
            let source = entry.latest_source.clone();
            let fingerprint = entry.latest_fingerprint;
            entry.in_flight = Some(TranscriptMarkdownInFlightParse {
                fingerprint,
                source: source.clone(),
            });
            result.follow_up_request = Some(parse_request_for(
                completion.key,
                fingerprint,
                self.scope_generation,
                source,
            ));
            self.stats.scheduled_parses = self.stats.scheduled_parses.saturating_add(1);
        }

        self.prune_if_needed();
        result
    }

    pub(crate) fn stats(&self) -> TranscriptMarkdownCacheStats {
        let pending_entries = self
            .entries
            .values()
            .filter(|entry| entry.in_flight.is_some())
            .count();
        let retained = self.retained_estimate();

        TranscriptMarkdownCacheStats {
            entries: self.entries.len(),
            pending_entries,
            source_bytes: self.source_bytes,
            estimated_retained_bytes: retained.total_bytes,
            in_flight_source_bytes: retained.in_flight_source_bytes,
            displayed_source_bytes: retained.displayed_source_bytes,
            parsed_source_bytes: retained.parsed_source_bytes,
            markdown_estimated_structure_bytes: retained.structure_bytes,
            markdown_blocks: retained.structure.blocks,
            markdown_inlines: retained.structure.inlines,
            markdown_media_requests: retained.structure.media_requests,
            ..self.stats
        }
    }

    fn remove_entry(&mut self, key: &TranscriptMarkdownCacheKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.source_bytes = self.source_bytes.saturating_sub(entry.source_len);
        }
    }

    fn prune_if_needed(&mut self) {
        while self.entries.len() > self.max_entries
            || self.source_bytes > self.max_source_bytes
            || self.retained_estimate().total_bytes > self.max_estimated_retained_bytes
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

    fn retained_estimate(&self) -> MarkdownRetainedEstimate {
        self.entries.iter().fold(
            MarkdownRetainedEstimate::default(),
            |mut estimate, (key, entry)| {
                let entry_estimate = markdown_entry_retained_estimate(key, entry);
                estimate.total_bytes = estimate
                    .total_bytes
                    .saturating_add(entry_estimate.total_bytes);
                estimate.in_flight_source_bytes = estimate
                    .in_flight_source_bytes
                    .saturating_add(entry_estimate.in_flight_source_bytes);
                estimate.displayed_source_bytes = estimate
                    .displayed_source_bytes
                    .saturating_add(entry_estimate.displayed_source_bytes);
                estimate.parsed_source_bytes = estimate
                    .parsed_source_bytes
                    .saturating_add(entry_estimate.parsed_source_bytes);
                estimate.structure_bytes = estimate
                    .structure_bytes
                    .saturating_add(entry_estimate.structure_bytes);
                estimate.structure.blocks = estimate
                    .structure
                    .blocks
                    .saturating_add(entry_estimate.structure.blocks);
                estimate.structure.inlines = estimate
                    .structure
                    .inlines
                    .saturating_add(entry_estimate.structure.inlines);
                estimate.structure.media_requests = estimate
                    .structure
                    .media_requests
                    .saturating_add(entry_estimate.structure.media_requests);
                estimate
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct MarkdownRetainedEstimate {
    total_bytes: usize,
    in_flight_source_bytes: usize,
    displayed_source_bytes: usize,
    parsed_source_bytes: usize,
    structure_bytes: usize,
    structure: MarkdownStructureCounts,
}

fn markdown_entry_retained_estimate(
    key: &TranscriptMarkdownCacheKey,
    entry: &TranscriptMarkdownCacheEntry,
) -> MarkdownRetainedEstimate {
    let structure = entry.displayed.structure_counts();
    let structure_bytes = structure
        .blocks
        .saturating_mul(ESTIMATED_BLOCK_BYTES)
        .saturating_add(structure.inlines.saturating_mul(ESTIMATED_INLINE_BYTES))
        .saturating_add(
            structure
                .media_requests
                .saturating_mul(ESTIMATED_MEDIA_REQUEST_BYTES),
        );
    let in_flight_source_bytes = entry
        .in_flight
        .as_ref()
        .map_or(0, |in_flight| in_flight.source.len());
    let displayed_source_bytes = entry.displayed_source.as_ref().map_or(0, String::len);
    let parsed_source_bytes = entry.displayed.source().len();
    let total_bytes = key
        .as_str()
        .len()
        .saturating_add(entry.latest_source.len())
        .saturating_add(in_flight_source_bytes)
        .saturating_add(displayed_source_bytes)
        .saturating_add(parsed_source_bytes)
        .saturating_add(structure_bytes);

    MarkdownRetainedEstimate {
        total_bytes,
        in_flight_source_bytes,
        displayed_source_bytes,
        parsed_source_bytes,
        structure_bytes,
        structure,
    }
}
