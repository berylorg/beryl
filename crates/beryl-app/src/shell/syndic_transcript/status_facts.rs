use super::{
    core::ResidentCoreSnapshot,
    frame::{RealizedFrameScrollMode, RealizedFrameScrollStateSnapshot},
    provider::TranscriptViewPosition,
    snapshot::{ResidentPresentationRecordId, ResidentTranscriptSnapshotState},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentTranscriptStatusFacts {
    pub(crate) state: ResidentTranscriptStatusState,
    pub(crate) activation_revision: u64,
    pub(crate) presentation_revision: u64,
    pub(crate) scroll_mode: ResidentTranscriptStatusScrollMode,
    pub(crate) anchor_record_id: Option<ResidentPresentationRecordId>,
    pub(crate) anchor_position: Option<TranscriptViewPosition>,
    pub(crate) resident_presentation_record_count: usize,
    pub(crate) resident_view_record_count: usize,
    pub(crate) resident_projection_record_count: usize,
    pub(crate) resident_resource_metadata_count: usize,
    pub(crate) resident_resource_slice_count: usize,
    pub(crate) resident_fallback_record_count: usize,
    pub(crate) budget_rejection_count: usize,
    pub(crate) pending_demand_fact_count: usize,
    pub(crate) pending_provider_request_count: usize,
    pub(crate) rejected_demand_count: usize,
    pub(crate) turn_view: ResidentTranscriptTurnViewFacts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentTranscriptStatusState {
    Empty,
    Unavailable { reason: String },
    FixtureBacked { label: String },
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResidentTranscriptStatusScrollMode {
    LiveTailFollowing,
    DetachedManual,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResidentTranscriptTurnViewFacts {
    pub(crate) current: Option<usize>,
    pub(crate) total: Option<usize>,
}

impl ResidentTranscriptStatusFacts {
    pub(crate) fn unknown() -> Self {
        Self {
            state: ResidentTranscriptStatusState::Unknown,
            activation_revision: 0,
            presentation_revision: 0,
            scroll_mode: ResidentTranscriptStatusScrollMode::Unknown,
            anchor_record_id: None,
            anchor_position: None,
            resident_presentation_record_count: 0,
            resident_view_record_count: 0,
            resident_projection_record_count: 0,
            resident_resource_metadata_count: 0,
            resident_resource_slice_count: 0,
            resident_fallback_record_count: 0,
            budget_rejection_count: 0,
            pending_demand_fact_count: 0,
            pending_provider_request_count: 0,
            rejected_demand_count: 0,
            turn_view: ResidentTranscriptTurnViewFacts::unknown(),
        }
    }

    pub(crate) fn from_core_snapshot(
        snapshot: &ResidentCoreSnapshot,
        scroll: RealizedFrameScrollStateSnapshot,
    ) -> Self {
        Self {
            state: ResidentTranscriptStatusState::from(&snapshot.presentation.state),
            activation_revision: snapshot.presentation.activation_revision,
            presentation_revision: snapshot.presentation.presentation_revision,
            scroll_mode: ResidentTranscriptStatusScrollMode::from(scroll.scroll_mode),
            anchor_record_id: scroll
                .anchor
                .as_ref()
                .map(|anchor| anchor.record_id.clone()),
            anchor_position: scroll.anchor.as_ref().and_then(|anchor| anchor.position),
            resident_presentation_record_count: snapshot.presentation.record_count(),
            resident_view_record_count: snapshot.resident.view_record_count,
            resident_projection_record_count: snapshot.resident.projection_record_count,
            resident_resource_metadata_count: snapshot.resident.resource_metadata_count,
            resident_resource_slice_count: snapshot.resident.resource_slice_count,
            resident_fallback_record_count: snapshot.resident.fallback_record_count,
            budget_rejection_count: snapshot.resident.budget_rejection_count,
            pending_demand_fact_count: snapshot.demand_facts.pending_count,
            pending_provider_request_count: snapshot.provider_requests.pending_count,
            rejected_demand_count: snapshot.provider_requests.rejected_result_count,
            turn_view: ResidentTranscriptTurnViewFacts::unknown(),
        }
    }
}

impl ResidentTranscriptTurnViewFacts {
    pub(crate) fn new(current: Option<usize>, total: Option<usize>) -> Self {
        Self {
            current: positive_usize(current),
            total: positive_usize(total),
        }
    }

    pub(crate) fn unknown() -> Self {
        Self::new(None, None)
    }
}

impl From<&ResidentTranscriptSnapshotState> for ResidentTranscriptStatusState {
    fn from(state: &ResidentTranscriptSnapshotState) -> Self {
        match state {
            ResidentTranscriptSnapshotState::Empty => Self::Empty,
            ResidentTranscriptSnapshotState::Unavailable { reason } => Self::Unavailable {
                reason: reason.clone(),
            },
            ResidentTranscriptSnapshotState::Fixture { label } => Self::FixtureBacked {
                label: label.clone(),
            },
        }
    }
}

impl From<RealizedFrameScrollMode> for ResidentTranscriptStatusScrollMode {
    fn from(mode: RealizedFrameScrollMode) -> Self {
        match mode {
            RealizedFrameScrollMode::LiveTailFollowing => Self::LiveTailFollowing,
            RealizedFrameScrollMode::DetachedManual => Self::DetachedManual,
        }
    }
}

fn positive_usize(value: Option<usize>) -> Option<usize> {
    value.filter(|value| *value > 0)
}
