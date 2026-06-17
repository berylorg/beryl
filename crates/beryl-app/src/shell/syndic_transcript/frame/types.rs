use std::ops::Range;

use super::super::{
    demand::DemandFact,
    provider::{TranscriptPageDirection, TranscriptViewPosition},
    snapshot::ResidentPresentationRecordId,
};

const DEFAULT_VIEWPORT_HEIGHT_PX: f32 = 640.0;
const DEFAULT_OVERSCAN_HEIGHT_PX: f32 = 320.0;
const DEFAULT_RECORD_HEIGHT_PX: f32 = 24.0;
const MIN_RECORD_HEIGHT_PX: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RealizedFrameRequest {
    pub(crate) viewport_height_px: f32,
    pub(crate) overscan_height_px: f32,
    pub(crate) default_record_height_px: f32,
    pub(crate) manual_delta_px: f32,
    pub(crate) observed_presentation_revision: Option<u64>,
}

impl Default for RealizedFrameRequest {
    fn default() -> Self {
        Self {
            viewport_height_px: DEFAULT_VIEWPORT_HEIGHT_PX,
            overscan_height_px: DEFAULT_OVERSCAN_HEIGHT_PX,
            default_record_height_px: DEFAULT_RECORD_HEIGHT_PX,
            manual_delta_px: 0.0,
            observed_presentation_revision: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RealizedFrameWindow {
    pub(crate) presentation_revision: u64,
    pub(crate) records: Vec<RealizedFrameRecord>,
    pub(crate) visible_range: Range<usize>,
    pub(crate) overscan_range: Range<usize>,
    pub(crate) anchor: Option<RealizedFrameAnchor>,
    pub(crate) clamp: Option<RealizedFrameClamp>,
    pub(crate) manual_delta_px: f32,
    pub(crate) manual_scroll_total_px: f32,
    pub(crate) demand_facts: Vec<DemandFact>,
}

impl RealizedFrameWindow {
    pub(super) fn stale(presentation_revision: u64, fact: DemandFact) -> Self {
        Self {
            presentation_revision,
            records: Vec::new(),
            visible_range: 0..0,
            overscan_range: 0..0,
            anchor: None,
            clamp: None,
            manual_delta_px: 0.0,
            manual_scroll_total_px: 0.0,
            demand_facts: vec![fact],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RealizedFrameRecord {
    pub(crate) index: usize,
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) top_px: f32,
    pub(crate) height_px: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RealizedFrameAnchor {
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) index: usize,
    pub(crate) viewport_y_px: f32,
    pub(crate) position: Option<TranscriptViewPosition>,
}

impl RealizedFrameAnchor {
    pub(super) fn from_state(state: &RealizedFrameAnchorState) -> Self {
        Self {
            record_id: state.record_id.clone(),
            index: state.index,
            viewport_y_px: state.viewport_y_px,
            position: state.position,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RealizedFrameClamp {
    pub(crate) direction: TranscriptPageDirection,
    pub(crate) anchor_index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RealizedRecordMeasurement {
    pub(crate) presentation_revision: u64,
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) height_px: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RealizedFrameScrollStateSnapshot {
    pub(crate) anchor: Option<RealizedFrameAnchor>,
    pub(crate) scroll_mode: RealizedFrameScrollMode,
    pub(crate) presentation_revision: Option<u64>,
    pub(crate) manual_scroll_total_px: f32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RealizedFrameScrollMode {
    LiveTailFollowing,
    #[default]
    DetachedManual,
}

impl RealizedFrameScrollMode {
    pub(crate) fn diagnostic_label(self) -> &'static str {
        match self {
            Self::LiveTailFollowing => "live-tail-following",
            Self::DetachedManual => "detached-manual",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct RealizedFrameAnchorState {
    pub(super) record_id: ResidentPresentationRecordId,
    pub(super) index: usize,
    pub(super) viewport_y_px: f32,
    pub(super) position: Option<TranscriptViewPosition>,
}

#[derive(Clone, Debug)]
pub(super) struct RealizedRecordMeasurementState {
    pub(super) record_id: ResidentPresentationRecordId,
    pub(super) height_px: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SanitizedFrameRequest {
    pub(super) viewport_height_px: f32,
    pub(super) overscan_height_px: f32,
    pub(super) default_record_height_px: f32,
    pub(super) manual_delta_px: f32,
}

impl SanitizedFrameRequest {
    pub(super) fn from(request: RealizedFrameRequest) -> Self {
        Self {
            viewport_height_px: sanitize_positive_px(
                request.viewport_height_px,
                DEFAULT_VIEWPORT_HEIGHT_PX,
            ),
            overscan_height_px: sanitize_nonnegative_px(request.overscan_height_px),
            default_record_height_px: sanitize_height(request.default_record_height_px),
            manual_delta_px: sanitize_delta(request.manual_delta_px),
        }
    }
}

pub(super) fn sanitize_height(value: f32) -> f32 {
    sanitize_positive_px(value, DEFAULT_RECORD_HEIGHT_PX).max(MIN_RECORD_HEIGHT_PX)
}

fn sanitize_positive_px(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn sanitize_nonnegative_px(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn sanitize_delta(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}
