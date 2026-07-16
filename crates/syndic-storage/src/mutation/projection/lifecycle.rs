use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};
use beryl_model::SyndicItemId;

use crate::{
    ContentPieceOrdinal, ItemProjectionBuildPhase, ItemProjectionBuildRecord,
    MarkdownParserCheckpoint, SyndicMutationError, codec::*, domain::SyndicDomain,
};

use crate::mutation::point;

pub(super) struct ProjectionSeed {
    pub(super) projection_count: u64,
    pub(super) resource_count: u64,
    pub(super) output_digest: [u8; 32],
    pub(super) checkpoint: MarkdownParserCheckpoint,
}

impl ProjectionSeed {
    fn empty() -> Self {
        Self {
            projection_count: 0,
            resource_count: 0,
            output_digest: crate::projection::item_set_digest_seed(),
            checkpoint: MarkdownParserCheckpoint::new(
                0,
                0,
                ContentPieceOrdinal::FIRST,
                0,
                Box::<str>::default(),
                false,
                None,
            ),
        }
    }
}

pub(super) fn latest_build(
    reader: &DomainReader<'_, SyndicDomain>,
    item: SyndicItemId,
) -> Result<Option<ItemProjectionBuildRecord>, SyndicMutationError> {
    let page = reader.cursor::<ItemProjectionBuildsCodec>(
        &CursorRange::closed(
            ItemProjectionSetKey::first_for_item(item),
            ItemProjectionSetKey::last_for_item(item),
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 1024 * 1024).expect("latest-build bounds are nonzero"),
    )?;
    Ok(page.records().first().map(|record| record.value().clone()))
}

pub(super) fn projection_seed(
    build: Option<&ItemProjectionBuildRecord>,
    set: Option<&crate::ItemProjectionSetRecord>,
    current: crate::ContentReference,
) -> Result<ProjectionSeed, SyndicMutationError> {
    let use_build = match (build, set) {
        (Some(build), Some(set)) => build.generation() > set.generation(),
        (Some(_), None) => true,
        (None, Some(_)) | (None, None) => false,
    };
    if use_build {
        let build = build.ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        let checkpoint = match build.phase() {
            ItemProjectionBuildPhase::Parsing(checkpoint)
            | ItemProjectionBuildPhase::Superseded(checkpoint) => checkpoint.clone(),
        };
        validate_projection_seed_source(
            build.source_content(),
            build.source_bytes(),
            &checkpoint,
            current,
        )?;
        return Ok(ProjectionSeed {
            projection_count: build.projection_count(),
            resource_count: build.resource_count(),
            output_digest: build.output_digest(),
            checkpoint,
        });
    }
    if let Some(set) = set {
        if set.stable_eof_resolved() && set.source_content() != current {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        validate_projection_seed_source(
            set.source_content(),
            set.source_bytes(),
            set.resume_checkpoint(),
            current,
        )?;
        if set.stable_projection_count() > set.projection_count()
            || set.stable_resource_count() > set.resource_count()
        {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        return Ok(ProjectionSeed {
            projection_count: set.stable_projection_count(),
            resource_count: set.stable_resource_count(),
            output_digest: set.stable_digest(),
            checkpoint: set.resume_checkpoint().clone(),
        });
    }
    Ok(ProjectionSeed::empty())
}

fn validate_projection_seed_source(
    previous: crate::ContentReference,
    previous_bytes: u64,
    checkpoint: &MarkdownParserCheckpoint,
    current: crate::ContentReference,
) -> Result<(), SyndicMutationError> {
    if previous.id() != current.id()
        || previous.encoding() != current.encoding()
        || previous.revision() > current.revision()
        || previous_bytes != previous.summary().logical_utf8_bytes()
        || previous_bytes > current.summary().logical_utf8_bytes()
        || checkpoint.consumed_source_bytes() > previous_bytes
        || checkpoint.closed_source_bytes() > checkpoint.consumed_source_bytes()
    {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    Ok(())
}

pub(in crate::mutation) fn invalidate_item_projection(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
) -> Result<
    (
        Option<ItemProjectionBuildRecord>,
        Option<crate::ItemProjectionHeadRecord>,
    ),
    SyndicMutationError,
> {
    let content = item
        .payload()
        .content()
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    let build = match latest_build(reader, item.id())? {
        Some(build) => match build.phase() {
            ItemProjectionBuildPhase::Parsing(checkpoint) => {
                if build.source_item_revision() != item.revision()
                    || build.source_content() != content
                {
                    return Err(SyndicMutationError::ProjectionBuildConflict);
                }
                Some(ItemProjectionBuildRecord::new(
                    build.item_id(),
                    build.generation(),
                    build.revision().checked_next()?,
                    build.format(),
                    build.source_item_revision(),
                    build.source_content(),
                    build.source_bytes(),
                    build.projection_count(),
                    build.resource_count(),
                    build.output_digest(),
                    ItemProjectionBuildPhase::Superseded(checkpoint.clone()),
                ))
            }
            ItemProjectionBuildPhase::Superseded(_) => None,
        },
        None => None,
    };
    let head = stale_current_item_head(reader, item)?;
    Ok((build, head))
}

fn stale_current_item_head(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
) -> Result<Option<crate::ItemProjectionHeadRecord>, SyndicMutationError> {
    match point::<ItemProjectionHeadsFamily>(reader, &item.id())? {
        Some(head) if head.lifecycle() == crate::ProjectionLifecycle::Current => {
            if head.source_item_revision() != item.revision() {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            Ok(Some(crate::ItemProjectionHeadRecord::new(
                head.item_id(),
                head.revision().checked_next()?,
                head.source_item_revision(),
                head.generation(),
                crate::ProjectionLifecycle::Stale,
            )))
        }
        Some(_) | None => Ok(None),
    }
}

pub(super) fn latest_set(
    reader: &DomainReader<'_, SyndicDomain>,
    item: SyndicItemId,
) -> Result<Option<crate::ItemProjectionSetRecord>, SyndicMutationError> {
    let page = reader.cursor::<ItemProjectionSetsCodec>(
        &CursorRange::closed(
            ItemProjectionSetKey::first_for_item(item),
            ItemProjectionSetKey::last_for_item(item),
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 1024 * 1024).expect("latest-set bounds are nonzero"),
    )?;
    Ok(page.records().first().map(|record| record.value().clone()))
}
