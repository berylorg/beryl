use beryl_home_store::DomainReader;

use crate::{ProjectionLifecycle, codec::*, domain::SyndicDomain, error::SyndicValidationError};

use crate::validation::scan::{require, scan};

use super::{invariant, visibility::is_transcript_visible};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut current_group = None;
    let mut expected_position = 1_u64;
    let mut observed = 0_u64;
    let mut previous_order = None;
    let mut digest = crate::projection::transcript_entry_digest_seed();
    scan::<TranscriptEntriesFamily>(reader, |key, entry| {
        let group = (key.thread, key.generation);
        if current_group != Some(group) {
            finish_group(reader, current_group, observed, digest)?;
            current_group = Some(group);
            expected_position = 1;
            observed = 0;
            previous_order = None;
            digest = crate::projection::transcript_entry_digest_seed();
        }
        if key.thread != entry.thread_id()
            || key.generation != entry.generation()
            || key.position != entry.position()
            || entry.position().get() != expected_position
        {
            return invariant("transcript-entry key or contiguous position disagrees");
        }
        let head = require::<TranscriptHeadsFamily>(
            reader,
            &entry.thread_id(),
            "transcript entry head is missing",
        )?;
        if entry.generation() > head.generation() {
            return invariant("transcript entry belongs to an unpublished future generation");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &entry.item_id(),
            "transcript entry item is missing",
        )?;
        let projection = require::<ProjectionsFamily>(
            reader,
            &entry.projection_id(),
            "transcript entry projection is missing",
        )?;
        if item.revision() != entry.item_revision()
            || projection.revision() != entry.projection_revision()
            || projection.item_id() != item.id()
            || projection.turn_id() != item.turn_id()
        {
            return invariant("transcript entry references disagree");
        }
        if !is_transcript_visible(item.kind()) {
            return invariant("transcript entry references a non-visible canonical item");
        }
        let item_set = require::<ItemProjectionSetsFamily>(
            reader,
            &ItemProjectionSetKey {
                item: item.id(),
                generation: entry.item_projection_generation(),
            },
            "transcript entry item projection set is missing",
        )?;
        if item_set.source_item_revision() != entry.item_revision() {
            return invariant("transcript entry item projection revision disagrees");
        }
        let projection_index = crate::membership::point(reader, &item_set, projection.ordinal())?
            .ok_or(SyndicValidationError::Invariant(
            "transcript entry projection index is missing",
        ))?;
        if projection_index.projection_id() != projection.id()
            || projection_index.projection_revision() != projection.revision()
        {
            return invariant("transcript entry projection index disagrees");
        }
        let turn =
            require::<TurnsFamily>(reader, &item.turn_id(), "transcript item turn is missing")?;
        let path = require::<TranscriptPathTurnsFamily>(
            reader,
            &ThreadTranscriptPathKey {
                thread: entry.thread_id(),
                generation: entry.generation(),
                depth: turn.depth(),
            },
            "transcript entry selected-path record is missing",
        )?;
        if path.turn_id() != turn.id() {
            return invariant("transcript entry is outside its selected path");
        }
        if item.ordinal().get() > path.finalized_item_count() {
            return invariant("transcript entry references an unfinalized item snapshot");
        }
        if entry.generation() == head.generation()
            && head.lifecycle() == ProjectionLifecycle::Current
        {
            let item_head = require::<ItemProjectionHeadsFamily>(
                reader,
                &item.id(),
                "current transcript entry item head is missing",
            )?;
            if item_head.lifecycle() != ProjectionLifecycle::Current
                || item_head.generation() != entry.item_projection_generation()
            {
                return invariant("current transcript entry references an uncurrent item set");
            }
        }
        let order = (
            turn.depth().get(),
            item.ordinal().get(),
            projection.ordinal().get(),
        );
        if previous_order.is_some_and(|previous| previous >= order) {
            return invariant("transcript entries are duplicated or not in strict canonical order");
        }
        previous_order = Some(order);
        digest = crate::projection::advance_transcript_entry_digest(
            digest,
            entry.thread_id(),
            entry.generation(),
            entry.position(),
            entry.item_id(),
            entry.item_revision(),
            entry.item_projection_generation(),
            entry.projection_id(),
            entry.projection_revision(),
        );
        expected_position =
            expected_position
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "transcript position exhausted",
                ))?;
        observed += 1;
        Ok(())
    })?;
    finish_group(reader, current_group, observed, digest)
}

fn finish_group(
    reader: &DomainReader<'_, SyndicDomain>,
    group: Option<(beryl_model::SyndicThreadId, crate::TranscriptGeneration)>,
    observed: u64,
    digest: [u8; 32],
) -> Result<(), SyndicValidationError> {
    let Some((thread_id, generation)) = group else {
        return Ok(());
    };
    let build = require::<TranscriptBuildsFamily>(
        reader,
        &ThreadTranscriptBuildKey {
            thread: thread_id,
            generation,
        },
        "transcript entry build owner is missing",
    )?;
    if build.entry_count() != observed || build.entry_digest() != digest {
        return invariant("transcript build entry frontier disagrees");
    }
    Ok(())
}
