use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};
use beryl_model::SyndicItemId;

use crate::{
    ItemProjectionBuildPhase, ItemProjectionBuildRecord, MarkdownParserCheckpoint,
    ProjectionTextSource, SyndicMutationError, codec::*, domain::SyndicDomain,
};

use crate::mutation::{point, required};

pub(super) struct ProjectionSeed {
    pub(super) projection_count: u64,
    pub(super) resource_count: u64,
    pub(super) output_digest: [u8; 32],
    pub(super) checkpoint: MarkdownParserCheckpoint,
}

pub(in crate::mutation) fn validate_projection_source(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
    source: ProjectionTextSource,
) -> Result<bool, SyndicMutationError> {
    if item.projection_text_source() != Some(source) {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    match source {
        ProjectionTextSource::Composer(content) => {
            let manifest = required::<ContentManifestsFamily>(reader, &content.id())?;
            if content.encoding() != crate::ContentEncoding::ComposerV1
                || manifest.lifecycle() != crate::ContentLifecycle::Sealed
                || manifest.sealed_reference() != Some(content)
            {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            Ok(true)
        }
        ProjectionTextSource::ProviderNarrative(narrative) => {
            let provider = item
                .provider()
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            let manifest = required::<ContentManifestsFamily>(reader, &narrative.content_id())?;
            let complete = provider.stream_state().is_complete();
            let valid_lifecycle = match manifest.lifecycle() {
                crate::ContentLifecycle::Live => true,
                crate::ContentLifecycle::Finalized => complete,
                crate::ContentLifecycle::Building | crate::ContentLifecycle::Sealed => false,
            };
            if provider.narrative() != Some(narrative)
                || provider.content().id() != narrative.content_id()
                || manifest.owner() != Some(item.id())
                || manifest.encoding() != crate::ContentEncoding::ProviderItemV1
                || manifest.current_reference() != Some(provider.content())
                || !valid_lifecycle
            {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            Ok(complete)
        }
    }
}

pub(in crate::mutation) fn validate_item_projection_source(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
    source: crate::ItemProjectionSource,
) -> Result<(), SyndicMutationError> {
    if item.projection_source() != Some(source) {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    match source {
        crate::ItemProjectionSource::Text(source) => {
            validate_projection_source(reader, item, source)?;
        }
        crate::ItemProjectionSource::GeneratedResource(resource_id) => {
            let resource = required::<ResourcesFamily>(reader, &resource_id)?;
            if item.kind() != crate::CanonicalItemKind::GeneratedMedia
                || resource.item_id() != item.id()
                || resource.projection_id().is_some()
                || resource.ordinal().is_some()
                || !matches!(
                    resource.backing(),
                    crate::ResourceBacking::GeneratedMedia(_)
                )
            {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
        }
    }
    Ok(())
}

impl ProjectionSeed {
    fn empty(source: ProjectionTextSource) -> Self {
        Self {
            projection_count: 0,
            resource_count: 0,
            output_digest: crate::projection::item_set_digest_seed(),
            checkpoint: MarkdownParserCheckpoint::new(
                0,
                0,
                source.initial_cursor(),
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
    current: ProjectionTextSource,
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
            build.source(),
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
        let text = set
            .text_state()
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        let previous = set
            .source()
            .text()
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        if text.stable_eof_resolved() && previous != current {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        validate_projection_seed_source(
            previous,
            text.source_bytes(),
            text.resume_checkpoint(),
            current,
        )?;
        if text.stable_projection_count() > set.projection_count()
            || text.stable_resource_count() > set.resource_count()
        {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        return Ok(ProjectionSeed {
            projection_count: text.stable_projection_count(),
            resource_count: text.stable_resource_count(),
            output_digest: text.stable_digest(),
            checkpoint: text.resume_checkpoint().clone(),
        });
    }
    Ok(ProjectionSeed::empty(current))
}

fn validate_projection_seed_source(
    previous: ProjectionTextSource,
    previous_bytes: u64,
    checkpoint: &MarkdownParserCheckpoint,
    current: ProjectionTextSource,
) -> Result<(), SyndicMutationError> {
    if !previous.can_extend(current)
        || previous_bytes != previous.logical_utf8_bytes()
        || previous_bytes > current.logical_utf8_bytes()
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
    let source = item
        .projection_source()
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    let build = match latest_build(reader, item.id())? {
        Some(build) => match build.phase() {
            ItemProjectionBuildPhase::Parsing(checkpoint) => {
                if build.source_item_revision() != item.revision()
                    || Some(build.source()) != source.text()
                {
                    return Err(SyndicMutationError::ProjectionBuildConflict);
                }
                Some(ItemProjectionBuildRecord::new(
                    build.item_id(),
                    build.generation(),
                    build.revision().checked_next()?,
                    build.format(),
                    build.source_item_revision(),
                    build.source(),
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
