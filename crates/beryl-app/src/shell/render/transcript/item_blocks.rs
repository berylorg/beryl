use std::{cell::Cell, collections::HashSet, rc::Rc, sync::Arc, time::Instant};

use beryl_model::workspace::WorkspaceId;
use gpui::{AnyElement, App, Pixels, Rgba, div, prelude::*, rgb};

use crate::shell::execution_detail::{
    AgentMessageDetail, ExecutionItem, ReasoningDetail, TurnExecutionRecord, TurnExecutionStatus,
};
use crate::shell::rgba_from_role_color;
use crate::shell::transcript_markdown::markdown_code_panel_ids;
use crate::shell::transcript_selection::TRANSCRIPT_NARRATIVE_BLOCK_BREAK_BEFORE;
use crate::{AppearanceForegroundSettings, AppearanceSettings};

use super::block_markdown::render_markdown_plan_with_style_and_selection;
use super::code_panel_controls::TranscriptCodePanelState;
use super::inline_markdown::{InlineMarkdownStyle, TranscriptInlineSelectionContext};
use super::{
    TranscriptCodeLayout, indexed_item_markdown_key, item_markdown_key,
    markdown_cache::TranscriptMarkdownRenderContext,
    stream_projection::{TranscriptStreamProjectionContext, TranscriptStreamProjectionKey},
    turn_media_units::{collect_markdown_render_unit_code_panel_ids, markdown_render_units},
};

pub(super) fn render_item(
    turn_index: usize,
    _workspace: &WorkspaceId,
    appearance: Arc<AppearanceSettings>,
    turn: Arc<TurnExecutionRecord>,
    item: &ExecutionItem,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    row_identity: &str,
    initial_break_before: usize,
    selection_order: Rc<Cell<usize>>,
    cx: &mut App,
) -> Option<gpui::AnyElement> {
    match item {
        ExecutionItem::AgentMessage(item) => render_agent_message(
            turn_index,
            turn.as_ref(),
            item,
            markdown_context,
            stream_projection_context,
            code_panel_state,
            row_identity,
            initial_break_before,
            selection_order,
            appearance.as_ref(),
            code_layout,
            conversation_m_advance,
            cx,
        ),
        ExecutionItem::Reasoning(item) => render_reasoning(
            turn_index,
            turn.as_ref(),
            item,
            markdown_context,
            stream_projection_context,
            code_panel_state,
            row_identity,
            initial_break_before,
            selection_order,
            appearance.as_ref(),
            code_layout,
            conversation_m_advance,
            cx,
        ),
        ExecutionItem::CommandExecution(_)
        | ExecutionItem::FileChange(_)
        | ExecutionItem::GeneratedImage(_)
        | ExecutionItem::Generic(_) => None,
    }
}

pub(super) fn render_agent_message(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    item: &AgentMessageDetail,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_panel_state: TranscriptCodePanelState,
    row_identity: &str,
    initial_break_before: usize,
    selection_order: Rc<Cell<usize>>,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    cx: &mut App,
) -> Option<gpui::AnyElement> {
    let markdown_style = agent_message_markdown_style(item, appearance);

    if item.text.is_empty() {
        return None;
    }

    let markdown_key = item_markdown_key(turn_index, turn, item.id.as_str(), "agent-message");
    let source = stream_projection_context.visible_text(
        TranscriptStreamProjectionKey::new(markdown_key.as_str()),
        item.text.as_str(),
        live_item_complete(turn, item.complete),
        Instant::now(),
    );
    if source.is_empty() {
        return None;
    }

    let markdown = markdown_context.markdown_for(markdown_key, source.as_str(), cx);
    let selection_context = TranscriptInlineSelectionContext::new_with_initial_break_before(
        code_panel_state.entity(),
        row_identity.to_string(),
        format!("item:{}:agent-message", item.id),
        selection_order,
        initial_break_before,
    );

    Some(render_markdown_plan_with_style_and_selection(
        markdown.render_plan(),
        appearance,
        code_layout,
        conversation_m_advance,
        markdown_style,
        code_panel_state.controls_for(
            row_identity.to_string(),
            format!("item:{}:agent-message", item.id),
        ),
        selection_context,
        cx,
    ))
}

pub(super) fn agent_message_markdown_style(
    item: &AgentMessageDetail,
    appearance: &AppearanceSettings,
) -> InlineMarkdownStyle {
    match item.phase {
        Some(beryl_backend::ProtocolPhase::Commentary) => {
            InlineMarkdownStyle::conversation_foreground(transcript_foreground(
                &appearance.transcript_commentary,
                rgb(0xcbd5e1),
            ))
        }
        Some(beryl_backend::ProtocolPhase::FinalAnswer) => InlineMarkdownStyle::default(),
        None => InlineMarkdownStyle::conversation_foreground(rgb(0xe2e8f0)),
    }
}

pub(super) fn render_reasoning(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    item: &ReasoningDetail,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_panel_state: TranscriptCodePanelState,
    row_identity: &str,
    initial_break_before: usize,
    selection_order: Rc<Cell<usize>>,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    cx: &mut App,
) -> Option<AnyElement> {
    let rendered_reasoning_block_count = Rc::new(Cell::new(0usize));
    let mut blocks = markdown_reasoning_blocks(
        turn_index,
        turn,
        item.id.as_str(),
        "reasoning-summary",
        &item.summary,
        item.complete,
        markdown_context.clone(),
        stream_projection_context.clone(),
        code_panel_state.clone(),
        row_identity,
        initial_break_before,
        rendered_reasoning_block_count.clone(),
        selection_order.clone(),
        appearance,
        code_layout,
        conversation_m_advance,
        cx,
    );
    blocks.extend(markdown_reasoning_blocks(
        turn_index,
        turn,
        item.id.as_str(),
        "reasoning-content",
        &item.content,
        item.complete,
        markdown_context,
        stream_projection_context,
        code_panel_state,
        row_identity,
        initial_break_before,
        rendered_reasoning_block_count,
        selection_order,
        appearance,
        code_layout,
        conversation_m_advance,
        cx,
    ));

    if blocks.is_empty() {
        return None;
    }

    Some(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(blocks)
            .into_any_element(),
    )
}

fn markdown_reasoning_blocks(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    item_id: &str,
    slot: &str,
    items: &[String],
    complete: bool,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_panel_state: TranscriptCodePanelState,
    row_identity: &str,
    first_block_break_before: usize,
    rendered_block_count: Rc<Cell<usize>>,
    selection_order: Rc<Cell<usize>>,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    cx: &mut App,
) -> Vec<AnyElement> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if item.is_empty() {
                return None;
            }

            let markdown_key = indexed_item_markdown_key(turn_index, turn, item_id, slot, index);
            let source = stream_projection_context.visible_text(
                TranscriptStreamProjectionKey::new(markdown_key.as_str()),
                item.as_str(),
                live_item_complete(turn, complete),
                Instant::now(),
            );
            if source.is_empty() {
                return None;
            }

            let markdown = markdown_context.markdown_for(markdown_key, source.as_str(), cx);
            let block_path = format!("item:{item_id}:{slot}:{index}");
            let initial_break_before = if rendered_block_count.get() == 0 {
                first_block_break_before
            } else {
                TRANSCRIPT_NARRATIVE_BLOCK_BREAK_BEFORE
            };
            rendered_block_count.set(rendered_block_count.get().saturating_add(1));
            let selection_context = TranscriptInlineSelectionContext::new_with_initial_break_before(
                code_panel_state.entity(),
                row_identity.to_string(),
                block_path.clone(),
                selection_order.clone(),
                initial_break_before,
            );
            Some(render_markdown_plan_with_style_and_selection(
                markdown.render_plan(),
                appearance,
                code_layout,
                conversation_m_advance,
                InlineMarkdownStyle::conversation_foreground(transcript_foreground(
                    &appearance.transcript_reasoning,
                    rgb(0xe2e8f0),
                )),
                code_panel_state.controls_for(row_identity.to_string(), block_path),
                selection_context,
                cx,
            ))
        })
        .collect()
}

fn transcript_foreground(settings: &AppearanceForegroundSettings, fallback: Rgba) -> Rgba {
    rgba_from_role_color(settings.parsed_foreground(), fallback)
}

pub(super) fn collect_item_markdown_code_panel_ids(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    item: &ExecutionItem,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    row_identity: &str,
    ids: &mut HashSet<String>,
    cx: &mut App,
) {
    match item {
        ExecutionItem::AgentMessage(item) => collect_agent_message_code_panel_ids(
            turn_index,
            turn,
            item,
            markdown_context,
            stream_projection_context,
            row_identity,
            ids,
            cx,
        ),
        ExecutionItem::Reasoning(item) => collect_reasoning_code_panel_ids(
            turn_index,
            turn,
            item,
            markdown_context,
            stream_projection_context,
            row_identity,
            ids,
            cx,
        ),
        ExecutionItem::CommandExecution(_)
        | ExecutionItem::FileChange(_)
        | ExecutionItem::GeneratedImage(_)
        | ExecutionItem::Generic(_) => {}
    }
}

fn collect_agent_message_code_panel_ids(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    item: &AgentMessageDetail,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    row_identity: &str,
    ids: &mut HashSet<String>,
    cx: &mut App,
) {
    if item.text.is_empty() {
        return;
    }

    let markdown_key = item_markdown_key(turn_index, turn, item.id.as_str(), "agent-message");
    let source = stream_projection_context.visible_text(
        TranscriptStreamProjectionKey::new(markdown_key.as_str()),
        item.text.as_str(),
        live_item_complete(turn, item.complete),
        Instant::now(),
    );
    if source.is_empty() {
        return;
    }

    let block_path = format!("item:{}:agent-message", item.id);
    let markdown = markdown_context.markdown_for(markdown_key.clone(), source.as_str(), cx);
    let units = markdown_render_units(&markdown_key, block_path.as_str(), markdown.as_ref());
    collect_markdown_render_unit_code_panel_ids(row_identity, units, markdown_context, ids, cx);
}

fn collect_reasoning_code_panel_ids(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    item: &ReasoningDetail,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    row_identity: &str,
    ids: &mut HashSet<String>,
    cx: &mut App,
) {
    collect_reasoning_part_code_panel_ids(
        turn_index,
        turn,
        item.id.as_str(),
        "reasoning-summary",
        &item.summary,
        item.complete,
        markdown_context.clone(),
        stream_projection_context.clone(),
        row_identity,
        ids,
        cx,
    );
    collect_reasoning_part_code_panel_ids(
        turn_index,
        turn,
        item.id.as_str(),
        "reasoning-content",
        &item.content,
        item.complete,
        markdown_context,
        stream_projection_context,
        row_identity,
        ids,
        cx,
    );
}

fn collect_reasoning_part_code_panel_ids(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    item_id: &str,
    slot: &str,
    items: &[String],
    complete: bool,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    row_identity: &str,
    ids: &mut HashSet<String>,
    cx: &mut App,
) {
    for (index, item) in items.iter().enumerate() {
        if item.is_empty() {
            continue;
        }

        let markdown_key = indexed_item_markdown_key(turn_index, turn, item_id, slot, index);
        let source = stream_projection_context.visible_text(
            TranscriptStreamProjectionKey::new(markdown_key.as_str()),
            item.as_str(),
            live_item_complete(turn, complete),
            Instant::now(),
        );
        if source.is_empty() {
            continue;
        }

        let block_path = format!("item:{item_id}:{slot}:{index}");
        let markdown = markdown_context.markdown_for(markdown_key, source.as_str(), cx);
        ids.extend(markdown_code_panel_ids(
            row_identity,
            block_path.as_str(),
            markdown.render_plan(),
        ));
    }
}

pub(super) fn live_item_complete(turn: &TurnExecutionRecord, item_complete: bool) -> bool {
    item_complete
        || !matches!(
            turn.status,
            TurnExecutionStatus::Starting | TurnExecutionStatus::Running
        )
}
