use std::sync::Arc;

use beryl_model::workspace::WorkspaceId;
use gpui::{
    AnyElement, App, MouseButton, ObjectFit, Pixels, div, img, prelude::*, px, relative, rgb,
};

use crate::diagnostic_dynamic_tools::VisibleMediaItemDiagnostic;
use crate::shell::transcript_branch_menu_state::TranscriptImageMenuTarget;
use crate::shell::transcript_media::{
    TranscriptMediaCacheKey, TranscriptMediaLoadOutcome, TranscriptMediaRunAlignment,
    TranscriptMediaSize, TranscriptMediaSizingInput, TranscriptMediaSlotLayout,
    TranscriptMediaSource, transcript_media_run_alignment, transcript_media_slot_layout,
};
use crate::shell::transcript_media_runs::{TranscriptMediaRunCopyLine, media_run_copy_line};
use crate::shell::transcript_selection::TranscriptLineCopyText;

use super::TranscriptMediaRenderIdentity;
use super::media_cache::TranscriptMediaRenderContext;
use super::selection_context::{TranscriptInlineSelectionContext, TranscriptSelectableTextLine};

#[derive(Clone, Debug)]
pub(super) struct TranscriptMediaRenderItem {
    pub(super) key: TranscriptMediaCacheKey,
    pub(super) source: TranscriptMediaSource,
    pub(super) identity: TranscriptMediaRenderIdentity,
}

#[derive(Clone, Copy)]
pub(super) struct TranscriptMediaRenderLayout {
    pub(super) padded_content_width: Pixels,
    pub(super) conversation_m_advance: Pixels,
    pub(super) window_scale: f32,
}

pub(super) fn render_media_run(
    items: &[TranscriptMediaRenderItem],
    context: TranscriptMediaRenderContext,
    execution_target: &WorkspaceId,
    layout: TranscriptMediaRenderLayout,
    selection_context: Option<TranscriptInlineSelectionContext>,
    cx: &mut App,
) -> AnyElement {
    let resolved_items = items
        .iter()
        .map(|item| ResolvedTranscriptMediaRenderItem {
            source: item.source.clone(),
            identity: item.identity.clone(),
            outcome: context.media_for(
                item.key.clone(),
                item.source.clone(),
                execution_target.clone(),
                cx,
            ),
        })
        .collect::<Vec<_>>();
    for item in &resolved_items {
        context.promotion().note_identity_rendered(&item.identity);
    }
    let selectable_line = media_selectable_line(
        selection_context,
        media_run_copy_line(
            resolved_items
                .iter()
                .map(|item| (&item.source, Some(item.outcome.as_ref()))),
        ),
    );

    let promoted_index = promoted_media_index(&resolved_items, context.promotion().promoted());

    if let Some(promoted_index) = promoted_index {
        return render_promoted_media_run(&resolved_items, layout, promoted_index, context)
            .when_some(selectable_line, register_media_selection)
            .into_any_element();
    }

    if resolved_items
        .iter()
        .any(|item| item.outcome.fallback_text().is_some())
    {
        return render_mixed_media_run(&resolved_items, layout, context)
            .when_some(selectable_line, register_media_selection)
            .into_any_element();
    }

    render_media_tile_group(
        &resolved_items,
        layout,
        resolved_items.len(),
        resolved_items.len() > 1,
        context,
    )
    .when_some(selectable_line, register_media_selection)
    .into_any_element()
}

#[derive(Clone)]
struct ResolvedTranscriptMediaRenderItem {
    source: TranscriptMediaSource,
    identity: TranscriptMediaRenderIdentity,
    outcome: Arc<TranscriptMediaLoadOutcome>,
}

fn render_media_item(
    item: Option<&ResolvedTranscriptMediaRenderItem>,
    sizing: TranscriptMediaSizingInput,
    context: TranscriptMediaRenderContext,
    promotable: bool,
) -> AnyElement {
    let outcome = item.map(|item| item.outcome.as_ref());
    let TranscriptMediaSlotLayout::Media(TranscriptMediaSize { width, height }) =
        transcript_media_slot_layout(sizing, outcome)
    else {
        let fallback = outcome
            .and_then(TranscriptMediaLoadOutcome::fallback_text)
            .unwrap_or_else(|| "image (file unavailable)".to_string());
        return render_media_fallback_row(fallback);
    };

    if let Some(item) = item {
        context
            .visible_media()
            .borrow_mut()
            .record_item(visible_media_diagnostic_item(item, outcome, width, height));
    }

    let tile = div()
        .w(width)
        .h(height)
        .max_w(relative(1.0))
        .rounded_sm()
        .border_1()
        .border_color(rgb(0x334155))
        .bg(rgb(0x020617))
        .relative()
        .overflow_hidden()
        .flex_none();

    let loaded_target = match (item, outcome) {
        (Some(item), Some(TranscriptMediaLoadOutcome::Loaded(image))) => Some((
            item.identity.clone(),
            TranscriptImageMenuTarget::new(
                item.identity.row_identity().to_string(),
                item.identity.image_menu_identity(),
                image.alt().to_string(),
                image.format(),
                image.bytes_arc(),
                image.image(),
                image.source_path().map(str::to_string),
            ),
        )),
        _ => None,
    };

    if let Some((_, image_target)) = loaded_target.as_ref() {
        context
            .image_menu()
            .note_loaded_image_rendered(image_target);
    }

    let mut tile = tile.child(match outcome {
        Some(TranscriptMediaLoadOutcome::Loaded(image)) => img(image.image())
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        Some(outcome) => render_media_status(
            outcome
                .fallback_text()
                .unwrap_or_else(|| pending_media_text(outcome)),
        ),
        None => render_media_status("image (file unavailable)".to_string()),
    });

    if let Some((identity, image_target)) = loaded_target.clone() {
        tile = tile.on_mouse_down(MouseButton::Right, {
            let panel = context.panel();
            move |event, window, cx| {
                let handled = panel.update(cx, |view, cx| {
                    view.handle_transcript_image_context_mouse_down(
                        identity.row_identity(),
                        image_target.clone(),
                        event,
                        window,
                        cx,
                    )
                });
                if handled {
                    cx.stop_propagation();
                }
            }
        });
    }

    if promotable && let Some((identity, _)) = loaded_target {
        tile = tile.cursor_pointer().on_mouse_down(MouseButton::Left, {
            let panel = context.panel();
            move |_, window, cx| {
                panel.update(cx, |view, cx| {
                    window.focus(&view.focus_handle);
                    view.toggle_promoted_media(identity.clone(), cx);
                });
                cx.stop_propagation();
            }
        });
    }

    tile.into_any_element()
}

fn render_mixed_media_run(
    resolved_items: &[ResolvedTranscriptMediaRenderItem],
    layout: TranscriptMediaRenderLayout,
    context: TranscriptMediaRenderContext,
) -> gpui::Div {
    let mut row = div().w_full().min_w(px(0.0)).flex().flex_col().gap_2();
    let mut media_start = None;
    let source_run_length = resolved_items.len();

    for (index, item) in resolved_items.iter().enumerate() {
        if let Some(fallback) = item.outcome.fallback_text() {
            if let Some(start) = media_start.take() {
                row = row.child(render_media_tile_group(
                    &resolved_items[start..index],
                    layout,
                    source_run_length,
                    source_run_length > 1,
                    context.clone(),
                ));
            }
            row = row.child(render_media_fallback_row(fallback));
        } else if media_start.is_none() {
            media_start = Some(index);
        }
    }

    if let Some(start) = media_start {
        row = row.child(render_media_tile_group(
            &resolved_items[start..],
            layout,
            source_run_length,
            source_run_length > 1,
            context,
        ));
    }

    row
}

fn promoted_media_index(
    resolved_items: &[ResolvedTranscriptMediaRenderItem],
    promoted: Option<&TranscriptMediaRenderIdentity>,
) -> Option<usize> {
    if resolved_items.len() <= 1 {
        return None;
    }
    let promoted = promoted?;
    resolved_items.iter().position(|item| {
        item.identity == *promoted
            && matches!(item.outcome.as_ref(), TranscriptMediaLoadOutcome::Loaded(_))
    })
}

fn render_promoted_media_run(
    resolved_items: &[ResolvedTranscriptMediaRenderItem],
    layout: TranscriptMediaRenderLayout,
    promoted_index: usize,
    context: TranscriptMediaRenderContext,
) -> gpui::Div {
    let mut row = div().w_full().min_w(px(0.0)).flex().flex_col().gap_2();
    let mut compact_start = None;
    let source_run_length = resolved_items.len();

    for (index, item) in resolved_items.iter().enumerate() {
        if index == promoted_index {
            if let Some(start) = compact_start.take() {
                row = row.child(render_media_tile_group(
                    &resolved_items[start..index],
                    layout,
                    source_run_length,
                    true,
                    context.clone(),
                ));
            }
            row = row.child(render_media_tile_group(
                &resolved_items[index..=index],
                layout,
                1,
                true,
                context.clone(),
            ));
        } else if let Some(fallback) = item.outcome.fallback_text() {
            if let Some(start) = compact_start.take() {
                row = row.child(render_media_tile_group(
                    &resolved_items[start..index],
                    layout,
                    source_run_length,
                    true,
                    context.clone(),
                ));
            }
            row = row.child(render_media_fallback_row(fallback));
        } else if compact_start.is_none() {
            compact_start = Some(index);
        }
    }

    if let Some(start) = compact_start {
        row = row.child(render_media_tile_group(
            &resolved_items[start..],
            layout,
            source_run_length,
            true,
            context,
        ));
    }

    row
}

fn render_media_tile_group(
    resolved_items: &[ResolvedTranscriptMediaRenderItem],
    layout: TranscriptMediaRenderLayout,
    sizing_run_length: usize,
    promotable: bool,
    context: TranscriptMediaRenderContext,
) -> gpui::Div {
    let mut row = div().w_full().min_w(px(0.0));
    match transcript_media_run_alignment(sizing_run_length) {
        TranscriptMediaRunAlignment::Center => {
            row = row.flex().justify_center();
        }
        TranscriptMediaRunAlignment::Start if resolved_items.len() > 1 => {
            row = row.flex().flex_wrap().gap_2();
        }
        TranscriptMediaRunAlignment::Start => {}
    }

    for item in resolved_items {
        row = row.child(render_media_item(
            Some(item),
            TranscriptMediaSizingInput {
                run_length: sizing_run_length,
                padded_content_width: layout.padded_content_width,
                conversation_m_advance: layout.conversation_m_advance,
                natural_dimensions: item
                    .outcome
                    .loaded()
                    .map(|image| image.natural_dimensions()),
                window_scale: layout.window_scale,
            },
            context.clone(),
            promotable,
        ));
    }

    if resolved_items.is_empty() {
        row = row.child(render_media_item(
            None,
            TranscriptMediaSizingInput {
                run_length: sizing_run_length,
                padded_content_width: layout.padded_content_width,
                conversation_m_advance: layout.conversation_m_advance,
                natural_dimensions: None,
                window_scale: layout.window_scale,
            },
            context,
            false,
        ));
    }

    row
}

fn media_selectable_line(
    selection_context: Option<TranscriptInlineSelectionContext>,
    copy_line: Option<TranscriptMediaRunCopyLine>,
) -> Option<TranscriptSelectableTextLine> {
    let selection_context = selection_context?;
    let copy_line = copy_line?;
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_atomic_run(copy_line.display_text.clone(), copy_line.copy_text);
    Some(selection_context.selectable_line(
        copy_line.display_text.clone(),
        copy_line.display_text.len(),
        copy_text,
    ))
}

fn register_media_selection(
    row: gpui::Div,
    selectable_line: TranscriptSelectableTextLine,
) -> gpui::Div {
    row.on_children_prepainted(move |_, _, cx| {
        register_selectable_media_line(selectable_line.clone(), cx);
    })
}

fn register_selectable_media_line(selectable_line: TranscriptSelectableTextLine, cx: &mut App) {
    selectable_line.entity.clone().update(cx, |view, _| {
        view.register_selectable_copy_line(selectable_line);
    });
}

fn render_media_status(text: String) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .px_3()
        .text_sm()
        .text_color(rgb(0x94a3b8))
        .child(text)
        .into_any_element()
}

fn render_media_fallback_row(text: String) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .py_1()
        .text_sm()
        .text_color(rgb(0x94a3b8))
        .child(text)
        .into_any_element()
}

fn pending_media_text(outcome: &TranscriptMediaLoadOutcome) -> String {
    match outcome {
        TranscriptMediaLoadOutcome::Pending { alt } => alt.to_string(),
        _ => "image".to_string(),
    }
}

fn visible_media_diagnostic_item(
    item: &ResolvedTranscriptMediaRenderItem,
    outcome: Option<&TranscriptMediaLoadOutcome>,
    displayed_width: gpui::Pixels,
    displayed_height: gpui::Pixels,
) -> VisibleMediaItemDiagnostic {
    let loaded = outcome.and_then(TranscriptMediaLoadOutcome::loaded);
    let dimensions = loaded.map(|image| image.natural_dimensions());
    VisibleMediaItemDiagnostic {
        row_identity: Some(item.identity.row_identity().to_string()),
        key: item.identity.key().as_str().to_string(),
        source_kind: transcript_media_source_kind(&item.source).to_string(),
        outcome: transcript_media_outcome_label(outcome).to_string(),
        format: loaded.map(|image| image_format_label(image.format()).to_string()),
        compressed_bytes: loaded.map(|image| image.bytes().len()),
        decoded_bytes_estimate: dimensions.map(|dimensions| {
            (dimensions.width() as usize)
                .saturating_mul(dimensions.height() as usize)
                .saturating_mul(4)
        }),
        natural_width: dimensions.map(|dimensions| dimensions.width()),
        natural_height: dimensions.map(|dimensions| dimensions.height()),
        displayed_width: f64::from(displayed_width),
        displayed_height: f64::from(displayed_height),
        image_id: loaded.map(|image| image.image_id()),
        image_asset_key_hash: loaded.map(|image| image.image_asset_key_hash()),
    }
}

fn transcript_media_source_kind(source: &TranscriptMediaSource) -> &'static str {
    match source {
        TranscriptMediaSource::MarkdownImage { .. } => "markdown_image",
        TranscriptMediaSource::NativeImageGeneration { .. } => "native_generated_image",
    }
}

fn transcript_media_outcome_label(outcome: Option<&TranscriptMediaLoadOutcome>) -> &'static str {
    match outcome {
        Some(TranscriptMediaLoadOutcome::Pending { .. }) => "pending",
        Some(TranscriptMediaLoadOutcome::Loaded(_)) => "loaded",
        Some(TranscriptMediaLoadOutcome::RenderNotSupported { .. }) => "render_not_supported",
        Some(TranscriptMediaLoadOutcome::TooLarge { .. }) => "too_large",
        Some(TranscriptMediaLoadOutcome::FileUnavailable { .. }) => "file_unavailable",
        Some(TranscriptMediaLoadOutcome::PathNotAllowed { .. }) => "path_not_allowed",
        None => "missing",
    }
}

fn image_format_label(format: gpui::ImageFormat) -> &'static str {
    match format {
        gpui::ImageFormat::Png => "png",
        gpui::ImageFormat::Jpeg => "jpeg",
        gpui::ImageFormat::Webp => "webp",
        gpui::ImageFormat::Gif => "gif",
        gpui::ImageFormat::Svg => "svg",
        gpui::ImageFormat::Bmp => "bmp",
        gpui::ImageFormat::Tiff => "tiff",
    }
}
