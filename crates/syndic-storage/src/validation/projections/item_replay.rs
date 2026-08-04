use std::cmp::Ordering;

use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};
use beryl_model::{ProjectionRevision, SyndicItemId};

use crate::{
    ItemProjectionBuildPhase, ItemProjectionBuildRecord, ItemProjectionGeneration,
    ItemProjectionSetRecord, MarkdownParserCheckpoint, ProjectionFormatVersion, ProjectionOrdinal,
    ProjectionTextSource, codec::*, domain::SyndicDomain, error::SyndicValidationError,
};

use super::{invariant, source};
use crate::validation::scan::{point, scan};

mod membership;

use membership::{Membership, validate_projection_membership, validate_resource_replay};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<CanonicalItemsFamily>(reader, |_, item| {
        let mut next_set = read_next_set(reader, item.id(), None)?;
        let mut next_build = read_next_build(reader, item.id(), None)?;
        let Some(initial_source) = item.projection_source() else {
            if next_set.is_some() || next_build.is_some() {
                return invariant("nonprojectable item owns projection generations");
            }
            return validate_head_selection(reader, item.id(), false, None);
        };

        let mut expected_generation = ItemProjectionGeneration::FIRST;
        let mut previous_source_revision = None;
        let mut projection_replay = ProjectionReplay::new(initial_source);
        let mut latest_set = None;
        let mut observed_generation = false;

        while next_set.is_some() || next_build.is_some() {
            let take_set = match (&next_set, &next_build) {
                (Some((set_key, _)), Some((build_key, _))) => {
                    match set_key.generation.cmp(&build_key.generation) {
                        Ordering::Less => true,
                        Ordering::Greater => false,
                        Ordering::Equal => {
                            return invariant(
                                "item projection generation has both set and build state",
                            );
                        }
                    }
                }
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };

            if take_set {
                let (key, set) = next_set.take().expect("selected set exists");
                if key.item != item.id()
                    || key.generation != set.generation()
                    || set.item_id() != item.id()
                    || key.generation != expected_generation
                {
                    return invariant("item projection generation sequence has a gap");
                }
                validate_source_revision(
                    &mut previous_source_revision,
                    set.source_item_revision(),
                )?;
                let source_is_immutable = source::validate_projection_snapshot(
                    reader,
                    item,
                    set.source_item_revision(),
                    set.source(),
                )?;
                projection_replay.validate_set(reader, item, &set, source_is_immutable)?;
                latest_set = Some(set.generation());
                next_set = read_next_set(reader, item.id(), Some(key.generation))?;
            } else {
                let (key, build) = next_build.take().expect("selected build exists");
                if key.item != item.id()
                    || key.generation != build.generation()
                    || build.item_id() != item.id()
                    || key.generation != expected_generation
                {
                    return invariant("item projection generation sequence has a gap");
                }
                validate_source_revision(
                    &mut previous_source_revision,
                    build.source_item_revision(),
                )?;
                source::validate_projection_snapshot(
                    reader,
                    item,
                    build.source_item_revision(),
                    build.source(),
                )?;
                projection_replay.validate_build(reader, item, &build)?;
                next_build = read_next_build(reader, item.id(), Some(key.generation))?;
            }
            observed_generation = true;
            expected_generation = expected_generation
                .checked_next()
                .map_err(|_| SyndicValidationError::Invariant("projection generation exhausted"))?;
        }

        validate_head_selection(reader, item.id(), observed_generation, latest_set)
    })
}

fn validate_source_revision(
    previous: &mut Option<ProjectionRevision>,
    current: ProjectionRevision,
) -> Result<(), SyndicValidationError> {
    if previous.is_some_and(|previous| previous >= current) {
        return invariant("item projection source revisions do not advance");
    }
    *previous = Some(current);
    Ok(())
}

fn read_next_set(
    reader: &DomainReader<'_, SyndicDomain>,
    item: SyndicItemId,
    after: Option<ItemProjectionGeneration>,
) -> Result<Option<(ItemProjectionSetKey, ItemProjectionSetRecord)>, SyndicValidationError> {
    next_generation::<ItemProjectionSetsFamily>(reader, item, after)
}

fn read_next_build(
    reader: &DomainReader<'_, SyndicDomain>,
    item: SyndicItemId,
    after: Option<ItemProjectionGeneration>,
) -> Result<Option<(ItemProjectionSetKey, ItemProjectionBuildRecord)>, SyndicValidationError> {
    next_generation::<ItemProjectionBuildsFamily>(reader, item, after)
}

fn next_generation<F>(
    reader: &DomainReader<'_, SyndicDomain>,
    item: SyndicItemId,
    after: Option<ItemProjectionGeneration>,
) -> Result<Option<(ItemProjectionSetKey, F::Value)>, SyndicValidationError>
where
    F: Family<Key = ItemProjectionSetKey>,
{
    let last = ItemProjectionSetKey::last_for_item(item);
    let range = match after {
        Some(generation) => CursorRange::after(ItemProjectionSetKey { item, generation }, last),
        None => CursorRange::closed(ItemProjectionSetKey::first_for_item(item), last),
    };
    let page = reader.cursor::<ExactCodec<F>>(
        &range,
        CursorDirection::Forward,
        CursorReadLimits::new(1, crate::codec::SMALL_MAX + 128)
            .expect("generation replay bounds are nonzero"),
    )?;
    Ok(page
        .records()
        .first()
        .map(|record| (*record.key(), record.value().clone())))
}

fn validate_head_selection(
    reader: &DomainReader<'_, SyndicDomain>,
    item: SyndicItemId,
    observed_generation: bool,
    latest_set: Option<ItemProjectionGeneration>,
) -> Result<(), SyndicValidationError> {
    let head = point::<ItemProjectionHeadsFamily>(reader, &item)?;
    match (observed_generation, latest_set, head) {
        (false, None, None) => Ok(()),
        (true, Some(generation), Some(head)) if head.generation() == generation => Ok(()),
        (true, None, None) => Ok(()),
        _ => invariant("item projection head does not select the latest completed set"),
    }
}

#[derive(Clone)]
struct ProjectionReplay {
    source: Option<ProjectionTextSource>,
    checkpoint: MarkdownParserCheckpoint,
    projection_count: u64,
    resource_count: u64,
    digest: [u8; 32],
    eof_resolved: bool,
}

impl ProjectionReplay {
    fn new(initial_source: ProjectionTextSource) -> Self {
        Self {
            source: None,
            checkpoint: MarkdownParserCheckpoint::new(
                0,
                0,
                initial_source.initial_cursor(),
                0,
                Box::<str>::default(),
                false,
                None,
            ),
            projection_count: 0,
            resource_count: 0,
            digest: crate::projection::item_set_digest_seed(),
            eof_resolved: false,
        }
    }

    fn validate_set(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        item: &crate::CanonicalItemRecord,
        set: &ItemProjectionSetRecord,
        source_is_immutable: bool,
    ) -> Result<(), SyndicValidationError> {
        if set.item_id() != item.id()
            || set.source_bytes() != set.source().logical_utf8_bytes()
            || set.projection_count() == 0
            || set.stable_projection_count() > set.projection_count()
            || set.stable_resource_count() > set.resource_count()
        {
            return invariant("item projection set frontiers are invalid");
        }
        if set.stable_eof_resolved() != source_is_immutable {
            return invariant("item projection set EOF stability disagrees with its source");
        }
        self.replay_to(
            reader,
            item,
            set.generation(),
            set.format(),
            set.source_item_revision(),
            set.source(),
            set.resume_checkpoint(),
            set.stable_eof_resolved(),
        )?;
        if self.projection_count != set.stable_projection_count()
            || self.resource_count != set.stable_resource_count()
            || self.digest != set.stable_digest()
        {
            return invariant("item projection set stable replay disagrees");
        }

        let mut total = self.clone();
        if set.stable_eof_resolved() {
            if set.projection_count() != set.stable_projection_count()
                || set.resource_count() != set.stable_resource_count()
                || set.digest() != set.stable_digest()
            {
                return invariant("EOF-resolved item projection set owns a suffix");
            }
        } else {
            let finished = total.advance_one(
                reader,
                item,
                set.generation(),
                set.format(),
                set.source_item_revision(),
                set.source(),
                Membership::Suffix(set.generation()),
            )?;
            if !finished {
                return invariant("item projection set suffix does not begin at EOF");
            }
            total.validate_finished(set.source_bytes())?;
            if total.projection_count != set.projection_count()
                || total.resource_count != set.resource_count()
                || total.digest != set.digest()
            {
                return invariant("item projection set suffix replay disagrees");
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn advance_one(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        item: &crate::CanonicalItemRecord,
        generation: ItemProjectionGeneration,
        format: ProjectionFormatVersion,
        source_item_revision: ProjectionRevision,
        source: ProjectionTextSource,
        membership: Membership,
    ) -> Result<bool, SyndicValidationError> {
        if self.eof_resolved {
            return invariant("projection replay advanced beyond EOF");
        }
        let build = ItemProjectionBuildRecord::new(
            item.id(),
            generation,
            ProjectionRevision::new(1).expect("replay build revision is nonzero"),
            format,
            source_item_revision,
            source,
            source.logical_utf8_bytes(),
            self.projection_count,
            self.resource_count,
            self.digest,
            ItemProjectionBuildPhase::Parsing(self.checkpoint.clone()),
        );
        let piece =
            crate::mutation::projection::range::load_piece(reader, &build, &self.checkpoint)
                .map_err(|_| {
                    SyndicValidationError::Invariant("projection replay source is invalid")
                })?;
        let step = crate::mutation::projection::parser::advance(&self.checkpoint, piece)
            .map_err(|_| SyndicValidationError::Invariant("projection parser replay failed"))?;
        for output in step.outputs {
            self.projection_count =
                self.projection_count
                    .checked_add(1)
                    .ok_or(SyndicValidationError::Invariant(
                        "projection replay frontier exhausted",
                    ))?;
            let ordinal = ProjectionOrdinal::new(self.projection_count).map_err(|_| {
                SyndicValidationError::Invariant("projection replay ordinal is invalid")
            })?;
            let materialized = crate::mutation::projection::materialize_output(
                reader, item, source, format, ordinal, output,
            )
            .map_err(|_| {
                SyndicValidationError::Invariant("projection replay materialization failed")
            })?;
            validate_projection_membership(
                reader,
                item.id(),
                generation,
                membership,
                &materialized.projection,
            )?;
            self.digest = crate::projection::advance_item_set_digest(
                self.digest,
                materialized.projection.id(),
                materialized.projection.revision(),
            );
            if let Some((resource, index)) = materialized.resource {
                self.resource_count =
                    self.resource_count
                        .checked_add(1)
                        .ok_or(SyndicValidationError::Invariant(
                            "resource replay frontier exhausted",
                        ))?;
                validate_resource_replay(reader, &resource, &index)?;
                self.digest = crate::projection::advance_item_set_resource_digest(
                    self.digest,
                    resource.id(),
                    resource.revision(),
                    *resource.digest().ok_or(SyndicValidationError::Invariant(
                        "replayed projection resource omitted its digest",
                    ))?,
                );
            }
        }
        self.checkpoint = step.checkpoint;
        self.eof_resolved = step.finished;
        Ok(step.finished)
    }

    fn validate_finished(&self, source_bytes: u64) -> Result<(), SyndicValidationError> {
        if !self.eof_resolved
            || self.checkpoint.consumed_source_bytes() != source_bytes
            || self.checkpoint.closed_source_bytes() != source_bytes
            || !self.checkpoint.line_carry().is_empty()
            || self.checkpoint.open_block().is_some()
        {
            return invariant("projection parser EOF checkpoint is invalid");
        }
        Ok(())
    }
}

impl ProjectionReplay {
    fn validate_build(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        item: &crate::CanonicalItemRecord,
        build: &ItemProjectionBuildRecord,
    ) -> Result<(), SyndicValidationError> {
        let (checkpoint, active) = match build.phase() {
            ItemProjectionBuildPhase::Parsing(checkpoint) => (checkpoint, true),
            ItemProjectionBuildPhase::Superseded(checkpoint) => (checkpoint, false),
        };
        if build.item_id() != item.id()
            || build.source_bytes() != build.source().logical_utf8_bytes()
            || active
                != (build.source_item_revision() == item.revision()
                    && item.projection_source() == Some(build.source()))
        {
            return invariant("item projection build source or lifecycle disagrees");
        }
        self.replay_to(
            reader,
            item,
            build.generation(),
            build.format(),
            build.source_item_revision(),
            build.source(),
            checkpoint,
            false,
        )?;
        if self.projection_count != build.projection_count()
            || self.resource_count != build.resource_count()
            || self.digest != build.output_digest()
        {
            return invariant("item projection build replay disagrees");
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn replay_to(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        item: &crate::CanonicalItemRecord,
        generation: ItemProjectionGeneration,
        format: ProjectionFormatVersion,
        source_item_revision: ProjectionRevision,
        source: ProjectionTextSource,
        target: &MarkdownParserCheckpoint,
        target_eof_resolved: bool,
    ) -> Result<(), SyndicValidationError> {
        if self
            .source
            .is_some_and(|previous| !previous.can_extend(source))
            || self.checkpoint.consumed_source_bytes() > source.logical_utf8_bytes()
            || self.eof_resolved && (!target_eof_resolved || self.checkpoint != *target)
        {
            return invariant("stable projection replay source lineage disagrees");
        }
        self.source = Some(source);
        while self.checkpoint != *target || self.eof_resolved != target_eof_resolved {
            if self.checkpoint.consumed_source_bytes() > target.consumed_source_bytes() {
                return invariant("stable projection checkpoint regressed");
            }
            let previous = self.checkpoint.clone();
            let previous_eof = self.eof_resolved;
            self.advance_one(
                reader,
                item,
                generation,
                format,
                source_item_revision,
                source,
                Membership::Stable,
            )?;
            if self.checkpoint.consumed_source_bytes() > target.consumed_source_bytes()
                || (self.checkpoint == previous && self.eof_resolved == previous_eof)
            {
                return invariant("stable projection checkpoint is unreachable");
            }
        }
        Ok(())
    }
}
