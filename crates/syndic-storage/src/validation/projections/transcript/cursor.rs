use beryl_home_store::DomainReader;

use crate::{
    ProjectionLifecycle, ProjectionOrdinal, TranscriptBuildRecord, TurnDepth, TurnItemOrdinal,
    codec::*, domain::SyndicDomain, error::SyndicValidationError,
};

use crate::validation::scan::require;

use super::visibility::is_transcript_visible;

pub(super) fn validate_publishing(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &TranscriptBuildRecord,
    next_depth: TurnDepth,
    next_item: TurnItemOrdinal,
    next_projection: ProjectionOrdinal,
) -> Result<(), SyndicValidationError> {
    let mut fold = PrefixFold::new();
    for depth_value in 1..=next_depth.get() {
        let depth = TurnDepth::new(depth_value)
            .map_err(|_| invariant("transcript publishing depth is invalid"))?;
        let path = require::<TranscriptPathTurnsFamily>(
            reader,
            &ThreadTranscriptPathKey {
                thread: build.thread_id(),
                generation: build.generation(),
                depth,
            },
            "transcript publishing path record is missing",
        )?;
        if depth < next_depth {
            fold.complete_turn(reader, build, &path)?;
        } else {
            fold.current_turn(reader, build, &path, next_item, next_projection)?;
        }
    }
    if fold.entry_count != build.entry_count() || fold.digest != build.entry_digest() {
        return Err(invariant("transcript publishing cursor prefix disagrees"));
    }
    Ok(())
}

struct PrefixFold {
    entry_count: u64,
    digest: [u8; 32],
}

impl PrefixFold {
    fn new() -> Self {
        Self {
            entry_count: 0,
            digest: crate::projection::transcript_entry_digest_seed(),
        }
    }

    fn complete_turn(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        build: &TranscriptBuildRecord,
        path: &crate::TranscriptPathTurnRecord,
    ) -> Result<(), SyndicValidationError> {
        for item_value in 1..=path.finalized_item_count() {
            let item = TurnItemOrdinal::new(item_value)
                .map_err(|_| invariant("transcript publishing item ordinal is invalid"))?;
            self.complete_item(reader, build, path.turn_id(), item)?;
        }
        Ok(())
    }

    fn current_turn(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        build: &TranscriptBuildRecord,
        path: &crate::TranscriptPathTurnRecord,
        next_item: TurnItemOrdinal,
        next_projection: ProjectionOrdinal,
    ) -> Result<(), SyndicValidationError> {
        if path.finalized_item_count() == 0 {
            if next_item != TurnItemOrdinal::FIRST || next_projection != ProjectionOrdinal::FIRST {
                return Err(invariant("empty turn publishing cursor is invalid"));
            }
            return Ok(());
        }
        if next_item.get() > path.finalized_item_count() {
            return Err(invariant(
                "transcript publishing item cursor exceeds its snapshot",
            ));
        }
        for item_value in 1..next_item.get() {
            let item = TurnItemOrdinal::new(item_value)
                .map_err(|_| invariant("transcript publishing item ordinal is invalid"))?;
            self.complete_item(reader, build, path.turn_id(), item)?;
        }
        let item = canonical_item(reader, path.turn_id(), next_item)?;
        if !is_transcript_visible(item.kind()) {
            if next_projection != ProjectionOrdinal::FIRST {
                return Err(invariant(
                    "operational item publishing cursor has a projection",
                ));
            }
            return Ok(());
        }
        let set = current_set(reader, &item)?;
        if next_projection.get() > set.projection_count() {
            return Err(invariant(
                "transcript publishing projection cursor exceeds its set",
            ));
        }
        for projection_value in 1..next_projection.get() {
            let projection = ProjectionOrdinal::new(projection_value)
                .map_err(|_| invariant("transcript publishing projection ordinal is invalid"))?;
            self.push_projection(reader, build, &item, &set, projection)?;
        }
        Ok(())
    }

    fn complete_item(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        build: &TranscriptBuildRecord,
        turn: beryl_model::SyndicTurnId,
        ordinal: TurnItemOrdinal,
    ) -> Result<(), SyndicValidationError> {
        let item = canonical_item(reader, turn, ordinal)?;
        if !is_transcript_visible(item.kind()) {
            return Ok(());
        }
        let set = current_set(reader, &item)?;
        for projection_value in 1..=set.projection_count() {
            let projection = ProjectionOrdinal::new(projection_value)
                .map_err(|_| invariant("transcript publishing projection ordinal is invalid"))?;
            self.push_projection(reader, build, &item, &set, projection)?;
        }
        Ok(())
    }

    fn push_projection(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        build: &TranscriptBuildRecord,
        item: &crate::CanonicalItemRecord,
        set: &crate::ItemProjectionSetRecord,
        ordinal: ProjectionOrdinal,
    ) -> Result<(), SyndicValidationError> {
        let index = crate::membership::point(reader, set, ordinal)?
            .ok_or_else(|| invariant("transcript publishing projection membership is missing"))?;
        let projection = require::<ProjectionsFamily>(
            reader,
            &index.projection_id(),
            "transcript publishing projection is missing",
        )?;
        if projection.item_id() != item.id()
            || projection.ordinal() != ordinal
            || projection.revision() != index.projection_revision()
        {
            return Err(invariant("transcript publishing projection disagrees"));
        }
        self.entry_count = self
            .entry_count
            .checked_add(1)
            .ok_or_else(|| invariant("transcript publishing entry count exhausted"))?;
        let position = crate::TranscriptPosition::new(self.entry_count)
            .map_err(|_| invariant("transcript publishing position is invalid"))?;
        self.digest = crate::projection::advance_transcript_entry_digest(
            self.digest,
            build.thread_id(),
            build.generation(),
            position,
            item.id(),
            item.revision(),
            set.generation(),
            projection.id(),
            projection.revision(),
        );
        Ok(())
    }
}

fn canonical_item(
    reader: &DomainReader<'_, SyndicDomain>,
    turn: beryl_model::SyndicTurnId,
    ordinal: TurnItemOrdinal,
) -> Result<crate::CanonicalItemRecord, SyndicValidationError> {
    let index = require::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: turn,
            ordinal,
        },
        "transcript publishing item index is missing",
    )?;
    let item = require::<CanonicalItemsFamily>(
        reader,
        &index.item_id(),
        "transcript publishing item is missing",
    )?;
    if item.turn_id() != turn
        || item.ordinal() != ordinal
        || item.revision() != index.item_revision()
    {
        return Err(invariant("transcript publishing item disagrees"));
    }
    Ok(item)
}

fn current_set(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
) -> Result<crate::ItemProjectionSetRecord, SyndicValidationError> {
    let head = require::<ItemProjectionHeadsFamily>(
        reader,
        &item.id(),
        "transcript publishing item head is missing",
    )?;
    if head.lifecycle() != ProjectionLifecycle::Current
        || head.source_item_revision() != item.revision()
    {
        return Err(invariant("transcript publishing item head is not current"));
    }
    let set = require::<ItemProjectionSetsFamily>(
        reader,
        &ItemProjectionSetKey {
            item: item.id(),
            generation: head.generation(),
        },
        "transcript publishing item set is missing",
    )?;
    if set.source_item_revision() != item.revision()
        || item.payload().content() != Some(set.source_content())
        || set.projection_count() == 0
    {
        return Err(invariant("transcript publishing item set is not current"));
    }
    Ok(set)
}

fn invariant(message: &'static str) -> SyndicValidationError {
    SyndicValidationError::Invariant(message)
}
