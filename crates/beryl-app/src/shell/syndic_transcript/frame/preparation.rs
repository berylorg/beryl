use std::{collections::VecDeque, sync::Arc};

use super::super::{
    demand::{DemandFact, DemandFactKind},
    provider::TranscriptPageDirection,
    snapshot::{
        ResidentPresentationRecord, ResidentPresentationRecordId, ResidentTranscriptSnapshot,
    },
};
use super::{
    RealizedFrameAnchor, RealizedFrameClamp, RealizedFrameRecord, RealizedFrameRequest,
    RealizedFrameScrollMode, RealizedFrameScrollStateSnapshot, RealizedFrameWindow,
    geometry::{
        presentation_record_position, push_clamp_demand, scroll_direction, snapshot_record_index,
    },
    types::SanitizedFrameRequest,
};

#[derive(Clone, Copy, Debug)]
enum Placement {
    Top(f32),
    Bottom(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PreparationStep {
    Seek,
    Forward,
    Backward,
    Ready,
    Taken,
}

#[derive(Debug)]
pub(crate) struct MeasuredFramePreparation {
    snapshot: Arc<ResidentTranscriptSnapshot>,
    request: SanitizedFrameRequest,
    manual_scroll_total_px: f32,
    cursor_index: usize,
    cursor_placement: Placement,
    step: PreparationStep,
    records: VecDeque<RealizedFrameRecord>,
    clamp: Option<RealizedFrameClamp>,
    work_last_step: usize,
}

impl MeasuredFramePreparation {
    pub(crate) fn new(
        snapshot: Arc<ResidentTranscriptSnapshot>,
        state: RealizedFrameScrollStateSnapshot,
        request: RealizedFrameRequest,
    ) -> Self {
        let request = SanitizedFrameRequest::from(request);
        let saved = state.anchor.as_ref().and_then(|anchor| {
            snapshot_record_index(&snapshot, &anchor.record_id)
                .map(|index| (index, anchor.viewport_y_px))
        });
        let tail = state.scroll_mode == RealizedFrameScrollMode::LiveTailFollowing
            && (request.manual_delta_px == 0.0 || saved.is_none());
        let (cursor_index, cursor_placement) = if tail {
            (
                snapshot.records.len().saturating_sub(1),
                Placement::Bottom(request.viewport_height_px - request.manual_delta_px),
            )
        } else {
            let (index, top) = saved.unwrap_or_else(|| {
                let index = snapshot
                    .visible_range
                    .as_ref()
                    .or(snapshot.realized_range.as_ref())
                    .map(|range| range.start.min(snapshot.records.len().saturating_sub(1)))
                    .unwrap_or(0);
                (index, 0.0)
            });
            (index, Placement::Top(top - request.manual_delta_px))
        };
        let empty = snapshot.records.is_empty();
        Self {
            snapshot,
            request,
            manual_scroll_total_px: state.manual_scroll_total_px + request.manual_delta_px,
            cursor_index,
            cursor_placement,
            step: if empty {
                PreparationStep::Ready
            } else {
                PreparationStep::Seek
            },
            records: VecDeque::new(),
            clamp: if empty {
                scroll_direction(request.manual_delta_px).map(|direction| RealizedFrameClamp {
                    direction,
                    anchor_index: 0,
                })
            } else {
                None
            },
            work_last_step: 0,
        }
    }

    pub(crate) fn retained_measurement_count(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn work_last_step(&self) -> usize {
        self.work_last_step
    }

    pub(crate) fn retains_record(&self, id: &ResidentPresentationRecordId) -> bool {
        if self.records.iter().any(|record| &record.record_id == id) {
            return true;
        }
        let current = match self.step {
            PreparationStep::Seek => Some(self.cursor_index),
            PreparationStep::Forward => self
                .records
                .back()
                .and_then(|record| record.index.checked_add(1)),
            PreparationStep::Backward => self
                .records
                .front()
                .and_then(|record| record.index.checked_sub(1)),
            PreparationStep::Ready | PreparationStep::Taken => None,
        };
        current
            .and_then(|index| self.snapshot.records.get(index))
            .is_some_and(|record| &record.id == id)
    }

    pub(crate) fn advance(
        &mut self,
        budget: usize,
        mut measure: impl FnMut(&ResidentPresentationRecord) -> f32,
    ) -> Option<RealizedFrameWindow> {
        self.advance_with_observations(budget, |record, _| (Some(measure(record)), 1))
    }

    pub(crate) fn advance_with_observations(
        &mut self,
        budget: usize,
        mut measure: impl FnMut(&ResidentPresentationRecord, usize) -> (Option<f32>, usize),
    ) -> Option<RealizedFrameWindow> {
        self.advance_with_retained_observations(budget, |record, remaining, _| {
            measure(record, remaining)
        })
    }

    pub(crate) fn advance_with_retained_observations(
        &mut self,
        budget: usize,
        mut measure: impl FnMut(
            &ResidentPresentationRecord,
            usize,
            &VecDeque<RealizedFrameRecord>,
        ) -> (Option<f32>, usize),
    ) -> Option<RealizedFrameWindow> {
        self.work_last_step = 0;
        let budget = budget.min(32);
        loop {
            match self.step {
                PreparationStep::Ready => {
                    self.step = PreparationStep::Taken;
                    return Some(self.finish());
                }
                PreparationStep::Taken => return None,
                PreparationStep::Forward if self.forward_complete() => {
                    self.qualify_trailing_edge();
                    self.step = PreparationStep::Backward;
                }
                PreparationStep::Backward if self.backward_complete() => {
                    self.qualify_leading_edge();
                    self.step = PreparationStep::Ready;
                }
                _ if self.work_last_step == budget => return None,
                PreparationStep::Seek => {
                    let height = self.observe(self.cursor_index, budget, &mut measure)?;
                    self.seek(height);
                }
                PreparationStep::Forward => {
                    let last = self.records.back().expect("measured cursor");
                    let index = last.index + 1;
                    let top = last.top_px + last.height_px;
                    let height = self.observe(index, budget, &mut measure)?;
                    self.records.push_back(self.record(index, top, height));
                }
                PreparationStep::Backward => {
                    let first = self.records.front().expect("measured cursor");
                    let index = first.index - 1;
                    let bottom = first.top_px;
                    let height = self.observe(index, budget, &mut measure)?;
                    self.records
                        .push_front(self.record(index, bottom - height, height));
                }
            }
        }
    }

    fn observe(
        &mut self,
        index: usize,
        budget: usize,
        measure: &mut impl FnMut(
            &ResidentPresentationRecord,
            usize,
            &VecDeque<RealizedFrameRecord>,
        ) -> (Option<f32>, usize),
    ) -> Option<f32> {
        let remaining = budget.saturating_sub(self.work_last_step);
        let (height, observations) =
            measure(&self.snapshot.records[index], remaining, &self.records);
        assert!(observations <= remaining && (height.is_none() || observations > 0));
        self.work_last_step += observations;
        let height = height?;
        (height.is_finite() && height > 0.0).then_some(height)
    }

    fn seek(&mut self, height: f32) {
        let mut top = match self.cursor_placement {
            Placement::Top(top) => top,
            Placement::Bottom(bottom) => bottom - height,
        };
        if top > 0.0 && self.cursor_index > 0 {
            self.cursor_index -= 1;
            self.cursor_placement = Placement::Bottom(top);
            return;
        }
        if self.cursor_index == 0 && top > 0.0 {
            top = 0.0;
            self.set_clamp(TranscriptPageDirection::Backward, 0);
        }
        let bottom = top + height;
        if bottom <= 0.0 && self.cursor_index + 1 < self.snapshot.records.len() {
            self.cursor_index += 1;
            self.cursor_placement = Placement::Top(bottom);
            return;
        }
        if self.cursor_index + 1 == self.snapshot.records.len()
            && bottom < self.request.viewport_height_px
        {
            top = self.request.viewport_height_px - height;
            self.set_clamp(TranscriptPageDirection::Forward, self.cursor_index);
            if top > 0.0 && self.cursor_index > 0 {
                self.cursor_index -= 1;
                self.cursor_placement = Placement::Bottom(top);
                return;
            }
            if self.cursor_index == 0 && top > 0.0 {
                top = 0.0;
            }
        }
        self.records
            .push_back(self.record(self.cursor_index, top, height));
        self.step = PreparationStep::Forward;
    }

    fn forward_complete(&self) -> bool {
        let last = self.records.back().expect("measured cursor");
        last.top_px + last.height_px
            >= self.request.viewport_height_px + self.request.overscan_height_px
            || last.index + 1 == self.snapshot.records.len()
    }

    fn backward_complete(&self) -> bool {
        let first = self.records.front().expect("measured cursor");
        first.top_px <= -self.request.overscan_height_px || first.index == 0
    }

    fn qualify_trailing_edge(&mut self) {
        let last = self.records.back().expect("measured cursor");
        let bottom = last.top_px + last.height_px;
        let first = self.records.front().expect("measured cursor");
        if last.index + 1 == self.snapshot.records.len()
            && bottom < self.request.viewport_height_px
            && !(first.index == 0 && first.top_px == 0.0)
        {
            self.set_clamp(TranscriptPageDirection::Forward, last.index);
            self.shift(self.request.viewport_height_px - bottom);
        }
    }

    fn qualify_leading_edge(&mut self) {
        let first = self.records.front().expect("measured cursor");
        if first.index == 0 && first.top_px > 0.0 {
            self.shift(-first.top_px);
        }
    }

    fn shift(&mut self, delta: f32) {
        for record in &mut self.records {
            record.top_px += delta;
        }
    }

    fn set_clamp(&mut self, direction: TranscriptPageDirection, anchor_index: usize) {
        self.clamp = Some(RealizedFrameClamp {
            direction,
            anchor_index,
        });
    }

    fn record(&self, index: usize, top_px: f32, height_px: f32) -> RealizedFrameRecord {
        RealizedFrameRecord {
            index,
            record_id: self.snapshot.records[index].id.clone(),
            top_px,
            height_px,
        }
    }

    fn finish(&mut self) -> RealizedFrameWindow {
        let records: Vec<_> = self.records.drain(..).collect();
        let first_visible = records.iter().find(|record| {
            record.top_px < self.request.viewport_height_px
                && record.top_px + record.height_px > 0.0
        });
        let last_visible = records.iter().rfind(|record| {
            record.top_px < self.request.viewport_height_px
                && record.top_px + record.height_px > 0.0
        });
        let visible_range = first_visible
            .zip(last_visible)
            .map(|(first, last)| first.index..last.index + 1)
            .unwrap_or(0..0);
        let overscan_range = records
            .first()
            .zip(records.last())
            .map(|(first, last)| first.index..last.index + 1)
            .unwrap_or(0..0);
        let anchor = first_visible.map(|record| RealizedFrameAnchor {
            record_id: record.record_id.clone(),
            index: record.index,
            viewport_y_px: record.top_px,
            position: presentation_record_position(&self.snapshot.records[record.index]),
        });
        let revision = self.snapshot.presentation_revision;
        let mut demand_facts = vec![
            DemandFact::new(
                revision,
                DemandFactKind::VisibleRange {
                    range: visible_range.clone(),
                },
            ),
            DemandFact::new(
                revision,
                DemandFactKind::OverscanRange {
                    range: overscan_range.clone(),
                },
            ),
            DemandFact::new(
                revision,
                DemandFactKind::CurrentAnchor {
                    record_id: anchor.as_ref().map(|anchor| anchor.record_id.clone()),
                    position: anchor.as_ref().and_then(|anchor| anchor.position),
                },
            ),
        ];
        demand_facts.extend(records.iter().map(|record| {
            DemandFact::new(
                revision,
                DemandFactKind::MeasuredRecord {
                    record_id: record.record_id.clone(),
                    height_px: record.height_px,
                },
            )
        }));
        if let Some(clamp) = &self.clamp {
            push_clamp_demand(&mut demand_facts, revision, clamp);
        }
        RealizedFrameWindow {
            presentation_revision: revision,
            records,
            visible_range,
            overscan_range,
            anchor,
            clamp: self.clamp.clone(),
            manual_delta_px: self.request.manual_delta_px,
            manual_scroll_total_px: self.manual_scroll_total_px,
            demand_facts,
        }
    }
}
