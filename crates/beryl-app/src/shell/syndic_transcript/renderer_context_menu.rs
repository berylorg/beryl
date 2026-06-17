use super::{
    context_menu::{
        ResidentContextMenuCommand, ResidentContextMenuUnavailable,
        ResidentTranscriptContextMenuTarget, resident_context_menu_record,
    },
    frame::{RealizedFrameRecord, RealizedFrameWindow},
    selection::ResidentSelectionRecordGeometry,
    snapshot::{
        ResidentPresentationRecord, ResidentPresentationRecordId, ResidentTranscriptSnapshot,
    },
};

pub(crate) fn resident_context_menu_command_for_realized_record_id(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    record_id: &ResidentPresentationRecordId,
) -> Result<ResidentContextMenuCommand, ResidentContextMenuUnavailable> {
    if snapshot.presentation_revision != frame_window.presentation_revision {
        return Err(ResidentContextMenuUnavailable::StalePresentationRevision {
            observed: frame_window.presentation_revision,
            current: snapshot.presentation_revision,
        });
    }

    let (frame_record, record) =
        realized_record_for_context_menu(snapshot, frame_window, record_id)?;
    let geometry = context_menu_geometry_from_frame_record(frame_record);
    if !geometry.is_stable() {
        return Err(ResidentContextMenuUnavailable::UnstableGeometry {
            record_id: record_id.clone(),
        });
    }

    resident_context_menu_record(record, geometry.clone())?;
    Ok(ResidentContextMenuCommand::new(
        frame_window.presentation_revision,
        geometry,
    ))
}

pub(crate) fn realized_resident_context_menu_record_ids(
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
            let geometry = context_menu_geometry_from_frame_record(frame_record);
            if !geometry.is_stable() {
                return None;
            }
            let record = snapshot_record_for_frame_record(snapshot, frame_record).ok()?;
            resident_context_menu_record(record, geometry).ok()?;
            Some(frame_record.record_id.clone())
        })
        .collect()
}

pub(crate) fn resident_context_menu_frame_loss(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    target: &ResidentTranscriptContextMenuTarget,
) -> Option<ResidentContextMenuUnavailable> {
    if snapshot.presentation_revision != frame_window.presentation_revision {
        return Some(ResidentContextMenuUnavailable::StalePresentationRevision {
            observed: frame_window.presentation_revision,
            current: snapshot.presentation_revision,
        });
    }
    if target.presentation_revision != snapshot.presentation_revision {
        return Some(ResidentContextMenuUnavailable::StalePresentationRevision {
            observed: target.presentation_revision,
            current: snapshot.presentation_revision,
        });
    }

    validate_context_menu_target_still_realized(snapshot, frame_window, target).err()
}

fn validate_context_menu_target_still_realized(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    target: &ResidentTranscriptContextMenuTarget,
) -> Result<(), ResidentContextMenuUnavailable> {
    let (frame_record, record) =
        realized_record_for_context_menu(snapshot, frame_window, &target.record.record_id)?;
    let geometry = context_menu_geometry_from_frame_record(frame_record);
    if !geometry.is_stable() {
        return Err(ResidentContextMenuUnavailable::UnstableGeometry {
            record_id: target.record.record_id.clone(),
        });
    }

    let current_target_record = resident_context_menu_record(record, geometry)?;
    if current_target_record != target.record {
        return Err(ResidentContextMenuUnavailable::StaleRecord {
            record_id: target.record.record_id.clone(),
        });
    }

    Ok(())
}

fn realized_record_for_context_menu<'a>(
    snapshot: &'a ResidentTranscriptSnapshot,
    frame_window: &'a RealizedFrameWindow,
    record_id: &ResidentPresentationRecordId,
) -> Result<(&'a RealizedFrameRecord, &'a ResidentPresentationRecord), ResidentContextMenuUnavailable>
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
            Err(ResidentContextMenuUnavailable::RecordNotRealized {
                record_id: record_id.clone(),
            })
        } else {
            Err(ResidentContextMenuUnavailable::RecordNotResident {
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
) -> Result<&'a ResidentPresentationRecord, ResidentContextMenuUnavailable> {
    let Some(record) = snapshot.records.get(frame_record.index) else {
        return Err(ResidentContextMenuUnavailable::RecordNotResident {
            record_id: frame_record.record_id.clone(),
        });
    };
    if record.id != frame_record.record_id {
        return Err(ResidentContextMenuUnavailable::StaleRecord {
            record_id: frame_record.record_id.clone(),
        });
    }

    Ok(record)
}

fn context_menu_geometry_from_frame_record(
    frame_record: &RealizedFrameRecord,
) -> ResidentSelectionRecordGeometry {
    ResidentSelectionRecordGeometry::new(
        frame_record.record_id.clone(),
        frame_record.top_px,
        frame_record.height_px,
    )
}
