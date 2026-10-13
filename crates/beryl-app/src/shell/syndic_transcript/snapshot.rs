use std::ops::Range;
use std::sync::Arc;

use super::provider::{
    ProjectionRecordId, ProviderRevision, ResourceId, ResourceKind, ResourceMetadata,
    SyndicSourceProvenance, TranscriptNarrativeKind, TranscriptProviderHistoryReason,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResidentTranscriptSnapshot {
    pub(crate) activation_revision: u64,
    pub(crate) presentation_revision: u64,
    pub(crate) state: ResidentTranscriptSnapshotState,
    pub(crate) records: Arc<Vec<ResidentPresentationRecord>>,
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
    pub(crate) metadata: Arc<Vec<ResourceMetadata>>,
    pub(crate) slices: Arc<Vec<ResidentResourceSlice>>,
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
    pub(crate) bytes: std::sync::Arc<[u8]>,
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

impl ResidentPresentationRecord {
    pub(crate) fn authored_parts(&self) -> &[Self] {
        match &self.kind {
            ResidentPresentationRecordKind::AuthoredInlineParts { parts } => parts,
            _ => std::slice::from_ref(self),
        }
    }

    pub(crate) fn authored_part(&self, id: &ResidentPresentationRecordId) -> Option<&Self> {
        self.authored_parts().iter().find(|part| part.id == *id)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResidentPresentationRecordId(pub(crate) String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentPresentationRecordKind {
    AuthoredInlineParts {
        parts: std::sync::Arc<[ResidentPresentationRecord]>,
    },
    ImageMarker(super::provider::TranscriptImageMarker),
    NarrativePending {
        source: std::sync::Arc<syndic_storage::ProjectionPayload>,
    },
    NarrativeReady {
        prepared: std::sync::Arc<syndic_storage::narrative::PreparedNarrative>,
    },
    NarrativeUnavailable {
        source: std::sync::Arc<syndic_storage::ProjectionPayload>,
        reason: syndic_storage::narrative::NarrativePreparationErrorKind,
    },
    InlineTable {
        prepared: std::sync::Arc<syndic_storage::inline_table::PreparedInlineTable>,
    },
    InlineTableUnavailable {
        source: std::sync::Arc<str>,
        reason: syndic_storage::inline_table::InlineTablePreparationErrorKind,
    },
    TextChunk {
        narrative_kind: TranscriptNarrativeKind,
        text: std::sync::Arc<str>,
        plain_text: Option<std::sync::Arc<str>>,
    },
    ResourceReference {
        resource_id: ResourceId,
        resource_kind: ResourceKind,
        label: Option<std::sync::Arc<str>>,
    },
    LocalUiFallback {
        reason: LocalPresentationReason,
        target: ResidentFallbackTarget,
    },
    LocalAffordance,
    ContextUnavailable,
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
    LocalContext {
        group: std::sync::Arc<super::SyntheticContextProvenance>,
        chunk_index: usize,
    },
}
