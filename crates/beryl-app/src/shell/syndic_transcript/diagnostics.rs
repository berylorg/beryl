use crate::{
    diagnostic_dynamic_tools::{
        MediaEventSnapshot, TranscriptFrameMetricsSnapshot, VisibleMediaSnapshot,
    },
    gui_control_dynamic_tools::MarkdownCacheUiState,
    memory_diagnostics::RetainedStateSnapshot,
};

use super::{
    DemandFactSinkSnapshot, ResidentCoreSnapshot, ResidentPresentationRecordKind,
    ResidentTranscriptSnapshot,
};

#[derive(Clone, Debug)]
pub(crate) struct SyndicTranscriptDiagnosticSnapshot {
    pub(crate) resident_data: ResidentDataDiagnostics,
    pub(crate) frame: ResidentFrameDiagnostics,
    pub(crate) visible_media: VisibleMediaSnapshot,
    pub(crate) media_events: MediaEventSnapshot,
    pub(crate) transcript_frame_metrics: TranscriptFrameMetricsSnapshot,
    pub(crate) demand_facts: DemandFactSinkSnapshot,
}

impl SyndicTranscriptDiagnosticSnapshot {
    pub(crate) fn empty() -> Self {
        Self {
            resident_data: ResidentDataDiagnostics::default(),
            frame: ResidentFrameDiagnostics::default(),
            visible_media: VisibleMediaSnapshot::default(),
            media_events: empty_media_events(),
            transcript_frame_metrics: TranscriptFrameMetricsSnapshot::default(),
            demand_facts: DemandFactSinkSnapshot::default(),
        }
    }

    pub(crate) fn from_empty_host(
        snapshot: &ResidentTranscriptSnapshot,
        demand_facts: DemandFactSinkSnapshot,
    ) -> Self {
        Self {
            resident_data: ResidentDataDiagnostics {
                resident_record_count: snapshot.record_count(),
                presentation_revision: snapshot.presentation_revision,
                activation_revision: snapshot.activation_revision,
                ..ResidentDataDiagnostics::default()
            },
            frame: ResidentFrameDiagnostics {
                realized_range: snapshot.realized_range.clone(),
                visible_range: snapshot.visible_range.clone(),
                ..ResidentFrameDiagnostics::default()
            },
            demand_facts,
            ..Self::empty()
        }
    }

    pub(crate) fn from_core_snapshot(snapshot: &ResidentCoreSnapshot) -> Self {
        let fallback_record_count = snapshot
            .presentation
            .records
            .iter()
            .filter(|record| {
                matches!(
                    &record.kind,
                    ResidentPresentationRecordKind::LocalUiFallback { .. }
                )
            })
            .count();

        Self {
            resident_data: ResidentDataDiagnostics {
                resident_record_count: snapshot.presentation.record_count(),
                resident_view_record_count: snapshot.resident.view_record_count,
                resident_projection_record_count: snapshot.resident.projection_record_count,
                resident_resource_metadata_count: snapshot.resident.resource_metadata_count,
                resident_resource_slice_count: snapshot.resident.resource_slice_count,
                resident_resource_rejection_count: snapshot.resident.resource_rejection_count,
                resident_fallback_record_count: snapshot.resident.fallback_record_count,
                budget_rejection_count: snapshot.resident.budget_rejection_count,
                estimated_resident_bytes: snapshot.resident.estimated_resident_bytes,
                projection_bytes: snapshot.resident.projection_bytes,
                presentation_bytes: snapshot.resident.presentation_bytes,
                resource_bytes: snapshot.resident.resource_bytes,
                resource_slice_bytes: snapshot.resident.resource_slice_bytes,
                decoded_or_uploaded_media_bytes: snapshot.resident.decoded_or_uploaded_media_bytes,
                geometry_bytes: snapshot.resident.geometry_bytes,
                pin_bytes: snapshot.resident.pin_bytes,
                active_pin_count: snapshot.resident.active_pin_count,
                pending_provider_requests: snapshot.provider_requests.pending_count,
                stale_provider_results: snapshot.provider_requests.stale_result_count,
                rejected_demand_count: snapshot.provider_requests.rejected_result_count,
                release_decision_count: snapshot.resident.release_decision_count,
                fallback_record_count: fallback_record_count
                    .max(snapshot.resident.fallback_record_count),
                activation_revision: snapshot.presentation.activation_revision,
                presentation_revision: snapshot.presentation.presentation_revision,
                ..ResidentDataDiagnostics::default()
            },
            frame: ResidentFrameDiagnostics {
                realized_range: snapshot.presentation.realized_range.clone(),
                visible_range: snapshot.presentation.visible_range.clone(),
                ..ResidentFrameDiagnostics::default()
            },
            demand_facts: snapshot.demand_facts,
            ..Self::empty()
        }
    }

    pub(crate) fn markdown_cache_ui_state(&self) -> MarkdownCacheUiState {
        MarkdownCacheUiState::default()
    }

    pub(crate) fn add_retained_counts(&self, _retained_state: &mut RetainedStateSnapshot) {}
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResidentDataDiagnostics {
    pub(crate) resident_record_count: usize,
    pub(crate) resident_view_record_count: usize,
    pub(crate) resident_projection_record_count: usize,
    pub(crate) resident_resource_metadata_count: usize,
    pub(crate) resident_resource_slice_count: usize,
    pub(crate) resident_resource_rejection_count: usize,
    pub(crate) resident_fallback_record_count: usize,
    pub(crate) budget_rejection_count: usize,
    pub(crate) estimated_resident_bytes: usize,
    pub(crate) projection_bytes: usize,
    pub(crate) presentation_bytes: usize,
    pub(crate) resource_bytes: usize,
    pub(crate) resource_slice_bytes: usize,
    pub(crate) decoded_or_uploaded_media_bytes: usize,
    pub(crate) geometry_bytes: usize,
    pub(crate) pin_bytes: usize,
    pub(crate) active_pin_count: usize,
    pub(crate) pending_provider_requests: usize,
    pub(crate) stale_provider_results: usize,
    pub(crate) rejected_demand_count: usize,
    pub(crate) fallback_record_count: usize,
    pub(crate) release_decision_count: usize,
    pub(crate) activation_revision: u64,
    pub(crate) presentation_revision: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResidentFrameDiagnostics {
    pub(crate) realized_range: Option<std::ops::Range<usize>>,
    pub(crate) visible_range: Option<std::ops::Range<usize>>,
    pub(crate) rendered_record_count: usize,
    pub(crate) overscan_record_count: usize,
    pub(crate) scroll_mode: &'static str,
    pub(crate) anchor_record: Option<String>,
}

fn empty_media_events() -> MediaEventSnapshot {
    MediaEventSnapshot {
        events: Vec::new(),
        event_count: 0,
        truncated: false,
        next_sequence: 0,
    }
}
