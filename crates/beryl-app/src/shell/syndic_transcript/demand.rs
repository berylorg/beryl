use std::{collections::VecDeque, ops::Range};

use super::{
    provider::{ResourceId, TranscriptPageDirection, TranscriptViewPosition},
    snapshot::ResidentPresentationRecordId,
};

const DEFAULT_DEMAND_FACT_LIMIT: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DemandFact {
    pub(crate) presentation_revision: u64,
    pub(crate) kind: DemandFactKind,
}

impl DemandFact {
    pub(crate) fn new(presentation_revision: u64, kind: DemandFactKind) -> Self {
        Self {
            presentation_revision,
            kind,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DemandFactKind {
    CurrentAnchor {
        record_id: Option<ResidentPresentationRecordId>,
        position: Option<TranscriptViewPosition>,
    },
    VisibleRange {
        range: Range<usize>,
    },
    OverscanRange {
        range: Range<usize>,
    },
    MissingBefore {
        anchor_index: usize,
    },
    MissingAfter {
        anchor_index: usize,
    },
    Viewport {
        width_px: f32,
        height_px: f32,
    },
    MeasuredRecord {
        record_id: ResidentPresentationRecordId,
        height_px: f32,
    },
    AdjacentRange {
        anchor_index: usize,
        direction: TranscriptPageDirection,
    },
    ResourceRange {
        resource_id: ResourceId,
        range: Range<u64>,
    },
    ActiveSelectionPin {
        record_id: ResidentPresentationRecordId,
    },
    OpenMenuPin {
        record_id: ResidentPresentationRecordId,
    },
    MediaPreviewPin {
        resource_id: ResourceId,
    },
    ObsoleteRange {
        range: Range<usize>,
    },
    StaleMeasurement {
        observed_revision: u64,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct DemandFactSink {
    limit: usize,
    pending: VecDeque<DemandFact>,
    dropped_count: usize,
}

impl Default for DemandFactSink {
    fn default() -> Self {
        Self {
            limit: DEFAULT_DEMAND_FACT_LIMIT,
            pending: VecDeque::new(),
            dropped_count: 0,
        }
    }
}

impl DemandFactSink {
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            pending: VecDeque::new(),
            dropped_count: 0,
        }
    }

    pub(crate) fn push(&mut self, fact: DemandFact) {
        if self.pending.len() == self.limit {
            self.pending.pop_front();
            self.dropped_count = self.dropped_count.saturating_add(1);
        }
        self.pending.push_back(fact);
    }

    pub(crate) fn drain(&mut self) -> Vec<DemandFact> {
        self.pending.drain(..).collect()
    }

    pub(crate) fn snapshot(&self) -> DemandFactSinkSnapshot {
        DemandFactSinkSnapshot {
            pending_count: self.pending.len(),
            dropped_count: self.dropped_count,
            limit: self.limit,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DemandFactSinkSnapshot {
    pub(crate) pending_count: usize,
    pub(crate) dropped_count: usize,
    pub(crate) limit: usize,
}
