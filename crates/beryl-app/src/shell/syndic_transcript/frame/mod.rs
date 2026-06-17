use std::ops::Range;

mod geometry;
mod types;

pub(crate) use types::RealizedFrameWindow;
pub(crate) use types::{
    RealizedFrameAnchor, RealizedFrameClamp, RealizedFrameRecord, RealizedFrameRequest,
    RealizedFrameScrollMode, RealizedFrameScrollStateSnapshot, RealizedRecordMeasurement,
};

use self::{
    geometry::{
        bounded_range, frame_range, initial_anchor_for_snapshot, obsolete_ranges,
        presentation_record_position, push_clamp_demand, realized_records, record_height,
        resident_height, scroll_direction, set_anchor_to_index, snapshot_record_index,
        tail_anchor_for_snapshot, trailing_anchor_y_limit, upsert_measurement,
    },
    types::{
        RealizedFrameAnchorState, RealizedRecordMeasurementState, SanitizedFrameRequest,
        sanitize_height,
    },
};
use super::{
    demand::{DemandFact, DemandFactKind},
    provider::TranscriptPageDirection,
    snapshot::{ResidentPresentationRecordId, ResidentTranscriptSnapshot},
};

#[derive(Clone, Debug, Default)]
pub(crate) struct RealizedFrameScrollController {
    anchor: Option<RealizedFrameAnchorState>,
    measured_records: Vec<RealizedRecordMeasurementState>,
    previous_overscan_range: Option<Range<usize>>,
    presentation_revision: Option<u64>,
    manual_scroll_total_px: f32,
    scroll_mode: RealizedFrameScrollMode,
    pending_tail_placement: bool,
    previous_snapshot_record_ids: Vec<ResidentPresentationRecordId>,
}

impl RealizedFrameScrollController {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn state_snapshot(&self) -> RealizedFrameScrollStateSnapshot {
        RealizedFrameScrollStateSnapshot {
            anchor: self.anchor.as_ref().map(RealizedFrameAnchor::from_state),
            scroll_mode: self.scroll_mode,
            presentation_revision: self.presentation_revision,
            manual_scroll_total_px: self.manual_scroll_total_px,
        }
    }

    pub(crate) fn begin_live_tail_following(&mut self) {
        self.scroll_mode = RealizedFrameScrollMode::LiveTailFollowing;
        self.pending_tail_placement = true;
    }

    pub(crate) fn detach_live_tail_following(&mut self) {
        self.scroll_mode = RealizedFrameScrollMode::DetachedManual;
        self.pending_tail_placement = false;
    }

    pub(crate) fn observe_record_measurement(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        measurement: RealizedRecordMeasurement,
    ) -> DemandFact {
        if measurement.presentation_revision != snapshot.presentation_revision {
            return DemandFact::new(
                measurement.presentation_revision,
                DemandFactKind::StaleMeasurement {
                    observed_revision: measurement.presentation_revision,
                },
            );
        }

        if snapshot_record_index(snapshot, &measurement.record_id).is_some() {
            upsert_measurement(
                &mut self.measured_records,
                RealizedRecordMeasurementState {
                    record_id: measurement.record_id.clone(),
                    height_px: sanitize_height(measurement.height_px),
                },
            );
        }

        DemandFact::new(
            measurement.presentation_revision,
            DemandFactKind::MeasuredRecord {
                record_id: measurement.record_id,
                height_px: sanitize_height(measurement.height_px),
            },
        )
    }

    pub(crate) fn realize(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        request: RealizedFrameRequest,
    ) -> RealizedFrameWindow {
        if let Some(observed_revision) = request.observed_presentation_revision {
            if observed_revision != snapshot.presentation_revision {
                return RealizedFrameWindow::stale(
                    snapshot.presentation_revision,
                    DemandFact::new(
                        observed_revision,
                        DemandFactKind::StaleMeasurement { observed_revision },
                    ),
                );
            }
        }

        let request = SanitizedFrameRequest::from(request);
        if request.manual_delta_px != 0.0 {
            self.detach_live_tail_following();
        }
        self.prepare_for_snapshot(snapshot);

        let window = if snapshot.records.is_empty() {
            self.realize_empty(snapshot, request)
        } else {
            self.realize_nonempty(snapshot, request)
        };
        self.remember_snapshot_records(snapshot);
        window
    }

    fn prepare_for_snapshot(&mut self, snapshot: &ResidentTranscriptSnapshot) {
        if self.presentation_revision != Some(snapshot.presentation_revision) {
            self.presentation_revision = Some(snapshot.presentation_revision);
            self.previous_overscan_range = None;
        }

        self.measured_records.retain(|measurement| {
            snapshot_record_index(snapshot, &measurement.record_id).is_some()
        });

        if self
            .anchor
            .as_ref()
            .is_some_and(|anchor| snapshot_record_index(snapshot, &anchor.record_id).is_none())
        {
            self.anchor = None;
        }
    }

    fn realize_empty(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        request: SanitizedFrameRequest,
    ) -> RealizedFrameWindow {
        let clamp = scroll_direction(request.manual_delta_px).map(|direction| RealizedFrameClamp {
            direction,
            anchor_index: 0,
        });
        let mut demand_facts = vec![
            DemandFact::new(
                snapshot.presentation_revision,
                DemandFactKind::VisibleRange { range: 0..0 },
            ),
            DemandFact::new(
                snapshot.presentation_revision,
                DemandFactKind::OverscanRange { range: 0..0 },
            ),
            DemandFact::new(
                snapshot.presentation_revision,
                DemandFactKind::CurrentAnchor {
                    record_id: None,
                    position: None,
                },
            ),
        ];

        if let Some(clamp) = &clamp {
            push_clamp_demand(&mut demand_facts, snapshot.presentation_revision, clamp);
        }

        self.anchor = None;
        self.previous_overscan_range = Some(0..0);
        self.manual_scroll_total_px += request.manual_delta_px;

        RealizedFrameWindow {
            presentation_revision: snapshot.presentation_revision,
            records: Vec::new(),
            visible_range: 0..0,
            overscan_range: 0..0,
            anchor: None,
            clamp,
            manual_delta_px: request.manual_delta_px,
            manual_scroll_total_px: self.manual_scroll_total_px,
            demand_facts,
        }
    }

    fn realize_nonempty(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        request: SanitizedFrameRequest,
    ) -> RealizedFrameWindow {
        let should_place_tail = self.should_place_live_tail(snapshot);
        let mut anchor = if should_place_tail {
            tail_anchor_for_snapshot(snapshot, request, &self.measured_records)
        } else {
            self.anchor_for_snapshot(snapshot)
                .unwrap_or_else(|| initial_anchor_for_snapshot(snapshot))
        };
        if !should_place_tail {
            anchor.viewport_y_px -= request.manual_delta_px;
        }
        self.manual_scroll_total_px += request.manual_delta_px;

        let attempted_direction = scroll_direction(request.manual_delta_px);
        let clamp = self.rebase_anchor(snapshot, &mut anchor, request, attempted_direction);
        let visible_range = frame_range(
            snapshot,
            &anchor,
            0.0..request.viewport_height_px,
            request,
            &self.measured_records,
        );
        let overscan_range = frame_range(
            snapshot,
            &anchor,
            -request.overscan_height_px..request.viewport_height_px + request.overscan_height_px,
            request,
            &self.measured_records,
        );
        let records = realized_records(
            snapshot,
            &anchor,
            overscan_range.clone(),
            request,
            &self.measured_records,
        );
        let demand_facts = self.demand_facts_for_window(
            snapshot,
            &anchor,
            visible_range.clone(),
            overscan_range.clone(),
            &records,
            clamp.as_ref(),
        );

        self.anchor = Some(anchor.clone());
        self.previous_overscan_range = Some(overscan_range.clone());

        RealizedFrameWindow {
            presentation_revision: snapshot.presentation_revision,
            records,
            visible_range,
            overscan_range,
            anchor: Some(RealizedFrameAnchor::from_state(&anchor)),
            clamp,
            manual_delta_px: request.manual_delta_px,
            manual_scroll_total_px: self.manual_scroll_total_px,
            demand_facts,
        }
    }

    fn should_place_live_tail(&self, snapshot: &ResidentTranscriptSnapshot) -> bool {
        if self.scroll_mode != RealizedFrameScrollMode::LiveTailFollowing {
            return false;
        }
        if self.pending_tail_placement {
            return true;
        }
        self.has_coherent_tail_growth(snapshot)
    }

    fn has_coherent_tail_growth(&self, snapshot: &ResidentTranscriptSnapshot) -> bool {
        if snapshot.records.len() <= self.previous_snapshot_record_ids.len() {
            return false;
        }

        self.previous_snapshot_record_ids
            .iter()
            .zip(&snapshot.records)
            .all(|(previous_id, current_record)| previous_id == &current_record.id)
    }

    fn remember_snapshot_records(&mut self, snapshot: &ResidentTranscriptSnapshot) {
        self.previous_snapshot_record_ids = snapshot
            .records
            .iter()
            .map(|record| record.id.clone())
            .collect();
        if !snapshot.records.is_empty() {
            self.pending_tail_placement = false;
        }
    }

    fn anchor_for_snapshot(
        &self,
        snapshot: &ResidentTranscriptSnapshot,
    ) -> Option<RealizedFrameAnchorState> {
        let anchor = self.anchor.as_ref()?;
        let index = snapshot_record_index(snapshot, &anchor.record_id)?;
        Some(RealizedFrameAnchorState {
            record_id: anchor.record_id.clone(),
            index,
            viewport_y_px: anchor.viewport_y_px,
            position: presentation_record_position(&snapshot.records[index]),
        })
    }

    fn rebase_anchor(
        &self,
        snapshot: &ResidentTranscriptSnapshot,
        anchor: &mut RealizedFrameAnchorState,
        request: SanitizedFrameRequest,
        attempted_direction: Option<TranscriptPageDirection>,
    ) -> Option<RealizedFrameClamp> {
        if resident_height(snapshot, request, &self.measured_records) <= request.viewport_height_px
        {
            set_anchor_to_index(snapshot, anchor, 0, 0.0);
            return attempted_direction.map(|direction| RealizedFrameClamp {
                direction,
                anchor_index: anchor.index,
            });
        }

        while anchor.index + 1 < snapshot.records.len()
            && anchor.viewport_y_px
                <= -self.record_height_for_index(snapshot, anchor.index, request)
        {
            let height = self.record_height_for_index(snapshot, anchor.index, request);
            let next_index = anchor.index.saturating_add(1);
            set_anchor_to_index(snapshot, anchor, next_index, anchor.viewport_y_px + height);
        }

        while anchor.index > 0 && anchor.viewport_y_px > 0.0 {
            let previous_index = anchor.index.saturating_sub(1);
            let previous_height = self.record_height_for_index(snapshot, previous_index, request);
            set_anchor_to_index(
                snapshot,
                anchor,
                previous_index,
                anchor.viewport_y_px - previous_height,
            );
        }

        match attempted_direction {
            Some(TranscriptPageDirection::Backward)
                if anchor.index == 0 && anchor.viewport_y_px > 0.0 =>
            {
                anchor.viewport_y_px = 0.0;
                Some(RealizedFrameClamp {
                    direction: TranscriptPageDirection::Backward,
                    anchor_index: anchor.index,
                })
            }
            Some(TranscriptPageDirection::Forward) => {
                let trailing_limit = trailing_anchor_y_limit(
                    snapshot,
                    anchor.index,
                    request,
                    &self.measured_records,
                );
                if anchor.viewport_y_px < trailing_limit {
                    anchor.viewport_y_px = trailing_limit;
                    Some(RealizedFrameClamp {
                        direction: TranscriptPageDirection::Forward,
                        anchor_index: anchor.index,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn record_height_for_index(
        &self,
        snapshot: &ResidentTranscriptSnapshot,
        index: usize,
        request: SanitizedFrameRequest,
    ) -> f32 {
        record_height(&snapshot.records[index], request, &self.measured_records)
    }

    fn demand_facts_for_window(
        &self,
        snapshot: &ResidentTranscriptSnapshot,
        anchor: &RealizedFrameAnchorState,
        visible_range: Range<usize>,
        overscan_range: Range<usize>,
        records: &[RealizedFrameRecord],
        clamp: Option<&RealizedFrameClamp>,
    ) -> Vec<DemandFact> {
        let presentation_revision = snapshot.presentation_revision;
        let mut facts = vec![
            DemandFact::new(
                presentation_revision,
                DemandFactKind::VisibleRange {
                    range: visible_range,
                },
            ),
            DemandFact::new(
                presentation_revision,
                DemandFactKind::OverscanRange {
                    range: overscan_range.clone(),
                },
            ),
            DemandFact::new(
                presentation_revision,
                DemandFactKind::CurrentAnchor {
                    record_id: Some(anchor.record_id.clone()),
                    position: anchor.position,
                },
            ),
        ];

        for obsolete_range in
            obsolete_ranges(self.previous_overscan_range.as_ref(), &overscan_range)
        {
            let range = bounded_range(obsolete_range, snapshot.records.len());
            if range.start < range.end {
                facts.push(DemandFact::new(
                    presentation_revision,
                    DemandFactKind::ObsoleteRange { range },
                ));
            }
        }

        for record in records {
            facts.push(DemandFact::new(
                presentation_revision,
                DemandFactKind::MeasuredRecord {
                    record_id: record.record_id.clone(),
                    height_px: record.height_px,
                },
            ));
        }

        if let Some(clamp) = clamp {
            push_clamp_demand(&mut facts, presentation_revision, clamp);
        }

        facts
    }
}
