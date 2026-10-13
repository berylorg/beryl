use super::*;

fn inline_source(record: &ResidentPresentationRecord) -> Option<&SyndicSourceProvenance> {
    if !matches!(
        record.kind,
        ResidentPresentationRecordKind::NarrativePending { .. }
            | ResidentPresentationRecordKind::NarrativeReady { .. }
            | ResidentPresentationRecordKind::NarrativeUnavailable { .. }
            | ResidentPresentationRecordKind::ImageMarker(_)
    ) {
        return None;
    }
    let ResidentRecordSource::Syndic(source) = &record.provenance.source else {
        return None;
    };
    source.item_id.as_ref()?;
    source.turn_id.as_ref()?;
    source.source_range.as_ref()?;
    if source.transcript_entry?.position().get() != source.position?.0 {
        return None;
    }
    Some(source)
}

fn inline_text(record: &ResidentPresentationRecord) -> Option<&str> {
    match &record.kind {
        ResidentPresentationRecordKind::NarrativeReady { prepared } => Some(prepared.source()),
        ResidentPresentationRecordKind::NarrativePending { source }
        | ResidentPresentationRecordKind::NarrativeUnavailable { source, .. } => {
            match source.as_ref() {
                syndic_storage::ProjectionPayload::InlineMarkdown { source, .. } => {
                    Some(source.as_ref())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn adjacent(left: &ResidentPresentationRecord, right: &ResidentPresentationRecord) -> bool {
    if inline_text(left).is_some_and(|source| source.ends_with(['\n', '\r']))
        || inline_text(right).is_some_and(|source| source.starts_with(['\n', '\r']))
    {
        return false;
    }
    let Some(left) = inline_source(left) else {
        return false;
    };
    let Some(right) = inline_source(right) else {
        return false;
    };
    left.view_id == right.view_id
        && left.item_id == right.item_id
        && left.turn_id == right.turn_id
        && left.transcript_entry.unwrap().generation()
            == right.transcript_entry.unwrap().generation()
        && left.position.unwrap().0.checked_add(1) == right.position.map(|position| position.0)
        && left.source_range.as_ref().unwrap().end == right.source_range.as_ref().unwrap().start
}

pub(super) fn group_inline_parts(records: &mut Vec<ResidentPresentationRecord>) {
    let mut grouped = Vec::with_capacity(records.len());
    let mut remaining = std::mem::take(records).into_iter().peekable();
    while let Some(first) = remaining.next() {
        let mut parts = vec![first];
        while remaining
            .peek()
            .is_some_and(|next| adjacent(parts.last().unwrap(), next))
        {
            parts.push(remaining.next().unwrap());
        }
        if parts.len() > 1
            && parts
                .iter()
                .any(|part| matches!(part.kind, ResidentPresentationRecordKind::ImageMarker(_)))
        {
            let first = &parts[0];
            let grouped_record = ResidentPresentationRecord {
                id: first.id.clone(),
                provenance: first.provenance.clone(),
                estimated_bytes: parts
                    .iter()
                    .map(|part| part.estimated_bytes)
                    .sum::<usize>()
                    .saturating_add(
                        parts.len() * std::mem::size_of::<ResidentPresentationRecord>(),
                    ),
                kind: ResidentPresentationRecordKind::AuthoredInlineParts {
                    parts: parts.into(),
                },
            };
            grouped.push(grouped_record);
        } else {
            grouped.extend(parts);
        }
    }
    *records = grouped;
}
