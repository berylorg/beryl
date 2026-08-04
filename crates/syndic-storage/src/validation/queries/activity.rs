use beryl_home_store::DomainReader;

use crate::{codec::*, domain::SyndicDomain, error::SyndicValidationError};

use super::super::scan::{point, require, scan, scan_range};

mod retention;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    validate_heads(reader)?;
    validate_entries(reader)
}

fn validate_heads(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<ThreadsFamily>(reader, |_, thread| {
        let head = require::<ActivityQueryHeadsFamily>(
            reader,
            &thread.id(),
            "thread activity-query head is missing",
        )?;
        if head.thread_id() != thread.id() {
            return invariant("thread activity-query head identity disagrees");
        }
        let Some(source) = head.source() else {
            return if head.source_frontier() == 0
                && head.logical_row_count() == 0
                && head.running_row_count() == 0
                && head.completed_row_count() == 0
                && head.completed_stored_bytes() == 0
                && head.completed_retention_cutoff().is_none()
            {
                Ok(())
            } else {
                invariant("empty activity-query head carries source state")
            };
        };
        let mut source_count = 0_u64;
        let mut source_frontier = 0_u64;
        let mut root_found = false;
        scan_range::<ActivityQuerySourcesFamily>(
            reader,
            ActivityQuerySourceKey::first_for_period(thread.id(), head.work_period()),
            ActivityQuerySourceKey::last_for_period(thread.id(), head.work_period()),
            |key, member| {
                validate_source_member(reader, key, member, thread.id())?;
                source_count =
                    checked_add(source_count, 1, "activity-query source count overflowed")?;
                source_frontier = checked_add(
                    source_frontier,
                    member.source_frontier(),
                    "activity-query source frontier overflowed",
                )?;
                if member.source() == source {
                    root_found = true;
                    if member.active() != head.source_active() {
                        return invariant("activity-query root source lifecycle disagrees");
                    }
                }
                Ok(())
            },
        )?;
        if !root_found
            || source_count != head.source_count()
            || source_frontier != head.source_frontier()
        {
            return invariant("activity-query source authority disagrees");
        }

        let mut logical = 0_u64;
        let mut running = 0_u64;
        let mut completed = 0_u64;
        let mut completed_bytes = 0_u64;
        let mut cutoff = None;
        let first = if head.source_active() {
            ActivityQueryEntryKey::first_for_period(thread.id(), head.work_period())
        } else {
            ActivityQueryEntryKey::first_completed_for_period(thread.id(), head.work_period())
        };
        scan_range::<ActivityQueryEntriesFamily>(
            reader,
            first,
            ActivityQueryEntryKey::last_for_period(thread.id(), head.work_period()),
            |key, entry| {
                logical = checked_add(logical, 1, "activity-query row count overflowed")?;
                if entry.order().running() {
                    running = checked_add(running, 1, "activity-query running count overflowed")?;
                } else {
                    completed =
                        checked_add(completed, 1, "activity-query completed count overflowed")?;
                    completed_bytes = checked_add(
                        completed_bytes,
                        activity_entry_stored_bytes(key, entry).map_err(|_| {
                            SyndicValidationError::Invariant(
                                "activity-query entry stored-byte encoding failed",
                            )
                        })?,
                        "activity-query completed bytes overflowed",
                    )?;
                    cutoff = Some(entry.order());
                }
                Ok(())
            },
        )?;
        if head.logical_row_count() != logical
            || head.running_row_count() != running
            || head.completed_row_count() != completed
            || head.completed_stored_bytes() != completed_bytes
            || head.completed_retention_cutoff() != cutoff
        {
            return invariant("activity-query head counters or retention cutoff disagree");
        }
        retention::validate_retained_projection(reader, &head)
    })?;

    scan::<ActivityQueryHeadsFamily>(reader, |key, head| {
        if *key != head.thread_id() || point::<ThreadsFamily>(reader, key)?.is_none() {
            return invariant("activity-query head owner or key disagrees");
        }
        Ok(())
    })?;
    scan::<ActivityQuerySourcesFamily>(reader, |key, member| {
        let head = require::<ActivityQueryHeadsFamily>(
            reader,
            &key.thread,
            "activity-query source owner head is missing",
        )?;
        if key.work_period > head.work_period() {
            return invariant("activity-query source belongs to a future work period");
        }
        if key.work_period == head.work_period() {
            return if head.source().is_some() {
                Ok(())
            } else {
                invariant("empty activity-query head retains current-period sources")
            };
        }
        validate_source_member(reader, key, member, key.thread)
    })
}

fn validate_entries(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<ActivityQueryEntriesFamily>(reader, |key, entry| {
        if key.thread != entry.thread_id()
            || key.work_period != entry.work_period()
            || key.order != entry.order()
        {
            return invariant("activity-query entry key disagrees");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &entry.item_id(),
            "activity-query source item is missing",
        )?;
        let turn = require::<TurnsFamily>(
            reader,
            &item.turn_id(),
            "activity-query source turn is missing",
        )?;
        if entry.source().thread_id() != turn.origin_thread_id()
            || entry.source().turn_id() != item.turn_id()
            || entry.source().item_id() != item.id()
            || item.cas_source() != Some(entry.source().cas_item())
            || item.source_event() != Some(entry.source_event())
            || item.provider_kind() != entry.provider_kind()
            || item.provider_lifecycle() != entry.provider_lifecycle()
            || key.order != activity_order(&item)?
        {
            return invariant("activity-query entry source authority disagrees");
        }
        let owner = require::<ThreadsFamily>(
            reader,
            &entry.thread_id(),
            "activity-query owner thread is missing",
        )?;
        let member = require::<ActivityQuerySourcesFamily>(
            reader,
            &ActivityQuerySourceKey {
                thread: entry.thread_id(),
                work_period: entry.work_period(),
                source_thread: entry.source().thread_id(),
                source_turn: entry.source().turn_id(),
            },
            "activity-query entry source membership is missing",
        )?;
        if member
            .activity_start()
            .is_none_or(|start| entry.source_event() < start)
            || member.source_frontier() < entry.source_event().get()
        {
            return invariant("activity-query entry lies outside its source membership range");
        }
        if entry.source().thread_id() != owner.id() {
            let child = require::<ThreadsFamily>(
                reader,
                &entry.source().thread_id(),
                "activity-query child source thread is missing",
            )?;
            if child.parent_thread_id() != Some(owner.id()) {
                return invariant("activity-query source is not owned or observed child work");
            }
        }
        match entry.compact_fact() {
            None if provider_activity_visible(item.presentation()) => Ok(()),
            Some(crate::ActivityCompactFact::ChildHandoff(fact)) => {
                let Some(handoff) = member.child_handoff() else {
                    return invariant("activity-query handoff entry has no membership authority");
                };
                if handoff.item_id() != item.id()
                    || fact.observed_child_thread_id() != member.source().thread_id()
                    || fact.final_answer_range() != handoff.final_answer_range()
                {
                    return invariant("activity-query handoff entry disagrees with membership");
                }
                validate_handoff(&item, owner.id(), fact, reader)
            }
            _ => invariant("activity-query entry visibility or compact fact disagrees"),
        }
    })
}

fn validate_source_member(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &ActivityQuerySourceKey,
    member: &crate::ActivityQuerySourceRecord,
    owner: beryl_model::SyndicThreadId,
) -> Result<(), SyndicValidationError> {
    validate_source_member_identity(reader, key, member, owner)?;
    let source_state = require::<TurnStatesFamily>(
        reader,
        &member.source().turn_id(),
        "activity-query member source state is missing",
    )?;
    if source_state.source_event_count() != member.source_frontier()
        || member.active() == source_state.lifecycle().is_proven_terminal()
    {
        return invariant("activity-query source membership authority disagrees");
    }
    if let Some(handoff) = member.child_handoff() {
        let item = require::<CanonicalItemsFamily>(
            reader,
            &handoff.item_id(),
            "activity-query handoff membership item is missing",
        )?;
        if item
            .source_event()
            .and_then(|event| event.get().checked_add(1))
            != Some(source_state.source_event_count())
        {
            return invariant("activity-query handoff is not terminal-adjacent");
        }
    }
    if member.activity_start() != expected_activity_start(reader, member)? {
        return invariant("activity-query source activity start disagrees");
    }
    Ok(())
}

fn validate_source_member_identity(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &ActivityQuerySourceKey,
    member: &crate::ActivityQuerySourceRecord,
    owner: beryl_model::SyndicThreadId,
) -> Result<(), SyndicValidationError> {
    if key.thread != owner
        || key.work_period != member.work_period()
        || key.source_thread != member.source().thread_id()
        || key.source_turn != member.source().turn_id()
        || member.thread_id() != owner
    {
        return invariant("activity-query source membership key disagrees");
    }
    if member
        .activity_start()
        .is_some_and(|start| start.get() > member.source_frontier())
    {
        return invariant("activity-query source membership range is inverted");
    }
    let source_thread = require::<ThreadsFamily>(
        reader,
        &member.source().thread_id(),
        "activity-query member source thread is missing",
    )?;
    let source_turn = require::<TurnsFamily>(
        reader,
        &member.source().turn_id(),
        "activity-query member source turn is missing",
    )?;
    if source_turn.origin_thread_id() != source_thread.id()
        || (source_thread.id() != owner && source_thread.parent_thread_id() != Some(owner))
    {
        return invariant("activity-query source membership identity disagrees");
    }
    if let Some(handoff) = member.child_handoff() {
        if source_thread.id() == owner {
            return invariant("activity-query root source claims a child handoff");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &handoff.item_id(),
            "activity-query handoff membership item is missing",
        )?;
        let fact =
            crate::ActivityChildHandoffFact::new(source_thread.id(), handoff.final_answer_range());
        if item.turn_id() != member.source().turn_id()
            || item.source_event().is_none_or(|event| {
                member.activity_start().is_none_or(|start| event < start)
                    || event.get() > member.source_frontier()
            })
        {
            return invariant("activity-query handoff lies outside its source membership range");
        }
        validate_handoff(&item, owner, &fact, reader)?;
    }
    Ok(())
}

fn expected_activity_start(
    reader: &DomainReader<'_, SyndicDomain>,
    member: &crate::ActivityQuerySourceRecord,
) -> Result<Option<crate::SourceEventSequence>, SyndicValidationError> {
    if let Some(handoff) = member.child_handoff() {
        let item = require::<CanonicalItemsFamily>(
            reader,
            &handoff.item_id(),
            "activity-query handoff membership item is missing",
        )?;
        return item
            .source_event()
            .map(Some)
            .ok_or(SyndicValidationError::Invariant(
                "activity-query handoff item has no source event",
            ));
    }
    if member.source().thread_id() != member.thread_id() {
        return invariant("activity-query child source has no handoff authority");
    }
    if member.source_frontier() == 0 {
        return Ok(None);
    }
    let last = crate::SourceEventSequence::new(member.source_frontier()).map_err(|_| {
        SyndicValidationError::Invariant("activity-query source frontier is invalid")
    })?;
    let mut first = None;
    scan_range::<SourceEventsFamily>(
        reader,
        TurnEventKey {
            owner: member.source().turn_id(),
            ordinal: crate::SourceEventSequence::FIRST,
        },
        TurnEventKey {
            owner: member.source().turn_id(),
            ordinal: last,
        },
        |key, event| {
            let crate::SourceEventPayload::ItemFrame { item_id, .. } = event.payload() else {
                return Ok(());
            };
            let item = require::<CanonicalItemsFamily>(
                reader,
                item_id,
                "activity-query visible source item is missing",
            )?;
            if provider_activity_visible(item.presentation())
                && first.is_none_or(|current| key.ordinal < current)
            {
                first = Some(key.ordinal);
            }
            Ok(())
        },
    )?;
    Ok(first)
}

fn validate_handoff(
    item: &crate::CanonicalItemRecord,
    owner: beryl_model::SyndicThreadId,
    fact: &crate::ActivityChildHandoffFact,
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let child = require::<ThreadsFamily>(
        reader,
        &fact.observed_child_thread_id(),
        "activity handoff child thread is missing",
    )?;
    let narrative_bytes = item
        .provider()
        .and_then(|provider| provider.narrative())
        .map(|narrative| narrative.logical_utf8_bytes());
    let source_turn = require::<TurnsFamily>(
        reader,
        &item.turn_id(),
        "activity handoff source turn is missing",
    )?;
    if child.parent_thread_id() != Some(owner)
        || source_turn.origin_thread_id() != child.id()
        || item.provider_kind() != crate::ProviderItemKind::AgentMessage
        || item.provider_lifecycle() != crate::ProviderItemLifecycle::Completed
        || item.assistant_phase() != Some(crate::AssistantMessagePhase::FinalAnswer)
        || !matches!(
            item.presentation(),
            crate::CanonicalItemPresentation::Narrative
        )
        || narrative_bytes.is_none_or(|bytes| fact.final_answer_range().end() > bytes)
    {
        return invariant("activity handoff source or narrative range disagrees");
    }
    Ok(())
}

const fn provider_activity_visible(presentation: &crate::CanonicalItemPresentation) -> bool {
    matches!(
        presentation,
        crate::CanonicalItemPresentation::Operational | crate::CanonicalItemPresentation::Activity
    )
}

fn activity_order(
    item: &crate::CanonicalItemRecord,
) -> Result<crate::ActivityQueryOrder, SyndicValidationError> {
    let provider = item.provider().ok_or(SyndicValidationError::Invariant(
        "activity-query source item has no provider frame",
    ))?;
    let running = item.provider_lifecycle() == crate::ProviderItemLifecycle::Started;
    let timestamp = match provider.stream_state().started_at() {
        Some(started) => started.get(),
        None => match provider.observation() {
            crate::ProviderFrameObservationSummaryV1::Completed(completed)
                if item.provider_lifecycle() == crate::ProviderItemLifecycle::Completed =>
            {
                completed.get()
            }
            _ => return invariant("activity-query source item has no lifecycle timestamp"),
        },
    };
    Ok(crate::ActivityQueryOrder::new(
        running,
        crate::SyndicTimestamp::from_unix_millis(timestamp),
        item.id(),
    ))
}

fn checked_add(left: u64, right: u64, message: &'static str) -> Result<u64, SyndicValidationError> {
    left.checked_add(right)
        .ok_or(SyndicValidationError::Invariant(message))
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
