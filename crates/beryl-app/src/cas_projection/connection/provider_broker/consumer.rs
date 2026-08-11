mod lifecycle;
mod staging;

use std::sync::atomic::{AtomicBool, Ordering};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, ProviderObservationId, SyndicItemId, SyndicThreadId};
use syndic_storage::{
    BoundProviderObservation, PreparedProviderObservationFrame, ProviderItemKind,
    ProviderObservationCursorError, ProviderObservationIssue, ProviderObservationRoute,
    SourceEventPayload, SyndicPointReadLimit, SyndicStorage, inspect_provider_observation,
    prepare_provider_observation_frame,
};
use thiserror::Error;

use self::{
    lifecycle::{ProviderObservationLifecycleError, ResolvedObservation},
    staging::{FrameCommitError, FrameCommitter},
};
use crate::cas_projection::live_source::{
    LiveSourceFrontier, LiveSourcePublicationError, LiveSourceTarget, publish_provider_reconciled,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    home: &HomeStore,
    storage: SyndicStorage,
    syndic_thread_id: SyndicThreadId,
    observation: BoundProviderObservation,
    limit: SyndicPointReadLimit,
    cancelled: &AtomicBool,
) -> Result<PreparedProviderObservationPublication, ProviderObservationPublicationError> {
    if cancelled.load(Ordering::Acquire) {
        observation.abandon();
        return Err(ProviderObservationPublicationError::Cancelled);
    }
    let target = LiveSourceTarget::resolve(
        home,
        storage,
        syndic_thread_id,
        observation.route().thread_id(),
        observation.route().turn_id(),
        limit,
    )?;
    let inspected = inspect_provider_observation(&storage, home, observation, limit)?;
    if inspected.item_kind() == ProviderItemKind::ContextCompaction {
        inspected.abandon();
        return Err(ProviderObservationPublicationError::CompactionTargetRequired);
    }
    let resolved = match lifecycle::resolve(
        home,
        storage,
        &target,
        inspected.lifecycle(),
        inspected.item_kind(),
        inspected.item_id().clone(),
        limit,
    ) {
        Ok(resolved) => resolved,
        Err(error) => {
            inspected.abandon();
            return Err(error.into());
        }
    };
    let frontier = match LiveSourceFrontier::read(home, storage, &target, limit) {
        Ok(frontier) => frontier,
        Err(error) => {
            inspected.abandon();
            return Err(error.into());
        }
    };
    let publication = match resolved {
        ResolvedObservation::Frame(resolved) => {
            let item_id = resolved.item_id();
            let plan = resolved.into_plan(&target, frontier.sequence());
            let prepared =
                prepare_provider_observation_frame(&storage, home, inspected, plan, limit)?;
            PreparedProviderObservationPayload::Frame { item_id, prepared }
        }
        ResolvedObservation::Issue(reason) => {
            PreparedProviderObservationPayload::Issue(inspected.into_issue(reason))
        }
    };
    Ok(PreparedProviderObservationPublication {
        target,
        frontier,
        publication,
    })
}

pub(super) fn reopen_exact(
    home: &HomeStore,
    storage: SyndicStorage,
    identity: ProviderObservationId,
    route: &ProviderObservationRoute,
    limit: SyndicPointReadLimit,
) -> Result<BoundProviderObservation, ProviderObservationPublicationError> {
    let sealed = storage
        .reopen_provider_observation(home, identity, limit)
        .map_err(syndic_storage::ProviderObservationFramePreparationError::from)?
        .ok_or(ProviderObservationCursorError::BuildMissing)
        .map_err(syndic_storage::ProviderObservationFramePreparationError::from)?;
    Ok(sealed
        .bind(route.clone(), route.clone())
        .expect("the exact admitted provider route binds to itself"))
}

pub(super) struct PreparedProviderObservationPublication {
    target: LiveSourceTarget,
    frontier: LiveSourceFrontier,
    publication: PreparedProviderObservationPayload,
}

enum PreparedProviderObservationPayload {
    Frame {
        item_id: SyndicItemId,
        prepared: PreparedProviderObservationFrame,
    },
    Issue(ProviderObservationIssue),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn publish(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    prepared: PreparedProviderObservationPublication,
    limit: SyndicPointReadLimit,
    cancelled: &AtomicBool,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<(), ProviderObservationPublicationError> {
    let PreparedProviderObservationPublication {
        target,
        frontier,
        publication,
    } = prepared;
    let event = match publication {
        PreparedProviderObservationPayload::Frame { item_id, prepared } => {
            let sealed = FrameCommitter::new(
                home,
                home_id,
                home_generation,
                storage,
                limit,
                cancelled,
                command,
            )
            .commit(&prepared)?;
            frontier.event(
                &target,
                Some(target.source().clone()),
                SourceEventPayload::ItemFrame {
                    item_id,
                    frame: Box::new(sealed.target().clone()),
                },
            )?
        }
        PreparedProviderObservationPayload::Issue(issue) => frontier.event(
            &target,
            Some(target.source().clone()),
            SourceEventPayload::ProviderObservationIssue(Box::new(issue)),
        )?,
    };
    publish_provider_reconciled(
        home,
        home_id,
        home_generation,
        storage,
        &event,
        limit,
        command,
    )?;
    Ok(())
}

#[derive(Debug, Error)]
pub(super) enum ProviderObservationPublicationError {
    #[error("provider-observation publication was cancelled")]
    Cancelled,
    #[error("context-compaction marker requires exact provider-operation target authority")]
    CompactionTargetRequired,
    #[error(transparent)]
    LiveSource(#[from] LiveSourcePublicationError),
    #[error(transparent)]
    Lifecycle(#[from] ProviderObservationLifecycleError),
    #[error(transparent)]
    Preparation(#[from] syndic_storage::ProviderObservationFramePreparationError),
    #[error(transparent)]
    FrameCommit(#[from] FrameCommitError),
}

impl ProviderObservationPublicationError {
    pub(super) fn authority(&self) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
        match self {
            Self::LiveSource(source) => source.authority(),
            Self::FrameCommit(source) => source.authority(),
            _ => None,
        }
    }

    pub(super) fn verification_ambiguous(
        &self,
        expected_generation: beryl_home_store::HomeGeneration,
    ) -> bool {
        match self {
            Self::LiveSource(LiveSourcePublicationError::Read(source))
            | Self::Lifecycle(ProviderObservationLifecycleError::Read(source)) => {
                syndic_read_is_health_gated(source, expected_generation)
            }
            Self::Preparation(
                syndic_storage::ProviderObservationFramePreparationError::Cursor(
                    ProviderObservationCursorError::Read(source),
                ),
            ) => syndic_read_is_health_gated(source, expected_generation),
            _ => false,
        }
    }
}

fn syndic_read_is_health_gated(
    source: &syndic_storage::SyndicReadError,
    expected_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match source {
        syndic_storage::SyndicReadError::Read(beryl_home_store::ReadError::HealthGate(source)) => {
            health_gate_values_match(
                source.state(),
                source.generation().get(),
                expected_generation.get(),
            )
        }
        _ => false,
    }
}

pub(super) const fn health_gate_values_match(
    state: beryl_home_store::HomeHealthState,
    actual_generation: u64,
    expected_generation: u64,
) -> bool {
    matches!(state, beryl_home_store::HomeHealthState::Verifying)
        && actual_generation == expected_generation
}
