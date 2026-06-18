use std::ops::Range;

use super::provider::{
    ProjectionRecordId, ProviderRevision, ResourceId, ResourceKind, ResourceMetadata,
    SyndicSourceProvenance, TranscriptNarrativeKind, TranscriptProviderHistoryReason,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResidentTranscriptSnapshot {
    pub(crate) activation_revision: u64,
    pub(crate) presentation_revision: u64,
    pub(crate) state: ResidentTranscriptSnapshotState,
    pub(crate) records: Vec<ResidentPresentationRecord>,
    pub(crate) resources: ResidentResourceSnapshot,
    pub(crate) realized_range: Option<Range<usize>>,
    pub(crate) visible_range: Option<Range<usize>>,
}

impl ResidentTranscriptSnapshot {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub(crate) fn record_count(&self) -> usize {
        self.records.len()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResidentResourceSnapshot {
    pub(crate) metadata: Vec<ResourceMetadata>,
    pub(crate) slices: Vec<ResidentResourceSlice>,
}

impl ResidentResourceSnapshot {
    pub(crate) fn metadata_for(&self, resource_id: &ResourceId) -> Option<&ResourceMetadata> {
        self.metadata
            .iter()
            .find(|metadata| &metadata.resource_id == resource_id)
    }

    pub(crate) fn slices_for<'a>(
        &'a self,
        resource_id: &'a ResourceId,
    ) -> impl Iterator<Item = &'a ResidentResourceSlice> + 'a {
        self.slices
            .iter()
            .filter(move |slice| &slice.resource_id == resource_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentResourceSlice {
    pub(crate) resource_id: ResourceId,
    pub(crate) revision: ProviderRevision,
    pub(crate) kind: ResourceKind,
    pub(crate) range: Range<u64>,
    pub(crate) bytes: Vec<u8>,
    pub(crate) complete: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum ResidentTranscriptSnapshotState {
    #[default]
    Empty,
    Unavailable {
        reason: String,
    },
    Incomplete {
        reason: TranscriptProviderHistoryReason,
        detail: Option<String>,
    },
    ProviderBacked {
        label: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentPresentationRecord {
    pub(crate) id: ResidentPresentationRecordId,
    pub(crate) kind: ResidentPresentationRecordKind,
    pub(crate) provenance: ResidentRecordProvenance,
    pub(crate) estimated_bytes: usize,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResidentPresentationRecordId(pub(crate) String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentPresentationRecordKind {
    TextChunk {
        narrative_kind: TranscriptNarrativeKind,
        text: String,
    },
    ResourceReference {
        resource_id: ResourceId,
        resource_kind: ResourceKind,
        label: Option<String>,
    },
    LocalUiFallback {
        reason: LocalPresentationReason,
        target: ResidentFallbackTarget,
    },
    LocalAffordance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LocalPresentationReason {
    BudgetRejected,
    PolicyDenied,
    ResourceUnavailable,
    PendingCoherentData,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentFallbackTarget {
    ProjectionRecord(ProjectionRecordId),
    Resource(ResourceId),
    ResourceRange {
        resource_id: ResourceId,
        range: Range<u64>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentRecordProvenance {
    pub(crate) source: ResidentRecordSource,
    pub(crate) projection_id: Option<ProjectionRecordId>,
    pub(crate) projection_revision: Option<ProviderRevision>,
    pub(crate) presentation_revision: u64,
    pub(crate) copy_source_range: Option<Range<u64>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentRecordSource {
    Syndic(SyndicSourceProvenance),
    LocalUi,
    LocalUiForSyndic(SyndicSourceProvenance),
}
