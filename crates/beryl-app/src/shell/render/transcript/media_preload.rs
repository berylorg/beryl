use std::{sync::Arc, time::Instant};

use beryl_model::workspace::WorkspaceId;
use gpui::{App, Window};

use crate::shell::execution_detail::{ExecutionItem, TurnExecutionRecord, TurnNarrativeEntry};
use crate::shell::transcript_media_runs::{TranscriptMediaRunSegment, markdown_media_run_segments};

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

pub(super) fn preload_turn_media_runs(
    turn_index: usize,
    workspace: &WorkspaceId,
    turn: Arc<TurnExecutionRecord>,
    row_identity: &str,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    media_layout: TranscriptMediaRenderLayout,
    window: &mut Window,
    cx: &mut App,
) {
    let mut pending_media = Vec::new();

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
                if !fragment.image_markers().is_empty() {
                    flush_preload_media_run(
                        workspace,
                        media_context.clone(),
                        &mut pending_media,
                        media_layout,
                        window,
                        cx,
                    );
                    continue;
                }

                let block_path = user_prompt_block_path(fragment_index);
                let markdown_key = turn_markdown_key(turn_index, turn.as_ref(), &block_path);
                let markdown =
                    markdown_context.markdown_for(markdown_key.clone(), fragment.text.as_str(), cx);
                if markdown.media_requests().is_empty() {
                    flush_preload_media_run(
                        workspace,
                        media_context.clone(),
                        &mut pending_media,
                        media_layout,
                        window,
                        cx,
                    );
                    continue;
                }
                for (segment_index, segment) in markdown_media_run_segments(markdown.as_ref())
                    .into_iter()
                    .enumerate()
                {
                    match segment {
                        TranscriptMediaRunSegment::Markdown(_) => flush_preload_media_run(
                            workspace,
                            media_context.clone(),
                            &mut pending_media,
                            media_layout,
                            window,
                            cx,
                        ),
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
                }
            }
            TurnNarrativeEntry::Item { item_id } => {
                let Some(item) = turn.item_by_id(item_id) else {
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
                        let markdown = markdown_context.markdown_for(
                            markdown_key.clone(),
                            source.as_ref(),
                            cx,
                        );
                        if markdown.media_requests().is_empty() {
                            flush_preload_media_run(
                                workspace,
                                media_context.clone(),
                                &mut pending_media,
                                media_layout,
                                window,
                                cx,
                            );
                            continue;
                        }
                        let segments = markdown_media_run_segments(markdown.as_ref());
                        if !segments
                            .iter()
                            .any(|segment| matches!(segment, TranscriptMediaRunSegment::Media(_)))
                        {
                            flush_preload_media_run(
                                workspace,
                                media_context.clone(),
                                &mut pending_media,
                                media_layout,
                                window,
                                cx,
                            );
                            continue;
                        }
                        for (segment_index, segment) in segments.into_iter().enumerate() {
                            match segment {
                                TranscriptMediaRunSegment::Markdown(_) => flush_preload_media_run(
                                    workspace,
                                    media_context.clone(),
                                    &mut pending_media,
                                    media_layout,
                                    window,
                                    cx,
                                ),
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
                        }
                    }
                    ExecutionItem::Reasoning(_) => flush_preload_media_run(
                        workspace,
                        media_context.clone(),
                        &mut pending_media,
                        media_layout,
                        window,
                        cx,
                    ),
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
        window,
        cx,
    );
}

fn flush_preload_media_run(
    workspace: &WorkspaceId,
    media_context: TranscriptMediaRenderContext,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    media_layout: TranscriptMediaRenderLayout,
    window: &mut Window,
    cx: &mut App,
) {
    if pending_media.is_empty() {
        return;
    }

    let items = std::mem::take(pending_media);
    preload_media_run(
        items.as_slice(),
        media_context,
        workspace,
        media_layout,
        window,
        cx,
    );
}
