use std::{borrow::Cow, cell::Cell, rc::Rc};

use beryl_model::workspace::WorkspaceId;
use gpui::{AnyElement, App};

use crate::shell::transcript_markdown::{ParsedTranscriptMarkdown, TranscriptMarkdownCacheKey};
use crate::shell::transcript_media::{TranscriptMediaCacheKey, TranscriptMediaSource};
use crate::shell::transcript_media_runs::{TranscriptMediaRunSegment, markdown_media_run_segments};
use crate::shell::transcript_selection::transcript_narrative_block_break_before;

use super::{
    media_blocks::{TranscriptMediaRenderItem, TranscriptMediaRenderLayout, render_media_run},
    media_cache::TranscriptMediaRenderContext,
    selection_context::TranscriptInlineSelectionContext,
};

pub(super) enum TranscriptMarkdownRenderUnit<'a> {
    Markdown {
        key: TranscriptMarkdownCacheKey,
        block_path: String,
        source: Cow<'a, str>,
    },
    Media {
        key: TranscriptMediaCacheKey,
        source: TranscriptMediaSource,
    },
}

pub(super) fn push_rendered_block(
    workspace: &WorkspaceId,
    media_context: TranscriptMediaRenderContext,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<usize>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    rendered: AnyElement,
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
        narrative_copy_block_count.clone(),
        cx,
    );
    narrative_copy_block_count.set(narrative_copy_block_count.get().saturating_add(1));
    narrative_blocks.push(rendered);
}

pub(super) fn flush_media_run(
    workspace: &WorkspaceId,
    media_context: TranscriptMediaRenderContext,
    pending_media: &mut Vec<TranscriptMediaRenderItem>,
    narrative_blocks: &mut Vec<AnyElement>,
    media_layout: TranscriptMediaRenderLayout,
    row_identity: &str,
    selection_order: Rc<Cell<usize>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    cx: &mut App,
) {
    if pending_media.is_empty() {
        return;
    }

    let items = std::mem::take(pending_media);
    let block_index = narrative_copy_block_count.get();
    let selection_context = TranscriptInlineSelectionContext::new_with_initial_break_before(
        media_context.panel(),
        row_identity.to_string(),
        format!("media-run:{block_index}"),
        selection_order,
        transcript_narrative_block_break_before(block_index),
    );
    narrative_blocks.push(render_media_run(
        items.as_slice(),
        media_context,
        workspace,
        media_layout,
        Some(selection_context),
        cx,
    ));
    narrative_copy_block_count.set(block_index.saturating_add(1));
}

pub(super) fn segment_markdown_key(
    key: &TranscriptMarkdownCacheKey,
    segment_index: usize,
) -> TranscriptMarkdownCacheKey {
    TranscriptMarkdownCacheKey::new(format!("{}:segment:{segment_index}", key.as_str()))
}

pub(super) fn segment_media_key(
    key: &TranscriptMarkdownCacheKey,
    segment_index: usize,
) -> TranscriptMediaCacheKey {
    TranscriptMediaCacheKey::new(format!("{}:media:{segment_index}", key.as_str()))
}

pub(super) fn markdown_render_units<'a>(
    key: &TranscriptMarkdownCacheKey,
    block_path: &str,
    markdown: &'a ParsedTranscriptMarkdown,
) -> Vec<TranscriptMarkdownRenderUnit<'a>> {
    if markdown.media_requests().is_empty() {
        return (!markdown.source().is_empty())
            .then(|| TranscriptMarkdownRenderUnit::Markdown {
                key: key.clone(),
                block_path: block_path.to_string(),
                source: Cow::Borrowed(markdown.source()),
            })
            .into_iter()
            .collect();
    }

    let segments = markdown_media_run_segments(markdown);
    if !segments
        .iter()
        .any(|segment| matches!(segment, TranscriptMediaRunSegment::Media(_)))
    {
        return (!markdown.source().is_empty())
            .then(|| TranscriptMarkdownRenderUnit::Markdown {
                key: key.clone(),
                block_path: block_path.to_string(),
                source: Cow::Borrowed(markdown.source()),
            })
            .into_iter()
            .collect();
    }

    segments
        .into_iter()
        .enumerate()
        .map(|(segment_index, segment)| match segment {
            TranscriptMediaRunSegment::Markdown(source) => TranscriptMarkdownRenderUnit::Markdown {
                key: segment_markdown_key(key, segment_index),
                block_path: format!("{block_path}:segment:{segment_index}"),
                source: Cow::Owned(source),
            },
            TranscriptMediaRunSegment::Media(source) => TranscriptMarkdownRenderUnit::Media {
                key: segment_media_key(key, segment_index),
                source,
            },
        })
        .collect()
}
