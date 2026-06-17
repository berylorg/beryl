use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use beryl_model::workspace::WorkspaceId;
use gpui::{App, DevicePixels, ImageRenderSource, Size, SourceBackedImageRequestStatus, Window};

use crate::shell::{
    execution_detail::ExecutionItem,
    transcript_markdown::TranscriptMarkdownCacheKey,
    transcript_media::{
        TranscriptMediaLoadOutcome, TranscriptMediaSize, TranscriptMediaSizingInput,
        TranscriptMediaSlotLayout, transcript_media_slot_layout,
        transcript_media_source_backed_request_size,
    },
    transcript_media_admission::{
        SourceBackedUploadAdmissionDecision, TranscriptMediaAdmissionRequest,
        TranscriptMediaAdmissionSummary, TranscriptMediaAdmissionTarget,
        estimated_required_media_item_count, note_source_backed_upload_admission,
    },
    transcript_presentability::{
        TranscriptMediaPathIdentity, TranscriptMediaReadinessKey,
        TranscriptMediaRequestedRenderSize,
    },
    transcript_presentation::{TranscriptPresentedRow, TranscriptRowMediaDescriptorKind},
};

use super::{
    item_blocks::live_item_complete,
    item_markdown_key,
    markdown_cache::TranscriptMarkdownRenderContext,
    media_blocks::TranscriptMediaRenderItem,
    media_blocks::TranscriptMediaRenderLayout,
    media_cache::TranscriptMediaRenderContext,
    stream_projection::{TranscriptStreamProjectionContext, TranscriptStreamProjectionKey},
    turn_blocks::user_prompt_block_path,
    turn_item_media_units::generated_image_media_item,
    turn_markdown_key,
    turn_media_units::{TranscriptMarkdownRenderUnit, markdown_render_units},
};

const DEFAULT_MAX_ROWS_PER_DRAIN: usize = 24;
const DEFAULT_MAX_MEDIA_ITEMS_PER_DRAIN: usize = 32;
const DEFAULT_MAX_SOURCE_BACKED_UPLOAD_BYTES_PER_DRAIN: usize = 32 * 1024 * 1024;
const DEFAULT_MAX_IN_FLIGHT_LOADS: usize = 8;
const DEFAULT_MAX_DRAIN_TIME: Duration = Duration::from_millis(3);

#[derive(Default)]
pub(super) struct TranscriptWindowMediaAdmissionDriver {
    active_target: Option<TranscriptMediaAdmissionTarget>,
    leased_source_backed: HashSet<TranscriptMediaReadinessKey>,
    budget: TranscriptWindowMediaAdmissionBudget,
}

impl TranscriptWindowMediaAdmissionDrainBudget {
    fn new(
        budget: TranscriptWindowMediaAdmissionBudget,
        started_at: Instant,
        remaining_load_requests: usize,
    ) -> Self {
        Self {
            started_at,
            max_rows: budget.max_rows_per_drain,
            max_items: budget.max_media_items_per_drain,
            max_upload_bytes: budget.max_source_backed_upload_bytes_per_drain,
            remaining_load_requests,
            processed_rows: 0,
            processed_items: 0,
            requested_upload_bytes: 0,
            max_elapsed: budget.max_drain_time,
        }
    }

    fn can_start_row(&self) -> bool {
        self.processed_rows < self.max_rows
            && self.processed_items < self.max_items
            && (self.processed_rows == 0 || self.started_at.elapsed() < self.max_elapsed)
    }

    fn can_start_item(&self) -> bool {
        self.processed_items < self.max_items || self.processed_items == 0
    }

    fn note_row_processed(&mut self) {
        self.processed_rows = self.processed_rows.saturating_add(1);
    }

    fn note_item_processed(&mut self) {
        self.processed_items = self.processed_items.saturating_add(1);
    }

    fn note_upload_bytes(&mut self, requested_upload_bytes: usize) {
        self.requested_upload_bytes = self
            .requested_upload_bytes
            .saturating_add(requested_upload_bytes);
    }

    fn remaining_upload_bytes(&self) -> usize {
        self.max_upload_bytes
            .saturating_sub(self.requested_upload_bytes)
    }
}

fn generated_image_admission_item(
    row: &TranscriptPresentedRow,
    image: &crate::shell::execution_detail::GeneratedImageDetail,
) -> AdmissionMediaItem {
    let render_item = generated_image_media_item(
        row.source_turn_index,
        row.turn.as_ref(),
        image,
        row.identity.as_str(),
    );
    let source_revision = row
        .model
        .media_descriptors()
        .iter()
        .find(|descriptor| descriptor.key == render_item.key.as_str())
        .map(|descriptor| descriptor.source_revision)
        .unwrap_or_default();
    AdmissionMediaItem {
        item: render_item,
        source_revision,
    }
}

fn markdown_source_revision(
    row: &TranscriptPresentedRow,
    markdown_key: &TranscriptMarkdownCacheKey,
) -> u64 {
    row.model
        .media_descriptors()
        .iter()
        .find(|descriptor| {
            descriptor.source_kind == TranscriptRowMediaDescriptorKind::MarkdownImageCandidate
                && descriptor.key == markdown_key.as_str()
        })
        .map(|descriptor| descriptor.source_revision)
        .unwrap_or_default()
}

fn push_markdown_admission_items(
    row: &TranscriptPresentedRow,
    markdown_key: TranscriptMarkdownCacheKey,
    block_path: &str,
    source: &str,
    markdown_context: TranscriptMarkdownRenderContext,
    items: &mut Vec<AdmissionMediaItem>,
    cx: &mut App,
) -> bool {
    let markdown = markdown_context.markdown_for(markdown_key.clone(), source, cx);
    if markdown.used_parser_fallback() {
        return false;
    }
    let source_revision = markdown_source_revision(row, &markdown_key);
    for unit in markdown_render_units(&markdown_key, block_path, markdown.as_ref()) {
        let TranscriptMarkdownRenderUnit::Media { key, source } = unit else {
            continue;
        };
        let identity =
            super::TranscriptMediaRenderIdentity::new(row.identity.as_str(), key.clone(), &source);
        items.push(AdmissionMediaItem {
            item: TranscriptMediaRenderItem {
                key,
                source,
                identity,
            },
            source_revision,
        });
    }
    true
}

fn row_admission_items(
    row: &TranscriptPresentedRow,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    cx: &mut App,
) -> AdmissionRowPlan {
    let mut items = Vec::new();
    let mut markdown_plan_pending = false;

    for unit in row.model.narrative_units() {
        match unit {
            crate::shell::transcript_presentation::TranscriptRowNarrativeUnit::UserInput {
                fragment_id,
                fragment_index,
            } => {
                let fragment = row
                    .turn
                    .user_input_fragments()
                    .get(*fragment_index)
                    .filter(|fragment| fragment.id == *fragment_id)
                    .or_else(|| {
                        row.turn
                            .user_input_fragment_by_id(*fragment_id)
                            .map(|(_, fragment)| fragment)
                    });
                let Some(fragment) = fragment else {
                    continue;
                };
                if fragment.text.is_empty() || !fragment.image_markers().is_empty() {
                    continue;
                }
                let block_path = user_prompt_block_path(*fragment_index);
                let markdown_key =
                    turn_markdown_key(row.source_turn_index, row.turn.as_ref(), &block_path);
                markdown_plan_pending |= !push_markdown_admission_items(
                    row,
                    markdown_key,
                    block_path.as_str(),
                    fragment.text.as_str(),
                    markdown_context.clone(),
                    &mut items,
                    cx,
                );
            }
            crate::shell::transcript_presentation::TranscriptRowNarrativeUnit::Item {
                item_id,
                item_index,
            } => {
                let item = row
                    .turn
                    .items
                    .get(*item_index)
                    .filter(|item| item.id() == item_id)
                    .or_else(|| row.turn.item_by_id(item_id));
                let Some(item) = item else {
                    continue;
                };
                match item {
                    ExecutionItem::GeneratedImage(image) => {
                        items.push(generated_image_admission_item(row, image));
                    }
                    ExecutionItem::AgentMessage(message) => {
                        if message.text.is_empty() {
                            continue;
                        }
                        let markdown_key = item_markdown_key(
                            row.source_turn_index,
                            row.turn.as_ref(),
                            message.id.as_str(),
                            "agent-message",
                        );
                        let source = stream_projection_context.visible_text(
                            TranscriptStreamProjectionKey::new(markdown_key.as_str()),
                            message.text.as_str(),
                            live_item_complete(row.turn.as_ref(), message.complete),
                            Instant::now(),
                        );
                        if source.is_empty() {
                            continue;
                        }
                        markdown_plan_pending |= !push_markdown_admission_items(
                            row,
                            markdown_key,
                            "agent-message",
                            source.as_ref(),
                            markdown_context.clone(),
                            &mut items,
                            cx,
                        );
                    }
                    ExecutionItem::Reasoning(_)
                    | ExecutionItem::CommandExecution(_)
                    | ExecutionItem::FileChange(_)
                    | ExecutionItem::Generic(_) => {}
                }
            }
            crate::shell::transcript_presentation::TranscriptRowNarrativeUnit::TerminalFallback => {
            }
        }
    }

    if markdown_plan_pending {
        AdmissionRowPlan::Pending {
            estimated_items: estimated_required_media_item_count(row).max(1),
        }
    } else {
        AdmissionRowPlan::Ready { items }
    }
}

fn requested_source_backed_size(
    run_length: usize,
    media_layout: TranscriptMediaRenderLayout,
    natural_dimensions: Option<crate::shell::transcript_media::TranscriptMediaNaturalDimensions>,
    outcome: &TranscriptMediaLoadOutcome,
) -> Option<(Size<DevicePixels>, usize)> {
    let TranscriptMediaSlotLayout::Media(TranscriptMediaSize { width, height }) =
        transcript_media_slot_layout(
            TranscriptMediaSizingInput {
                run_length,
                padded_content_width: media_layout.padded_content_width,
                conversation_m_advance: media_layout.conversation_m_advance,
                natural_dimensions,
                window_scale: media_layout.window_scale,
            },
            Some(outcome),
        )
    else {
        return None;
    };
    let requested_size = transcript_media_source_backed_request_size(
        TranscriptMediaSize { width, height },
        media_layout.window_scale,
    );
    if requested_size.width.0 <= 0 || requested_size.height.0 <= 0 {
        return None;
    }
    let requested_upload_bytes = source_backed_requested_upload_bytes(requested_size);
    Some((requested_size, requested_upload_bytes))
}

fn source_backed_requested_upload_bytes(size: Size<DevicePixels>) -> usize {
    let width = size.width.0.max(0) as usize;
    let height = size.height.0.max(0) as usize;
    width.saturating_mul(height).saturating_mul(4)
}

fn note_unprocessed_estimated_items(
    summary: &mut TranscriptMediaAdmissionSummary,
    estimated_items: &[usize],
) {
    let pending = estimated_items.iter().copied().sum::<usize>();
    summary.note_deferred_items(pending);
}

#[derive(Clone, Copy)]
struct TranscriptWindowMediaAdmissionBudget {
    max_rows_per_drain: usize,
    max_media_items_per_drain: usize,
    max_source_backed_upload_bytes_per_drain: usize,
    max_in_flight_loads: usize,
    max_drain_time: Duration,
}

struct TranscriptWindowMediaAdmissionDrainBudget {
    started_at: Instant,
    max_rows: usize,
    max_items: usize,
    max_upload_bytes: usize,
    remaining_load_requests: usize,
    processed_rows: usize,
    processed_items: usize,
    requested_upload_bytes: usize,
    max_elapsed: Duration,
}

struct AdmissionMediaItem {
    item: TranscriptMediaRenderItem,
    source_revision: u64,
}

enum AdmissionRowPlan {
    Pending { estimated_items: usize },
    Ready { items: Vec<AdmissionMediaItem> },
}

enum AdmissionMediaItemDrain {
    Scanned,
    RetryCurrent,
}

pub(super) struct TranscriptWindowMediaAdmissionDrain {
    pub(super) target: TranscriptMediaAdmissionTarget,
    pub(super) summary: TranscriptMediaAdmissionSummary,
}

impl Default for TranscriptWindowMediaAdmissionBudget {
    fn default() -> Self {
        Self {
            max_rows_per_drain: DEFAULT_MAX_ROWS_PER_DRAIN,
            max_media_items_per_drain: DEFAULT_MAX_MEDIA_ITEMS_PER_DRAIN,
            max_source_backed_upload_bytes_per_drain:
                DEFAULT_MAX_SOURCE_BACKED_UPLOAD_BYTES_PER_DRAIN,
            max_in_flight_loads: DEFAULT_MAX_IN_FLIGHT_LOADS,
            max_drain_time: DEFAULT_MAX_DRAIN_TIME,
        }
    }
}

impl TranscriptWindowMediaAdmissionDriver {
    pub(super) fn clear(&mut self) {
        self.active_target = None;
        self.leased_source_backed.clear();
    }

    pub(super) fn release_rows(&mut self, row_identities: &HashSet<String>) {
        self.leased_source_backed
            .retain(|key| !row_identities.contains(key.row_identity().as_str()));
    }

    pub(super) fn drain_pending(
        &mut self,
        request: TranscriptMediaAdmissionRequest,
        workspace: &WorkspaceId,
        media_context: TranscriptMediaRenderContext,
        markdown_context: TranscriptMarkdownRenderContext,
        stream_projection_context: TranscriptStreamProjectionContext,
        media_layout: TranscriptMediaRenderLayout,
        window: &mut Window,
        cx: &mut App,
    ) -> TranscriptWindowMediaAdmissionDrain {
        self.reset_leases_if_target_changed(request.target());
        let started_at = Instant::now();
        let total_rows = request.total_rows();
        let scan_start_row_index = request.scan_start_row_index();
        let scan_start_item_index = request.scan_start_item_index();
        let prefix_recheck_required = request.prefix_recheck_required();
        let rows = request.into_rows();
        let estimated_required_media_items = rows
            .iter()
            .map(estimated_required_media_item_count)
            .collect::<Vec<_>>();
        let in_flight = media_context.pending_media_load_count();
        let mut budget = TranscriptWindowMediaAdmissionDrainBudget::new(
            self.budget,
            started_at,
            self.budget.max_in_flight_loads.saturating_sub(in_flight),
        );
        let mut summary = TranscriptMediaAdmissionSummary {
            row_count: total_rows,
            scan_start_row_index,
            scan_start_item_index,
            prefix_recheck_required,
            ..TranscriptMediaAdmissionSummary::default()
        };

        for (row_index, row) in rows.iter().enumerate() {
            if !budget.can_start_row() {
                summary.rows_budget_exhausted = true;
                note_unprocessed_estimated_items(
                    &mut summary,
                    &estimated_required_media_items[row_index..],
                );
                break;
            }

            let row_media_items = match row_admission_items(
                row,
                markdown_context.clone(),
                stream_projection_context.clone(),
                cx,
            ) {
                AdmissionRowPlan::Pending { estimated_items } => {
                    summary.completed_media_items = summary
                        .completed_media_items
                        .saturating_add(estimated_items);
                    summary.pending_completed_media_items = summary
                        .pending_completed_media_items
                        .saturating_add(estimated_items);
                    budget.note_row_processed();
                    summary.scanned_rows = summary.scanned_rows.saturating_add(1);
                    continue;
                }
                AdmissionRowPlan::Ready { items } => items,
            };

            summary.completed_media_items =
                summary
                    .completed_media_items
                    .saturating_add(row_media_items.len().saturating_sub(if row_index == 0 {
                        scan_start_item_index.min(row_media_items.len())
                    } else {
                        0
                    }));
            let run_length = row_media_items.len().max(1);
            let mut row_fully_scanned = true;
            let mut row_scanned_media_items = 0usize;
            let item_start_index = if row_index == 0 {
                scan_start_item_index.min(run_length)
            } else {
                0
            };
            for (media_index, media_item) in row_media_items
                .into_iter()
                .enumerate()
                .skip(item_start_index)
            {
                if !budget.can_start_item() {
                    summary.media_budget_exhausted = true;
                    let deferred_items = run_length.saturating_sub(media_index);
                    summary.pending_completed_media_items = summary
                        .pending_completed_media_items
                        .saturating_add(deferred_items);
                    summary.deferred_completed_media_items = summary
                        .deferred_completed_media_items
                        .saturating_add(deferred_items);
                    summary.scanned_media_items = row_scanned_media_items;
                    row_fully_scanned = false;
                    break;
                }
                match self.admit_media_item(
                    row,
                    media_item,
                    run_length,
                    workspace,
                    media_context
                        .clone()
                        .for_row(row.identity.as_str().to_string()),
                    media_layout,
                    &mut budget,
                    &mut summary,
                    window,
                    cx,
                ) {
                    AdmissionMediaItemDrain::Scanned => {
                        row_scanned_media_items = row_scanned_media_items.saturating_add(1);
                    }
                    AdmissionMediaItemDrain::RetryCurrent => {
                        let deferred_items = run_length.saturating_sub(media_index);
                        summary.pending_completed_media_items = summary
                            .pending_completed_media_items
                            .saturating_add(deferred_items);
                        summary.deferred_completed_media_items = summary
                            .deferred_completed_media_items
                            .saturating_add(deferred_items);
                        summary.scanned_media_items = row_scanned_media_items;
                        row_fully_scanned = false;
                        break;
                    }
                }
            }
            if !row_fully_scanned {
                break;
            }
            budget.note_row_processed();
            summary.scanned_rows = summary.scanned_rows.saturating_add(1);
            if started_at.elapsed() >= self.budget.max_drain_time {
                summary.time_budget_exhausted = true;
                note_unprocessed_estimated_items(
                    &mut summary,
                    &estimated_required_media_items[row_index.saturating_add(1)..],
                );
                break;
            }
        }

        summary.requested_upload_bytes = budget.requested_upload_bytes;
        TranscriptWindowMediaAdmissionDrain {
            target: self
                .active_target
                .clone()
                .expect("admission target should be active during drain"),
            summary,
        }
    }

    fn reset_leases_if_target_changed(&mut self, target: &TranscriptMediaAdmissionTarget) {
        if self.active_target.as_ref() == Some(target) {
            return;
        }
        self.active_target = Some(target.clone());
        self.leased_source_backed.clear();
    }

    #[allow(clippy::too_many_arguments)]
    fn admit_media_item(
        &mut self,
        row: &TranscriptPresentedRow,
        media_item: AdmissionMediaItem,
        run_length: usize,
        workspace: &WorkspaceId,
        media_context: TranscriptMediaRenderContext,
        media_layout: TranscriptMediaRenderLayout,
        budget: &mut TranscriptWindowMediaAdmissionDrainBudget,
        summary: &mut TranscriptMediaAdmissionSummary,
        window: &mut Window,
        cx: &mut App,
    ) -> AdmissionMediaItemDrain {
        budget.note_item_processed();
        let lookup = if budget.remaining_load_requests > 0 {
            media_context.preload_media_for(
                media_item.item.key.clone(),
                media_item.item.source.clone(),
                workspace.clone(),
                cx,
            )
        } else {
            super::media_cache::TranscriptMediaLookupResult {
                outcome: media_context.media_for(
                    media_item.item.key.clone(),
                    media_item.item.source.clone(),
                    workspace.clone(),
                    cx,
                ),
                load_scheduled: false,
            }
        };
        if lookup.load_scheduled {
            budget.remaining_load_requests = budget.remaining_load_requests.saturating_sub(1);
            summary.scheduled_loads = summary.scheduled_loads.saturating_add(1);
        }

        match lookup.outcome.as_ref() {
            TranscriptMediaLoadOutcome::Pending { .. } => {
                summary.note_pending_item();
                AdmissionMediaItemDrain::Scanned
            }
            outcome if outcome.fallback_text().is_some() => {
                summary.note_terminal_fallback_item();
                AdmissionMediaItemDrain::Scanned
            }
            TranscriptMediaLoadOutcome::Loaded(image) => {
                let Some(path) = image.source_backed_file_path() else {
                    summary.note_ready_item();
                    return AdmissionMediaItemDrain::Scanned;
                };
                let Some((requested_size, requested_upload_bytes)) = requested_source_backed_size(
                    run_length,
                    media_layout,
                    Some(image.natural_dimensions()),
                    lookup.outcome.as_ref(),
                ) else {
                    summary.note_pending_item();
                    return AdmissionMediaItemDrain::Scanned;
                };
                match note_source_backed_upload_admission(
                    summary,
                    requested_upload_bytes,
                    budget.max_upload_bytes,
                    budget.remaining_upload_bytes(),
                ) {
                    SourceBackedUploadAdmissionDecision::ReadyToRequest => {}
                    SourceBackedUploadAdmissionDecision::RetryCurrent => {
                        return AdmissionMediaItemDrain::RetryCurrent;
                    }
                    SourceBackedUploadAdmissionDecision::TerminalFallback => {
                        return AdmissionMediaItemDrain::Scanned;
                    }
                }

                let source = ImageRenderSource::file(path.clone());
                let render_request =
                    source.render_request(0, media_layout.window_scale, requested_size);
                let requested_render_size = TranscriptMediaRequestedRenderSize::new(
                    requested_size.width.0,
                    requested_size.height.0,
                );
                let readiness_key = TranscriptMediaReadinessKey::new(
                    row.identity.clone(),
                    media_item.item.key.as_str(),
                    media_item.source_revision,
                    TranscriptMediaPathIdentity::from_media_source(&media_item.item.source),
                    requested_render_size,
                    media_layout.window_scale,
                    row.model.revision(),
                );

                match window.source_backed_image_request_status(render_request.clone()) {
                    SourceBackedImageRequestStatus::Live => {
                        summary.note_ready_item();
                        AdmissionMediaItemDrain::Scanned
                    }
                    SourceBackedImageRequestStatus::Failed => {
                        summary.note_terminal_fallback_item();
                        AdmissionMediaItemDrain::Scanned
                    }
                    SourceBackedImageRequestStatus::Missing
                    | SourceBackedImageRequestStatus::BudgetDeferred => {
                        if self.leased_source_backed.insert(readiness_key) {
                            window.preload_source_backed_image(source, render_request, cx);
                            budget.note_upload_bytes(requested_upload_bytes);
                            summary.source_backed_preloads =
                                summary.source_backed_preloads.saturating_add(1);
                        }
                        summary.note_pending_item();
                        AdmissionMediaItemDrain::Scanned
                    }
                    SourceBackedImageRequestStatus::Loading
                    | SourceBackedImageRequestStatus::ReadyForUpload => {
                        summary.note_pending_item();
                        AdmissionMediaItemDrain::Scanned
                    }
                }
            }
            TranscriptMediaLoadOutcome::RenderNotSupported { .. }
            | TranscriptMediaLoadOutcome::TooLarge { .. }
            | TranscriptMediaLoadOutcome::FileUnavailable { .. }
            | TranscriptMediaLoadOutcome::PathNotAllowed { .. } => {
                summary.note_terminal_fallback_item();
                AdmissionMediaItemDrain::Scanned
            }
        }
    }
}
