use super::frame::RealizedFrameRecord;
use super::frame::RealizedFrameWindow;
use super::{
    selection::{
        ResidentSelectedRecord, ResidentSelectionCommand, ResidentSelectionRecordGeometry,
        ResidentSelectionUnavailable, ResidentTranscriptSelection, resident_selected_record,
    },
    snapshot::{
        ResidentPresentationRecord, ResidentPresentationRecordId, ResidentTranscriptSnapshot,
    },
};

pub(crate) fn resident_selection_command_for_realized_record_ids(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    record_ids: &[ResidentPresentationRecordId],
) -> Result<ResidentSelectionCommand, ResidentSelectionUnavailable> {
    if snapshot.presentation_revision != frame_window.presentation_revision {
        return Err(ResidentSelectionUnavailable::StalePresentationRevision {
            observed: frame_window.presentation_revision,
            current: snapshot.presentation_revision,
        });
    }

    let mut geometries = Vec::new();
    for record_id in record_ids {
        let (frame_record, record) =
            realized_record_for_selection(snapshot, frame_window, record_id)?;
        let geometry = selection_geometry_from_frame_record(frame_record);
        if !geometry.is_stable() {
            return Err(ResidentSelectionUnavailable::UnstableGeometry {
                record_id: record_id.clone(),
            });
        }

        resident_selected_record(record)?;
        geometries.push(geometry);
    }

    Ok(ResidentSelectionCommand::new(
        frame_window.presentation_revision,
        geometries,
    ))
}

pub(crate) fn realized_resident_selectable_record_ids(
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
            let geometry = selection_geometry_from_frame_record(frame_record);
            if !geometry.is_stable() {
                return None;
            }
            let record = snapshot_record_for_frame_record(snapshot, frame_record).ok()?;
            resident_selected_record(record).ok()?;
            Some(frame_record.record_id.clone())
        })
        .collect()
}

pub(crate) fn resident_selection_frame_loss(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    selection: &ResidentTranscriptSelection,
) -> Option<ResidentSelectionUnavailable> {
    if snapshot.presentation_revision != frame_window.presentation_revision {
        return Some(ResidentSelectionUnavailable::StalePresentationRevision {
            observed: frame_window.presentation_revision,
            current: snapshot.presentation_revision,
        });
    }
    if selection.presentation_revision != snapshot.presentation_revision {
        return Some(ResidentSelectionUnavailable::StalePresentationRevision {
            observed: selection.presentation_revision,
            current: snapshot.presentation_revision,
        });
    }

    for selected_record in &selection.records {
        if let Err(error) =
            validate_selected_record_still_realized(snapshot, frame_window, selected_record)
        {
            return Some(error);
        }
    }

    None
}

fn validate_selected_record_still_realized(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    selected_record: &ResidentSelectedRecord,
) -> Result<(), ResidentSelectionUnavailable> {
    let (frame_record, record) =
        realized_record_for_selection(snapshot, frame_window, &selected_record.record_id)?;
    let geometry = selection_geometry_from_frame_record(frame_record);
    if !geometry.is_stable() {
        return Err(ResidentSelectionUnavailable::UnstableGeometry {
            record_id: selected_record.record_id.clone(),
        });
    }

    let current_selected_record = resident_selected_record(record)?;
    if &current_selected_record != selected_record {
        return Err(ResidentSelectionUnavailable::StaleRecord {
            record_id: selected_record.record_id.clone(),
        });
    }

    Ok(())
}

fn realized_record_for_selection<'a>(
    snapshot: &'a ResidentTranscriptSnapshot,
    frame_window: &'a RealizedFrameWindow,
    record_id: &ResidentPresentationRecordId,
) -> Result<(&'a RealizedFrameRecord, &'a ResidentPresentationRecord), ResidentSelectionUnavailable>
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
            Err(ResidentSelectionUnavailable::RecordNotRealized {
                record_id: record_id.clone(),
            })
        } else {
            Err(ResidentSelectionUnavailable::RecordNotResident {
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
) -> Result<&'a ResidentPresentationRecord, ResidentSelectionUnavailable> {
    let Some(record) = snapshot.records.get(frame_record.index) else {
        return Err(ResidentSelectionUnavailable::RecordNotResident {
            record_id: frame_record.record_id.clone(),
        });
    };
    if record.id != frame_record.record_id {
        return Err(ResidentSelectionUnavailable::StaleRecord {
            record_id: frame_record.record_id.clone(),
        });
    }

    Ok(record)
}

fn selection_geometry_from_frame_record(
    frame_record: &RealizedFrameRecord,
) -> ResidentSelectionRecordGeometry {
    ResidentSelectionRecordGeometry::new(
        frame_record.record_id.clone(),
        frame_record.top_px,
        frame_record.height_px,
    )
}
