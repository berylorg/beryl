use super::*;

pub(super) fn validate_entry(
    reader: &DomainReader<'_, SyndicDomain>,
    entry: &crate::ActivityQueryEntryRecord,
) -> Result<(), SyndicValidationError> {
    let event = require::<SourceEventsFamily>(
        reader,
        &TurnEventKey {
            owner: entry.source().turn_id(),
            ordinal: entry.source_event(),
        },
        "retired Activity entry source event is missing",
    )?;
    let crate::SourceEventPayload::ItemFrame { item_id, frame } = event.payload() else {
        return invariant("retired Activity entry does not name an item frame");
    };
    let running = !frame.stream_state().is_complete();
    let lifecycle = if running {
        crate::ProviderItemLifecycle::Started
    } else {
        crate::ProviderItemLifecycle::Completed
    };
    let timestamp = match frame.stream_state().started_at() {
        Some(started) => started.get(),
        None => match frame.observation() {
            crate::ProviderFrameObservationSummaryV1::Completed(completed) if !running => {
                completed.get()
            }
            _ => return invariant("retired Activity frame has no lifecycle timestamp"),
        },
    };
    if event.turn_id() != entry.source().turn_id()
        || event.sequence() != entry.source_event()
        || event.source() != Some(entry.source().cas_item().turn())
        || *item_id != entry.item_id()
        || frame.frame().item_id() != entry.source().cas_item().item_id()
        || frame.frame().item_kind() != entry.provider_kind()
        || lifecycle != entry.provider_lifecycle()
        || entry.order()
            != crate::ActivityQueryOrder::new(
                running,
                crate::SyndicTimestamp::from_unix_millis(timestamp),
                *item_id,
            )
    {
        return invariant("retired Activity entry disagrees with its immutable source frame");
    }
    Ok(())
}
