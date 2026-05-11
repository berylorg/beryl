use gpui::{AnyElement, Pixels, ScrollHandle, div, prelude::*, px, rgb};

const SCROLLBAR_THICKNESS: f32 = 4.0;
const SCROLLBAR_INSET: f32 = 6.0;
const SCROLLBAR_MIN_THUMB_LENGTH: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScrollbarAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollbarMetrics {
    pub thumb_offset: Pixels,
    pub thumb_length: Pixels,
}

pub(crate) fn scrollbar_metrics(
    viewport_length: Pixels,
    overflow_length: Pixels,
    scroll_offset: Pixels,
) -> Option<ScrollbarMetrics> {
    if viewport_length <= px(0.0) || overflow_length <= px(0.0) {
        return None;
    }

    let track_length = (viewport_length - px(SCROLLBAR_INSET * 2.0)).max(px(0.0));
    if track_length <= px(0.0) {
        return None;
    }

    let content_length = viewport_length + overflow_length;
    let thumb_length = (track_length * (viewport_length / content_length))
        .max(px(SCROLLBAR_MIN_THUMB_LENGTH))
        .min(track_length);
    let thumb_travel = (track_length - thumb_length).max(px(0.0));
    let progress = if overflow_length <= px(0.0) {
        0.0
    } else {
        (scroll_offset.clamp(px(0.0), overflow_length) / overflow_length).clamp(0.0, 1.0)
    };

    Some(ScrollbarMetrics {
        thumb_offset: thumb_travel * progress,
        thumb_length,
    })
}

pub(super) fn render_div_scrollbar(
    scroll_handle: &ScrollHandle,
    axis: ScrollbarAxis,
    opacity: f32,
) -> Option<AnyElement> {
    if opacity <= 0.0 {
        return None;
    }
    let bounds = scroll_handle.bounds();
    let max_offset = scroll_handle.max_offset();
    let offset = scroll_handle.offset();
    let metrics = match axis {
        ScrollbarAxis::Horizontal => {
            scrollbar_metrics(bounds.size.width, max_offset.width, -offset.x)?
        }
        ScrollbarAxis::Vertical => {
            scrollbar_metrics(bounds.size.height, max_offset.height, -offset.y)?
        }
    };
    Some(render_scrollbar(axis, metrics, opacity).into_any_element())
}

pub(super) fn render_vertical_scrollbar(
    viewport_length: Pixels,
    overflow_length: Pixels,
    scroll_offset: Pixels,
    opacity: f32,
) -> Option<AnyElement> {
    if opacity <= 0.0 {
        return None;
    }
    let metrics = scrollbar_metrics(viewport_length, overflow_length, scroll_offset)?;
    Some(render_scrollbar(ScrollbarAxis::Vertical, metrics, opacity).into_any_element())
}

fn render_scrollbar(
    axis: ScrollbarAxis,
    metrics: ScrollbarMetrics,
    opacity: f32,
) -> impl IntoElement {
    match axis {
        ScrollbarAxis::Horizontal => div()
            .absolute()
            .left(px(SCROLLBAR_INSET) + metrics.thumb_offset)
            .bottom(px(SCROLLBAR_INSET))
            .h(px(SCROLLBAR_THICKNESS))
            .w(metrics.thumb_length)
            .rounded_full()
            .bg(rgb(0x94a3b8))
            .opacity(opacity),
        ScrollbarAxis::Vertical => div()
            .absolute()
            .top(px(SCROLLBAR_INSET) + metrics.thumb_offset)
            .right(px(SCROLLBAR_INSET))
            .w(px(SCROLLBAR_THICKNESS))
            .h(metrics.thumb_length)
            .rounded_full()
            .bg(rgb(0x94a3b8))
            .opacity(opacity),
    }
}
