use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CanonicalItemPayload, CanonicalItemRecord, ContentManifestRecord, ItemProjectionBuildRecord,
    ItemProjectionGeneration, ItemProjectionHeadRecord, ItemProjectionSetRecord,
    ProjectionLifecycle, ResourceMetadataRecord, SyndicPointReadLimit, SyndicStorage,
    TranscriptBuildRecord, TranscriptViewHeadRecord, TurnItemIndexRecord, TurnItemOrdinal,
    TurnRecord, TurnStateRecord,
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
    pub(super) manifest: Option<ContentManifestRecord>,
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
    if thread.record().id() != thread_id
        || turn.record().id() != turn_id
        || turn.record().origin_thread_id() != thread_id
        || state.record().turn_id() != turn_id
    {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "ordinary history convergence ownership disagrees",
        ));
    }
    Ok(TerminalTurnSnapshot {
        turn: turn.record().clone(),
        state: state.record().clone(),
    })
}

pub(super) fn turn_frontier(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    limit: SyndicPointReadLimit,
) -> Result<TurnFrontierSnapshot, OrdinaryTurnExecutionError> {
    let cursor_limits = CursorReadLimits::new(1, limit.max_stored_bytes())
        .expect("a Syndic point-read limit is nonzero");
    let state = storage.turn_state(store, turn_id, limit)?;
    let index = match state.as_ref().map(|stored| stored.record()) {
        Some(state) if state.finalized_item_count() < state.item_count() => {
            let after = ordinal_after(state.finalized_item_count());
            storage
                .turn_items(store, turn_id, after, cursor_limits)?
                .records()
                .first()
                .cloned()
        }
        Some(_) | None => None,
    };
    let item = match index.as_ref() {
        Some(index) => storage.canonical_item(store, index.item_id(), limit)?,
        None => None,
    };
    let manifest = match item
        .as_ref()
        .and_then(|item| item.record().payload().content())
    {
        Some(content) => storage.content_manifest(store, content.id(), limit)?,
        None => None,
    };
    let resource = match item.as_ref().map(|item| item.record().payload()) {
        Some(CanonicalItemPayload::GeneratedMedia(resource_id)) => {
            storage.resource(store, *resource_id, limit)?
        }
        Some(_) | None => None,
    };
    let confirmed_resource = match resource.as_ref() {
        Some(resource) => storage.resource(store, resource.record().id(), limit)?,
        None => None,
    };
    if storage.turn_state(store, turn_id, limit)? != state
        || confirmed_resource.as_ref().map(|record| record.record())
            != resource.as_ref().map(|record| record.record())
    {
        return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
    }
    assemble_frontier(turn_id, state, index, item, manifest, resource)
}

fn ordinal_after(finalized: u64) -> Option<TurnItemOrdinal> {
    (finalized != 0).then(|| {
        TurnItemOrdinal::new(finalized).expect("a nonzero finalized frontier is a valid ordinal")
    })
}

fn assemble_frontier(
    turn_id: SyndicTurnId,
    state: Option<syndic_storage::SyndicStoredRecord<TurnStateRecord>>,
    index: Option<TurnItemIndexRecord>,
    item: Option<syndic_storage::SyndicStoredRecord<CanonicalItemRecord>>,
    manifest: Option<syndic_storage::SyndicStoredRecord<ContentManifestRecord>>,
    resource: Option<syndic_storage::SyndicStoredRecord<ResourceMetadataRecord>>,
) -> Result<TurnFrontierSnapshot, OrdinaryTurnExecutionError> {
    let state = state.ok_or(OrdinaryTurnExecutionError::Invariant(
        "ordinary history convergence turn state disappeared",
    ))?;
    let state = state.record().clone();
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
    let item = item.record().clone();
    let manifest = match (item.payload().content(), manifest) {
        (Some(content), Some(manifest))
            if manifest.record().id() == content.id()
                && manifest.record().current_reference() == Some(content) =>
        {
            Some(manifest.record().clone())
        }
        (None, None) => None,
        (Some(_), None) => {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "ordinary history convergence content manifest is missing",
            ));
        }
        (None, Some(_)) | (Some(_), Some(_)) => {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "ordinary history convergence canonical item content disagrees",
            ));
        }
    };
    let resource = match (item.payload(), resource) {
        (CanonicalItemPayload::GeneratedMedia(expected), Some(resource))
            if resource.record().id() == *expected && resource.record().item_id() == item.id() =>
        {
            Some(resource.record().clone())
        }
        (CanonicalItemPayload::GeneratedMedia(_), None) => {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "ordinary history convergence generated resource is missing",
            ));
        }
        (CanonicalItemPayload::GeneratedMedia(_), Some(_)) | (_, Some(_)) => {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "ordinary history convergence generated resource disagrees",
            ));
        }
        (_, None) => None,
    };
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
            manifest,
            resource,
        }),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProjectionSnapshot {
    pub(super) item: CanonicalItemRecord,
    pub(super) manifest: ContentManifestRecord,
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
    let manifest = match item
        .as_ref()
        .and_then(|item| item.record().payload().content())
    {
        Some(content) => storage.content_manifest(store, content.id(), limit)?,
        None => None,
    };
    let head = storage.item_projection_head(store, item_id, limit)?;
    let generation = projection_generation(head.as_ref().map(|head| head.record()));
    let (build, set) = match generation {
        Some(generation) => (
            storage.item_projection_build(store, item_id, generation, limit)?,
            storage.item_projection_set(store, item_id, generation, limit)?,
        ),
        None => (None, None),
    };
    let confirmed_item = storage.canonical_item(store, item_id, limit)?;
    let confirmed_head = storage.item_projection_head(store, item_id, limit)?;
    let (confirmed_build, confirmed_set) = match generation {
        Some(generation) => (
            storage.item_projection_build(store, item_id, generation, limit)?,
            storage.item_projection_set(store, item_id, generation, limit)?,
        ),
        None => (None, None),
    };
    if confirmed_item != item
        || confirmed_head != head
        || confirmed_build != build
        || confirmed_set != set
    {
        return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
    }
    assemble_projection(item_id, item, manifest, head, generation, build, set)
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
    item: Option<syndic_storage::SyndicStoredRecord<CanonicalItemRecord>>,
    manifest: Option<syndic_storage::SyndicStoredRecord<ContentManifestRecord>>,
    head: Option<syndic_storage::SyndicStoredRecord<ItemProjectionHeadRecord>>,
    generation: Option<ItemProjectionGeneration>,
    build: Option<syndic_storage::SyndicStoredRecord<ItemProjectionBuildRecord>>,
    set: Option<syndic_storage::SyndicStoredRecord<ItemProjectionSetRecord>>,
) -> Result<ProjectionSnapshot, OrdinaryTurnExecutionError> {
    let item = item.ok_or(OrdinaryTurnExecutionError::Invariant(
        "item projection source is missing",
    ))?;
    let manifest = manifest.ok_or(OrdinaryTurnExecutionError::Invariant(
        "item projection content manifest is missing",
    ))?;
    let generation = generation.ok_or(OrdinaryTurnExecutionError::Invariant(
        "item-projection generation is exhausted",
    ))?;
    let item = item.record().clone();
    let manifest = manifest.record().clone();
    let content = item
        .payload()
        .content()
        .ok_or(OrdinaryTurnExecutionError::Invariant(
            "item projection source has no text content",
        ))?;
    let head = head.map(|head| *head.record());
    let build = build.map(|build| build.record().clone());
    let set = set.map(|set| set.record().clone());
    if item.id() != item_id
        || manifest.id() != content.id()
        || manifest.current_reference() != Some(content)
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
        manifest,
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
        Some(head) => {
            storage.transcript_build(store, thread_id, head.record().generation(), limit)?
        }
        None => None,
    };
    let confirmed_thread = storage.thread(store, thread_id, limit)?;
    let confirmed_head = storage.transcript_view_head(store, thread_id, limit)?;
    let confirmed_build = match head.as_ref() {
        Some(head) => {
            storage.transcript_build(store, thread_id, head.record().generation(), limit)?
        }
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
    let thread = thread.record().clone();
    let head = head.record().clone();
    let build = build.map(|build| *build.record());
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
