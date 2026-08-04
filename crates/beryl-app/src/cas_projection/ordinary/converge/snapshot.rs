use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CanonicalItemRecord, ContentLifecycle, ContentManifestRecord, ItemProjectionBuildRecord,
    ItemProjectionGeneration, ItemProjectionHeadRecord, ItemProjectionSetRecord,
    ProjectionLifecycle, ProjectionTextSource, ResourceMetadataRecord, SyndicPointReadLimit,
    SyndicStorage, TranscriptBuildRecord, TranscriptViewHeadRecord, TurnItemIndexRecord,
    TurnItemOrdinal, TurnRecord, TurnStateRecord,
};

use super::super::OrdinaryTurnExecutionError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TerminalTurnSnapshot {
    pub(super) turn: TurnRecord,
    pub(super) state: TurnStateRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CanonicalSnapshot {
    pub(super) index: TurnItemIndexRecord,
    pub(super) item: CanonicalItemRecord,
    pub(super) provider_manifest: Option<ContentManifestRecord>,
    pub(super) projection_manifest: Option<ContentManifestRecord>,
    pub(super) resource: Option<ResourceMetadataRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TurnFrontierSnapshot {
    pub(super) state: TurnStateRecord,
    pub(super) next: Option<CanonicalSnapshot>,
}

pub(super) fn terminal_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    limit: SyndicPointReadLimit,
) -> Result<TerminalTurnSnapshot, OrdinaryTurnExecutionError> {
    let thread = storage.thread(store, thread_id, limit)?;
    let state = storage.turn_state(store, turn_id, limit)?;
    let turn = storage.turn(store, turn_id, limit)?;
    let confirmed_thread = storage.thread(store, thread_id, limit)?;
    let confirmed_state = storage.turn_state(store, turn_id, limit)?;
    if confirmed_thread != thread || confirmed_state != state {
        return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
    }
    let thread = thread.ok_or(OrdinaryTurnExecutionError::Invariant(
        "ordinary history convergence thread is missing",
    ))?;
    let turn = turn.ok_or(OrdinaryTurnExecutionError::Invariant(
        "ordinary history convergence turn is missing",
    ))?;
    let state = state.ok_or(OrdinaryTurnExecutionError::Invariant(
        "ordinary history convergence turn state is missing",
    ))?;
    if thread.id() != thread_id
        || turn.id() != turn_id
        || turn.origin_thread_id() != thread_id
        || state.turn_id() != turn_id
    {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "ordinary history convergence ownership disagrees",
        ));
    }
    Ok(TerminalTurnSnapshot { turn, state })
}

pub(super) fn turn_frontier(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    limit: SyndicPointReadLimit,
) -> Result<TurnFrontierSnapshot, OrdinaryTurnExecutionError> {
    let cursor_limits =
        CursorReadLimits::new(1, limit.max_bytes()).expect("a Syndic point-read limit is nonzero");
    let state = storage.turn_state(store, turn_id, limit)?;
    let index = next_item_index(store, storage, turn_id, state.as_ref(), cursor_limits)?;
    let item = match index.as_ref() {
        Some(index) => storage.canonical_item(store, index.item_id(), limit)?,
        None => None,
    };
    let provider_manifest = match item
        .as_ref()
        .and_then(CanonicalItemRecord::provider_content)
    {
        Some(content) => storage.content_manifest(store, content.id(), limit)?,
        None => None,
    };
    let projection_manifest = match item
        .as_ref()
        .and_then(CanonicalItemRecord::projection_source)
    {
        Some(source) => storage.content_manifest(store, source.content_id(), limit)?,
        None => None,
    };
    let resource = match item
        .as_ref()
        .and_then(|item| item.presentation().resource_id())
    {
        Some(resource_id) => storage.resource(store, resource_id, limit)?,
        None => None,
    };
    let confirmed_item = match index.as_ref() {
        Some(index) => storage.canonical_item(store, index.item_id(), limit)?,
        None => None,
    };
    let confirmed_provider_manifest = reread_manifest(store, storage, &provider_manifest, limit)?;
    let confirmed_projection_manifest =
        reread_manifest(store, storage, &projection_manifest, limit)?;
    let confirmed_resource = reread_resource(store, storage, &resource, limit)?;
    let confirmed_index = next_item_index(store, storage, turn_id, state.as_ref(), cursor_limits)?;
    if confirmed_item != item
        || confirmed_provider_manifest != provider_manifest
        || confirmed_projection_manifest != projection_manifest
        || confirmed_resource != resource
        || confirmed_index != index
        || storage.turn_state(store, turn_id, limit)? != state
    {
        return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
    }
    assemble_frontier(
        turn_id,
        state,
        index,
        item,
        provider_manifest,
        projection_manifest,
        resource,
    )
}

fn next_item_index(
    store: &HomeStore,
    storage: SyndicStorage,
    turn_id: SyndicTurnId,
    state: Option<&TurnStateRecord>,
    limits: CursorReadLimits,
) -> Result<Option<TurnItemIndexRecord>, OrdinaryTurnExecutionError> {
    match state {
        Some(state) if state.finalized_item_count() < state.item_count() => Ok(storage
            .turn_items(
                store,
                turn_id,
                ordinal_after(state.finalized_item_count()),
                limits,
            )?
            .records()
            .first()
            .cloned()),
        Some(_) | None => Ok(None),
    }
}

fn reread_manifest(
    store: &HomeStore,
    storage: SyndicStorage,
    manifest: &Option<ContentManifestRecord>,
    limit: SyndicPointReadLimit,
) -> Result<Option<ContentManifestRecord>, OrdinaryTurnExecutionError> {
    match manifest {
        Some(manifest) => Ok(storage.content_manifest(store, manifest.id(), limit)?),
        None => Ok(None),
    }
}

fn reread_resource(
    store: &HomeStore,
    storage: SyndicStorage,
    resource: &Option<ResourceMetadataRecord>,
    limit: SyndicPointReadLimit,
) -> Result<Option<ResourceMetadataRecord>, OrdinaryTurnExecutionError> {
    match resource {
        Some(resource) => Ok(storage.resource(store, resource.id(), limit)?),
        None => Ok(None),
    }
}

fn ordinal_after(finalized: u64) -> Option<TurnItemOrdinal> {
    (finalized != 0).then(|| {
        TurnItemOrdinal::new(finalized).expect("a nonzero finalized frontier is a valid ordinal")
    })
}

fn assemble_frontier(
    turn_id: SyndicTurnId,
    state: Option<TurnStateRecord>,
    index: Option<TurnItemIndexRecord>,
    item: Option<CanonicalItemRecord>,
    provider_manifest: Option<ContentManifestRecord>,
    projection_manifest: Option<ContentManifestRecord>,
    resource: Option<ResourceMetadataRecord>,
) -> Result<TurnFrontierSnapshot, OrdinaryTurnExecutionError> {
    let state = state.ok_or(OrdinaryTurnExecutionError::Invariant(
        "ordinary history convergence turn state disappeared",
    ))?;
    if state.turn_id() != turn_id || state.finalized_item_count() > state.item_count() {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "ordinary history convergence turn frontier is invalid",
        ));
    }
    if state.finalized_item_count() == state.item_count() {
        return Ok(TurnFrontierSnapshot { state, next: None });
    }
    let expected = state
        .finalized_item_count()
        .checked_add(1)
        .and_then(|value| TurnItemOrdinal::new(value).ok())
        .ok_or(OrdinaryTurnExecutionError::Invariant(
            "ordinary history convergence item ordinal is exhausted",
        ))?;
    let index = index.ok_or(OrdinaryTurnExecutionError::Invariant(
        "ordinary history convergence next item index is missing",
    ))?;
    let item = item.ok_or(OrdinaryTurnExecutionError::Invariant(
        "ordinary history convergence canonical item is missing",
    ))?;
    validate_item_backings(
        &item,
        provider_manifest.as_ref(),
        projection_manifest.as_ref(),
        resource.as_ref(),
    )?;
    if index.turn_id() != turn_id
        || index.ordinal() != expected
        || item.id() != index.item_id()
        || item.turn_id() != turn_id
        || item.ordinal() != expected
        || item.revision() != index.item_revision()
    {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "ordinary history convergence canonical item disagrees",
        ));
    }
    Ok(TurnFrontierSnapshot {
        state,
        next: Some(CanonicalSnapshot {
            index,
            item,
            provider_manifest,
            projection_manifest,
            resource,
        }),
    })
}

fn validate_item_backings(
    item: &CanonicalItemRecord,
    provider_manifest: Option<&ContentManifestRecord>,
    projection_manifest: Option<&ContentManifestRecord>,
    resource: Option<&ResourceMetadataRecord>,
) -> Result<(), OrdinaryTurnExecutionError> {
    match (item.provider_content(), provider_manifest) {
        (Some(content), Some(manifest))
            if manifest.id() == content.id()
                && manifest.owner() == Some(item.id())
                && matches!(
                    manifest.lifecycle(),
                    ContentLifecycle::Live | ContentLifecycle::Finalized
                )
                && manifest.current_reference() == Some(content) => {}
        (None, None) => {}
        _ => {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "ordinary history convergence provider manifest disagrees",
            ));
        }
    }
    match (item.projection_source(), projection_manifest) {
        (Some(ProjectionTextSource::Composer(content)), Some(manifest))
            if manifest.id() == content.id()
                && manifest.lifecycle() == ContentLifecycle::Sealed
                && manifest.sealed_reference() == Some(content) => {}
        (Some(ProjectionTextSource::ProviderNarrative(narrative)), Some(manifest))
            if manifest.id() == narrative.content_id() && provider_manifest == Some(manifest) => {}
        (None, None) => {}
        _ => {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "ordinary history convergence projection manifest disagrees",
            ));
        }
    }
    match (item.presentation().resource_id(), resource) {
        (Some(resource_id), Some(resource))
            if resource.id() == resource_id && resource.item_id() == item.id() => {}
        (None, None) => {}
        _ => {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "ordinary history convergence generated resource disagrees",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProjectionSnapshot {
    pub(super) item: CanonicalItemRecord,
    pub(super) provider_manifest: Option<ContentManifestRecord>,
    pub(super) projection_manifest: ContentManifestRecord,
    pub(super) head: Option<ItemProjectionHeadRecord>,
    pub(super) generation: ItemProjectionGeneration,
    pub(super) build: Option<ItemProjectionBuildRecord>,
    pub(super) set: Option<ItemProjectionSetRecord>,
}

pub(super) fn item_projection(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    item_id: SyndicItemId,
    limit: SyndicPointReadLimit,
) -> Result<ProjectionSnapshot, OrdinaryTurnExecutionError> {
    let item = storage.canonical_item(store, item_id, limit)?;
    let provider_manifest = match item
        .as_ref()
        .and_then(CanonicalItemRecord::provider_content)
    {
        Some(content) => storage.content_manifest(store, content.id(), limit)?,
        None => None,
    };
    let projection_manifest = match item
        .as_ref()
        .and_then(CanonicalItemRecord::projection_source)
    {
        Some(source) => storage.content_manifest(store, source.content_id(), limit)?,
        None => None,
    };
    let head = storage.item_projection_head(store, item_id, limit)?;
    let generation = projection_generation(head.as_ref());
    let (build, set) = match generation {
        Some(generation) => (
            storage.item_projection_build(store, item_id, generation, limit)?,
            storage.item_projection_set(store, item_id, generation, limit)?,
        ),
        None => (None, None),
    };
    let (confirmed_build, confirmed_set) = match generation {
        Some(generation) => (
            storage.item_projection_build(store, item_id, generation, limit)?,
            storage.item_projection_set(store, item_id, generation, limit)?,
        ),
        None => (None, None),
    };
    let confirmed_provider_manifest = reread_manifest(store, storage, &provider_manifest, limit)?;
    let confirmed_projection_manifest =
        reread_manifest(store, storage, &projection_manifest, limit)?;
    let confirmed_head = storage.item_projection_head(store, item_id, limit)?;
    let confirmed_item = storage.canonical_item(store, item_id, limit)?;
    if confirmed_build != build
        || confirmed_set != set
        || confirmed_provider_manifest != provider_manifest
        || confirmed_projection_manifest != projection_manifest
        || confirmed_head != head
        || confirmed_item != item
    {
        return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
    }
    assemble_projection(
        item_id,
        item,
        provider_manifest,
        projection_manifest,
        head,
        generation,
        build,
        set,
    )
}

fn projection_generation(
    head: Option<&ItemProjectionHeadRecord>,
) -> Option<ItemProjectionGeneration> {
    match head {
        Some(head) if head.lifecycle() == ProjectionLifecycle::Current => Some(head.generation()),
        Some(head) => head.generation().checked_next().ok(),
        None => Some(ItemProjectionGeneration::FIRST),
    }
}

#[allow(clippy::too_many_arguments)]
fn assemble_projection(
    item_id: SyndicItemId,
    item: Option<CanonicalItemRecord>,
    provider_manifest: Option<ContentManifestRecord>,
    projection_manifest: Option<ContentManifestRecord>,
    head: Option<ItemProjectionHeadRecord>,
    generation: Option<ItemProjectionGeneration>,
    build: Option<ItemProjectionBuildRecord>,
    set: Option<ItemProjectionSetRecord>,
) -> Result<ProjectionSnapshot, OrdinaryTurnExecutionError> {
    let item = item.ok_or(OrdinaryTurnExecutionError::Invariant(
        "item projection source is missing",
    ))?;
    let projection_manifest = projection_manifest.ok_or(OrdinaryTurnExecutionError::Invariant(
        "item projection content manifest is missing",
    ))?;
    let generation = generation.ok_or(OrdinaryTurnExecutionError::Invariant(
        "item-projection generation is exhausted",
    ))?;
    validate_item_backings(
        &item,
        provider_manifest.as_ref(),
        Some(&projection_manifest),
        None,
    )?;
    let source = item
        .projection_source()
        .ok_or(OrdinaryTurnExecutionError::Invariant(
            "item projection source has no text content",
        ))?;
    if item.id() != item_id
        || projection_manifest.id() != source.content_id()
        || head.as_ref().is_some_and(|head| head.item_id() != item_id)
        || build
            .as_ref()
            .is_some_and(|build| build.item_id() != item_id || build.generation() != generation)
        || set
            .as_ref()
            .is_some_and(|set| set.item_id() != item_id || set.generation() != generation)
    {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "item projection snapshot disagrees with its source",
        ));
    }
    Ok(ProjectionSnapshot {
        item,
        provider_manifest,
        projection_manifest,
        head,
        generation,
        build,
        set,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TranscriptSnapshot {
    pub(super) thread: syndic_storage::ThreadRecord,
    pub(super) head: TranscriptViewHeadRecord,
    pub(super) build: Option<TranscriptBuildRecord>,
}

pub(super) fn transcript(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
) -> Result<TranscriptSnapshot, OrdinaryTurnExecutionError> {
    let thread = storage.thread(store, thread_id, limit)?;
    let head = storage.transcript_view_head(store, thread_id, limit)?;
    let build = match head.as_ref() {
        Some(head) => storage.transcript_build(store, thread_id, head.generation(), limit)?,
        None => None,
    };
    let confirmed_thread = storage.thread(store, thread_id, limit)?;
    let confirmed_head = storage.transcript_view_head(store, thread_id, limit)?;
    let confirmed_build = match head.as_ref() {
        Some(head) => storage.transcript_build(store, thread_id, head.generation(), limit)?,
        None => None,
    };
    if confirmed_thread != thread || confirmed_head != head || confirmed_build != build {
        return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
    }
    let thread = thread.ok_or(OrdinaryTurnExecutionError::Invariant(
        "selected transcript thread is missing",
    ))?;
    let head = head.ok_or(OrdinaryTurnExecutionError::Invariant(
        "selected transcript head is missing",
    ))?;
    if thread.id() != thread_id
        || head.thread_id() != thread_id
        || build.as_ref().is_some_and(|build| {
            build.thread_id() != thread_id || build.generation() != head.generation()
        })
    {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "selected transcript snapshot disagrees",
        ));
    }
    Ok(TranscriptSnapshot {
        thread,
        head,
        build,
    })
}
