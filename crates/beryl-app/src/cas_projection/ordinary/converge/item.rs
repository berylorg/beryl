use beryl_home_store::{CommandError, HomeStore};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AdvanceItemProjectionBuild, ContentLifecycle, FinalizeNextTurnItem, FreezeNextTurnItem,
    GeneratedMediaResourceDisposition, ItemProjectionBuildPhase, ProjectionLifecycle,
    ProjectionTextSource, ProviderItemLifecycle, ResourceBacking, StartItemProjectionBuild,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnKind, TurnStateRecord,
};

use super::super::OrdinaryTurnExecutionError;
use super::{command, snapshot};

#[derive(Clone, Copy)]
struct TerminalTurnIdentity {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
}

pub(super) fn converge_turn_items(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    let terminal = snapshot::terminal_turn(store, storage, thread_id, turn_id, limit)?;
    if terminal.turn.kind() != TurnKind::OrdinaryUser {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "ordinary history convergence received a provider-operation turn",
        ));
    }
    require_terminal(&terminal.state, turn_id)?;

    loop {
        let frontier = snapshot::turn_frontier(store, storage, thread_id, turn_id, limit)?;
        require_terminal(&frontier.state, turn_id)?;
        let Some(next) = frontier.next.as_ref() else {
            return Ok(());
        };
        if next.item.provider_lifecycle() != ProviderItemLifecycle::Completed {
            return Ok(());
        }
        let provider_manifest =
            next.provider_manifest
                .as_ref()
                .ok_or(OrdinaryTurnExecutionError::Invariant(
                    "completed provider item has no ProviderItemV1 manifest",
                ))?;
        match provider_manifest.lifecycle() {
            ContentLifecycle::Live => {
                freeze_live_item(
                    store,
                    storage,
                    thread_id,
                    turn_id,
                    minimum_observed_at,
                    limit,
                    &frontier,
                )?;
                continue;
            }
            ContentLifecycle::Finalized => {}
            ContentLifecycle::Building | ContentLifecycle::Sealed => {
                return Err(OrdinaryTurnExecutionError::Invariant(
                    "completed provider item has an invalid content lifecycle",
                ));
            }
        }
        if let Some(resource_id) = next.item.presentation().resource_id() {
            let resource = next
                .resource
                .as_ref()
                .ok_or(OrdinaryTurnExecutionError::Invariant(
                    "terminal generated item has no resource metadata",
                ))?;
            if resource.id() != resource_id || resource.item_id() != next.item.id() {
                return Err(OrdinaryTurnExecutionError::Invariant(
                    "terminal generated item resource disagrees",
                ));
            }
            match resource.backing() {
                ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::Asset(_)) => {}
                ResourceBacking::GeneratedMedia(
                    GeneratedMediaResourceDisposition::PendingAsset
                    | GeneratedMediaResourceDisposition::Unavailable(_),
                ) => return Ok(()),
                ResourceBacking::TextRange { .. } => {
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "generated item resource has text backing",
                    ));
                }
            }
        } else if next.resource.is_some() {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "non-generated item unexpectedly owns a resource",
            ));
        }
        if next.item.projection_source().is_some() {
            converge_item_projection(store, storage, thread_id, next, limit)?;
        }
        finalize_item(
            store,
            storage,
            thread_id,
            turn_id,
            minimum_observed_at,
            limit,
            &frontier,
        )?;
    }
}

fn require_terminal(
    state: &TurnStateRecord,
    turn_id: SyndicTurnId,
) -> Result<(), OrdinaryTurnExecutionError> {
    if state.turn_id() != turn_id || !state.lifecycle().is_proven_terminal() {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "ordinary history convergence requires a proven-terminal turn",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn freeze_live_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
    before: &snapshot::TurnFrontierSnapshot,
) -> Result<(), OrdinaryTurnExecutionError> {
    let item = before
        .next
        .as_ref()
        .expect("freeze is called only with a next item");
    let updated_at = before.state.updated_at().max(minimum_observed_at);
    let request = FreezeNextTurnItem::new(
        thread_id,
        turn_id,
        before.state.revision(),
        item.index.ordinal(),
        item.item.id(),
        updated_at,
    );
    let dispatch = command::dispatch(store, storage.current_freeze_next_turn_item(request));
    let Err(error) = dispatch else {
        return Ok(());
    };
    reconcile_freeze_error(
        store,
        storage,
        TerminalTurnIdentity { thread_id, turn_id },
        updated_at,
        limit,
        before,
        error,
    )
}

fn reconcile_freeze_error(
    store: &HomeStore,
    storage: SyndicStorage,
    turn: TerminalTurnIdentity,
    updated_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
    before: &snapshot::TurnFrontierSnapshot,
    error: CommandError,
) -> Result<(), OrdinaryTurnExecutionError> {
    let after = snapshot::turn_frontier(store, storage, turn.thread_id, turn.turn_id, limit)?;
    require_terminal(&after.state, turn.turn_id)?;
    let ordinal = before
        .next
        .as_ref()
        .expect("freeze reconciliation has a next item")
        .index
        .ordinal()
        .get();
    if after.state.finalized_item_count() >= ordinal
        || exact_freeze_progress(before, &after, updated_at)
    {
        return Ok(());
    }
    if &after == before {
        return Err(error.into());
    }
    Err(OrdinaryTurnExecutionError::ConcurrentChange {
        thread_id: turn.thread_id,
    })
}

fn exact_freeze_progress(
    before: &snapshot::TurnFrontierSnapshot,
    after: &snapshot::TurnFrontierSnapshot,
    updated_at: SyndicTimestamp,
) -> bool {
    let (Some(before_item), Some(after_item)) = (&before.next, &after.next) else {
        return false;
    };
    let (Some(before_manifest), Some(after_manifest)) = (
        &before_item.provider_manifest,
        &after_item.provider_manifest,
    ) else {
        return false;
    };
    let (Some(before_provider), Some(after_provider)) =
        (before_item.item.provider(), after_item.item.provider())
    else {
        return false;
    };
    exact_freeze_state(&before.state, &after.state, updated_at)
        && after_item.index.turn_id() == before_item.index.turn_id()
        && after_item.index.ordinal() == before_item.index.ordinal()
        && after_item.index.item_id() == before_item.index.item_id()
        && next_is(
            before_item.index.item_revision().get(),
            after_item.index.item_revision().get(),
        )
        && after_item.item.id() == before_item.item.id()
        && after_item.item.turn_id() == before_item.item.turn_id()
        && after_item.item.ordinal() == before_item.item.ordinal()
        && next_is(
            before_item.item.revision().get(),
            after_item.item.revision().get(),
        )
        && after_item.item.kind() == before_item.item.kind()
        && after_item.item.source_event() == before_item.item.source_event()
        && after_item.item.source_event_count() == before_item.item.source_event_count()
        && after_item.item.cas_source() == before_item.item.cas_source()
        && after_item.item.provider_kind() == before_item.item.provider_kind()
        && after_item.item.provider_lifecycle() == before_item.item.provider_lifecycle()
        && after_item.item.assistant_phase() == before_item.item.assistant_phase()
        && after_item.item.narrative_completion() == before_item.item.narrative_completion()
        && after_item.item.presentation() == before_item.item.presentation()
        && after_item.item.projection_source() == before_item.item.projection_source()
        && exact_frozen_provider(before_provider, after_provider)
        && exact_frozen_manifest(before_manifest, after_manifest)
        && exact_projection_manifest_after_freeze(before_item, after_item)
        && after_item.resource == before_item.resource
}

fn exact_freeze_state(
    before: &TurnStateRecord,
    after: &TurnStateRecord,
    updated_at: SyndicTimestamp,
) -> bool {
    after.turn_id() == before.turn_id()
        && next_is(before.revision().get(), after.revision().get())
        && after.lifecycle() == before.lifecycle()
        && after.source_event_count() == before.source_event_count()
        && after.item_count() == before.item_count()
        && after.finalized_item_count() == before.finalized_item_count()
        && after.open_item_count() == before.open_item_count()
        && after.history_blocking_item_count() == before.history_blocking_item_count()
        && after.provider_observation_issue() == before.provider_observation_issue()
        && after.end_status() == before.end_status()
        && after.updated_at() == updated_at
}

fn exact_frozen_provider(
    before: &syndic_storage::SealedProviderFrameReference,
    after: &syndic_storage::SealedProviderFrameReference,
) -> bool {
    before.frame() == after.frame()
        && before.observation() == after.observation()
        && before.stream_state() == after.stream_state()
        && before.narrative() == after.narrative()
}

fn exact_frozen_manifest(
    before: &syndic_storage::ContentManifestRecord,
    after: &syndic_storage::ContentManifestRecord,
) -> bool {
    before.lifecycle() == ContentLifecycle::Live
        && after.id() == before.id()
        && after.owner() == before.owner()
        && next_is(before.revision().get(), after.revision().get())
        && after.encoding() == before.encoding()
        && after.lifecycle() == ContentLifecycle::Finalized
        && after.chunk_count() == before.chunk_count()
        && after.encoded_bytes() == before.encoded_bytes()
        && after.chain_digest() == before.chain_digest()
        && after.expected() == before.expected()
}

fn exact_projection_manifest_after_freeze(
    before: &snapshot::CanonicalSnapshot,
    after: &snapshot::CanonicalSnapshot,
) -> bool {
    match before.item.projection_source() {
        Some(ProjectionTextSource::ProviderNarrative(_)) => {
            before.projection_manifest == before.provider_manifest
                && after.projection_manifest == after.provider_manifest
        }
        Some(ProjectionTextSource::Composer(_)) | None => {
            before.projection_manifest == after.projection_manifest
        }
    }
}

fn next_is(before: u64, after: u64) -> bool {
    before.checked_add(1) == Some(after)
}

#[allow(clippy::too_many_arguments)]
fn finalize_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
    before: &snapshot::TurnFrontierSnapshot,
) -> Result<(), OrdinaryTurnExecutionError> {
    let item = before
        .next
        .as_ref()
        .expect("finalization is called only with a next item");
    let request = FinalizeNextTurnItem::new(
        thread_id,
        turn_id,
        before.state.revision(),
        item.index.ordinal(),
        item.item.id(),
        before.state.updated_at().max(minimum_observed_at),
    );
    let dispatch = command::dispatch(store, storage.current_finalize_next_turn_item(request));
    let Err(error) = dispatch else {
        return Ok(());
    };
    let after = snapshot::turn_frontier(store, storage, thread_id, turn_id, limit)?;
    require_terminal(&after.state, turn_id)?;
    if after.state.finalized_item_count() >= item.index.ordinal().get() {
        return Ok(());
    }
    if &after == before {
        return Err(error.into());
    }
    Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id })
}

fn converge_item_projection(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    source: &snapshot::CanonicalSnapshot,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    loop {
        let current =
            snapshot::item_projection(store, storage, thread_id, source.item.id(), limit)?;
        validate_projection_source(&current, source)?;
        if current_projection(&current)? {
            return Ok(());
        }
        match current.build.as_ref() {
            Some(build) => {
                if !valid_parsing_build(&current, build) {
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "terminal item projection build is not resumable",
                    ));
                }
                advance_projection(store, storage, thread_id, source, limit, &current, build)?;
            }
            None => start_projection(store, storage, thread_id, source, limit, &current)?,
        }
    }
}

fn validate_projection_source(
    current: &snapshot::ProjectionSnapshot,
    source: &snapshot::CanonicalSnapshot,
) -> Result<(), OrdinaryTurnExecutionError> {
    if current.item != source.item
        || current.provider_manifest != source.provider_manifest
        || source.projection_manifest.as_ref() != Some(&current.projection_manifest)
        || !current.projection_manifest.lifecycle().is_immutable()
    {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "terminal item projection source changed",
        ));
    }
    Ok(())
}

fn current_projection(
    current: &snapshot::ProjectionSnapshot,
) -> Result<bool, OrdinaryTurnExecutionError> {
    let source = current
        .item
        .projection_source()
        .ok_or(OrdinaryTurnExecutionError::Invariant(
            "visible terminal item projection source has no text content",
        ))?;
    match current.head.as_ref() {
        Some(head) if head.lifecycle() == ProjectionLifecycle::Current => {
            let exact = head.generation() == current.generation
                && head.source_item_revision() == current.item.revision()
                && current.build.is_none()
                && current.set.as_ref().is_some_and(|set| {
                    set.item_id() == current.item.id()
                        && set.generation() == head.generation()
                        && set.source_item_revision() == current.item.revision()
                        && set.source() == source
                        && set.source_bytes() == source.logical_utf8_bytes()
                        && set.projection_count() != 0
                });
            if exact {
                Ok(true)
            } else {
                Err(OrdinaryTurnExecutionError::Invariant(
                    "current terminal item projection is incoherent",
                ))
            }
        }
        Some(_) | None if current.set.is_none() => Ok(false),
        Some(_) | None => Err(OrdinaryTurnExecutionError::Invariant(
            "uncurrent item projection unexpectedly owns the selected set",
        )),
    }
}

fn valid_parsing_build(
    current: &snapshot::ProjectionSnapshot,
    build: &syndic_storage::ItemProjectionBuildRecord,
) -> bool {
    let Some(source) = current.item.projection_source() else {
        return false;
    };
    build.item_id() == current.item.id()
        && build.generation() == current.generation
        && build.source_item_revision() == current.item.revision()
        && build.source() == source
        && build.source_bytes() == source.logical_utf8_bytes()
        && matches!(build.phase(), ItemProjectionBuildPhase::Parsing(_))
}

fn start_projection(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    source: &snapshot::CanonicalSnapshot,
    limit: SyndicPointReadLimit,
    before: &snapshot::ProjectionSnapshot,
) -> Result<(), OrdinaryTurnExecutionError> {
    let request =
        StartItemProjectionBuild::new(source.item.id(), source.item.revision(), before.generation);
    let dispatch = command::dispatch(store, storage.current_start_item_projection_build(request));
    let Err(error) = dispatch else {
        return Ok(());
    };
    let after = snapshot::item_projection(store, storage, thread_id, source.item.id(), limit)?;
    validate_projection_source(&after, source)?;
    if current_projection(&after)?
        || after.generation == before.generation
            && after
                .build
                .as_ref()
                .is_some_and(|build| valid_parsing_build(&after, build))
    {
        return Ok(());
    }
    dispatch_or_concurrent(error, &after, before, thread_id)
}

fn advance_projection(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    source: &snapshot::CanonicalSnapshot,
    limit: SyndicPointReadLimit,
    before: &snapshot::ProjectionSnapshot,
    build: &syndic_storage::ItemProjectionBuildRecord,
) -> Result<(), OrdinaryTurnExecutionError> {
    let request =
        AdvanceItemProjectionBuild::new(source.item.id(), before.generation, build.revision());
    let dispatch = command::dispatch(
        store,
        storage.current_advance_item_projection_build(request),
    );
    let Err(error) = dispatch else {
        return Ok(());
    };
    let after = snapshot::item_projection(store, storage, thread_id, source.item.id(), limit)?;
    validate_projection_source(&after, source)?;
    if current_projection(&after)? {
        return Ok(());
    }
    if after.generation == before.generation
        && after.build.as_ref().is_some_and(|advanced| {
            valid_parsing_build(&after, advanced) && advanced.revision() > build.revision()
        })
    {
        return Ok(());
    }
    dispatch_or_concurrent(error, &after, before, thread_id)
}

fn dispatch_or_concurrent(
    error: CommandError,
    after: &snapshot::ProjectionSnapshot,
    before: &snapshot::ProjectionSnapshot,
    thread_id: SyndicThreadId,
) -> Result<(), OrdinaryTurnExecutionError> {
    if after == before {
        Err(error.into())
    } else {
        Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id })
    }
}
