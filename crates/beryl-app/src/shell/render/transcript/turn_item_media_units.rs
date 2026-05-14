use std::{cell::Cell, rc::Rc, sync::Arc, time::Instant};

use beryl_model::workspace::WorkspaceId;
use gpui::{AnyElement, App};

use crate::AppearanceSettings;
use crate::shell::execution_detail::{
    AgentMessageDetail, ExecutionItem, GeneratedImageDetail, TurnExecutionRecord,
};
use crate::shell::transcript_markdown::TranscriptMarkdownCacheKey;
use crate::shell::transcript_media::{TranscriptMediaCacheKey, TranscriptMediaSource};
use crate::shell::transcript_media_runs::{TranscriptMediaRunSegment, markdown_media_run_segments};
use crate::shell::transcript_selection::transcript_narrative_block_break_before;

use super::{
    TranscriptCodeLayout, item_markdown_key,
    markdown_cache::TranscriptMarkdownRenderContext,
    media_cache::TranscriptMediaRenderContext,
    stream_projection::{TranscriptStreamProjectionContext, TranscriptStreamProjectionKey},
};
use super::{
    TranscriptMediaRenderIdentity,
    block_markdown::render_markdown_plan_with_style_and_selection,
    code_panel_controls::TranscriptCodePanelState,
    inline_markdown::{InlineMarkdownStyle, TranscriptInlineSelectionContext},
    item_blocks::{
        agent_message_markdown_style, live_item_complete, render_agent_message, render_item,
    },
    media_blocks::{TranscriptMediaRenderItem, TranscriptMediaRenderLayout},
    turn_media_units::{push_rendered_block, segment_markdown_key, segment_media_key},
};

pub(super) fn render_item_units(
    turn_index: usize,
    workspace: &WorkspaceId,
    appearance: Arc<AppearanceSettings>,
    turn: Arc<TurnExecutionRecord>,
    item: &ExecutionItem,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<usize>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
    match item {
        ExecutionItem::AgentMessage(message) => render_agent_message_units(
            turn_index,
            workspace,
            appearance,
            turn,
            message,
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
        ),
        ExecutionItem::GeneratedImage(image) => {
            pending_media.push(generated_image_media_item(
                turn_index,
                turn.as_ref(),
                image,
                row_identity,
            ));
        }
        ExecutionItem::Reasoning(_)
        | ExecutionItem::CommandExecution(_)
        | ExecutionItem::FileChange(_)
        | ExecutionItem::Generic(_) => render_plain_item(
            turn_index,
            workspace,
            appearance,
            turn,
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
        ),
    }
}

fn render_plain_item(
    turn_index: usize,
    workspace: &WorkspaceId,
    appearance: Arc<AppearanceSettings>,
    turn: Arc<TurnExecutionRecord>,
    item: &ExecutionItem,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<usize>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
    let initial_break_before =
        transcript_narrative_block_break_before(narrative_copy_block_count.get());
    if let Some(rendered) = render_item(
        turn_index,
        workspace,
        appearance,
        turn,
        item,
        code_panel_state,
        markdown_context,
        stream_projection_context,
        code_layout,
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
}

fn render_agent_message_units(
    turn_index: usize,
    workspace: &WorkspaceId,
    appearance: Arc<AppearanceSettings>,
    turn: Arc<TurnExecutionRecord>,
    item: &AgentMessageDetail,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<usize>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
    if item.text.is_empty() {
        return;
    }

    let markdown_key =
        item_markdown_key(turn_index, turn.as_ref(), item.id.as_str(), "agent-message");
    let source = stream_projection_context.visible_text(
        TranscriptStreamProjectionKey::new(markdown_key.as_str()),
        item.text.as_str(),
        live_item_complete(turn.as_ref(), item.complete),
        Instant::now(),
    );
    if source.is_empty() {
        return;
    }

    let markdown = markdown_context.markdown_for(markdown_key.clone(), source.as_str(), cx);
    let segments = markdown_media_run_segments(markdown.as_ref());
    if !segments
        .iter()
        .any(|segment| matches!(segment, TranscriptMediaRunSegment::Media(_)))
    {
        render_unsplit_agent_message(
            turn_index,
            workspace,
            appearance,
            turn,
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
        return;
    }

    let style = agent_message_markdown_style(item, appearance.as_ref());
    let block_path = format!("item:{}:agent-message", item.id);
    for (segment_index, segment) in segments.into_iter().enumerate() {
        match segment {
            TranscriptMediaRunSegment::Markdown(source) => {
                let segment_key = segment_markdown_key(&markdown_key, segment_index);
                let segment_block_path = format!("{block_path}:segment:{segment_index}");
                let initial_break_before =
                    transcript_narrative_block_break_before(narrative_copy_block_count.get());
                let rendered = render_item_markdown_source(
                    source.as_str(),
                    segment_key,
                    segment_block_path,
                    appearance.as_ref(),
                    code_panel_state.clone(),
                    markdown_context.clone(),
                    code_layout,
                    row_identity,
                    initial_break_before,
                    selection_order.clone(),
                    style,
                    cx,
                );
                push_rendered_block(
                    workspace,
                    media_context.clone(),
                    pending_media,
                    narrative_blocks,
                    media_layout,
                    row_identity,
                    selection_order.clone(),
                    narrative_copy_block_count.clone(),
                    rendered,
                    cx,
                );
            }
            TranscriptMediaRunSegment::Media(source) => {
                let key = segment_media_key(&markdown_key, segment_index);
                let identity =
                    TranscriptMediaRenderIdentity::new(row_identity, key.clone(), &source);
                pending_media.push(TranscriptMediaRenderItem {
                    key,
                    source,
                    identity,
                });
            }
        }
    }
}

fn render_unsplit_agent_message(
    turn_index: usize,
    workspace: &WorkspaceId,
    appearance: Arc<AppearanceSettings>,
    turn: Arc<TurnExecutionRecord>,
    item: &AgentMessageDetail,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<usize>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
    let initial_break_before =
        transcript_narrative_block_break_before(narrative_copy_block_count.get());
    if let Some(rendered) = render_agent_message(
        turn_index,
        turn.as_ref(),
        item,
        markdown_context,
        stream_projection_context,
        code_panel_state,
        row_identity,
        initial_break_before,
        selection_order.clone(),
        appearance.as_ref(),
        code_layout,
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
}

fn render_item_markdown_source(
    source: &str,
    markdown_key: TranscriptMarkdownCacheKey,
    block_path: String,
    appearance: &AppearanceSettings,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    code_layout: TranscriptCodeLayout,
    row_identity: &str,
    initial_break_before: usize,
    selection_order: Rc<Cell<usize>>,
    style: InlineMarkdownStyle,
    cx: &mut App,
) -> AnyElement {
    let markdown = markdown_context.markdown_for(markdown_key, source, cx);
    let selection_context = TranscriptInlineSelectionContext::new_with_initial_break_before(
        code_panel_state.entity(),
        row_identity.to_string(),
        block_path.clone(),
        selection_order,
        initial_break_before,
    );

    render_markdown_plan_with_style_and_selection(
        markdown.render_plan(),
        appearance,
        code_layout,
        style,
        code_panel_state.controls_for(row_identity.to_string(), block_path),
        selection_context,
    )
}

pub(super) fn generated_image_media_item(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    image: &GeneratedImageDetail,
    row_identity: &str,
) -> TranscriptMediaRenderItem {
    let key = TranscriptMediaCacheKey::new(format!(
        "{}:generated-image",
        item_markdown_key(turn_index, turn, image.id.as_str(), "generated-image").as_str()
    ));
    let source = TranscriptMediaSource::native_image_generation(
        image.id.clone(),
        image.revised_prompt.clone(),
        image.result.clone(),
        image.saved_path.clone(),
        image.complete,
    );
    let identity = TranscriptMediaRenderIdentity::new(row_identity, key.clone(), &source);
    TranscriptMediaRenderItem {
        key,
        source,
        identity,
    }
}
