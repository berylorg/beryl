use std::{cell::Cell, rc::Rc, sync::Arc, time::Instant};

use beryl_model::workspace::WorkspaceId;
use gpui::{AnyElement, App, Entity, Pixels, div, prelude::*, px};

use super::{
    TranscriptCodeLayout, TranscriptFrameProfile, TranscriptTextRole, TranscriptTheme,
    item_markdown_key, markdown_cache::TranscriptMarkdownRenderContext,
    media_cache::TranscriptMediaRenderContext,
    stream_projection::TranscriptStreamProjectionContext, turn_markdown_key,
};
use super::{
    TranscriptMediaRenderIdentity,
    code_panel_controls::TranscriptCodePanelState,
    inline_markdown::InlineMarkdownStyle,
    item_blocks::{agent_message_markdown_style, live_item_complete},
    media_blocks::{TranscriptMediaRenderItem, TranscriptMediaRenderLayout},
    turn_item_media_units::{
        generated_image_media_item, render_item_markdown_source_slice, render_item_units,
    },
    turn_media_units::{
        TranscriptMarkdownRenderUnit, flush_media_run, markdown_render_units, push_rendered_block,
    },
    turn_user_media_units::{
        render_user_prompt_fragment_markdown_source_slice,
        render_user_prompt_markdown_source_slice, render_user_prompt_units,
    },
};
use crate::shell::execution_detail::{ExecutionItem, ReasoningDetail, TurnExecutionRecord};
use crate::shell::transcript_markdown::TranscriptMarkdownCacheKey;
use crate::shell::transcript_presentation::{
    TranscriptRenderBudgetFallbackReason, TranscriptRenderBudgetPolicy,
    TranscriptRenderChunkAdmissionDecision, TranscriptRowChunkMeasurementKey,
    TranscriptRowChunkOwner, TranscriptRowMeasurementKey, TranscriptRowNarrativeUnit,
    TranscriptRowPresentationModel, TranscriptRowRenderChunk, TranscriptRowStreamedRenderAnchor,
    transcript_render_window_admission, transcript_row_chunk_render_window,
};
use crate::shell::transcript_selection::{
    TranscriptTextLineOrder, transcript_narrative_block_break_before,
};

use super::TranscriptPanel;

#[derive(Clone)]
pub(super) struct TranscriptRowChunkRenderState {
    row_key: TranscriptRowMeasurementKey,
    measured_heights: Vec<Option<Pixels>>,
    anchor: TranscriptRowStreamedRenderAnchor,
    measurement_entity: Entity<TranscriptPanel>,
}

impl TranscriptRowChunkRenderState {
    pub(super) fn new(
        row_key: TranscriptRowMeasurementKey,
        measured_heights: Vec<Option<Pixels>>,
        anchor: TranscriptRowStreamedRenderAnchor,
        measurement_entity: Entity<TranscriptPanel>,
    ) -> Self {
        Self {
            row_key,
            measured_heights,
            anchor,
            measurement_entity,
        }
    }
}

pub(super) fn render_turn_card(
    turn_index: usize,
    workspace: &WorkspaceId,
    theme: Arc<TranscriptTheme>,
    turn: Arc<TurnExecutionRecord>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    row_model: Arc<TranscriptRowPresentationModel>,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    show_activity_caret: bool,
    activity_caret_opacity: f32,
    viewport_height: Pixels,
    chunk_render_state: Option<TranscriptRowChunkRenderState>,
    profiler: Option<Rc<TranscriptFrameProfile>>,
    cx: &mut App,
) -> AnyElement {
    if row_model.chunk_presentation().requires_chunking()
        && let Some(chunk_render_state) = chunk_render_state
    {
        return render_turn_card_chunk_window(
            turn_index,
            workspace,
            theme,
            turn,
            code_panel_state,
            markdown_context,
            media_context,
            stream_projection_context,
            row_model,
            code_layout,
            media_layout,
            row_identity,
            selection_order,
            narrative_copy_block_count,
            show_activity_caret,
            activity_caret_opacity,
            viewport_height,
            chunk_render_state,
            profiler,
            cx,
        );
    }

    render_turn_card_full(
        turn_index,
        workspace,
        theme,
        turn,
        code_panel_state,
        markdown_context,
        media_context,
        stream_projection_context,
        row_model,
        code_layout,
        media_layout,
        row_identity,
        selection_order,
        narrative_copy_block_count,
        show_activity_caret,
        activity_caret_opacity,
        cx,
    )
}

fn render_turn_card_full(
    turn_index: usize,
    workspace: &WorkspaceId,
    theme: Arc<TranscriptTheme>,
    turn: Arc<TurnExecutionRecord>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    row_model: Arc<TranscriptRowPresentationModel>,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    show_activity_caret: bool,
    activity_caret_opacity: f32,
    cx: &mut App,
) -> AnyElement {
    let mut narrative_blocks = Vec::new();
    let mut pending_media = Vec::new();
    for unit in row_model.narrative_units() {
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
                render_user_prompt_units(
                    turn_index,
                    workspace,
                    turn.as_ref(),
                    *fragment_index,
                    fragment,
                    theme.as_ref(),
                    code_panel_state.clone(),
                    markdown_context.clone(),
                    media_context.clone(),
                    code_layout,
                    media_layout,
                    row_identity,
                    selection_order.clone(),
                    narrative_copy_block_count.clone(),
                    &mut pending_media,
                    &mut narrative_blocks,
                    cx,
                );
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
                render_item_units(
                    turn_index,
                    workspace,
                    theme.clone(),
                    turn.clone(),
                    item,
                    code_panel_state.clone(),
                    markdown_context.clone(),
                    media_context.clone(),
                    stream_projection_context.clone(),
                    code_layout,
                    media_layout,
                    row_identity,
                    selection_order.clone(),
                    narrative_copy_block_count.clone(),
                    &mut pending_media,
                    &mut narrative_blocks,
                    cx,
                );
            }
            TranscriptRowNarrativeUnit::TerminalFallback => {
                if let Some(message) = turn.terminal_fallback_text() {
                    narrative_blocks.push(render_terminal_fallback(message, theme.as_ref()));
                }
            }
        }
    }
    flush_media_run(
        workspace,
        media_context,
        &mut pending_media,
        &mut narrative_blocks,
        media_layout,
        row_identity,
        selection_order,
        narrative_copy_block_count,
        cx,
    );

    div()
        .flex()
        .flex_col()
        .gap_3()
        .children(narrative_blocks)
        .when(show_activity_caret, |this| {
            this.child(render_activity_caret(
                activity_caret_opacity,
                theme.as_ref(),
            ))
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_turn_card_chunk_window(
    turn_index: usize,
    workspace: &WorkspaceId,
    theme: Arc<TranscriptTheme>,
    turn: Arc<TurnExecutionRecord>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    row_model: Arc<TranscriptRowPresentationModel>,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    show_activity_caret: bool,
    activity_caret_opacity: f32,
    viewport_height: Pixels,
    chunk_render_state: TranscriptRowChunkRenderState,
    profiler: Option<Rc<TranscriptFrameProfile>>,
    cx: &mut App,
) -> AnyElement {
    let chunks = row_model.chunk_presentation().chunks();
    let chunk_window_started = Instant::now();
    let render_window = transcript_row_chunk_render_window(
        chunks.len(),
        chunk_render_state.measured_heights.as_slice(),
        chunk_render_state.anchor.clone(),
        viewport_height,
    );
    let policy = TranscriptRenderBudgetPolicy::default_frame();
    let admission = transcript_render_window_admission(chunks, render_window.range.clone(), policy);
    if let Some(profiler) = profiler.as_ref() {
        profiler.observe_chunk_window_admission(
            &render_window,
            &admission,
            policy,
            chunk_window_started.elapsed(),
        );
    }
    let mut children = Vec::new();

    for chunk_admission in admission.chunks {
        let chunk_index = chunk_admission.chunk_index;
        let Some(chunk) = chunks.get(chunk_index) else {
            continue;
        };
        let blocks = match chunk_admission.decision {
            TranscriptRenderChunkAdmissionDecision::Render => {
                let selection_scope = chunk.identity.clone();
                render_turn_card_chunk_blocks(
                    turn_index,
                    workspace,
                    theme.clone(),
                    turn.clone(),
                    code_panel_state
                        .clone()
                        .with_viewport_local_selection_scope(selection_scope.clone()),
                    markdown_context.clone(),
                    media_context
                        .clone()
                        .with_viewport_local_selection_scope(selection_scope),
                    stream_projection_context.clone(),
                    row_model.as_ref(),
                    code_layout,
                    media_layout,
                    row_identity,
                    selection_order.clone(),
                    narrative_copy_block_count.clone(),
                    chunk,
                    cx,
                )
            }
            TranscriptRenderChunkAdmissionDecision::Fallback(reason) => {
                vec![render_render_budget_fallback(reason, theme.as_ref())]
            }
        };
        let is_last_chunk = chunk_index.saturating_add(1) == chunks.len();
        children.push(render_measured_chunk(
            blocks,
            TranscriptRowChunkMeasurementKey::new(chunk_render_state.row_key.clone(), chunk),
            chunk_render_state.measurement_entity.clone(),
            !is_last_chunk,
        ));
    }

    div()
        .flex()
        .flex_col()
        .children(children)
        .when(show_activity_caret, |this| {
            this.child(render_activity_caret(
                activity_caret_opacity,
                theme.as_ref(),
            ))
        })
        .into_any_element()
}

fn render_measured_chunk(
    blocks: Vec<AnyElement>,
    measurement_key: TranscriptRowChunkMeasurementKey,
    measurement_entity: Entity<TranscriptPanel>,
    include_following_gap: bool,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_3()
        .children(blocks)
        .when(include_following_gap, |this| {
            this.child(div().h(px(0.0)).flex_none())
        })
        .on_children_prepainted(move |children, window, cx| {
            let Some(first) = children.first().copied() else {
                return;
            };
            let mut top = first.top();
            let mut bottom = first.bottom();
            for child in children.iter().copied().skip(1) {
                top = top.min(child.top());
                bottom = bottom.max(child.bottom());
            }
            let height = (bottom - top).max(px(0.0));
            let measurement_key = measurement_key.clone();
            let measurement_entity = measurement_entity.clone();
            window.defer(cx, move |_, cx| {
                measurement_entity.update(cx, |view, cx| {
                    view.record_transcript_row_chunk_measurement(measurement_key, height, cx);
                });
            });
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_turn_card_chunk_blocks(
    turn_index: usize,
    workspace: &WorkspaceId,
    theme: Arc<TranscriptTheme>,
    turn: Arc<TurnExecutionRecord>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    row_model: &TranscriptRowPresentationModel,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    chunk: &TranscriptRowRenderChunk,
    cx: &mut App,
) -> Vec<AnyElement> {
    let mut narrative_blocks = Vec::new();
    let mut pending_media = Vec::new();

    match &chunk.owner {
        TranscriptRowChunkOwner::NarrativeUnit { unit_index } => {
            if let Some(unit) = row_model.narrative_units().get(*unit_index) {
                render_narrative_unit_chunk(
                    turn_index,
                    workspace,
                    theme,
                    turn,
                    code_panel_state,
                    markdown_context,
                    media_context.clone(),
                    stream_projection_context,
                    row_model,
                    code_layout,
                    media_layout,
                    row_identity,
                    selection_order.clone(),
                    narrative_copy_block_count.clone(),
                    unit,
                    &mut pending_media,
                    &mut narrative_blocks,
                    cx,
                );
            }
        }
        TranscriptRowChunkOwner::MarkdownSource {
            key,
            block_path,
            first_unit_index,
            unit_count,
        } => {
            render_markdown_source_chunk(
                turn_index,
                workspace,
                theme.as_ref(),
                turn.as_ref(),
                row_model,
                key,
                block_path,
                *first_unit_index..first_unit_index.saturating_add(*unit_count),
                code_panel_state,
                markdown_context,
                media_context.clone(),
                stream_projection_context,
                code_layout,
                media_layout,
                row_identity,
                selection_order.clone(),
                narrative_copy_block_count.clone(),
                &mut pending_media,
                &mut narrative_blocks,
                cx,
            );
        }
        TranscriptRowChunkOwner::MediaDescriptor { key } => {
            if let Some(image) = turn.items.iter().find_map(|item| match item {
                ExecutionItem::GeneratedImage(image) if image.id == *key => Some(image),
                _ => None,
            }) {
                pending_media.push(generated_image_media_item(
                    turn_index,
                    turn.as_ref(),
                    image,
                    row_identity,
                ));
            }
        }
    }

    flush_media_run(
        workspace,
        media_context,
        &mut pending_media,
        &mut narrative_blocks,
        media_layout,
        row_identity,
        selection_order,
        narrative_copy_block_count,
        cx,
    );
    narrative_blocks
}

#[allow(clippy::too_many_arguments)]
fn render_narrative_unit_chunk(
    turn_index: usize,
    workspace: &WorkspaceId,
    theme: Arc<TranscriptTheme>,
    turn: Arc<TurnExecutionRecord>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    _row_model: &TranscriptRowPresentationModel,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    unit: &TranscriptRowNarrativeUnit,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
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
                return;
            };
            render_user_prompt_units(
                turn_index,
                workspace,
                turn.as_ref(),
                *fragment_index,
                fragment,
                theme.as_ref(),
                code_panel_state,
                markdown_context,
                media_context,
                code_layout,
                media_layout,
                row_identity,
                selection_order,
                narrative_copy_block_count,
                pending_media,
                narrative_blocks,
                cx,
            );
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
                return;
            };
            render_item_units(
                turn_index,
                workspace,
                theme,
                turn.clone(),
                item,
                code_panel_state,
                markdown_context,
                media_context,
                stream_projection_context,
                code_layout,
                media_layout,
                row_identity,
                selection_order,
                narrative_copy_block_count,
                pending_media,
                narrative_blocks,
                cx,
            );
        }
        TranscriptRowNarrativeUnit::TerminalFallback => {
            if let Some(message) = turn.terminal_fallback_text() {
                narrative_blocks.push(render_terminal_fallback(message, theme.as_ref()));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_markdown_source_chunk(
    turn_index: usize,
    workspace: &WorkspaceId,
    theme: &TranscriptTheme,
    turn: &TurnExecutionRecord,
    row_model: &TranscriptRowPresentationModel,
    key: &str,
    block_path: &str,
    local_range: std::ops::Range<usize>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
    let Some(source) = row_model
        .markdown_sources()
        .iter()
        .find(|source| source.key == key && source.block_path == block_path)
    else {
        return;
    };

    match source.source_kind {
        crate::shell::transcript_presentation::TranscriptRowMarkdownSourceKind::UserInput => {
            render_user_markdown_source_chunk(
                turn_index,
                workspace,
                theme,
                turn,
                block_path,
                local_range,
                code_panel_state,
                markdown_context,
                media_context,
                code_layout,
                media_layout,
                row_identity,
                selection_order,
                narrative_copy_block_count,
                pending_media,
                narrative_blocks,
                cx,
            );
        }
        crate::shell::transcript_presentation::TranscriptRowMarkdownSourceKind::AgentMessage => {
            render_agent_markdown_source_chunk(
                turn_index,
                workspace,
                theme,
                turn,
                block_path,
                local_range,
                code_panel_state,
                markdown_context,
                media_context,
                stream_projection_context,
                code_layout,
                media_layout,
                row_identity,
                selection_order,
                narrative_copy_block_count,
                pending_media,
                narrative_blocks,
                cx,
            );
        }
        crate::shell::transcript_presentation::TranscriptRowMarkdownSourceKind::ReasoningSummary
        | crate::shell::transcript_presentation::TranscriptRowMarkdownSourceKind::ReasoningContent => {
            render_reasoning_markdown_source_chunk(
                workspace,
                theme,
                turn,
                key,
                block_path,
                local_range,
                code_panel_state,
                markdown_context,
                media_context,
                stream_projection_context,
                code_layout,
                media_layout,
                row_identity,
                selection_order,
                narrative_copy_block_count,
                pending_media,
                narrative_blocks,
                cx,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_user_markdown_source_chunk(
    turn_index: usize,
    workspace: &WorkspaceId,
    theme: &TranscriptTheme,
    turn: &TurnExecutionRecord,
    block_path: &str,
    local_range: std::ops::Range<usize>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
    let Some(fragment_index) = user_prompt_fragment_index(block_path) else {
        return;
    };
    let Some(fragment) = turn.user_input_fragments().get(fragment_index) else {
        return;
    };
    if fragment.text.is_empty() {
        return;
    }
    if !fragment.image_markers().is_empty() {
        let initial_break_before =
            transcript_narrative_block_break_before(narrative_copy_block_count.get());
        if let Some(rendered) = render_user_prompt_fragment_markdown_source_slice(
            turn_index,
            turn,
            fragment_index,
            fragment,
            local_range,
            theme,
            code_panel_state,
            markdown_context,
            code_layout,
            media_layout.conversation_m_advance,
            row_identity,
            initial_break_before,
            selection_order.clone(),
            cx,
        ) {
            push_rendered_block(
                workspace,
                media_context,
                pending_media,
                narrative_blocks,
                media_layout,
                row_identity,
                selection_order,
                narrative_copy_block_count,
                rendered,
                cx,
            );
        }
        return;
    }

    render_markdown_source_window(
        workspace,
        turn_markdown_key(turn_index, turn, block_path),
        block_path.to_string(),
        fragment.text.as_str(),
        local_range,
        theme,
        code_panel_state,
        markdown_context,
        media_context,
        code_layout,
        media_layout,
        row_identity,
        selection_order,
        narrative_copy_block_count,
        true,
        InlineMarkdownStyle::base(TranscriptTextRole::UserInput),
        cx,
        pending_media,
        narrative_blocks,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_agent_markdown_source_chunk(
    turn_index: usize,
    workspace: &WorkspaceId,
    theme: &TranscriptTheme,
    turn: &TurnExecutionRecord,
    block_path: &str,
    local_range: std::ops::Range<usize>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
    let Some(item_id) = agent_message_item_id(block_path) else {
        return;
    };
    let Some(ExecutionItem::AgentMessage(message)) = turn.item_by_id(item_id) else {
        return;
    };
    let markdown_key = item_markdown_key(turn_index, turn, message.id.as_str(), "agent-message");
    let source = stream_projection_context.visible_text(
        super::stream_projection::TranscriptStreamProjectionKey::new(markdown_key.as_str()),
        message.text.as_str(),
        live_item_complete(turn, message.complete),
        std::time::Instant::now(),
    );
    if source.is_empty() {
        return;
    }
    render_markdown_source_window(
        workspace,
        markdown_key,
        block_path.to_string(),
        source.as_ref(),
        local_range,
        theme,
        code_panel_state,
        markdown_context,
        media_context,
        code_layout,
        media_layout,
        row_identity,
        selection_order,
        narrative_copy_block_count,
        false,
        agent_message_markdown_style(message),
        cx,
        pending_media,
        narrative_blocks,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_reasoning_markdown_source_chunk(
    workspace: &WorkspaceId,
    theme: &TranscriptTheme,
    turn: &TurnExecutionRecord,
    key: &str,
    block_path: &str,
    local_range: std::ops::Range<usize>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
    let Some((reasoning, source_text)) = turn.items.iter().find_map(|item| match item {
        ExecutionItem::Reasoning(reasoning) => {
            reasoning_source_text(reasoning, block_path).map(|source| (reasoning, source))
        }
        _ => None,
    }) else {
        return;
    };
    let markdown_key = TranscriptMarkdownCacheKey::new(key.to_string());
    let source = stream_projection_context.visible_text(
        super::stream_projection::TranscriptStreamProjectionKey::new(markdown_key.as_str()),
        source_text,
        live_item_complete(turn, reasoning.complete),
        std::time::Instant::now(),
    );
    if source.is_empty() {
        return;
    }
    render_markdown_source_window(
        workspace,
        markdown_key,
        block_path.to_string(),
        source.as_ref(),
        local_range,
        theme,
        code_panel_state,
        markdown_context,
        media_context,
        code_layout,
        media_layout,
        row_identity,
        selection_order,
        narrative_copy_block_count,
        false,
        InlineMarkdownStyle::base(TranscriptTextRole::AssistantReasoning),
        cx,
        pending_media,
        narrative_blocks,
    );
}

fn user_prompt_fragment_index(block_path: &str) -> Option<usize> {
    block_path
        .strip_prefix("user-prompt:")
        .and_then(|index| index.parse::<usize>().ok())
}

fn agent_message_item_id(block_path: &str) -> Option<&str> {
    block_path
        .strip_prefix("item:")
        .and_then(|path| path.strip_suffix(":agent-message"))
}

fn render_markdown_source_window(
    workspace: &WorkspaceId,
    markdown_key: TranscriptMarkdownCacheKey,
    block_path: String,
    source: &str,
    local_range: std::ops::Range<usize>,
    theme: &TranscriptTheme,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    user_prompt: bool,
    style: InlineMarkdownStyle,
    cx: &mut App,
    pending_media: &mut Vec<super::media_blocks::TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
) {
    let markdown = markdown_context.markdown_for(markdown_key.clone(), source, cx);
    if !markdown.used_parser_fallback() {
        let units = markdown_render_units(&markdown_key, block_path.as_str(), markdown.as_ref());
        if units
            .iter()
            .any(|unit| matches!(unit, TranscriptMarkdownRenderUnit::Media { .. }))
        {
            render_markdown_render_units_window(
                workspace,
                units,
                local_range,
                theme,
                code_panel_state,
                markdown_context,
                media_context,
                code_layout,
                media_layout,
                row_identity,
                selection_order,
                narrative_copy_block_count,
                user_prompt,
                style,
                cx,
                pending_media,
                narrative_blocks,
            );
            return;
        }
    }

    render_markdown_source_slice_window(
        workspace,
        source,
        markdown_key,
        block_path,
        local_range,
        theme,
        code_panel_state,
        markdown_context,
        media_context,
        code_layout,
        media_layout,
        row_identity,
        selection_order,
        narrative_copy_block_count,
        user_prompt,
        style,
        cx,
        pending_media,
        narrative_blocks,
    );
}

fn render_markdown_render_units_window(
    workspace: &WorkspaceId,
    units: Vec<TranscriptMarkdownRenderUnit<'_>>,
    local_range: std::ops::Range<usize>,
    theme: &TranscriptTheme,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    user_prompt: bool,
    style: InlineMarkdownStyle,
    cx: &mut App,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
) {
    let mut unit_cursor = 0usize;

    for unit in units {
        match unit {
            TranscriptMarkdownRenderUnit::Markdown {
                key,
                block_path,
                source,
            } => {
                let markdown = markdown_context.markdown_for(key.clone(), source.as_ref(), cx);
                let unit_blocks = markdown
                    .render_plan()
                    .blocks
                    .len()
                    .max(estimate_markdown_window_blocks(source.as_ref()));
                let Some(unit_range) =
                    intersect_block_range(unit_cursor, unit_blocks.max(1), &local_range)
                else {
                    unit_cursor = unit_cursor.saturating_add(unit_blocks.max(1));
                    continue;
                };
                render_markdown_source_slice_window(
                    workspace,
                    source.as_ref(),
                    key,
                    block_path,
                    unit_range,
                    theme,
                    code_panel_state.clone(),
                    markdown_context.clone(),
                    media_context.clone(),
                    code_layout,
                    media_layout,
                    row_identity,
                    selection_order.clone(),
                    narrative_copy_block_count.clone(),
                    user_prompt,
                    style,
                    cx,
                    pending_media,
                    narrative_blocks,
                );
                unit_cursor = unit_cursor.saturating_add(unit_blocks.max(1));
            }
            TranscriptMarkdownRenderUnit::Media { key, source } => {
                if local_range.contains(&unit_cursor) {
                    let identity =
                        TranscriptMediaRenderIdentity::new(row_identity, key.clone(), &source);
                    pending_media.push(TranscriptMediaRenderItem {
                        key,
                        source,
                        identity,
                    });
                }
                unit_cursor = unit_cursor.saturating_add(1);
            }
        }
    }

    let rendered_until = unit_cursor.min(local_range.end).max(local_range.start);
    if rendered_until < local_range.end {
        push_block_window_spacer(
            workspace,
            media_context,
            media_layout,
            row_identity,
            selection_order,
            narrative_copy_block_count,
            pending_media,
            narrative_blocks,
            local_range.end.saturating_sub(rendered_until),
            cx,
        );
    }
}

fn render_markdown_source_slice_window(
    workspace: &WorkspaceId,
    source: &str,
    markdown_key: TranscriptMarkdownCacheKey,
    block_path: String,
    local_range: std::ops::Range<usize>,
    theme: &TranscriptTheme,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    user_prompt: bool,
    style: InlineMarkdownStyle,
    cx: &mut App,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
) {
    let markdown = markdown_context.markdown_for(markdown_key.clone(), source, cx);
    if markdown.used_parser_fallback() && local_range.end > 1 {
        push_block_window_spacer(
            workspace,
            media_context,
            media_layout,
            row_identity,
            selection_order,
            narrative_copy_block_count,
            pending_media,
            narrative_blocks,
            local_range.len().max(1),
            cx,
        );
        return;
    }
    let actual_end = local_range.end.min(markdown.render_plan().blocks.len());
    let actual_start = local_range.start.min(actual_end);
    if actual_start == actual_end {
        push_block_window_spacer(
            workspace,
            media_context,
            media_layout,
            row_identity,
            selection_order,
            narrative_copy_block_count,
            pending_media,
            narrative_blocks,
            local_range.len().max(1),
            cx,
        );
        return;
    }
    let initial_break_before =
        transcript_narrative_block_break_before(narrative_copy_block_count.get());
    let rendered = if user_prompt {
        render_user_prompt_markdown_source_slice(
            source,
            markdown_key,
            block_path,
            actual_start..actual_end,
            theme,
            code_panel_state,
            markdown_context,
            code_layout,
            media_layout.conversation_m_advance,
            row_identity,
            initial_break_before,
            selection_order.clone(),
            cx,
        )
    } else {
        render_item_markdown_source_slice(
            source,
            markdown_key,
            block_path,
            actual_start..actual_end,
            theme,
            code_panel_state,
            markdown_context,
            code_layout,
            media_layout.conversation_m_advance,
            row_identity,
            initial_break_before,
            selection_order.clone(),
            style,
            cx,
        )
    };
    push_rendered_block(
        workspace,
        media_context,
        pending_media,
        narrative_blocks,
        media_layout,
        row_identity,
        selection_order,
        narrative_copy_block_count,
        rendered,
        cx,
    );
}

fn push_block_window_spacer(
    workspace: &WorkspaceId,
    media_context: TranscriptMediaRenderContext,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<TranscriptTextLineOrder>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    _block_count: usize,
    cx: &mut App,
) {
    flush_media_run(
        workspace,
        media_context,
        pending_media,
        narrative_blocks,
        media_layout,
        row_identity,
        selection_order,
        narrative_copy_block_count,
        cx,
    );
}

fn estimate_markdown_window_blocks(source: &str) -> usize {
    let source = source.replace("\r\n", "\n").replace('\r', "\n");
    source
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .map(|block| {
            let fenced_lines =
                if block.trim_start().starts_with("```") || block.trim_start().starts_with("~~~") {
                    block.lines().count().max(1)
                } else {
                    1
                };
            fenced_lines.max(1).max(
                block
                    .len()
                    .max(1)
                    .div_ceil(TRANSCRIPT_ROW_MARKDOWN_SOURCE_BYTES_PER_BLOCK),
            )
        })
        .sum::<usize>()
        .max(1)
}

fn reasoning_source_text<'a>(reasoning: &'a ReasoningDetail, block_path: &str) -> Option<&'a str> {
    let summary_prefix = format!("item:{}:reasoning-summary:", reasoning.id);
    if let Some(index) = block_path
        .strip_prefix(summary_prefix.as_str())
        .and_then(|index| index.parse::<usize>().ok())
    {
        return reasoning.summary.get(index).map(String::as_str);
    }

    let content_prefix = format!("item:{}:reasoning-content:", reasoning.id);
    block_path
        .strip_prefix(content_prefix.as_str())
        .and_then(|index| index.parse::<usize>().ok())
        .and_then(|index| reasoning.content.get(index))
        .map(String::as_str)
}

const TRANSCRIPT_ROW_MARKDOWN_SOURCE_BYTES_PER_BLOCK: usize = 4 * 1024;

fn intersect_block_range(
    block_start: usize,
    block_count: usize,
    range: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let block_end = block_start.saturating_add(block_count);
    let start = block_start.max(range.start);
    let end = block_end.min(range.end);
    (start < end).then(|| start.saturating_sub(block_start)..end.saturating_sub(block_start))
}

pub(super) fn user_prompt_block_path(fragment_index: usize) -> String {
    format!("user-prompt:{fragment_index}")
}

fn render_activity_caret(opacity: f32, theme: &TranscriptTheme) -> impl IntoElement {
    div()
        .w(px(9.0))
        .h(px(18.0))
        .flex_none()
        .opacity(opacity.clamp(0.0, 1.0))
        .bg(theme.activity_caret.color())
}

fn render_render_budget_fallback(
    reason: TranscriptRenderBudgetFallbackReason,
    theme: &TranscriptTheme,
) -> gpui::AnyElement {
    render_transcript_unavailable_message(reason.message(), theme)
}

fn render_terminal_fallback(message: &'static str, theme: &TranscriptTheme) -> gpui::AnyElement {
    render_transcript_unavailable_message(message, theme)
}

fn render_transcript_unavailable_message(
    message: &'static str,
    theme: &TranscriptTheme,
) -> gpui::AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .rounded_sm()
        .border_1()
        .border_color(theme.unavailable.border())
        .bg(theme.unavailable.background())
        .px_3()
        .py_2()
        .text_sm()
        .font_family(theme.unavailable.font_family().to_string())
        .text_size(px(theme.unavailable.font_size()))
        .font_weight(theme.unavailable.font_weight())
        .text_color(theme.unavailable.foreground())
        .child(message)
        .into_any_element()
}
