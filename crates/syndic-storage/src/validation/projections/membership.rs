use beryl_home_store::DomainReader;

use crate::validation::scan::{point, require, scan, scan_range};
use crate::{
    CanonicalItemKind, ProjectionPayload, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::invariant;

pub(super) fn validate_stable(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut current_item = None;
    let mut expected = 1_u64;
    let mut observed = 0_u64;
    scan::<StableItemProjectionsFamily>(reader, |key, index| {
        if current_item != Some(key.item) {
            finish_stable_item_frontier(reader, current_item, observed)?;
            current_item = Some(key.item);
            expected = 1;
            observed = 0;
        }
        if key.item != index.item_id()
            || key.ordinal != index.ordinal()
            || index.ordinal().get() != expected
        {
            return invariant("stable item-projection key or contiguous order disagrees");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &index.item_id(),
            "stable item-projection source item is missing",
        )?;
        let projection = require::<ProjectionsFamily>(
            reader,
            &index.projection_id(),
            "stable item-projection target is missing",
        )?;
        if projection.item_id() != index.item_id()
            || projection.ordinal() != index.ordinal()
            || projection.revision() != index.projection_revision()
            || projection.turn_id() != item.turn_id()
        {
            return invariant("stable item-projection target disagrees");
        }
        validate_projection_source(
            reader,
            &projection,
            item.payload()
                .content()
                .ok_or(SyndicValidationError::Invariant(
                    "stable projection source item omitted canonical content",
                ))?,
        )?;
        expected = expected
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "stable projection order exhausted",
            ))?;
        observed = observed
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "stable projection frontier exhausted",
            ))?;
        Ok(())
    })?;
    finish_stable_item_frontier(reader, current_item, observed)
}

fn finish_stable_item_frontier(
    reader: &DomainReader<'_, SyndicDomain>,
    item: Option<beryl_model::SyndicItemId>,
    observed: u64,
) -> Result<(), SyndicValidationError> {
    let Some(item) = item else {
        return Ok(());
    };
    let mut declared = 0_u64;
    scan_range::<ItemProjectionSetsFamily>(
        reader,
        ItemProjectionSetKey::first_for_item(item),
        ItemProjectionSetKey::last_for_item(item),
        |_, set| {
            declared = declared.max(set.stable_projection_count());
            Ok(())
        },
    )?;
    scan_range::<ItemProjectionBuildsFamily>(
        reader,
        ItemProjectionSetKey::first_for_item(item),
        ItemProjectionSetKey::last_for_item(item),
        |_, build| {
            declared = declared.max(build.projection_count());
            Ok(())
        },
    )?;
    if declared != observed {
        return invariant("stable item-projection frontier disagrees");
    }
    Ok(())
}

pub(super) fn validate_generation_suffixes(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut current_generation = None;
    let mut expected = 1_u64;
    let mut observed = 0_u64;
    scan::<ItemProjectionsFamily>(reader, |key, index| {
        let generation = (key.item, key.generation);
        if current_generation != Some(generation) {
            finish_item_projection_generation(reader, current_generation, observed)?;
            current_generation = Some(generation);
            let set = require::<ItemProjectionSetsFamily>(
                reader,
                &ItemProjectionSetKey {
                    item: key.item,
                    generation: key.generation,
                },
                "item-projection suffix owner set is missing",
            )?;
            expected = set.stable_projection_count().checked_add(1).ok_or(
                SyndicValidationError::Invariant("item-projection suffix frontier exhausted"),
            )?;
            observed = 0;
        }
        if key.item != index.item_id()
            || key.generation != index.generation()
            || key.ordinal != index.ordinal()
            || index.ordinal().get() != expected
        {
            return invariant("item-projection key or contiguous order disagrees");
        }
        let projection = require::<ProjectionsFamily>(
            reader,
            &index.projection_id(),
            "item-projection target is missing",
        )?;
        if projection.item_id() != index.item_id()
            || projection.ordinal() != index.ordinal()
            || projection.revision() != index.projection_revision()
        {
            return invariant("item-projection target disagrees");
        }
        let source = generation_source(reader, key)?;
        validate_projection_source(reader, &projection, source)?;
        expected = expected
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "projection order exhausted",
            ))?;
        observed += 1;
        Ok(())
    })?;
    finish_item_projection_generation(reader, current_generation, observed)
}

fn generation_source(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &ItemProjectionKey,
) -> Result<crate::ContentReference, SyndicValidationError> {
    let owner = ItemProjectionSetKey {
        item: key.item,
        generation: key.generation,
    };
    let set = require::<ItemProjectionSetsFamily>(
        reader,
        &owner,
        "item-projection suffix owner set is missing",
    )?;
    if point::<ItemProjectionBuildsFamily>(reader, &owner)?.is_some() {
        return invariant("item projection generation has two owners");
    }
    Ok(set.source_content())
}

fn validate_projection_source(
    reader: &DomainReader<'_, SyndicDomain>,
    projection: &crate::ProjectionRecord,
    source: crate::ContentReference,
) -> Result<(), SyndicValidationError> {
    match projection.payload() {
        ProjectionPayload::Empty => {
            if source.summary().logical_utf8_bytes() != 0 {
                return invariant("empty projection has nonempty generation source");
            }
        }
        ProjectionPayload::InlineMarkdown {
            source_range,
            source: inline,
            ..
        } => {
            let resolved = crate::validation::content::read_logical_range(
                reader,
                source,
                source_range.start(),
                source_range.end(),
            )?;
            if source_range.end() > source.summary().logical_utf8_bytes()
                || resolved != inline.as_bytes()
            {
                return invariant("inline projection bytes disagree with generation source");
            }
        }
        ProjectionPayload::ResourceReference { source_range, .. } => {
            if source_range.end() > source.summary().logical_utf8_bytes() {
                return invariant("resource projection exceeds its generation source");
            }
        }
        ProjectionPayload::ImageMarker { source_offset, .. } => {
            if *source_offset > source.summary().logical_utf8_bytes() {
                return invariant("image marker exceeds its generation source");
            }
        }
    }
    Ok(())
}

fn finish_item_projection_generation(
    reader: &DomainReader<'_, SyndicDomain>,
    generation: Option<(beryl_model::SyndicItemId, crate::ItemProjectionGeneration)>,
    observed: u64,
) -> Result<(), SyndicValidationError> {
    let Some((item, generation)) = generation else {
        return Ok(());
    };
    let key = ItemProjectionSetKey { item, generation };
    let set = require::<ItemProjectionSetsFamily>(
        reader,
        &key,
        "item projection suffix has no owning set",
    )?;
    if point::<ItemProjectionBuildsFamily>(reader, &key)?.is_some() {
        return invariant("item projection generation has both set and build state");
    }
    if set
        .projection_count()
        .checked_sub(set.stable_projection_count())
        != Some(observed)
    {
        return invariant("item projection generation suffix frontier disagrees");
    }
    Ok(())
}

pub(super) fn validate_heads(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ItemProjectionHeadsFamily>(reader, |key, head| {
        if *key != head.item_id() {
            return invariant("item projection head key disagrees");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            key,
            "item projection head source item is missing",
        )?;
        if !matches!(
            item.kind(),
            CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
        ) {
            return invariant("non-visible item has a projection head");
        }
        let set = require::<ItemProjectionSetsFamily>(
            reader,
            &ItemProjectionSetKey {
                item: *key,
                generation: head.generation(),
            },
            "item projection head selects a missing set",
        )?;
        if set.source_item_revision() != head.source_item_revision() {
            return invariant("item projection head and set source revisions disagree");
        }
        let source_is_current = set.source_item_revision() == item.revision()
            && item.payload().content() == Some(set.source_content());
        if source_is_current != (head.lifecycle() == crate::ProjectionLifecycle::Current) {
            return invariant("item projection head lifecycle disagrees with its source");
        }
        Ok(())
    })?;
    scan::<CanonicalItemsFamily>(reader, |key, item| {
        if !matches!(
            item.kind(),
            CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
        ) && point::<ItemProjectionHeadsFamily>(reader, key)?.is_some()
        {
            return invariant("operational canonical item has a projection head");
        }
        Ok(())
    })
}
