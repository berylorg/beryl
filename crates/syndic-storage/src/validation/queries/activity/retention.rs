use beryl_home_store::DomainReader;

use crate::{codec::*, domain::SyndicDomain, error::SyndicValidationError};

use super::{activity_order, invariant, provider_activity_visible};
use crate::validation::scan::{point, require, scan_range};

pub(super) fn validate_retained_projection(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &crate::ActivityQueryHeadRecord,
) -> Result<(), SyndicValidationError> {
    if head.source().is_none() {
        return Ok(());
    }
    if head.completed_row_count() > crate::ACTIVITY_COMPLETED_RETAINED_ROWS
        || head.completed_stored_bytes() > crate::ACTIVITY_COMPLETED_RETAINED_BYTES
    {
        return invariant("activity-query completed retention exceeds its bounds");
    }
    let mut first_excluded = None;
    scan_range::<ActivityQuerySourcesFamily>(
        reader,
        ActivityQuerySourceKey::first_for_period(head.thread_id(), head.work_period()),
        ActivityQuerySourceKey::last_for_period(head.thread_id(), head.work_period()),
        |_, member| {
            scan_range::<TurnItemsFamily>(
                reader,
                TurnItemKey {
                    owner: member.source().turn_id(),
                    ordinal: crate::TurnItemOrdinal::FIRST,
                },
                TurnItemKey {
                    owner: member.source().turn_id(),
                    ordinal: crate::TurnItemOrdinal::new(u64::MAX).expect("maximum is nonzero"),
                },
                |_, index| {
                    let item = require::<CanonicalItemsFamily>(
                        reader,
                        &index.item_id(),
                        "activity-query retained source item is missing",
                    )?;
                    let provider_visible = provider_activity_visible(item.presentation())
                        && item.source_event().is_some_and(|event| {
                            member.activity_start().is_some_and(|start| event >= start)
                                && event.get() <= member.source_frontier()
                        });
                    let admitted_handoff = member
                        .child_handoff()
                        .is_some_and(|handoff| handoff.item_id() == item.id());
                    if !provider_visible && !admitted_handoff {
                        return Ok(());
                    }
                    validate_candidate(reader, head, member, &item, &mut first_excluded)
                },
            )
        },
    )?;
    if let Some((_, next_bytes)) = first_excluded {
        let next_total = head
            .completed_stored_bytes()
            .checked_add(next_bytes)
            .ok_or(SyndicValidationError::Invariant(
                "activity-query completed bytes overflowed",
            ))?;
        if head.completed_row_count() < crate::ACTIVITY_COMPLETED_RETAINED_ROWS
            && next_total <= crate::ACTIVITY_COMPLETED_RETAINED_BYTES
        {
            return invariant("activity-query completed retention is not maximal");
        }
    }
    Ok(())
}

fn validate_candidate(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &crate::ActivityQueryHeadRecord,
    member: &crate::ActivityQuerySourceRecord,
    item: &crate::CanonicalItemRecord,
    first_excluded: &mut Option<(crate::ActivityQueryOrder, u64)>,
) -> Result<(), SyndicValidationError> {
    let order = activity_order(item)?;
    let key = ActivityQueryEntryKey {
        thread: head.thread_id(),
        work_period: head.work_period(),
        order,
    };
    if order.running() {
        if point::<ActivityQueryEntriesFamily>(reader, &key)?.is_none() {
            return invariant("activity-query running source item has no exact entry");
        }
        return Ok(());
    }
    let must_retain = head
        .completed_retention_cutoff()
        .is_some_and(|cutoff| order <= cutoff);
    let present = point::<ActivityQueryEntriesFamily>(reader, &key)?.is_some();
    if must_retain != present {
        return invariant("activity-query completed retention is not a newest prefix");
    }
    if !must_retain
        && first_excluded
            .as_ref()
            .is_none_or(|(current, _)| order < *current)
    {
        let expected = expected_entry(reader, head, member, item)?;
        let bytes = activity_entry_stored_bytes(&key, &expected).map_err(|_| {
            SyndicValidationError::Invariant("activity-query entry stored-byte encoding failed")
        })?;
        *first_excluded = Some((order, bytes));
    }
    Ok(())
}

fn expected_entry(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &crate::ActivityQueryHeadRecord,
    member: &crate::ActivityQuerySourceRecord,
    item: &crate::CanonicalItemRecord,
) -> Result<crate::ActivityQueryEntryRecord, SyndicValidationError> {
    let turn = require::<TurnsFamily>(
        reader,
        &item.turn_id(),
        "activity-query source turn is missing",
    )?;
    let compact_fact = member
        .child_handoff()
        .filter(|handoff| handoff.item_id() == item.id())
        .map(|handoff| {
            crate::ActivityCompactFact::ChildHandoff(crate::ActivityChildHandoffFact::new(
                member.source().thread_id(),
                handoff.final_answer_range(),
            ))
        });
    crate::ActivityQueryEntryRecord::new(
        head.thread_id(),
        head.work_period(),
        activity_order(item)?,
        crate::ActivityItemSource::new(
            turn.origin_thread_id(),
            item.turn_id(),
            item.id(),
            item.cas_source()
                .cloned()
                .ok_or(SyndicValidationError::Invariant(
                    "activity-query source item has no CAS source",
                ))?,
        ),
        item.source_event().ok_or(SyndicValidationError::Invariant(
            "activity-query source item has no source event",
        ))?,
        item.provider_kind(),
        item.provider_lifecycle(),
        compact_fact,
    )
    .map_err(|_| SyndicValidationError::Invariant("activity-query expected entry is invalid"))
}
