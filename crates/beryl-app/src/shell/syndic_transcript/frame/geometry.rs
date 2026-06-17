use std::ops::Range;

use super::{
    super::{
        demand::{DemandFact, DemandFactKind},
        provider::{TranscriptPageDirection, TranscriptViewPosition},
        snapshot::{
            ResidentPresentationRecord, ResidentPresentationRecordId, ResidentRecordSource,
            ResidentTranscriptSnapshot,
        },
    },
    types::{
        RealizedFrameAnchorState, RealizedFrameClamp, RealizedFrameRecord,
        RealizedRecordMeasurementState, SanitizedFrameRequest,
    },
};

pub(super) fn initial_anchor_for_snapshot(
    snapshot: &ResidentTranscriptSnapshot,
) -> RealizedFrameAnchorState {
    let preferred_index = snapshot
        .visible_range
        .as_ref()
        .or(snapshot.realized_range.as_ref())
        .map(|range| range.start.min(snapshot.records.len().saturating_sub(1)))
        .unwrap_or(0);
    let record = &snapshot.records[preferred_index];
    RealizedFrameAnchorState {
        record_id: record.id.clone(),
        index: preferred_index,
        viewport_y_px: 0.0,
        position: presentation_record_position(record),
    }
}

pub(super) fn tail_anchor_for_snapshot(
    snapshot: &ResidentTranscriptSnapshot,
    request: SanitizedFrameRequest,
    measured_records: &[RealizedRecordMeasurementState],
) -> RealizedFrameAnchorState {
    let index = snapshot.records.len().saturating_sub(1);
    let record = &snapshot.records[index];
    RealizedFrameAnchorState {
        record_id: record.id.clone(),
        index,
        viewport_y_px: request.viewport_height_px
            - record_height(record, request, measured_records),
        position: presentation_record_position(record),
    }
}

pub(super) fn set_anchor_to_index(
    snapshot: &ResidentTranscriptSnapshot,
    anchor: &mut RealizedFrameAnchorState,
    index: usize,
    viewport_y_px: f32,
) {
    let index = index.min(snapshot.records.len().saturating_sub(1));
    let record = &snapshot.records[index];
    anchor.record_id = record.id.clone();
    anchor.index = index;
    anchor.viewport_y_px = viewport_y_px;
    anchor.position = presentation_record_position(record);
}

pub(super) fn frame_range(
    snapshot: &ResidentTranscriptSnapshot,
    anchor: &RealizedFrameAnchorState,
    span: Range<f32>,
    request: SanitizedFrameRequest,
    measured_records: &[RealizedRecordMeasurementState],
) -> Range<usize> {
    if snapshot.records.is_empty() || span.start >= span.end {
        return 0..0;
    }

    let mut first = anchor.index;
    let mut first_top = anchor.viewport_y_px;
    while first > 0 && first_top > span.start {
        let previous_index = first.saturating_sub(1);
        first_top -= record_height(&snapshot.records[previous_index], request, measured_records);
        first = previous_index;
    }

    let mut current_top = first_top;
    while first < snapshot.records.len()
        && current_top + record_height(&snapshot.records[first], request, measured_records)
            <= span.start
    {
        current_top += record_height(&snapshot.records[first], request, measured_records);
        first = first.saturating_add(1);
    }

    let mut end = first;
    while end < snapshot.records.len() && current_top < span.end {
        current_top += record_height(&snapshot.records[end], request, measured_records);
        end = end.saturating_add(1);
    }

    first.min(snapshot.records.len())..end.min(snapshot.records.len())
}

pub(super) fn realized_records(
    snapshot: &ResidentTranscriptSnapshot,
    anchor: &RealizedFrameAnchorState,
    range: Range<usize>,
    request: SanitizedFrameRequest,
    measured_records: &[RealizedRecordMeasurementState],
) -> Vec<RealizedFrameRecord> {
    let mut top_px = top_for_index(snapshot, anchor, range.start, request, measured_records);
    let mut records = Vec::new();

    for index in range {
        let record = &snapshot.records[index];
        let height_px = record_height(record, request, measured_records);
        records.push(RealizedFrameRecord {
            index,
            record_id: record.id.clone(),
            top_px,
            height_px,
        });
        top_px += height_px;
    }

    records
}

fn top_for_index(
    snapshot: &ResidentTranscriptSnapshot,
    anchor: &RealizedFrameAnchorState,
    target_index: usize,
    request: SanitizedFrameRequest,
    measured_records: &[RealizedRecordMeasurementState],
) -> f32 {
    let target_index = target_index.min(snapshot.records.len().saturating_sub(1));
    if target_index == anchor.index {
        return anchor.viewport_y_px;
    }

    if target_index < anchor.index {
        let mut top_px = anchor.viewport_y_px;
        for index in target_index..anchor.index {
            top_px -= record_height(&snapshot.records[index], request, measured_records);
        }
        return top_px;
    }

    let mut top_px = anchor.viewport_y_px;
    for index in anchor.index..target_index {
        top_px += record_height(&snapshot.records[index], request, measured_records);
    }
    top_px
}

pub(super) fn record_height(
    record: &ResidentPresentationRecord,
    request: SanitizedFrameRequest,
    measured_records: &[RealizedRecordMeasurementState],
) -> f32 {
    measured_records
        .iter()
        .find(|measurement| measurement.record_id == record.id)
        .map(|measurement| measurement.height_px)
        .unwrap_or(request.default_record_height_px)
}

pub(super) fn resident_height(
    snapshot: &ResidentTranscriptSnapshot,
    request: SanitizedFrameRequest,
    measured_records: &[RealizedRecordMeasurementState],
) -> f32 {
    snapshot
        .records
        .iter()
        .map(|record| record_height(record, request, measured_records))
        .sum()
}

pub(super) fn trailing_anchor_y_limit(
    snapshot: &ResidentTranscriptSnapshot,
    anchor_index: usize,
    request: SanitizedFrameRequest,
    measured_records: &[RealizedRecordMeasurementState],
) -> f32 {
    let before_height: f32 = snapshot.records[..anchor_index]
        .iter()
        .map(|record| record_height(record, request, measured_records))
        .sum();
    let remaining_height: f32 = snapshot.records[anchor_index..]
        .iter()
        .map(|record| record_height(record, request, measured_records))
        .sum();

    (request.viewport_height_px - remaining_height).min(before_height)
}

pub(super) fn scroll_direction(delta_px: f32) -> Option<TranscriptPageDirection> {
    if delta_px > 0.0 {
        Some(TranscriptPageDirection::Forward)
    } else if delta_px < 0.0 {
        Some(TranscriptPageDirection::Backward)
    } else {
        None
    }
}

pub(super) fn snapshot_record_index(
    snapshot: &ResidentTranscriptSnapshot,
    record_id: &ResidentPresentationRecordId,
) -> Option<usize> {
    snapshot
        .records
        .iter()
        .position(|record| &record.id == record_id)
}

pub(super) fn presentation_record_position(
    record: &ResidentPresentationRecord,
) -> Option<TranscriptViewPosition> {
    match &record.provenance.source {
        ResidentRecordSource::Syndic(source) | ResidentRecordSource::LocalUiForSyndic(source) => {
            source.position
        }
        ResidentRecordSource::LocalUi => None,
    }
}

pub(super) fn obsolete_ranges(
    previous_range: Option<&Range<usize>>,
    current_range: &Range<usize>,
) -> Vec<Range<usize>> {
    let Some(previous_range) = previous_range else {
        return Vec::new();
    };
    let mut ranges = Vec::new();

    if previous_range.start < current_range.start {
        ranges.push(previous_range.start..previous_range.end.min(current_range.start));
    }
    if previous_range.end > current_range.end {
        ranges.push(previous_range.start.max(current_range.end)..previous_range.end);
    }

    ranges
        .into_iter()
        .filter(|range| range.start < range.end)
        .collect()
}

pub(super) fn bounded_range(range: Range<usize>, record_count: usize) -> Range<usize> {
    range.start.min(record_count)..range.end.min(record_count)
}

pub(super) fn push_clamp_demand(
    facts: &mut Vec<DemandFact>,
    presentation_revision: u64,
    clamp: &RealizedFrameClamp,
) {
    match clamp.direction {
        TranscriptPageDirection::Backward => {
            facts.push(DemandFact::new(
                presentation_revision,
                DemandFactKind::MissingBefore {
                    anchor_index: clamp.anchor_index,
                },
            ));
        }
        TranscriptPageDirection::Forward => {
            facts.push(DemandFact::new(
                presentation_revision,
                DemandFactKind::MissingAfter {
                    anchor_index: clamp.anchor_index,
                },
            ));
        }
    }
    facts.push(DemandFact::new(
        presentation_revision,
        DemandFactKind::AdjacentRange {
            anchor_index: clamp.anchor_index,
            direction: clamp.direction,
        },
    ));
}

pub(super) fn upsert_measurement(
    measurements: &mut Vec<RealizedRecordMeasurementState>,
    measurement: RealizedRecordMeasurementState,
) {
    if let Some(existing) = measurements
        .iter_mut()
        .find(|existing| existing.record_id == measurement.record_id)
    {
        existing.height_px = measurement.height_px;
        return;
    }

    measurements.push(measurement);
}
