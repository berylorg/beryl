mod lifecycle;
mod staging;

use std::sync::atomic::{AtomicBool, Ordering};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{
    BerylHomeId, CasLoadedSessionGeneration, CasThreadId, CasTurnId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::{
    BoundProviderObservation, ProviderFrameObservationSummaryV1, ProviderItemKind,
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
use crate::cas_projection::stop::{
    PublishedHardStopActivity, PublishedHardStopActivityKind, PublishedHardStopActivityLifecycle,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn consume(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    syndic_thread_id: SyndicThreadId,
    observation: BoundProviderObservation,
    limit: SyndicPointReadLimit,
    cancelled: &AtomicBool,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<ProviderObservationPublicationEffect, ProviderObservationPublicationError> {
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
    let frontier = match LiveSourceFrontier::read_provider_current(
        home,
        home_id,
        home_generation,
        storage,
        &target,
        limit,
        command,
    ) {
        Ok(frontier) => frontier,
        Err(error) => {
            inspected.abandon();
            return Err(error.into());
        }
    };
    let event = match resolved {
        ResolvedObservation::Frame(resolved) => {
            let item_id = resolved.item_id();
            let plan = resolved.into_plan(&target, frontier.sequence());
            let prepared =
                prepare_provider_observation_frame(&storage, home, inspected, plan, limit)?;
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
        ResolvedObservation::Issue(reason) => frontier.event(
            &target,
            Some(target.source().clone()),
            SourceEventPayload::ProviderObservationIssue(Box::new(inspected.into_issue(reason))),
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
}
