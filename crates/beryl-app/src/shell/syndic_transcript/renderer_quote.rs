use super::{
    RealizedFrameWindow, ResidentPresentationRecordId, ResidentQuoteCommand,
    ResidentSelectionUnavailable, ResidentTranscriptQuoteTarget, ResidentTranscriptSelection,
    ResidentTranscriptSnapshot, realized_resident_selectable_record_ids,
    resident_selection_command_for_realized_record_ids, resident_selection_frame_loss,
};

pub(crate) fn resident_quote_command_for_realized_record_ids(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    record_ids: &[ResidentPresentationRecordId],
) -> Result<ResidentQuoteCommand, ResidentSelectionUnavailable> {
    let selection_command =
        resident_selection_command_for_realized_record_ids(snapshot, frame_window, record_ids)?;
    if let Some(record) = snapshot.records.iter().find(|record| {
        record_ids.contains(&record.id)
            && matches!(
                record.provenance.source,
                super::ResidentRecordSource::LocalContext { .. }
            )
    }) {
        return Err(ResidentSelectionUnavailable::NonContentRecord {
            record_id: record.id.clone(),
        });
    }
    Ok(ResidentQuoteCommand::new(
        selection_command.presentation_revision,
        selection_command.records,
    ))
}

pub(crate) fn realized_resident_quotable_record_ids(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
) -> Vec<ResidentPresentationRecordId> {
    realized_resident_selectable_record_ids(snapshot, frame_window)
        .into_iter()
        .filter(|id| {
            snapshot.records.iter().any(|record| {
                record.id == *id
                    && !matches!(
                        record.provenance.source,
                        super::ResidentRecordSource::LocalContext { .. }
                    )
            })
        })
        .collect()
}

pub(crate) fn resident_quote_frame_loss(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    target: &ResidentTranscriptQuoteTarget,
) -> Option<ResidentSelectionUnavailable> {
    let selection =
        ResidentTranscriptSelection::new(target.presentation_revision, target.records.clone());
    resident_selection_frame_loss(snapshot, frame_window, &selection)
}
