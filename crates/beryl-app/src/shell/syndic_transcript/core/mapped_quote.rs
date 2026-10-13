use super::*;

impl ResidentTranscriptCore {
    pub(crate) fn resource_discussion_source(
        &self,
        owner: &super::ResourceTranscriptDemand,
        activation: u64,
        presentation: u64,
        range: Range<u64>,
        text: String,
    ) -> Option<CapturedDiscussionSource> {
        let snapshot = self.shared_presentation_snapshot();
        if activation != snapshot.activation_revision
            || presentation != snapshot.presentation_revision
            || owner.activation_revision != activation
            || owner.kind != super::super::ResourceKind::Code
        {
            return None;
        }
        let record = snapshot
            .records
            .iter()
            .flat_map(|record| record.authored_parts())
            .find(|record| {
                record.id == owner.record_id
                    && record.provenance.source
                        == ResidentRecordSource::Syndic(owner.provenance.clone())
            })?;
        let source = &owner.provenance;
        let projection = record.provenance.projection_id.as_ref()?;
        let view = self.resident.view_records.iter().find(|view| {
            &view.projection_id == projection
                && view.provenance.transcript_entry == source.transcript_entry
        })?;
        if !matches!(
            view.narrative_kind,
            super::super::provider::TranscriptNarrativeKind::AssistantCommentary
                | super::super::provider::TranscriptNarrativeKind::AssistantFinalAnswer
                | super::super::provider::TranscriptNarrativeKind::AssistantText
        ) {
            return None;
        }
        let span = source.source_range.as_ref()?;
        if range.start < span.start
            || range.end > span.end
            || text.len() as u64 != range.end.checked_sub(range.start)?
        {
            return None;
        }
        Some(CapturedDiscussionSource {
            source: syndic_storage::DiscussionContextSource::new(
                source.view_id.0.parse().ok()?,
                source.turn_id.as_ref()?.0.parse().ok()?,
                source.item_id.as_ref()?.0.parse().ok()?,
                projection.0.parse().ok()?,
                beryl_model::ProjectionRevision::new(record.provenance.projection_revision?.0)
                    .ok()?,
                syndic_storage::DiscussionContextRange::new(range.start, range.end).ok()?,
            ),
            entry: source.transcript_entry?,
            text: syndic_storage::DiscussionContextText::new(text).ok()?,
        })
    }

    pub(crate) fn mapped_discussion_source(
        &self,
        activation: u64,
        revision: u64,
        ranges: &[(ResidentPresentationRecordId, Range<usize>)],
    ) -> Option<CapturedDiscussionSource> {
        let text = self
            .mapped_quote_markdown(activation, revision, ranges)
            .ok()?;
        let snapshot = self.shared_presentation_snapshot();
        let mut captured = None;
        for (id, local) in ranges {
            let record = snapshot
                .records
                .iter()
                .flat_map(|row| row.authored_parts())
                .find(|part| &part.id == id)?;
            let ResidentRecordSource::Syndic(source) = &record.provenance.source else {
                return None;
            };
            let projection = record.provenance.projection_id.as_ref()?;
            let view = self.resident.view_records.iter().find(|row| {
                &row.projection_id == projection
                    && row.provenance.transcript_entry == source.transcript_entry
            })?;
            if !matches!(
                view.narrative_kind,
                super::super::provider::TranscriptNarrativeKind::AssistantCommentary
                    | super::super::provider::TranscriptNarrativeKind::AssistantFinalAnswer
                    | super::super::provider::TranscriptNarrativeKind::AssistantText
            ) {
                return None;
            }
            let span = source.source_range.as_ref()?;
            let start = span.start.checked_add(u64::try_from(local.start).ok()?)?;
            let end = span.start.checked_add(u64::try_from(local.end).ok()?)?;
            let owner = (
                source.view_id.0.parse().ok()?,
                source.turn_id.as_ref()?.0.parse().ok()?,
                source.item_id.as_ref()?.0.parse().ok()?,
                projection.0.parse().ok()?,
                beryl_model::ProjectionRevision::new(record.provenance.projection_revision?.0)
                    .ok()?,
                source.transcript_entry?,
            );
            if let Some((previous, _, prior_end)) = &captured {
                if previous != &owner || *prior_end != start {
                    return None;
                }
            }
            let initial = captured
                .as_ref()
                .map(|(_, initial, _)| *initial)
                .unwrap_or(start);
            captured = Some((owner, initial, end));
        }
        let ((thread, turn, item, projection, revision, entry), start, end) = captured?;
        Some(CapturedDiscussionSource {
            source: syndic_storage::DiscussionContextSource::new(
                thread,
                turn,
                item,
                projection,
                revision,
                syndic_storage::DiscussionContextRange::new(start, end).ok()?,
            ),
            entry,
            text: syndic_storage::DiscussionContextText::new(text).ok()?,
        })
    }

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
