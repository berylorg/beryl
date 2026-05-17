use std::{cell::Cell, rc::Rc, sync::Arc, time::Instant};

use beryl_model::workspace::WorkspaceId;
use gpui::{AnyElement, App, Pixels, div, prelude::*};

use crate::shell::execution_detail::{
    AgentMessageDetail, ExecutionItem, ReasoningDetail, TurnExecutionRecord, TurnExecutionStatus,
};
use crate::shell::transcript_selection::TRANSCRIPT_NARRATIVE_BLOCK_BREAK_BEFORE;

use super::block_markdown::render_markdown_plan_with_style_and_selection;
use super::code_panel_controls::TranscriptCodePanelState;
use super::inline_markdown::{InlineMarkdownStyle, TranscriptInlineSelectionContext};
use super::{
    TranscriptCodeLayout, TranscriptTextRole, TranscriptTheme, indexed_item_markdown_key,
    item_markdown_key,
    markdown_cache::TranscriptMarkdownRenderContext,
    stream_projection::{TranscriptStreamProjectionContext, TranscriptStreamProjectionKey},
};

pub(super) fn render_item(
    turn_index: usize,
    _workspace: &WorkspaceId,
    theme: Arc<TranscriptTheme>,
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
            theme.as_ref(),
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
            theme.as_ref(),
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
    theme: &TranscriptTheme,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    cx: &mut App,
) -> Option<gpui::AnyElement> {
    let markdown_style = agent_message_markdown_style(item);

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

    let markdown = markdown_context.markdown_for(markdown_key, source.as_ref(), cx);
    let selection_context = TranscriptInlineSelectionContext::new_with_initial_break_before(
        code_panel_state.entity(),
        row_identity.to_string(),
        format!("item:{}:agent-message", item.id),
        selection_order,
        initial_break_before,
    );

    Some(render_markdown_plan_with_style_and_selection(
        markdown.render_plan(),
        theme,
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

pub(super) fn agent_message_markdown_style(item: &AgentMessageDetail) -> InlineMarkdownStyle {
    match item.phase {
        Some(beryl_backend::ProtocolPhase::Commentary) => {
            InlineMarkdownStyle::base(TranscriptTextRole::AssistantCommentary)
        }
        Some(beryl_backend::ProtocolPhase::FinalAnswer) => InlineMarkdownStyle::default(),
        None => InlineMarkdownStyle::base(TranscriptTextRole::AssistantFinal),
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
    theme: &TranscriptTheme,
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
        theme,
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
        theme,
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
    theme: &TranscriptTheme,
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

            let markdown = markdown_context.markdown_for(markdown_key, source.as_ref(), cx);
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
                theme,
                code_layout,
                conversation_m_advance,
                InlineMarkdownStyle::base(TranscriptTextRole::AssistantReasoning),
                code_panel_state.controls_for(row_identity.to_string(), block_path),
                selection_context,
                cx,
            ))
        })
        .collect()
}

pub(super) fn live_item_complete(turn: &TurnExecutionRecord, item_complete: bool) -> bool {
    item_complete
        || !matches!(
            turn.status,
            TurnExecutionStatus::Starting | TurnExecutionStatus::Running
        )
}
