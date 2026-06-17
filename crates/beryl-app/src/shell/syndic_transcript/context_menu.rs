use std::ops::Range;

use super::{
    frame::RealizedFrameRecord,
    provider::{
        ProjectionRecordId, ProviderRevision, ResourceId, ResourceKind, SyndicSourceProvenance,
    },
    selection::ResidentSelectionRecordGeometry,
    snapshot::{
        ResidentPresentationRecord, ResidentPresentationRecordId, ResidentPresentationRecordKind,
        ResidentRecordSource,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentContextMenuCommand {
    pub(crate) presentation_revision: u64,
    pub(crate) record: ResidentSelectionRecordGeometry,
}

impl ResidentContextMenuCommand {
    pub(crate) fn new(presentation_revision: u64, record: ResidentSelectionRecordGeometry) -> Self {
        Self {
            presentation_revision,
            record,
        }
    }

    pub(crate) fn from_realized_frame_record(
        presentation_revision: u64,
        record: &RealizedFrameRecord,
    ) -> Self {
        Self {
            presentation_revision,
            record: ResidentSelectionRecordGeometry::new(
                record.record_id.clone(),
                record.top_px,
                record.height_px,
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentTranscriptContextMenuTarget {
    pub(crate) presentation_revision: u64,
    pub(crate) record: ResidentContextMenuRecord,
}

impl ResidentTranscriptContextMenuTarget {
    pub(crate) fn new(presentation_revision: u64, record: ResidentContextMenuRecord) -> Self {
        Self {
            presentation_revision,
            record,
        }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        vec![self.record.record_id.clone()]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentContextMenuRecord {
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) source: SyndicSourceProvenance,
    pub(crate) projection_id: ProjectionRecordId,
    pub(crate) projection_revision: ProviderRevision,
    pub(crate) content_kind: ResidentContextMenuContentKind,
    pub(crate) source_range: Option<Range<u64>>,
    pub(crate) resource_range: Option<Range<u64>>,
    pub(crate) geometry: ResidentSelectionRecordGeometry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentContextMenuContentKind {
    TextChunk,
    ResourceReference {
        resource_id: ResourceId,
        resource_kind: ResourceKind,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidentContextMenuOutcome {
    Targeted(ResidentTranscriptContextMenuTarget),
    Cleared,
    Unavailable(ResidentContextMenuUnavailable),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentContextMenuUnavailable {
    NoActiveContextMenuTarget,
    NoRealizedFrame,
    StalePresentationRevision {
        observed: u64,
        current: u64,
    },
    StaleRecord {
        record_id: ResidentPresentationRecordId,
    },
    RecordNotResident {
        record_id: ResidentPresentationRecordId,
    },
    RecordNotRealized {
        record_id: ResidentPresentationRecordId,
    },
    UnstableGeometry {
        record_id: ResidentPresentationRecordId,
    },
    NonContentRecord {
        record_id: ResidentPresentationRecordId,
    },
    MissingStableProvenance {
        record_id: ResidentPresentationRecordId,
    },
}

pub(crate) fn resident_context_menu_record(
    record: &ResidentPresentationRecord,
    geometry: ResidentSelectionRecordGeometry,
) -> Result<ResidentContextMenuRecord, ResidentContextMenuUnavailable> {
    let content_kind = match &record.kind {
        ResidentPresentationRecordKind::TextChunk { text, .. } if !text.is_empty() => {
            ResidentContextMenuContentKind::TextChunk
        }
        ResidentPresentationRecordKind::ResourceReference {
            resource_id,
            resource_kind,
            ..
        } => ResidentContextMenuContentKind::ResourceReference {
            resource_id: resource_id.clone(),
            resource_kind: resource_kind.clone(),
        },
        _ => {
            return Err(ResidentContextMenuUnavailable::NonContentRecord {
                record_id: record.id.clone(),
            });
        }
    };

    let ResidentRecordSource::Syndic(source) = &record.provenance.source else {
        return Err(ResidentContextMenuUnavailable::NonContentRecord {
            record_id: record.id.clone(),
        });
    };
    let Some(projection_id) = &record.provenance.projection_id else {
        return Err(ResidentContextMenuUnavailable::MissingStableProvenance {
            record_id: record.id.clone(),
        });
    };
    let Some(projection_revision) = record.provenance.projection_revision else {
        return Err(ResidentContextMenuUnavailable::MissingStableProvenance {
            record_id: record.id.clone(),
        });
    };

    if !source_has_stable_context_menu_provenance(source, projection_id, &content_kind) {
        return Err(ResidentContextMenuUnavailable::MissingStableProvenance {
            record_id: record.id.clone(),
        });
    }

    Ok(ResidentContextMenuRecord {
        record_id: record.id.clone(),
        source: source.clone(),
        projection_id: projection_id.clone(),
        projection_revision,
        content_kind,
        source_range: source.source_range.clone(),
        resource_range: source.resource_range.clone(),
        geometry,
    })
}

fn source_has_stable_context_menu_provenance(
    source: &SyndicSourceProvenance,
    projection_id: &ProjectionRecordId,
    content_kind: &ResidentContextMenuContentKind,
) -> bool {
    if source.position.is_none()
        || source.turn_id.is_none()
        || source.item_id.is_none()
        || source.projection_id.as_ref() != Some(projection_id)
    {
        return false;
    }

    match content_kind {
        ResidentContextMenuContentKind::TextChunk => true,
        ResidentContextMenuContentKind::ResourceReference { resource_id, .. } => {
            source.resource_id.as_ref() == Some(resource_id)
        }
    }
}
