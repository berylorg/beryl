use std::{cell::Cell, collections::HashSet, rc::Rc, sync::Arc};

use beryl_model::workspace::WorkspaceId;
use gpui::{App, div, prelude::*, px, rgb};

use crate::AppearanceSettings;
use crate::shell::execution_detail::{TurnExecutionRecord, TurnNarrativeEntry};
use crate::shell::transcript_markdown::markdown_code_panel_ids;

use super::{
    TranscriptCodeLayout, markdown_cache::TranscriptMarkdownRenderContext,
    media_cache::TranscriptMediaRenderContext,
    stream_projection::TranscriptStreamProjectionContext, turn_markdown_key,
};
use super::{
    code_panel_controls::TranscriptCodePanelState,
    image_markdown::markdown_source_with_image_marker_placeholders,
    item_blocks::collect_item_markdown_code_panel_ids, media_blocks::TranscriptMediaRenderLayout,
    turn_item_media_units::render_item_units, turn_media_units::flush_media_run,
    turn_user_media_units::render_user_prompt_units,
};

pub(super) fn render_turn_card(
    turn_index: usize,
    workspace: &WorkspaceId,
    appearance: Arc<AppearanceSettings>,
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
                    appearance.as_ref(),
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
                    appearance.clone(),
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
            this.child(render_activity_caret(activity_caret_opacity))
        })
        .into_any_element()
}

pub(super) fn collect_turn_card_markdown_code_panel_ids(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    row_identity: &str,
    markdown_context: TranscriptMarkdownRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    cx: &mut App,
) -> HashSet<String> {
    let mut ids = HashSet::new();

    for entry in turn.narrative_entries() {
        match entry {
            TurnNarrativeEntry::UserInput { fragment_id } => {
                let Some((fragment_index, fragment)) = turn.user_input_fragment_by_id(*fragment_id)
                else {
                    continue;
                };
                if fragment.text.is_empty() {
                    continue;
                }

                let block_path = user_prompt_block_path(fragment_index);
                let markdown_key = turn_markdown_key(turn_index, turn, &block_path);
                let markdown_source = markdown_source_with_image_marker_placeholders(
                    fragment.text.as_str(),
                    fragment.image_markers(),
                );
                let markdown = markdown_context.markdown_for(markdown_key, &markdown_source, cx);
                ids.extend(markdown_code_panel_ids(
                    row_identity,
                    block_path.as_str(),
                    markdown.render_plan(),
                ));
            }
            TurnNarrativeEntry::Item { item_id } => {
                let Some(item) = turn.item_by_id(item_id) else {
                    continue;
                };
                collect_item_markdown_code_panel_ids(
                    turn_index,
                    turn,
                    item,
                    markdown_context.clone(),
                    stream_projection_context.clone(),
                    row_identity,
                    &mut ids,
                    cx,
                );
            }
        }
    }

    ids
}

pub(super) fn user_prompt_block_path(fragment_index: usize) -> String {
    format!("user-prompt:{fragment_index}")
}

fn render_activity_caret(opacity: f32) -> impl IntoElement {
    div()
        .w(px(9.0))
        .h(px(18.0))
        .flex_none()
        .opacity(opacity.clamp(0.0, 1.0))
        .bg(rgb(0x2563eb))
}
