use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::Arc,
    time::Instant,
};

use beryl_model::workspace::WorkspaceId;
use gpui::{App, Pixels, Window};

mod coordinator_state;

use crate::shell::execution_detail::{ExecutionItem, TurnExecutionRecord};
use crate::shell::transcript_markdown::{ParsedTranscriptMarkdown, TranscriptMarkdownCacheKey};
use crate::shell::transcript_media_runs::{TranscriptMediaRunSegment, markdown_media_run_segments};
use crate::shell::transcript_presentation::{
    TranscriptPresentedRow, TranscriptRowNarrativeUnit, TranscriptRowPresentationModel,
};
use coordinator_state::{
    TranscriptMediaPreloadBudget, TranscriptMediaPreloadDrainBudget,
    TranscriptMediaPreloadDrainStats, TranscriptMediaRunSegmentCacheKey, duration_micros,
    preload_requests_can_coalesce, preload_row_distance, union_range,
};

use super::{
    item_blocks::live_item_complete,
    item_markdown_key,
    markdown_cache::TranscriptMarkdownRenderContext,
    media_blocks::{TranscriptMediaRenderItem, TranscriptMediaRenderLayout, preload_media_run},
    media_cache::TranscriptMediaRenderContext,
    stream_projection::{TranscriptStreamProjectionContext, TranscriptStreamProjectionKey},
    turn_blocks::user_prompt_block_path,
    turn_item_media_units::generated_image_media_item,
    turn_markdown_key,
    turn_media_units::segment_media_key,
};

const MEDIA_RUN_SEGMENT_CACHE_MAX_ENTRIES: usize = 512;

#[derive(Clone)]
pub(super) struct TranscriptMediaPreloadRequest {
    pub(super) selected_thread_id: Option<String>,
    pub(super) workspace: WorkspaceId,
    pub(super) visible_range: Range<usize>,
    pub(super) preload_range: Range<usize>,
    pub(super) viewport_height: Pixels,
    pub(super) rows: Vec<TranscriptPresentedRow>,
    pub(super) media_layout: TranscriptMediaRenderLayout,
}

#[derive(Default)]
pub(super) struct TranscriptMediaPreloadCoordinator {
    next_generation: u64,
    pending: Option<PendingTranscriptMediaPreload>,
    segment_cache: HashMap<TranscriptMediaRunSegmentCacheKey, Vec<TranscriptMediaRunSegment>>,
    budget: TranscriptMediaPreloadBudget,
    coalesced_requests: u64,
    superseded_requests: u64,
    last_stats: TranscriptMediaPreloadDrainStats,
}

#[derive(Clone)]
struct PendingTranscriptMediaPreload {
    generation: u64,
    request: TranscriptMediaPreloadRequest,
}

impl TranscriptMediaPreloadCoordinator {
    pub(super) fn clear(&mut self) {
        self.pending = None;
        self.segment_cache.clear();
        self.last_stats = TranscriptMediaPreloadDrainStats::default();
        self.next_generation = self.next_generation.saturating_add(1);
    }

    pub(super) fn pending_preload_range(&self) -> Option<Range<usize>> {
        self.pending
            .as_ref()
            .map(|pending| pending.request.preload_range.clone())
    }

    pub(super) fn request_preload(&mut self, request: TranscriptMediaPreloadRequest) {
        self.next_generation = self.next_generation.saturating_add(1);
        let generation = self.next_generation;
        let pending = PendingTranscriptMediaPreload {
            generation,
            request,
        };

        let Some(current) = self.pending.as_mut() else {
            self.pending = Some(pending);
            return;
        };

        if current.can_coalesce(&pending) {
            current.coalesce(pending);
            self.coalesced_requests = self.coalesced_requests.saturating_add(1);
        } else {
            self.pending = Some(pending);
            self.superseded_requests = self.superseded_requests.saturating_add(1);
        }
    }

    pub(super) fn drain_pending(
        &mut self,
        row_is_current: impl Fn(usize, &str) -> bool,
        media_context: TranscriptMediaRenderContext,
        markdown_context: TranscriptMarkdownRenderContext,
        stream_projection_context: TranscriptStreamProjectionContext,
        window: &mut Window,
        cx: &mut App,
    ) -> TranscriptMediaPreloadDrainStats {
        let Some(pending) = self.pending.take() else {
            return self.last_stats;
        };

        let started_at = Instant::now();
        let request = pending.request;
        let mut stats = TranscriptMediaPreloadDrainStats {
            generation: pending.generation,
            coalesced_requests: self.coalesced_requests,
            superseded_requests: self.superseded_requests,
            ..TranscriptMediaPreloadDrainStats::default()
        };
        let in_flight = media_context.pending_media_load_count();
        let mut budget = TranscriptMediaPreloadDrainBudget::new(
            self.budget,
            started_at,
            self.budget.max_in_flight_loads.saturating_sub(in_flight),
        );
        let mut rows = request.rows;
        rows.sort_by_key(|row| preload_row_distance(row.index, &request.visible_range));

        for row in rows {
            stats.rows_considered = stats.rows_considered.saturating_add(1);
            if !budget.can_start_row() {
                stats.budget_exhausted = true;
                break;
            }
            if !row_is_current(row.index, row.identity.as_str()) {
                stats.rows_stale = stats.rows_stale.saturating_add(1);
                continue;
            }
            preload_turn_media_runs(
                row.index,
                &request.workspace,
                row.turn,
                row.model,
                row.identity.as_str(),
                markdown_context.clone(),
                media_context
                    .clone()
                    .for_row(row.identity.as_str().to_string()),
                stream_projection_context.clone(),
                request.media_layout,
                self,
                &mut budget,
                window,
                cx,
            );
            budget.rows_processed = budget.rows_processed.saturating_add(1);
            stats.rows_processed = budget.rows_processed;
        }

        stats.media_items_considered = budget.media_items_considered;
        stats.media_items_preloaded = budget.media_items_preloaded;
        stats.scheduled_loads = budget.scheduled_loads;
        stats.source_backed_preloads = budget.source_backed_preloads;
        stats.markdown_source_bytes = budget.source_bytes;
        stats.requested_upload_bytes = budget.requested_upload_bytes;
        stats.segment_cache_hits = budget.segment_cache_hits;
        stats.segment_cache_misses = budget.segment_cache_misses;
        stats.budget_exhausted |= budget.budget_exhausted;
        stats.elapsed_micros = duration_micros(started_at.elapsed());
        self.last_stats = stats;
        stats
    }

    fn media_run_segments(
        &mut self,
        key: &TranscriptMarkdownCacheKey,
        markdown: &ParsedTranscriptMarkdown,
        budget: &mut TranscriptMediaPreloadDrainBudget,
    ) -> Vec<TranscriptMediaRunSegment> {
        let cache_key = TranscriptMediaRunSegmentCacheKey::new(key.as_str(), markdown.source());
        if let Some(segments) = self.segment_cache.get(&cache_key) {
            budget.segment_cache_hits = budget.segment_cache_hits.saturating_add(1);
            return segments.clone();
        }

        budget.segment_cache_misses = budget.segment_cache_misses.saturating_add(1);
        let segments = markdown_media_run_segments(markdown);
        if self.segment_cache.len() >= MEDIA_RUN_SEGMENT_CACHE_MAX_ENTRIES
            && let Some(key) = self.segment_cache.keys().next().cloned()
        {
            self.segment_cache.remove(&key);
        }
        self.segment_cache.insert(cache_key, segments.clone());
        segments
    }
}

impl PendingTranscriptMediaPreload {
    fn can_coalesce(&self, next: &Self) -> bool {
        preload_requests_can_coalesce(
            &self.request.selected_thread_id,
            &self.request.workspace,
            &self.request.preload_range,
            &next.request.selected_thread_id,
            &next.request.workspace,
            &next.request.preload_range,
        )
    }

    fn coalesce(&mut self, next: Self) {
        self.generation = next.generation;
        self.request.visible_range = next.request.visible_range;
        self.request.viewport_height = next.request.viewport_height;
        self.request.media_layout = next.request.media_layout;
        self.request.preload_range = union_range(
            self.request.preload_range.clone(),
            next.request.preload_range.clone(),
        );
        let mut rows = BTreeMap::new();
        for row in self.request.rows.drain(..).chain(next.request.rows) {
            rows.insert(row.index, row);
        }
        self.request.rows = rows.into_values().collect();
    }
}

fn preload_turn_media_runs(
    turn_index: usize,
    workspace: &WorkspaceId,
    turn: Arc<TurnExecutionRecord>,
    row_model: Arc<TranscriptRowPresentationModel>,
    row_identity: &str,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    media_layout: TranscriptMediaRenderLayout,
    coordinator: &mut TranscriptMediaPreloadCoordinator,
    budget: &mut TranscriptMediaPreloadDrainBudget,
    window: &mut Window,
    cx: &mut App,
) {
    let mut pending_media = Vec::new();

    for unit in row_model.narrative_units() {
        if budget.budget_exhausted {
            break;
        }
        match unit {
            TranscriptRowNarrativeUnit::UserInput {
                fragment_id,
                fragment_index,
            } => {
                let fragment = turn
                    .user_input_fragments()
                    .get(*fragment_index)
                    .filter(|fragment| fragment.id == *fragment_id)
                    .or_else(|| {
                        turn.user_input_fragment_by_id(*fragment_id)
                            .map(|(_, fragment)| fragment)
                    });
                let Some(fragment) = fragment else {
                    continue;
                };
                if fragment.text.is_empty() {
                    continue;
                }
                if !fragment.image_markers().is_empty() {
                    if !flush_preload_media_run(
                        workspace,
                        media_context.clone(),
                        &mut pending_media,
                        media_layout,
                        budget,
                        window,
                        cx,
                    ) {
                        return;
                    }
                    continue;
                }
                if !budget.admit_markdown_source(fragment.text.len()) {
                    break;
                }

                let block_path = user_prompt_block_path(*fragment_index);
                let markdown_key = turn_markdown_key(turn_index, turn.as_ref(), &block_path);
                let markdown =
                    markdown_context.markdown_for(markdown_key.clone(), fragment.text.as_str(), cx);
                if markdown.media_requests().is_empty() {
                    if !flush_preload_media_run(
                        workspace,
                        media_context.clone(),
                        &mut pending_media,
                        media_layout,
                        budget,
                        window,
                        cx,
                    ) {
                        return;
                    }
                    continue;
                }
                for (segment_index, segment) in coordinator
                    .media_run_segments(&markdown_key, markdown.as_ref(), budget)
                    .into_iter()
                    .enumerate()
                {
                    match segment {
                        TranscriptMediaRunSegment::Markdown(_) => {
                            if !flush_preload_media_run(
                                workspace,
                                media_context.clone(),
                                &mut pending_media,
                                media_layout,
                                budget,
                                window,
                                cx,
                            ) {
                                return;
                            }
                        }
                        TranscriptMediaRunSegment::Media(source) => {
                            let key = segment_media_key(&markdown_key, segment_index);
                            let identity = super::TranscriptMediaRenderIdentity::new(
                                row_identity,
                                key.clone(),
                                &source,
                            );
                            pending_media.push(TranscriptMediaRenderItem {
                                key,
                                source,
                                identity,
                            });
                        }
                    }
                    if budget.budget_exhausted {
                        break;
                    }
                }
            }
            TranscriptRowNarrativeUnit::Item {
                item_id,
                item_index,
            } => {
                let item = turn
                    .items
                    .get(*item_index)
                    .filter(|item| item.id() == item_id)
                    .or_else(|| turn.item_by_id(item_id));
                let Some(item) = item else {
                    continue;
                };
                match item {
                    ExecutionItem::GeneratedImage(image) => {
                        pending_media.push(generated_image_media_item(
                            turn_index,
                            turn.as_ref(),
                            image,
                            row_identity,
                        ));
                    }
                    ExecutionItem::AgentMessage(message) => {
                        if message.text.is_empty() {
                            continue;
                        }
                        let markdown_key = item_markdown_key(
                            turn_index,
                            turn.as_ref(),
                            message.id.as_str(),
                            "agent-message",
                        );
                        let source = stream_projection_context.visible_text(
                            TranscriptStreamProjectionKey::new(markdown_key.as_str()),
                            message.text.as_str(),
                            live_item_complete(turn.as_ref(), message.complete),
                            Instant::now(),
                        );
                        if source.is_empty() {
                            continue;
                        }
                        if !budget.admit_markdown_source(source.len()) {
                            break;
                        }
                        let markdown = markdown_context.markdown_for(
                            markdown_key.clone(),
                            source.as_ref(),
                            cx,
                        );
                        if markdown.media_requests().is_empty() {
                            if !flush_preload_media_run(
                                workspace,
                                media_context.clone(),
                                &mut pending_media,
                                media_layout,
                                budget,
                                window,
                                cx,
                            ) {
                                return;
                            }
                            continue;
                        }
                        let segments = coordinator.media_run_segments(
                            &markdown_key,
                            markdown.as_ref(),
                            budget,
                        );
                        if !segments
                            .iter()
                            .any(|segment| matches!(segment, TranscriptMediaRunSegment::Media(_)))
                        {
                            if !flush_preload_media_run(
                                workspace,
                                media_context.clone(),
                                &mut pending_media,
                                media_layout,
                                budget,
                                window,
                                cx,
                            ) {
                                return;
                            }
                            continue;
                        }
                        for (segment_index, segment) in segments.into_iter().enumerate() {
                            match segment {
                                TranscriptMediaRunSegment::Markdown(_) => {
                                    if !flush_preload_media_run(
                                        workspace,
                                        media_context.clone(),
                                        &mut pending_media,
                                        media_layout,
                                        budget,
                                        window,
                                        cx,
                                    ) {
                                        return;
                                    }
                                }
                                TranscriptMediaRunSegment::Media(source) => {
                                    let key = segment_media_key(&markdown_key, segment_index);
                                    let identity = super::TranscriptMediaRenderIdentity::new(
                                        row_identity,
                                        key.clone(),
                                        &source,
                                    );
                                    pending_media.push(TranscriptMediaRenderItem {
                                        key,
                                        source,
                                        identity,
                                    });
                                }
                            }
                            if budget.budget_exhausted {
                                break;
                            }
                        }
                    }
                    ExecutionItem::Reasoning(_) => {
                        if !flush_preload_media_run(
                            workspace,
                            media_context.clone(),
                            &mut pending_media,
                            media_layout,
                            budget,
                            window,
                            cx,
                        ) {
                            return;
                        }
                    }
                    ExecutionItem::CommandExecution(_)
                    | ExecutionItem::FileChange(_)
                    | ExecutionItem::Generic(_) => {}
                }
            }
        }
    }

    flush_preload_media_run(
        workspace,
        media_context,
        &mut pending_media,
        media_layout,
        budget,
        window,
        cx,
    );
}

fn flush_preload_media_run(
    workspace: &WorkspaceId,
    media_context: TranscriptMediaRenderContext,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    media_layout: TranscriptMediaRenderLayout,
    budget: &mut TranscriptMediaPreloadDrainBudget,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    if pending_media.is_empty() {
        return true;
    }

    let items = std::mem::take(pending_media);
    let stats = preload_media_run(
        items.as_slice(),
        media_context,
        workspace,
        media_layout,
        budget.remaining_load_requests(),
        budget.remaining_upload_bytes(),
        window,
        cx,
    );
    budget.note_media_run(
        stats.item_count,
        stats.scheduled_loads,
        stats.source_backed_preloads,
        stats.requested_upload_bytes,
    );
    !budget.budget_exhausted
}
