mod lifecycle;
mod staging;

use std::sync::atomic::{AtomicBool, Ordering};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{
    BerylHomeId, CasLoadedSessionGeneration, CasThreadId, CasTurnId, ProviderObservationId,
    SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    BoundProviderObservation, PreparedProviderObservationFrame, ProviderFrameObservationSummaryV1,
    ProviderItemKind, ProviderObservationCursorError, ProviderObservationIssue,
    ProviderObservationRoute, SourceEventPayload, SyndicPointReadLimit, SyndicStorage,
    inspect_provider_observation, prepare_provider_observation_frame,
};
use thiserror::Error;

use self::{
    lifecycle::{ProviderObservationLifecycleError, ResolvedObservation},
    staging::{FrameCommitError, FrameCommitter},
};
use crate::cas_projection::live_source::{
    LiveSourceFrontier, LiveSourcePublicationError, LiveSourceTarget, publish_provider_reconciled,
};
use crate::cas_projection::stop::{
    PublishedHardStopActivity, PublishedHardStopActivityKind, PublishedHardStopActivityLifecycle,
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
    let hard_stop_activity = match &resolved {
        ResolvedObservation::Frame(frame) => PublishedHardStopActivityEffect::from_frame(
            &target,
            frame.item_id(),
            inspected.item_kind(),
            inspected.lifecycle(),
        ),
        ResolvedObservation::Issue(_) => None,
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
        hard_stop_activity,
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
    hard_stop_activity: Option<PublishedHardStopActivityEffect>,
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
) -> Result<ProviderObservationPublicationEffect, ProviderObservationPublicationError> {
    let PreparedProviderObservationPublication {
        target,
        frontier,
        publication,
        hard_stop_activity,
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
    Ok(ProviderObservationPublicationEffect { hard_stop_activity })
}

pub(super) struct ProviderObservationPublicationEffect {
    hard_stop_activity: Option<PublishedHardStopActivityEffect>,
}

impl ProviderObservationPublicationEffect {
    pub(super) fn into_activity(self) -> Option<PublishedHardStopActivityEffect> {
        self.hard_stop_activity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublishedHardStopActivityEffect {
    syndic_thread_id: SyndicThreadId,
    syndic_turn_id: SyndicTurnId,
    item_id: SyndicItemId,
    kind: PublishedHardStopActivityKind,
    lifecycle: PublishedHardStopActivityLifecycle,
}

pub(super) struct BoundPublishedHardStopActivity {
    effect: PublishedHardStopActivityEffect,
    loaded_generation: CasLoadedSessionGeneration,
    cas_thread_id: CasThreadId,
    cas_turn_id: CasTurnId,
}

impl PublishedHardStopActivityEffect {
    fn from_frame(
        target: &LiveSourceTarget,
        item_id: SyndicItemId,
        kind: ProviderItemKind,
        lifecycle: ProviderFrameObservationSummaryV1,
    ) -> Option<Self> {
        let (kind, lifecycle) = hard_stop_activity_transition(kind, lifecycle)?;
        Some(Self {
            syndic_thread_id: target.thread_id(),
            syndic_turn_id: target.turn_id(),
            item_id,
            kind,
            lifecycle,
        })
    }

    pub(super) fn bind(
        self,
        permit: &crate::cas_projection::connection::router::SourcePublicationPermit,
    ) -> BoundPublishedHardStopActivity {
        BoundPublishedHardStopActivity {
            effect: self,
            loaded_generation: permit.loaded_generation(),
            cas_thread_id: permit.cas_thread_id().clone(),
            cas_turn_id: permit.cas_turn_id().clone(),
        }
    }
}

impl BoundPublishedHardStopActivity {
    pub(super) fn into_published(self) -> PublishedHardStopActivity {
        PublishedHardStopActivity::new(
            self.effect.syndic_thread_id,
            self.effect.syndic_turn_id,
            self.loaded_generation,
            self.cas_thread_id,
            self.cas_turn_id,
            self.effect.item_id,
            self.effect.kind,
            self.effect.lifecycle,
        )
    }
}

fn hard_stop_activity_transition(
    kind: ProviderItemKind,
    lifecycle: ProviderFrameObservationSummaryV1,
) -> Option<(
    PublishedHardStopActivityKind,
    PublishedHardStopActivityLifecycle,
)> {
    let kind = match kind {
        ProviderItemKind::CommandExecution => PublishedHardStopActivityKind::Command,
        ProviderItemKind::SubAgentActivity => PublishedHardStopActivityKind::ChildOrSubagent,
        _ => return None,
    };
    let lifecycle = match lifecycle {
        ProviderFrameObservationSummaryV1::Started(_) => PublishedHardStopActivityLifecycle::Active,
        ProviderFrameObservationSummaryV1::Delta => return None,
        ProviderFrameObservationSummaryV1::Completed(_) => {
            PublishedHardStopActivityLifecycle::Completed
        }
    };
    Some((kind, lifecycle))
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/provider_broker_consumer.rs"
    ));
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
