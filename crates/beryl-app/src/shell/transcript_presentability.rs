#![allow(dead_code)]

use std::sync::Arc;

use beryl_backend::{ThreadInfo, TurnInfo, TurnItemsView};

use super::{
    execution_detail::{
        ExecutionDetailState, ExecutionItem, TranscriptImagePathResolver, TurnExecutionRecord,
    },
    transcript_media::{TranscriptMediaLoadOutcome, TranscriptMediaSource},
    transcript_presentation::{
        TranscriptRowIdentity, TranscriptRowMediaDescriptorKind, TranscriptRowPresentationModel,
        TranscriptRowPresentationRevision,
    },
    transcript_projection::project_parent_narrative_turn,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptPresentabilityWindow {
    rows: Vec<TranscriptRowPresentabilityState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptRowPresentabilityState {
    row_identity: TranscriptRowIdentity,
    source_turn_index: usize,
    row_revision: TranscriptRowPresentationRevision,
    full_detail: TranscriptFullDetailReadiness,
    row_presentation: TranscriptRowPresentationReadiness,
    markdown_media_plan: TranscriptMarkdownMediaPlanReadiness,
    completed_media: TranscriptCompletedMediaReadiness,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptPresentabilitySummary {
    pub(crate) row_count: usize,
    pub(crate) presentable_rows: usize,
    pub(crate) missing_full_detail_rows: usize,
    pub(crate) markdown_plan_pending_rows: usize,
    pub(crate) completed_media_pending_rows: usize,
    pub(crate) terminal_fallback_media_items: usize,
    pub(crate) live_pending_placeholder_items: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptFullDetailReadiness {
    Available,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptRowPresentationReadiness {
    Ready,
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptMarkdownMediaPlanReadiness {
    NotRequired,
    Ready {
        markdown_source_count: usize,
        media_candidate_count: usize,
    },
    Pending {
        markdown_source_count: usize,
        media_candidate_count: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptCompletedMediaReadiness {
    NotRequired,
    Settled {
        items: Vec<TranscriptMediaPresentability>,
    },
    Pending {
        items: Vec<TranscriptMediaPresentability>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptRowPresentabilityContext {
    HistoricalOrCompleted,
    Live,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptMediaPresentability {
    Ready {
        key: TranscriptMediaReadinessKey,
    },
    TerminalFallback {
        key: TranscriptMediaReadinessKey,
        reason: TranscriptMediaTerminalFallback,
    },
    Pending {
        key: TranscriptMediaReadinessKey,
        reason: TranscriptMediaPendingReadiness,
    },
    LivePendingGeneratedImage {
        key: TranscriptMediaReadinessKey,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptMediaReadinessKey {
    row_identity: TranscriptRowIdentity,
    media_key: String,
    media_source_revision: u64,
    path_identity: Option<TranscriptMediaPathIdentity>,
    requested_render_size: TranscriptMediaRequestedRenderSize,
    window_scale_bits: u32,
    row_presentation_revision: TranscriptRowPresentationRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptMediaPathIdentity(String);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptMediaRequestedRenderSize {
    width_device_pixels: i32,
    height_device_pixels: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptMediaPendingReadiness {
    FilesystemRead,
    Decode,
    Upload,
    SourceBackedPreload,
    MarkdownImageReadiness,
    HistoricalGeneratedImageReadiness,
    Admission,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptMediaTerminalFallback {
    Unavailable,
    Unsupported,
    PathDisallowed,
    TooLarge,
    DecodeFailed,
    AdmissionFailed,
}

impl TranscriptPresentabilityWindow {
    pub(crate) fn from_selected_thread_activation(
        thread: &ThreadInfo,
        image_resolver: &TranscriptImagePathResolver,
    ) -> Self {
        let mut details = ExecutionDetailState::default();
        details.load_thread_history_with_image_resolver_and_partial_mode(
            thread,
            image_resolver,
            false,
        );
        Self::from_turn_records(
            details.turns(),
            0,
            TranscriptRowPresentabilityContext::HistoricalOrCompleted,
        )
    }

    pub(crate) fn from_history_page(
        thread_id: &str,
        turns: &[TurnInfo],
        image_resolver: &TranscriptImagePathResolver,
        source_start: usize,
    ) -> Self {
        let mut details = ExecutionDetailState::default();
        let _ = details.prepend_thread_history_page_with_image_resolver_and_partial_mode(
            thread_id,
            turns.to_vec(),
            image_resolver,
            false,
        );
        Self::from_turn_records(
            details.turns(),
            source_start,
            TranscriptRowPresentabilityContext::HistoricalOrCompleted,
        )
    }

    pub(crate) fn from_turn_records(
        turns: &[Arc<TurnExecutionRecord>],
        source_start: usize,
        context: TranscriptRowPresentabilityContext,
    ) -> Self {
        let rows = turns
            .iter()
            .enumerate()
            .filter_map(|(offset, turn)| {
                let source_turn_index = source_start.saturating_add(offset);
                TranscriptRowPresentabilityState::from_turn_record(
                    source_turn_index,
                    turn.as_ref(),
                    context,
                )
            })
            .collect();
        Self { rows }
    }

    pub(crate) fn rows(&self) -> &[TranscriptRowPresentabilityState] {
        &self.rows
    }

    pub(crate) fn is_presentable(&self) -> bool {
        self.rows
            .iter()
            .all(TranscriptRowPresentabilityState::is_presentable)
    }

    pub(crate) fn structural_readiness_settled(&self) -> bool {
        self.rows
            .iter()
            .all(TranscriptRowPresentabilityState::structural_readiness_settled)
    }

    pub(crate) fn summary(&self) -> TranscriptPresentabilitySummary {
        self.rows.iter().fold(
            TranscriptPresentabilitySummary::default(),
            |mut summary, row| {
                summary.row_count = summary.row_count.saturating_add(1);
                if row.is_presentable() {
                    summary.presentable_rows = summary.presentable_rows.saturating_add(1);
                }
                if row.full_detail == TranscriptFullDetailReadiness::Missing {
                    summary.missing_full_detail_rows =
                        summary.missing_full_detail_rows.saturating_add(1);
                }
                if !row.markdown_media_plan.is_presentable() {
                    summary.markdown_plan_pending_rows =
                        summary.markdown_plan_pending_rows.saturating_add(1);
                }
                if !row.completed_media.is_presentable() {
                    summary.completed_media_pending_rows =
                        summary.completed_media_pending_rows.saturating_add(1);
                }
                summary.terminal_fallback_media_items = summary
                    .terminal_fallback_media_items
                    .saturating_add(row.completed_media.terminal_fallback_count());
                summary.live_pending_placeholder_items = summary
                    .live_pending_placeholder_items
                    .saturating_add(row.completed_media.live_pending_placeholder_count());
                summary
            },
        )
    }
}

impl TranscriptRowPresentabilityState {
    fn from_turn_record(
        source_turn_index: usize,
        turn: &TurnExecutionRecord,
        context: TranscriptRowPresentabilityContext,
    ) -> Option<Self> {
        let projected = project_parent_narrative_turn(turn)?;
        let row_identity = stable_row_identity(&projected).unwrap_or_else(|| {
            TranscriptRowIdentity::new(format!("ephemeral-turn:{source_turn_index}"))
        });
        let model = TranscriptRowPresentationModel::derive(source_turn_index, &projected);
        Some(Self::from_projected_model(
            row_identity,
            source_turn_index,
            &projected,
            &model,
            context,
        ))
    }

    fn from_projected_model(
        row_identity: TranscriptRowIdentity,
        source_turn_index: usize,
        turn: &TurnExecutionRecord,
        model: &TranscriptRowPresentationModel,
        context: TranscriptRowPresentabilityContext,
    ) -> Self {
        let markdown_media_candidates = model
            .media_descriptors()
            .iter()
            .filter(|descriptor| {
                descriptor.source_kind == TranscriptRowMediaDescriptorKind::MarkdownImageCandidate
            })
            .count();
        let markdown_media_plan = if markdown_media_candidates == 0 {
            TranscriptMarkdownMediaPlanReadiness::NotRequired
        } else {
            TranscriptMarkdownMediaPlanReadiness::Pending {
                markdown_source_count: model.markdown_sources().len(),
                media_candidate_count: markdown_media_candidates,
            }
        };
        let completed_media = completed_media_readiness(&row_identity, turn, model, context);

        Self {
            row_identity,
            source_turn_index,
            row_revision: model.revision(),
            full_detail: TranscriptFullDetailReadiness::Available,
            row_presentation: TranscriptRowPresentationReadiness::Ready,
            markdown_media_plan,
            completed_media,
        }
    }

    pub(crate) fn is_presentable(&self) -> bool {
        self.full_detail == TranscriptFullDetailReadiness::Available
            && self.row_presentation == TranscriptRowPresentationReadiness::Ready
            && self.markdown_media_plan.is_presentable()
            && self.completed_media.is_presentable()
    }

    fn structural_readiness_settled(&self) -> bool {
        self.full_detail == TranscriptFullDetailReadiness::Available
            && self.row_presentation == TranscriptRowPresentationReadiness::Ready
    }

    pub(crate) fn row_identity(&self) -> &TranscriptRowIdentity {
        &self.row_identity
    }

    pub(crate) fn completed_media(&self) -> &TranscriptCompletedMediaReadiness {
        &self.completed_media
    }

    pub(crate) fn markdown_media_plan(&self) -> &TranscriptMarkdownMediaPlanReadiness {
        &self.markdown_media_plan
    }
}

impl TranscriptMarkdownMediaPlanReadiness {
    pub(crate) fn is_presentable(&self) -> bool {
        matches!(self, Self::NotRequired | Self::Ready { .. })
    }
}

impl TranscriptCompletedMediaReadiness {
    pub(crate) fn is_presentable(&self) -> bool {
        match self {
            Self::NotRequired => true,
            Self::Settled { items } => items
                .iter()
                .all(TranscriptMediaPresentability::is_presentable),
            Self::Pending { .. } => false,
        }
    }

    fn terminal_fallback_count(&self) -> usize {
        self.items()
            .iter()
            .filter(|item| matches!(item, TranscriptMediaPresentability::TerminalFallback { .. }))
            .count()
    }

    fn live_pending_placeholder_count(&self) -> usize {
        self.items()
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    TranscriptMediaPresentability::LivePendingGeneratedImage { .. }
                )
            })
            .count()
    }

    pub(crate) fn items(&self) -> &[TranscriptMediaPresentability] {
        match self {
            Self::NotRequired => &[],
            Self::Settled { items } | Self::Pending { items } => items,
        }
    }
}

impl TranscriptMediaPresentability {
    pub(crate) fn from_load_outcome(
        key: TranscriptMediaReadinessKey,
        outcome: &TranscriptMediaLoadOutcome,
    ) -> Self {
        match outcome {
            TranscriptMediaLoadOutcome::Pending { .. } => Self::Pending {
                key,
                reason: TranscriptMediaPendingReadiness::FilesystemRead,
            },
            TranscriptMediaLoadOutcome::Loaded(_) => Self::Ready { key },
            TranscriptMediaLoadOutcome::RenderNotSupported { .. } => Self::TerminalFallback {
                key,
                reason: TranscriptMediaTerminalFallback::Unsupported,
            },
            TranscriptMediaLoadOutcome::TooLarge { .. } => Self::TerminalFallback {
                key,
                reason: TranscriptMediaTerminalFallback::TooLarge,
            },
            TranscriptMediaLoadOutcome::FileUnavailable { .. } => Self::TerminalFallback {
                key,
                reason: TranscriptMediaTerminalFallback::Unavailable,
            },
            TranscriptMediaLoadOutcome::PathNotAllowed { .. } => Self::TerminalFallback {
                key,
                reason: TranscriptMediaTerminalFallback::PathDisallowed,
            },
        }
    }

    pub(crate) fn is_presentable(&self) -> bool {
        matches!(
            self,
            Self::Ready { .. }
                | Self::TerminalFallback { .. }
                | Self::LivePendingGeneratedImage { .. }
        )
    }

    pub(crate) fn key(&self) -> &TranscriptMediaReadinessKey {
        match self {
            Self::Ready { key }
            | Self::TerminalFallback { key, .. }
            | Self::Pending { key, .. }
            | Self::LivePendingGeneratedImage { key } => key,
        }
    }
}

impl TranscriptMediaReadinessKey {
    pub(crate) fn new(
        row_identity: TranscriptRowIdentity,
        media_key: impl Into<String>,
        media_source_revision: u64,
        path_identity: Option<TranscriptMediaPathIdentity>,
        requested_render_size: TranscriptMediaRequestedRenderSize,
        window_scale: f32,
        row_presentation_revision: TranscriptRowPresentationRevision,
    ) -> Self {
        Self {
            row_identity,
            media_key: media_key.into(),
            media_source_revision,
            path_identity,
            requested_render_size,
            window_scale_bits: window_scale.to_bits(),
            row_presentation_revision,
        }
    }

    pub(crate) fn row_identity(&self) -> &TranscriptRowIdentity {
        &self.row_identity
    }

    pub(crate) fn media_source_revision(&self) -> u64 {
        self.media_source_revision
    }

    pub(crate) fn requested_render_size(&self) -> TranscriptMediaRequestedRenderSize {
        self.requested_render_size
    }
}

impl TranscriptMediaPathIdentity {
    pub(crate) fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        let value = value.trim();
        (!value.is_empty()).then(|| Self(value.to_string()))
    }

    pub(crate) fn from_media_source(source: &TranscriptMediaSource) -> Option<Self> {
        match source {
            TranscriptMediaSource::MarkdownImage { destination, .. } => {
                Self::new(destination.as_str())
            }
            TranscriptMediaSource::NativeImageGeneration { saved_path, .. } => {
                saved_path.as_deref().and_then(|path| Self::new(path))
            }
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TranscriptMediaRequestedRenderSize {
    pub(crate) fn new(width_device_pixels: i32, height_device_pixels: i32) -> Self {
        Self {
            width_device_pixels: width_device_pixels.max(0),
            height_device_pixels: height_device_pixels.max(0),
        }
    }

    pub(crate) fn width_device_pixels(&self) -> i32 {
        self.width_device_pixels
    }

    pub(crate) fn height_device_pixels(&self) -> i32 {
        self.height_device_pixels
    }
}

fn completed_media_readiness(
    row_identity: &TranscriptRowIdentity,
    turn: &TurnExecutionRecord,
    model: &TranscriptRowPresentationModel,
    context: TranscriptRowPresentabilityContext,
) -> TranscriptCompletedMediaReadiness {
    let mut items = Vec::new();
    for item in &turn.items {
        let ExecutionItem::GeneratedImage(image) = item else {
            continue;
        };
        let Some(descriptor) = model
            .media_descriptors()
            .iter()
            .find(|descriptor| descriptor.key.contains(image.id.as_str()))
        else {
            continue;
        };
        let key = TranscriptMediaReadinessKey::new(
            row_identity.clone(),
            descriptor.key.clone(),
            descriptor.source_revision,
            image
                .saved_path
                .as_deref()
                .and_then(|path| TranscriptMediaPathIdentity::new(path)),
            TranscriptMediaRequestedRenderSize::default(),
            1.0,
            model.revision(),
        );
        items.push(generated_image_presentability(key, image, context));
    }

    if items.is_empty() {
        TranscriptCompletedMediaReadiness::NotRequired
    } else if items
        .iter()
        .all(TranscriptMediaPresentability::is_presentable)
    {
        TranscriptCompletedMediaReadiness::Settled { items }
    } else {
        TranscriptCompletedMediaReadiness::Pending { items }
    }
}

fn generated_image_presentability(
    key: TranscriptMediaReadinessKey,
    image: &super::execution_detail::GeneratedImageDetail,
    context: TranscriptRowPresentabilityContext,
) -> TranscriptMediaPresentability {
    let has_saved_path = image
        .saved_path
        .as_deref()
        .is_some_and(|path| !path.trim().is_empty());
    if has_saved_path {
        return TranscriptMediaPresentability::Pending {
            key,
            reason: TranscriptMediaPendingReadiness::SourceBackedPreload,
        };
    }

    if image
        .result
        .as_ref()
        .is_some_and(|result| !result.trim().is_empty())
    {
        return TranscriptMediaPresentability::Pending {
            key,
            reason: TranscriptMediaPendingReadiness::Decode,
        };
    }

    if !image.complete && context == TranscriptRowPresentabilityContext::Live {
        return TranscriptMediaPresentability::LivePendingGeneratedImage { key };
    }

    if image.complete {
        TranscriptMediaPresentability::TerminalFallback {
            key,
            reason: TranscriptMediaTerminalFallback::Unavailable,
        }
    } else {
        TranscriptMediaPresentability::Pending {
            key,
            reason: TranscriptMediaPendingReadiness::HistoricalGeneratedImageReadiness,
        }
    }
}

fn stable_row_identity(turn: &TurnExecutionRecord) -> Option<TranscriptRowIdentity> {
    match (turn.thread_id.as_deref(), turn.turn_id.as_deref()) {
        (Some(thread_id), Some(turn_id)) => Some(TranscriptRowIdentity::new(format!(
            "thread:{thread_id}:turn:{turn_id}"
        ))),
        _ => None,
    }
}

pub(crate) fn full_detail_readiness(turn: &TurnInfo) -> TranscriptFullDetailReadiness {
    if turn.items_view == TurnItemsView::Full {
        TranscriptFullDetailReadiness::Available
    } else {
        TranscriptFullDetailReadiness::Missing
    }
}
