use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, MutationBuilder,
};
use beryl_model::{SyndicThreadId, SyndicTurnId};

use crate::mutation::{point, required};
use crate::{
    ActivityItemSource, ActivityQueryEntryRecord, ActivityQueryHeadRecord, ActivityQueryOrder,
    ActivityQuerySourceRecord, CanonicalItemPresentation, CanonicalItemRecord, ProjectionLifecycle,
    ProviderFrameObservationSummaryV1, ProviderItemLifecycle, SourceEventSequence,
    SyndicMutationError, SyndicTimestamp, codec::*, domain::SyndicDomain,
};

const PRUNE_READ_ROWS: usize = crate::ACTIVITY_COMPLETED_RETAINED_ROWS as usize;
const PRUNE_READ_BYTES: usize = crate::ACTIVITY_COMPLETED_RETAINED_BYTES as usize;

pub(in crate::mutation) struct ActivityEffect {
    head: ActivityQueryHeadRecord,
    source: ActivityQuerySourceRecord,
    delete: Vec<ActivityQueryEntryKey>,
    entry: Option<ActivityQueryEntryRecord>,
}

pub(in crate::mutation) fn advance(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    source_turn_id: SyndicTurnId,
    source_frontier: SourceEventSequence,
    terminal: bool,
    next_item: Option<&CanonicalItemRecord>,
) -> Result<ActivityEffect, SyndicMutationError> {
    let current_head = required::<ActivityQueryHeadsFamily>(reader, &thread_id)?;
    let source = current_head
        .source()
        .ok_or(SyndicMutationError::ActivityQueryConflict)?;
    if current_head.thread_id() != thread_id
        || source.thread_id() != thread_id
        || source.turn_id() != source_turn_id
        || !current_head.source_active()
        || current_head.lifecycle() != ProjectionLifecycle::Current
    {
        return Err(SyndicMutationError::ActivityQueryConflict);
    }
    let source_key = ActivityQuerySourceKey {
        thread: thread_id,
        work_period: current_head.work_period(),
        source_thread: source.thread_id(),
        source_turn: source.turn_id(),
    };
    let current_source = required::<ActivityQuerySourcesFamily>(reader, &source_key)?;
    if current_source.thread_id() != thread_id
        || current_source.work_period() != current_head.work_period()
        || current_source.source() != source
        || !current_source.active()
        || current_source.source_frontier().checked_add(1) != Some(source_frontier.get())
    {
        return Err(SyndicMutationError::ActivityQueryConflict);
    }

    let mut logical_count = current_head.logical_row_count();
    let mut running_count = current_head.running_row_count();
    let mut completed_count = current_head.completed_row_count();
    let mut completed_bytes = current_head.completed_stored_bytes();
    let mut completed_changed = false;
    let mut delete = Vec::new();
    let mut entry = None;
    let visible_source_event = next_item
        .filter(|item| activity_visible(item.presentation()))
        .and_then(CanonicalItemRecord::source_event);

    if let Some(next_item) = next_item.filter(|item| activity_visible(item.presentation())) {
        let next_order = activity_order(next_item)?;
        let next_source = activity_source(reader, next_item)?;
        let current_item = point::<CanonicalItemsFamily>(reader, &next_item.id())?;
        let compact_fact = match current_item {
            Some(current) => {
                let current_fact =
                    validate_current_entry(reader, &current_head, &current, &next_source)?;
                let current_order = activity_order(&current)?;
                if current_order != next_order {
                    delete.push(ActivityQueryEntryKey {
                        thread: thread_id,
                        work_period: current_head.work_period(),
                        order: current_order,
                    });
                }
                if current.provider_lifecycle() == ProviderItemLifecycle::Started
                    && next_item.provider_lifecycle() == ProviderItemLifecycle::Completed
                {
                    completed_changed = true;
                    running_count = running_count
                        .checked_sub(1)
                        .ok_or(SyndicMutationError::ActivityQueryConflict)?;
                    completed_count = completed_count
                        .checked_add(1)
                        .ok_or(SyndicMutationError::ActivityQueryConflict)?;
                }
                current_fact
            }
            None => {
                logical_count = logical_count
                    .checked_add(1)
                    .ok_or(SyndicMutationError::ActivityQueryConflict)?;
                if next_item.provider_lifecycle() == ProviderItemLifecycle::Started {
                    running_count = running_count
                        .checked_add(1)
                        .ok_or(SyndicMutationError::ActivityQueryConflict)?;
                } else {
                    completed_changed = true;
                    completed_count = completed_count
                        .checked_add(1)
                        .ok_or(SyndicMutationError::ActivityQueryConflict)?;
                }
                None
            }
        };
        let next = ActivityQueryEntryRecord::new(
            thread_id,
            current_head.work_period(),
            next_order,
            next_source,
            next_item.source_event().expect("provider item source"),
            next_item.provider_kind(),
            next_item.provider_lifecycle(),
            compact_fact,
        )?;
        if !next_order.running() {
            completed_bytes = completed_bytes
                .checked_add(entry_stored_bytes(&next)?)
                .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        }
        entry = Some(next);
    }

    let cutoff = if completed_changed {
        prune_completed(
            reader,
            &current_head,
            &mut logical_count,
            &mut completed_count,
            &mut completed_bytes,
            &mut delete,
            &mut entry,
        )?
    } else {
        current_head.completed_retention_cutoff()
    };

    if terminal {
        logical_count = logical_count
            .checked_sub(running_count)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        running_count = 0;
    }
    let head = ActivityQueryHeadRecord::new(
        thread_id,
        current_head.work_period(),
        current_head.source(),
        !terminal,
        current_head
            .source_frontier()
            .checked_add(1)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?,
        current_head.revision().checked_next()?,
        current_head.source_count(),
        logical_count,
        running_count,
        completed_count,
        completed_bytes,
        cutoff,
        ProjectionLifecycle::Current,
    )?;
    Ok(ActivityEffect {
        head,
        source: ActivityQuerySourceRecord::new(
            thread_id,
            current_head.work_period(),
            source,
            current_source.activity_start().or(visible_source_event),
            source_frontier.get(),
            !terminal,
            current_source.child_handoff(),
        ),
        delete,
        entry,
    })
}

fn activity_source(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
) -> Result<ActivityItemSource, SyndicMutationError> {
    let turn = required::<TurnsFamily>(reader, &item.turn_id())?;
    let cas_item = item
        .cas_source()
        .cloned()
        .ok_or(SyndicMutationError::ActivityQueryConflict)?;
    Ok(ActivityItemSource::new(
        turn.origin_thread_id(),
        item.turn_id(),
        item.id(),
        cas_item,
    ))
}

fn validate_current_entry(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &ActivityQueryHeadRecord,
    current: &CanonicalItemRecord,
    next_source: &ActivityItemSource,
) -> Result<Option<crate::ActivityCompactFact>, SyndicMutationError> {
    if !activity_visible(current.presentation()) || current.turn_id() != next_source.turn_id() {
        return Err(SyndicMutationError::ActivityQueryConflict);
    }
    let order = activity_order(current)?;
    let key = ActivityQueryEntryKey {
        thread: head.thread_id(),
        work_period: head.work_period(),
        order,
    };
    let stored = point::<ActivityQueryEntriesFamily>(reader, &key)?
        .ok_or(SyndicMutationError::ActivityQueryConflict)?;
    if stored.thread_id() != head.thread_id()
        || stored.work_period() != head.work_period()
        || stored.item_id() != current.id()
        || stored.source_event() != current.source_event().expect("provider item source")
        || stored.provider_kind() != current.provider_kind()
        || stored.provider_lifecycle() != current.provider_lifecycle()
        || stored.source().thread_id()
            != required::<TurnsFamily>(reader, &current.turn_id())?.origin_thread_id()
        || stored.source().turn_id() != current.turn_id()
        || stored.source().cas_item() != current.cas_source().expect("provider item CAS source")
    {
        return Err(SyndicMutationError::ActivityQueryConflict);
    }
    Ok(stored.compact_fact().cloned())
}

pub(in crate::mutation) fn entry_stored_bytes(
    entry: &ActivityQueryEntryRecord,
) -> Result<u64, SyndicMutationError> {
    activity_entry_stored_bytes(
        &ActivityQueryEntryKey {
            thread: entry.thread_id(),
            work_period: entry.work_period(),
            order: entry.order(),
        },
        entry,
    )
    .map_err(|_| SyndicMutationError::ActivityQueryConflict)
}

#[derive(Clone)]
struct CompletedCandidate {
    order: ActivityQueryOrder,
    stored_bytes: u64,
    key: Option<ActivityQueryEntryKey>,
}

#[allow(clippy::too_many_arguments)]
pub(in crate::mutation) fn prune_completed(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &ActivityQueryHeadRecord,
    logical_count: &mut u64,
    completed_count: &mut u64,
    completed_bytes: &mut u64,
    delete: &mut Vec<ActivityQueryEntryKey>,
    entry: &mut Option<ActivityQueryEntryRecord>,
) -> Result<Option<ActivityQueryOrder>, SyndicMutationError> {
    let first =
        ActivityQueryEntryKey::first_completed_for_period(head.thread_id(), head.work_period());
    let last = ActivityQueryEntryKey::last_for_period(head.thread_id(), head.work_period());
    let page = reader.cursor::<ActivityQueryEntriesCodec>(
        &CursorRange::closed(first, last),
        CursorDirection::Forward,
        CursorReadLimits::new(PRUNE_READ_ROWS, PRUNE_READ_BYTES)
            .expect("activity prune bounds are nonzero"),
    )?;
    if page.has_more() {
        return Err(SyndicMutationError::ActivityQueryConflict);
    }
    let mut candidates = Vec::with_capacity(PRUNE_READ_ROWS + 1);
    for record in page.records() {
        let key = *record.key();
        let value = record.value();
        if delete.contains(&key) {
            continue;
        }
        candidates.push(CompletedCandidate {
            order: key.order,
            stored_bytes: activity_entry_stored_bytes(&key, value)
                .map_err(|_| SyndicMutationError::ActivityQueryConflict)?,
            key: Some(key),
        });
    }
    if let Some(value) = entry.as_ref().filter(|value| !value.order().running()) {
        candidates.push(CompletedCandidate {
            order: value.order(),
            stored_bytes: entry_stored_bytes(value)?,
            key: None,
        });
    }
    candidates.sort_unstable_by_key(|candidate| candidate.order);
    if usize::try_from(*completed_count).ok() != Some(candidates.len()) {
        return Err(SyndicMutationError::ActivityQueryConflict);
    }
    let retained = retained_prefix_len(
        &candidates,
        crate::ACTIVITY_COMPLETED_RETAINED_ROWS,
        crate::ACTIVITY_COMPLETED_RETAINED_BYTES,
    )?;
    for candidate in candidates.drain(retained..) {
        *completed_count = completed_count
            .checked_sub(1)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        *logical_count = logical_count
            .checked_sub(1)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        *completed_bytes = completed_bytes
            .checked_sub(candidate.stored_bytes)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        if let Some(key) = candidate.key {
            delete.push(key);
        } else {
            *entry = None;
        }
    }
    Ok(candidates.last().map(|candidate| candidate.order))
}

fn retained_prefix_len(
    candidates: &[CompletedCandidate],
    row_limit: u64,
    byte_limit: u64,
) -> Result<usize, SyndicMutationError> {
    let mut rows = 0_u64;
    let mut bytes = 0_u64;
    for candidate in candidates {
        let next_bytes = bytes
            .checked_add(candidate.stored_bytes)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        if rows == row_limit || next_bytes > byte_limit {
            break;
        }
        rows += 1;
        bytes = next_bytes;
    }
    usize::try_from(rows).map_err(|_| SyndicMutationError::ActivityQueryConflict)
}

impl ActivityEffect {
    pub(in crate::mutation) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<ActivityQueryHeadsCodec>(&self.head.thread_id(), &self.head)?;
        mutations.put::<ActivityQuerySourcesCodec>(
            &ActivityQuerySourceKey {
                thread: self.source.thread_id(),
                work_period: self.source.work_period(),
                source_thread: self.source.source().thread_id(),
                source_turn: self.source.source().turn_id(),
            },
            &self.source,
        )?;
        for key in self.delete {
            mutations.delete::<ActivityQueryEntriesCodec>(&key)?;
        }
        if let Some(entry) = self.entry {
            mutations.put::<ActivityQueryEntriesCodec>(
                &ActivityQueryEntryKey {
                    thread: entry.thread_id(),
                    work_period: entry.work_period(),
                    order: entry.order(),
                },
                &entry,
            )?;
        }
        Ok(())
    }
}

pub(crate) const fn activity_visible(presentation: &CanonicalItemPresentation) -> bool {
    matches!(
        presentation,
        CanonicalItemPresentation::Operational | CanonicalItemPresentation::Activity
    )
}

pub(crate) fn activity_order(
    item: &CanonicalItemRecord,
) -> Result<ActivityQueryOrder, SyndicMutationError> {
    let provider = item
        .provider()
        .ok_or(SyndicMutationError::ActivityQueryConflict)?;
    let lifecycle = item.provider_lifecycle();
    let running = lifecycle == ProviderItemLifecycle::Started;
    let timestamp = match provider.stream_state().started_at() {
        Some(started) => started.get(),
        None => match provider.observation() {
            ProviderFrameObservationSummaryV1::Completed(completed)
                if lifecycle == ProviderItemLifecycle::Completed =>
            {
                completed.get()
            }
            _ => return Err(SyndicMutationError::ActivityQueryConflict),
        },
    };
    Ok(ActivityQueryOrder::new(
        running,
        SyndicTimestamp::from_unix_millis(timestamp),
        item.id(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(timestamp: u64, item: u8, stored_bytes: u64) -> CompletedCandidate {
        CompletedCandidate {
            order: ActivityQueryOrder::new(
                false,
                SyndicTimestamp::from_unix_millis(timestamp),
                beryl_model::SyndicItemId::from_bytes([item; 16]),
            ),
            stored_bytes,
            key: None,
        }
    }

    #[test]
    fn retention_caps_select_one_exact_full_order_newest_prefix() {
        let mut candidates = vec![candidate(8, 3, 1), candidate(9, 2, 1), candidate(9, 1, 1)];
        candidates.sort_unstable_by_key(|candidate| candidate.order);
        assert_eq!(retained_prefix_len(&candidates, 2, 100).unwrap(), 2);
        assert_eq!(candidates[0].order.item_id().as_bytes(), &[1; 16]);
        assert_eq!(candidates[1].order.item_id().as_bytes(), &[2; 16]);
        assert_eq!(retained_prefix_len(&candidates, 3, 1).unwrap(), 1);
    }
}
