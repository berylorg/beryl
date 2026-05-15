use gpui::{Pixels, px};

use super::{SCROLLBAR_INSET, SCROLLBAR_MIN_THUMB_LENGTH};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollbarMetrics {
    pub thumb_offset: Pixels,
    pub thumb_length: Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollbarAxisHit {
    LaneBeforeThumb,
    Thumb,
    LaneAfterThumb,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollbarTrackGeometry {
    pub track_start: Pixels,
    pub track_length: Pixels,
    pub thumb_start: Pixels,
    pub thumb_end: Pixels,
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

pub(crate) fn scrollbar_track_geometry(
    viewport_length: Pixels,
    metrics: ScrollbarMetrics,
) -> Option<ScrollbarTrackGeometry> {
    let track_length = (viewport_length - px(SCROLLBAR_INSET * 2.0)).max(px(0.0));
    (track_length > px(0.0)).then_some(ScrollbarTrackGeometry {
        track_start: px(SCROLLBAR_INSET),
        track_length,
        thumb_start: px(SCROLLBAR_INSET) + metrics.thumb_offset,
        thumb_end: px(SCROLLBAR_INSET) + metrics.thumb_offset + metrics.thumb_length,
    })
}

pub(crate) fn classify_scrollbar_axis_hit(
    viewport_length: Pixels,
    metrics: ScrollbarMetrics,
    axis_position: Pixels,
) -> Option<ScrollbarAxisHit> {
    let geometry = scrollbar_track_geometry(viewport_length, metrics)?;
    if axis_position < geometry.thumb_start {
        Some(ScrollbarAxisHit::LaneBeforeThumb)
    } else if axis_position <= geometry.thumb_end {
        Some(ScrollbarAxisHit::Thumb)
    } else {
        Some(ScrollbarAxisHit::LaneAfterThumb)
    }
}

pub(crate) fn scrollbar_thumb_grab_offset(
    viewport_length: Pixels,
    metrics: ScrollbarMetrics,
    axis_position: Pixels,
) -> Option<Pixels> {
    let geometry = scrollbar_track_geometry(viewport_length, metrics)?;
    Some((axis_position - geometry.thumb_start).clamp(px(0.0), metrics.thumb_length))
}

pub(crate) fn scroll_offset_from_thumb_drag(
    viewport_length: Pixels,
    overflow_length: Pixels,
    metrics: ScrollbarMetrics,
    pointer_axis_position: Pixels,
    thumb_grab_offset: Pixels,
) -> Option<Pixels> {
    let geometry = scrollbar_track_geometry(viewport_length, metrics)?;
    let thumb_travel = (geometry.track_length - metrics.thumb_length).max(px(0.0));
    if thumb_travel <= px(0.0) {
        return Some(px(0.0));
    }

    let desired_thumb_offset = (pointer_axis_position - geometry.track_start - thumb_grab_offset)
        .clamp(px(0.0), thumb_travel);
    Some(overflow_length * (desired_thumb_offset / thumb_travel))
}
