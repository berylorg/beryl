use std::{cell::Cell, rc::Rc, sync::Arc};

use beryl_model::workspace::WorkspaceId;
use gpui::{App, div, prelude::*, px};

use super::{
    TranscriptCodeLayout, TranscriptTheme, markdown_cache::TranscriptMarkdownRenderContext,
    media_cache::TranscriptMediaRenderContext,
    stream_projection::TranscriptStreamProjectionContext,
};
use super::{
    code_panel_controls::TranscriptCodePanelState, media_blocks::TranscriptMediaRenderLayout,
    turn_item_media_units::render_item_units, turn_media_units::flush_media_run,
    turn_user_media_units::render_user_prompt_units,
};
use crate::shell::execution_detail::{TurnExecutionRecord, TurnNarrativeEntry};

pub(super) fn render_turn_card(
    turn_index: usize,
    workspace: &WorkspaceId,
    theme: Arc<TranscriptTheme>,
    turn: Arc<TurnExecutionRecord>,
    code_panel_state: TranscriptCodePanelState,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<usize>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    show_activity_caret: bool,
    activity_caret_opacity: f32,
    cx: &mut App,
) -> impl IntoElement {
    let mut narrative_blocks = Vec::new();
    let mut pending_media = Vec::new();
    for entry in turn.narrative_entries() {
        match entry {
            TurnNarrativeEntry::UserInput { fragment_id } => {
                let Some((fragment_index, fragment)) = turn.user_input_fragment_by_id(*fragment_id)
                else {
                    continue;
                };
                render_user_prompt_units(
                    turn_index,
                    workspace,
                    turn.as_ref(),
                    fragment_index,
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
            TurnNarrativeEntry::Item { item_id } => {
                let Some(item) = turn.item_by_id(item_id) else {
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

pub(super) fn user_prompt_block_path(fragment_index: usize) -> String {
    format!("user-prompt:{fragment_index}")
}

fn render_activity_caret(opacity: f32, theme: &TranscriptTheme) -> impl IntoElement {
    div()
        .w(px(9.0))
        .h(px(18.0))
        .flex_none()
        .opacity(opacity.clamp(0.0, 1.0))
        .bg(theme.activity_caret.background)
}
