use super::access::HistoryAccess;

use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    ContentLifecycle, FinalizeNextTurnItem, FreezeNextTurnItem, GeneratedMediaResourceDisposition,
    ProjectionTextSource, ProviderItemLifecycle, ResourceBacking, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp, TurnKind, TurnStateRecord,
};

use super::super::OrdinaryTurnExecutionError;
use super::{command, live, snapshot};

pub(super) fn converge_turn_items(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    let terminal = snapshot::terminal_turn(store, storage, thread_id, turn_id, limit)?;
    if !matches!(
        terminal.turn.kind(),
        TurnKind::OrdinaryUser
            | TurnKind::BerylLifecycleContinuation
            | TurnKind::BerylDiscussionHandoff
    ) {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "ordinary history convergence received a provider-operation turn",
        ));
    }
    require_terminal(&terminal.state, turn_id)?;

    let after = loop {
        let frontier = snapshot::turn_frontier(store, storage, thread_id, turn_id, limit)?;
        require_terminal(&frontier.state, turn_id)?;
        let Some(next) = frontier.next.as_ref() else {
            return Ok(());
        };
        if next.item.provider_lifecycle() != ProviderItemLifecycle::Completed {
            break snapshot::ordinal_after(frontier.state.finalized_item_count());
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
                ) => break snapshot::ordinal_after(frontier.state.finalized_item_count()),
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
    };
    live::converge_turn_projections(store, storage, thread_id, turn_id, after, limit)
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
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    _limit: SyndicPointReadLimit,
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
    command::dispatch(store, storage.current_freeze_next_turn_item(request))
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
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
    _limit: SyndicPointReadLimit,
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
    command::dispatch(store, storage.current_finalize_next_turn_item(request))
}

fn converge_item_projection(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    source: &snapshot::CanonicalSnapshot,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    loop {
        let current =
            snapshot::item_projection(store, storage, thread_id, source.item.id(), limit)?;
        validate_projection_source(&current, source)?;
        if super::projection::step(store, storage, &current)?
            == super::projection::ItemProjectionStep::Current
        {
            return Ok(());
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
