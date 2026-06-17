use std::ops::Range;

use super::{
    frame::RealizedFrameRecord,
    provider::{
        ProjectionRecordId, ProviderRevision, ResourceId, ResourceKind, ResourceMetadata,
        SyndicSourceProvenance, TranscriptProviderRejectionReason,
    },
    selection::ResidentSelectionRecordGeometry,
    snapshot::{
        ResidentPresentationRecord, ResidentPresentationRecordId, ResidentPresentationRecordKind,
        ResidentRecordSource,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentMediaActionCommand {
    pub(crate) presentation_revision: u64,
    pub(crate) record: ResidentSelectionRecordGeometry,
}

impl ResidentMediaActionCommand {
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
pub(crate) struct ResidentTranscriptMediaActionTarget {
    pub(crate) presentation_revision: u64,
    pub(crate) record: ResidentMediaActionRecord,
}

impl ResidentTranscriptMediaActionTarget {
    pub(crate) fn new(presentation_revision: u64, record: ResidentMediaActionRecord) -> Self {
        Self {
            presentation_revision,
            record,
        }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        vec![self.record.record_id.clone()]
    }

    pub(crate) fn resource_id(&self) -> ResourceId {
        self.record.resource_id.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentMediaActionRecord {
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) source: SyndicSourceProvenance,
    pub(crate) projection_id: ProjectionRecordId,
    pub(crate) projection_revision: ProviderRevision,
    pub(crate) resource_id: ResourceId,
    pub(crate) resource_kind: ResourceKind,
    pub(crate) media_type: Option<String>,
    pub(crate) byte_len: u64,
    pub(crate) digest: Option<String>,
    pub(crate) source_range: Option<Range<u64>>,
    pub(crate) resource_range: Range<u64>,
    pub(crate) range_availability: ResidentMediaRangeAvailability,
    pub(crate) geometry: ResidentSelectionRecordGeometry,
}

impl ResidentMediaActionRecord {
    pub(crate) fn has_same_resident_identity_as(&self, other: &Self) -> bool {
        self.record_id == other.record_id
            && self.source == other.source
            && self.projection_id == other.projection_id
            && self.projection_revision == other.projection_revision
            && self.resource_id == other.resource_id
            && self.resource_kind == other.resource_kind
            && self.media_type == other.media_type
            && self.byte_len == other.byte_len
            && self.digest == other.digest
            && self.source_range == other.source_range
            && self.resource_range == other.resource_range
            && self.geometry == other.geometry
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentMediaRangeAvailability {
    Resident {
        requested_range: Range<u64>,
        resident_range: Range<u64>,
        complete: bool,
    },
    Demandable {
        range: Range<u64>,
    },
}

impl ResidentMediaRangeAvailability {
    pub(crate) fn requested_range(&self) -> Range<u64> {
        match self {
            Self::Resident {
                requested_range, ..
            } => requested_range.clone(),
            Self::Demandable { range } => range.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentTranscriptMediaPayload {
    pub(crate) presentation_revision: u64,
    pub(crate) record: ResidentMediaActionRecord,
    pub(crate) range: Range<u64>,
    pub(crate) bytes: Vec<u8>,
    pub(crate) complete: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidentMediaActionOutcome {
    Targeted(ResidentTranscriptMediaActionTarget),
    Cleared,
    Unavailable(ResidentMediaActionUnavailable),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentMediaActionUnavailable {
    NoActiveMediaActionTarget,
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
    NonMediaRecord {
        record_id: ResidentPresentationRecordId,
    },
    MissingStableProvenance {
        record_id: ResidentPresentationRecordId,
    },
    MissingResourceMetadata {
        record_id: ResidentPresentationRecordId,
        resource_id: ResourceId,
    },
    RejectedResource {
        resource_id: ResourceId,
        reason: TranscriptProviderRejectionReason,
    },
    RejectedResourceRange {
        resource_id: ResourceId,
        range: Range<u64>,
        reason: TranscriptProviderRejectionReason,
    },
    ResourceRangeNotResident {
        resource_id: ResourceId,
        range: Range<u64>,
    },
    InvalidResourceRange {
        resource_id: ResourceId,
        range: Range<u64>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentMediaReference {
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) source: SyndicSourceProvenance,
    pub(crate) projection_id: ProjectionRecordId,
    pub(crate) projection_revision: ProviderRevision,
    pub(crate) resource_id: ResourceId,
    pub(crate) resource_kind: ResourceKind,
    pub(crate) source_range: Option<Range<u64>>,
    pub(crate) resource_range: Range<u64>,
}

pub(crate) fn resident_media_reference(
    record: &ResidentPresentationRecord,
) -> Result<ResidentMediaReference, ResidentMediaActionUnavailable> {
    let (resource_id, resource_kind) = match &record.kind {
        ResidentPresentationRecordKind::ResourceReference {
            resource_id,
            resource_kind,
            ..
        } if resource_kind_is_media(resource_kind) => (resource_id.clone(), resource_kind.clone()),
        _ => {
            return Err(ResidentMediaActionUnavailable::NonMediaRecord {
                record_id: record.id.clone(),
            });
        }
    };

    let ResidentRecordSource::Syndic(source) = &record.provenance.source else {
        return Err(ResidentMediaActionUnavailable::NonMediaRecord {
            record_id: record.id.clone(),
        });
    };
    let Some(projection_id) = &record.provenance.projection_id else {
        return Err(ResidentMediaActionUnavailable::MissingStableProvenance {
            record_id: record.id.clone(),
        });
    };
    let Some(projection_revision) = record.provenance.projection_revision else {
        return Err(ResidentMediaActionUnavailable::MissingStableProvenance {
            record_id: record.id.clone(),
        });
    };
    let Some(resource_range) = source.resource_range.clone() else {
        return Err(ResidentMediaActionUnavailable::MissingStableProvenance {
            record_id: record.id.clone(),
        });
    };

    if !source_has_stable_media_provenance(source, projection_id, &resource_id) {
        return Err(ResidentMediaActionUnavailable::MissingStableProvenance {
            record_id: record.id.clone(),
        });
    }

    Ok(ResidentMediaReference {
        record_id: record.id.clone(),
        source: source.clone(),
        projection_id: projection_id.clone(),
        projection_revision,
        resource_id,
        resource_kind,
        source_range: source.source_range.clone(),
        resource_range,
    })
}

pub(crate) fn resident_media_action_record(
    reference: ResidentMediaReference,
    metadata: ResourceMetadata,
    range_availability: ResidentMediaRangeAvailability,
    geometry: ResidentSelectionRecordGeometry,
) -> Result<ResidentMediaActionRecord, ResidentMediaActionUnavailable> {
    if metadata.resource_id != reference.resource_id || metadata.kind != reference.resource_kind {
        return Err(ResidentMediaActionUnavailable::StaleRecord {
            record_id: reference.record_id,
        });
    }

    Ok(ResidentMediaActionRecord {
        record_id: reference.record_id,
        source: reference.source,
        projection_id: reference.projection_id,
        projection_revision: reference.projection_revision,
        resource_id: metadata.resource_id,
        resource_kind: metadata.kind,
        media_type: metadata.media_type,
        byte_len: metadata.byte_len,
        digest: metadata.digest,
        source_range: reference.source_range,
        resource_range: reference.resource_range,
        range_availability,
        geometry,
    })
}

pub(crate) fn resource_kind_is_media(kind: &ResourceKind) -> bool {
    matches!(kind, ResourceKind::Image | ResourceKind::GeneratedImage)
}

fn source_has_stable_media_provenance(
    source: &SyndicSourceProvenance,
    projection_id: &ProjectionRecordId,
    resource_id: &ResourceId,
) -> bool {
    source.position.is_some()
        && source.turn_id.is_some()
        && source.item_id.is_some()
        && source.projection_id.as_ref() == Some(projection_id)
        && source.resource_id.as_ref() == Some(resource_id)
}
