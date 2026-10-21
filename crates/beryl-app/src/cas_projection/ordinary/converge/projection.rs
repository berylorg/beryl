use syndic_storage::{
    AdvanceItemProjectionBuild, ItemProjectionBuildPhase, ItemProjectionSource,
    ProjectionLifecycle, PublishGeneratedResourceProjection, StartItemProjectionBuild,
    SyndicStorage,
};

use super::{access::HistoryAccess, command, snapshot};
use crate::cas_projection::OrdinaryTurnExecutionError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ItemProjectionStep {
    Current,
    Advanced,
}

pub(super) fn step(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    current: &snapshot::ProjectionSnapshot,
) -> Result<ItemProjectionStep, OrdinaryTurnExecutionError> {
    if current_projection(current)? {
        return Ok(ItemProjectionStep::Current);
    }
    if let snapshot::ProjectionBacking::GeneratedResource(resource) = &current.backing {
        command::dispatch(
            store,
            storage.current_publish_generated_resource_projection(
                PublishGeneratedResourceProjection::new(
                    current.item.id(),
                    current.item.revision(),
                    resource.id(),
                    current.generation,
                ),
            ),
        )?;
        return Ok(ItemProjectionStep::Advanced);
    }
    let request = match current.build.as_ref() {
        Some(build) => {
            if !valid_parsing_build(current, build) {
                return Err(OrdinaryTurnExecutionError::Invariant(
                    "item projection build is not resumable",
                ));
            }
            storage.current_advance_item_projection_build(AdvanceItemProjectionBuild::new(
                current.item.id(),
                current.generation,
                build.revision(),
            ))
        }
        None => storage.current_start_item_projection_build(StartItemProjectionBuild::new(
            current.item.id(),
            current.item.revision(),
            current.generation,
        )),
    };
    command::dispatch(store, request)?;
    Ok(ItemProjectionStep::Advanced)
}

fn current_projection(
    current: &snapshot::ProjectionSnapshot,
) -> Result<bool, OrdinaryTurnExecutionError> {
    let source = current
        .item
        .projection_source()
        .ok_or(OrdinaryTurnExecutionError::Invariant(
            "visible item projection source is missing",
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
                        && match source {
                            ItemProjectionSource::Text(source) => {
                                set.text_state().is_some_and(|text| {
                                    text.source_bytes() == source.logical_utf8_bytes()
                                })
                            }
                            ItemProjectionSource::GeneratedResource(_) => {
                                set.text_state().is_none()
                                    && set.projection_count() == 1
                                    && set.resource_count() == 0
                            }
                        }
                        && set.projection_count() != 0
                });
            if exact {
                Ok(true)
            } else {
                Err(OrdinaryTurnExecutionError::Invariant(
                    "current item projection is incoherent",
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
    let Some(source) = current.item.projection_text_source() else {
        return false;
    };
    build.item_id() == current.item.id()
        && build.generation() == current.generation
        && build.source_item_revision() == current.item.revision()
        && build.source() == source
        && build.source_bytes() == source.logical_utf8_bytes()
        && matches!(build.phase(), ItemProjectionBuildPhase::Parsing(_))
}
