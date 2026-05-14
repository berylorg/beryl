use gpui::{DevicePixels, Pixels, Size, px, size};

use super::types::TranscriptMediaLoadOutcome;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptMediaNaturalDimensions {
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TranscriptMediaSize {
    pub(crate) width: Pixels,
    pub(crate) height: Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TranscriptMediaSizingInput {
    pub(crate) run_length: usize,
    pub(crate) padded_content_width: Pixels,
    pub(crate) conversation_m_advance: Pixels,
    pub(crate) natural_dimensions: Option<TranscriptMediaNaturalDimensions>,
    pub(crate) window_scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TranscriptMediaLayoutInput {
    pub(crate) transcript_width: Pixels,
    pub(crate) row_horizontal_padding: Pixels,
    pub(crate) conversation_m_advance: Pixels,
    pub(crate) window_scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TranscriptMediaLayoutMetrics {
    pub(crate) padded_content_width: Pixels,
    pub(crate) conversation_m_advance: Pixels,
    pub(crate) window_scale: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TranscriptMediaSlotLayout {
    Media(TranscriptMediaSize),
    TextFallback(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptMediaRunAlignment {
    Start,
    Center,
}

const MULTI_IMAGE_TARGET_M_ADVANCES: f32 = 30.0;
const MEDIA_TILE_BORDER_WIDTH: Pixels = px(1.0);

impl TranscriptMediaNaturalDimensions {
    pub(crate) fn new(width: u32, height: u32) -> Option<Self> {
        (width > 0 && height > 0).then_some(Self { width, height })
    }

    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }
}

pub(crate) fn transcript_media_size(input: TranscriptMediaSizingInput) -> TranscriptMediaSize {
    let content_width = input.padded_content_width.max(px(0.0));
    let target_width = if input.run_length <= 1 {
        content_width
    } else {
        content_width.min(multi_image_target_width(input.conversation_m_advance))
    };

    let Some(natural_dimensions) = input.natural_dimensions else {
        return square_placeholder(target_width);
    };

    let natural_width = scale_adjusted_natural_width(natural_dimensions, input.window_scale);
    let width = target_width.min(natural_width).max(px(0.0));
    let aspect_height = natural_dimensions.height as f32 / natural_dimensions.width as f32;

    TranscriptMediaSize {
        width,
        height: (width * aspect_height).max(px(0.0)),
    }
}

pub(crate) fn transcript_media_layout_metrics(
    input: TranscriptMediaLayoutInput,
) -> TranscriptMediaLayoutMetrics {
    TranscriptMediaLayoutMetrics {
        padded_content_width: (input.transcript_width - input.row_horizontal_padding).max(px(0.0)),
        conversation_m_advance: input.conversation_m_advance.max(px(1.0)),
        window_scale: input.window_scale,
    }
}

pub(crate) fn transcript_media_slot_layout(
    input: TranscriptMediaSizingInput,
    outcome: Option<&TranscriptMediaLoadOutcome>,
) -> TranscriptMediaSlotLayout {
    if let Some(fallback) = outcome.and_then(TranscriptMediaLoadOutcome::fallback_text) {
        return TranscriptMediaSlotLayout::TextFallback(fallback);
    }

    TranscriptMediaSlotLayout::Media(transcript_media_size(input))
}

pub(crate) fn transcript_media_source_backed_request_size(
    slot_size: TranscriptMediaSize,
    window_scale: f32,
) -> Size<DevicePixels> {
    let content_size = transcript_media_image_content_size(slot_size);
    size(
        DevicePixels((f32::from(content_size.width) * window_scale).round() as i32),
        DevicePixels((f32::from(content_size.height) * window_scale).round() as i32),
    )
}

pub(crate) fn transcript_media_run_alignment(run_length: usize) -> TranscriptMediaRunAlignment {
    if run_length == 1 {
        TranscriptMediaRunAlignment::Center
    } else {
        TranscriptMediaRunAlignment::Start
    }
}

fn multi_image_target_width(conversation_m_advance: Pixels) -> Pixels {
    conversation_m_advance.max(px(0.0)) * MULTI_IMAGE_TARGET_M_ADVANCES
}

fn transcript_media_image_content_size(slot_size: TranscriptMediaSize) -> TranscriptMediaSize {
    let border_extent = MEDIA_TILE_BORDER_WIDTH * 2.0;
    TranscriptMediaSize {
        width: (slot_size.width - border_extent).max(px(0.0)),
        height: (slot_size.height - border_extent).max(px(0.0)),
    }
}

fn scale_adjusted_natural_width(
    natural_dimensions: TranscriptMediaNaturalDimensions,
    window_scale: f32,
) -> Pixels {
    let scale = if window_scale.is_finite() && window_scale > 0.0 {
        window_scale
    } else {
        1.0
    };
    px(natural_dimensions.width as f32 / scale)
}

fn square_placeholder(width: Pixels) -> TranscriptMediaSize {
    let width = width.max(px(0.0));
    TranscriptMediaSize {
        width,
        height: width,
    }
}
