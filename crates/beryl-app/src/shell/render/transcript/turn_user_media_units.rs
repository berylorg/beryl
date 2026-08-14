use std::{cell::Cell, rc::Rc};

use beryl_model::workspace::WorkspaceId;
use gpui::{AnyElement, App, IntoElement};

use crate::shell::execution_detail::{TurnExecutionRecord, UserInputFragment};
use crate::shell::transcript_selection::transcript_narrative_block_break_before;

use super::{
    TranscriptCodeLayout, TranscriptTextRole, TranscriptTheme,
    markdown_cache::TranscriptMarkdownRenderContext, media_cache::TranscriptMediaRenderContext,
    turn_blocks::user_prompt_block_path, turn_markdown_key,
};
use super::{
    TranscriptMediaRenderIdentity,
    block_markdown::{
        markdown_prose_block_with_image_markers_and_selection, markdown_prose_block_with_selection,
    },
    code_panel_controls::TranscriptCodePanelState,
    image_markdown::markdown_source_with_image_marker_placeholders,
    inline_markdown::{
        InlineMarkdownStyle, TranscriptInlineImageMarker, TranscriptInlineSelectionContext,
    },
    media_blocks::{TranscriptMediaRenderItem, TranscriptMediaRenderLayout},
    turn_media_units::{TranscriptMarkdownRenderUnit, markdown_render_units, push_rendered_block},
};

pub(super) fn render_user_prompt_units(
    turn_index: usize,
    workspace: &WorkspaceId,
    turn: &TurnExecutionRecord,
    fragment_index: usize,
    fragment: &UserInputFragment,
    theme: &TranscriptTheme,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<usize>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    cx: &mut App,
) {
    if fragment.text.is_empty() {
        return;
    }

    if !fragment.image_markers().is_empty() {
        let initial_break_before =
            transcript_narrative_block_break_before(narrative_copy_block_count.get());
        if let Some(rendered) = render_user_prompt(
            turn_index,
            turn,
            fragment_index,
            fragment,
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

    let block_path = user_prompt_block_path(fragment_index);
    let markdown_key = turn_markdown_key(turn_index, turn, &block_path);
    let markdown = markdown_context.markdown_for(markdown_key.clone(), fragment.text.as_str(), cx);
    for unit in markdown_render_units(&markdown_key, block_path.as_str(), markdown.as_ref()) {
        match unit {
            TranscriptMarkdownRenderUnit::Markdown {
                key,
                block_path,
                source,
            } => {
                let initial_break_before =
                    transcript_narrative_block_break_before(narrative_copy_block_count.get());
                let rendered = render_user_prompt_markdown_source(
                    source.as_ref(),
                    key,
                    block_path,
                    theme,
                    code_panel_state.clone(),
                    markdown_context.clone(),
                    code_layout,
                    media_layout.conversation_m_advance,
                    row_identity,
                    initial_break_before,
                    selection_order.clone(),
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
            TranscriptMarkdownRenderUnit::Media { key, source } => {
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

fn render_user_prompt_markdown_source(
    source: &str,
    markdown_key: crate::shell::transcript_markdown::TranscriptMarkdownCacheKey,
    block_path: String,
    theme: &TranscriptTheme,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: gpui::Pixels,
    row_identity: &str,
    initial_break_before: usize,
    selection_order: Rc<Cell<usize>>,
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
    markdown_prose_block_with_selection(
        "",
        markdown.render_plan(),
        theme.user_input.background(),
        theme,
        code_layout,
        conversation_m_advance,
        InlineMarkdownStyle::base(TranscriptTextRole::UserInput),
        code_panel_state.controls_for(row_identity.to_string(), block_path),
        selection_context,
        cx,
    )
}

fn render_user_prompt(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    fragment_index: usize,
    fragment: &UserInputFragment,
    theme: &TranscriptTheme,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: gpui::Pixels,
    row_identity: &str,
    initial_break_before: usize,
    selection_order: Rc<Cell<usize>>,
    cx: &mut App,
) -> Option<AnyElement> {
    if fragment.text.is_empty() {
        return None;
    }

    let block_path = user_prompt_block_path(fragment_index);
    let markdown_key = turn_markdown_key(turn_index, turn, &block_path);
    let markdown_source = markdown_source_with_image_marker_placeholders(
        fragment.text.as_str(),
        fragment.image_markers(),
    );
    let markdown = markdown_context.markdown_for(markdown_key, &markdown_source, cx);
    let selection_context = TranscriptInlineSelectionContext::new_with_initial_break_before(
        code_panel_state.entity(),
        row_identity.to_string(),
        block_path.clone(),
        selection_order,
        initial_break_before,
    );
    let image_markers = fragment
        .image_markers()
        .iter()
        .map(TranscriptInlineImageMarker::from_transcript_marker)
        .collect::<Vec<_>>();
    let block = if image_markers.is_empty() {
        markdown_prose_block_with_selection(
            "",
            markdown.render_plan(),
            theme.user_input.background(),
            theme,
            code_layout,
            conversation_m_advance,
            InlineMarkdownStyle::base(TranscriptTextRole::UserInput),
            code_panel_state.controls_for(row_identity.to_string(), block_path),
            selection_context,
            cx,
        )
    } else {
        markdown_prose_block_with_image_markers_and_selection(
            "",
            markdown.render_plan(),
            theme.user_input.background(),
            theme,
            code_layout,
            conversation_m_advance,
            InlineMarkdownStyle::base(TranscriptTextRole::UserInput),
            code_panel_state.controls_for(row_identity.to_string(), block_path),
            selection_context,
            image_markers.as_slice(),
            cx,
        )
    };
    Some(block.into_any_element())
}
