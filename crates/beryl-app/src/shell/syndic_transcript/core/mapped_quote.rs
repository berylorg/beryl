use super::*;

impl ResidentTranscriptCore {
    pub(crate) fn mapped_quote_markdown(
        &self,
        activation: u64,
        revision: u64,
        ranges: &[(ResidentPresentationRecordId, Range<usize>)],
    ) -> Result<String, ResidentSelectionUnavailable> {
        let snapshot = self.shared_presentation_snapshot();
        if snapshot.activation_revision != activation || snapshot.presentation_revision != revision
        {
            return Err(ResidentSelectionUnavailable::StalePresentationRevision {
                observed: revision,
                current: snapshot.presentation_revision,
            });
        }
        let realized = self
            .presentation
            .realized_range
            .as_ref()
            .ok_or(ResidentSelectionUnavailable::NoRealizedFrame)?;
        let mut bytes = 0usize;
        let mut previous = None;
        let mut parts = Vec::new();
        for (id, local) in ranges {
            let (order, index, record) = snapshot
                .records
                .iter()
                .enumerate()
                .flat_map(|(index, record)| {
                    record
                        .authored_parts()
                        .iter()
                        .map(move |part| (index, part))
                })
                .enumerate()
                .find_map(|(order, (index, part))| (&part.id == id).then_some((order, index, part)))
                .ok_or_else(|| ResidentSelectionUnavailable::RecordNotResident {
                    record_id: id.clone(),
                })?;
            if !realized.contains(&index) || previous.is_some_and(|previous| previous + 1 != order)
            {
                return Err(ResidentSelectionUnavailable::RecordNotRealized {
                    record_id: id.clone(),
                });
            }
            previous = Some(order);
            let ResidentRecordSource::Syndic(source) = &record.provenance.source else {
                return Err(ResidentSelectionUnavailable::NonContentRecord {
                    record_id: id.clone(),
                });
            };
            let stale = || ResidentSelectionUnavailable::MissingStableProvenance {
                record_id: id.clone(),
            };
            if source.turn_id.is_none()
                || source.item_id.is_none()
                || source.position.is_none()
                || record.provenance.projection_id.is_none()
                || record.provenance.projection_revision.is_none()
                || record.provenance.presentation_revision != revision
            {
                return Err(stale());
            }
            let text = match &record.kind {
                ResidentPresentationRecordKind::NarrativeReady { prepared } => prepared.source(),
                ResidentPresentationRecordKind::TextChunk { text, .. } => text.as_ref(),
                _ => {
                    return Err(ResidentSelectionUnavailable::NonContentRecord {
                        record_id: id.clone(),
                    });
                }
            };
            let span = source.source_range.as_ref().ok_or_else(stale)?;
            let start = span
                .start
                .checked_add(u64::try_from(local.start).map_err(|_| stale())?)
                .ok_or_else(stale)?;
            let end = span
                .start
                .checked_add(u64::try_from(local.end).map_err(|_| stale())?)
                .ok_or_else(stale)?;
            if start > end || end > span.end {
                return Err(stale());
            }
            let selected = text.get(local.clone()).ok_or_else(stale)?;
            bytes = bytes
                .checked_add(selected.len())
                .filter(|bytes| *bytes <= 65_536)
                .ok_or(ResidentSelectionUnavailable::CopyCapacity)?;
            parts.push(selected);
        }
        if bytes == 0 {
            return Err(ResidentSelectionUnavailable::EmptySelection);
        }
        let mut markdown = String::new();
        markdown
            .try_reserve_exact(bytes)
            .map_err(|_| ResidentSelectionUnavailable::CopyAllocation)?;
        for part in parts {
            markdown.push_str(part);
        }
        Ok(markdown)
    }
}
