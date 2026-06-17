use std::ops::Range;

use super::{
    frame::{RealizedFrameRecord, RealizedFrameWindow},
    media_action::{
        ResidentMediaActionCommand, ResidentMediaActionRecord, ResidentMediaActionUnavailable,
        ResidentMediaRangeAvailability, ResidentTranscriptMediaActionTarget,
        resident_media_action_record, resident_media_reference,
    },
    provider::ResourceId,
    selection::ResidentSelectionRecordGeometry,
    snapshot::{
        ResidentPresentationRecord, ResidentPresentationRecordId, ResidentResourceSlice,
        ResidentTranscriptSnapshot,
    },
};

pub(crate) fn resident_media_action_command_for_realized_record_id(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    record_id: &ResidentPresentationRecordId,
) -> Result<ResidentMediaActionCommand, ResidentMediaActionUnavailable> {
    if snapshot.presentation_revision != frame_window.presentation_revision {
        return Err(ResidentMediaActionUnavailable::StalePresentationRevision {
            observed: frame_window.presentation_revision,
            current: snapshot.presentation_revision,
        });
    }

    let (frame_record, record) =
        realized_record_for_media_action(snapshot, frame_window, record_id)?;
    let geometry = media_action_geometry_from_frame_record(frame_record);
    if !geometry.is_stable() {
        return Err(ResidentMediaActionUnavailable::UnstableGeometry {
            record_id: record_id.clone(),
        });
    }

    renderer_media_action_record(snapshot, record, geometry.clone())?;
    Ok(ResidentMediaActionCommand::new(
        frame_window.presentation_revision,
        geometry,
    ))
}

pub(crate) fn realized_resident_media_action_record_ids(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
) -> Vec<ResidentPresentationRecordId> {
    if snapshot.presentation_revision != frame_window.presentation_revision {
        return Vec::new();
    }

    frame_window
        .records
        .iter()
        .filter_map(|frame_record| {
            let geometry = media_action_geometry_from_frame_record(frame_record);
            if !geometry.is_stable() {
                return None;
            }
            let record = snapshot_record_for_frame_record(snapshot, frame_record).ok()?;
            renderer_media_action_record(snapshot, record, geometry).ok()?;
            Some(frame_record.record_id.clone())
        })
        .collect()
}

pub(crate) fn resident_media_action_frame_loss(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    target: &ResidentTranscriptMediaActionTarget,
) -> Option<ResidentMediaActionUnavailable> {
    if snapshot.presentation_revision != frame_window.presentation_revision {
        return Some(ResidentMediaActionUnavailable::StalePresentationRevision {
            observed: frame_window.presentation_revision,
            current: snapshot.presentation_revision,
        });
    }
    if target.presentation_revision != snapshot.presentation_revision {
        return Some(ResidentMediaActionUnavailable::StalePresentationRevision {
            observed: target.presentation_revision,
            current: snapshot.presentation_revision,
        });
    }

    validate_media_action_target_still_realized(snapshot, frame_window, target).err()
}

fn validate_media_action_target_still_realized(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    target: &ResidentTranscriptMediaActionTarget,
) -> Result<(), ResidentMediaActionUnavailable> {
    let (frame_record, record) =
        realized_record_for_media_action(snapshot, frame_window, &target.record.record_id)?;
    let geometry = media_action_geometry_from_frame_record(frame_record);
    if !geometry.is_stable() {
        return Err(ResidentMediaActionUnavailable::UnstableGeometry {
            record_id: target.record.record_id.clone(),
        });
    }

    let current_target_record = renderer_media_action_record(snapshot, record, geometry)?;
    if !current_target_record.has_same_resident_identity_as(&target.record) {
        return Err(ResidentMediaActionUnavailable::StaleRecord {
            record_id: target.record.record_id.clone(),
        });
    }

    Ok(())
}

fn renderer_media_action_record(
    snapshot: &ResidentTranscriptSnapshot,
    record: &ResidentPresentationRecord,
    geometry: ResidentSelectionRecordGeometry,
) -> Result<ResidentMediaActionRecord, ResidentMediaActionUnavailable> {
    let reference = resident_media_reference(record)?;
    let Some(metadata) = snapshot
        .resources
        .metadata_for(&reference.resource_id)
        .cloned()
    else {
        return Err(ResidentMediaActionUnavailable::MissingResourceMetadata {
            record_id: record.id.clone(),
            resource_id: reference.resource_id,
        });
    };
    let range_availability = media_range_availability_from_snapshot(
        snapshot,
        &reference.resource_id,
        &reference.resource_range,
    )?;

    resident_media_action_record(reference, metadata, range_availability, geometry)
}

fn media_range_availability_from_snapshot(
    snapshot: &ResidentTranscriptSnapshot,
    resource_id: &ResourceId,
    range: &Range<u64>,
) -> Result<ResidentMediaRangeAvailability, ResidentMediaActionUnavailable> {
    if range.start >= range.end {
        return Err(ResidentMediaActionUnavailable::InvalidResourceRange {
            resource_id: resource_id.clone(),
            range: range.clone(),
        });
    }
    if let Some(slice) = resident_resource_slice_covering(snapshot, resource_id, range) {
        return Ok(ResidentMediaRangeAvailability::Resident {
            requested_range: range.clone(),
            resident_range: slice.range.clone(),
            complete: slice.complete,
        });
    }

    Ok(ResidentMediaRangeAvailability::Demandable {
        range: range.clone(),
    })
}

fn resident_resource_slice_covering<'a>(
    snapshot: &'a ResidentTranscriptSnapshot,
    resource_id: &'a ResourceId,
    range: &Range<u64>,
) -> Option<&'a ResidentResourceSlice> {
    snapshot
        .resources
        .slices_for(resource_id)
        .find(|slice| slice.range.start <= range.start && slice.range.end >= range.end)
}

fn realized_record_for_media_action<'a>(
    snapshot: &'a ResidentTranscriptSnapshot,
    frame_window: &'a RealizedFrameWindow,
    record_id: &ResidentPresentationRecordId,
) -> Result<(&'a RealizedFrameRecord, &'a ResidentPresentationRecord), ResidentMediaActionUnavailable>
{
    let Some(frame_record) = frame_window
        .records
        .iter()
        .find(|frame_record| &frame_record.record_id == record_id)
    else {
        return if snapshot
            .records
            .iter()
            .any(|record| &record.id == record_id)
        {
            Err(ResidentMediaActionUnavailable::RecordNotRealized {
                record_id: record_id.clone(),
            })
        } else {
            Err(ResidentMediaActionUnavailable::RecordNotResident {
                record_id: record_id.clone(),
            })
        };
    };
    let record = snapshot_record_for_frame_record(snapshot, frame_record)?;

    Ok((frame_record, record))
}

fn snapshot_record_for_frame_record<'a>(
    snapshot: &'a ResidentTranscriptSnapshot,
    frame_record: &RealizedFrameRecord,
) -> Result<&'a ResidentPresentationRecord, ResidentMediaActionUnavailable> {
    let Some(record) = snapshot.records.get(frame_record.index) else {
        return Err(ResidentMediaActionUnavailable::RecordNotResident {
            record_id: frame_record.record_id.clone(),
        });
    };
    if record.id != frame_record.record_id {
        return Err(ResidentMediaActionUnavailable::StaleRecord {
            record_id: frame_record.record_id.clone(),
        });
    }

    Ok(record)
}

fn media_action_geometry_from_frame_record(
    frame_record: &RealizedFrameRecord,
) -> ResidentSelectionRecordGeometry {
    ResidentSelectionRecordGeometry::new(
        frame_record.record_id.clone(),
        frame_record.top_px,
        frame_record.height_px,
    )
}
