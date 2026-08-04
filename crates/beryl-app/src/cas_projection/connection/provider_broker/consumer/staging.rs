use std::sync::atomic::{AtomicBool, Ordering};

use beryl_home_store::{CommandError, HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, SyndicItemId};
use syndic_storage::{
    PreparedProviderObservationFrame, ProviderFrameStageBatch, ProviderItemBuildLifecycle,
    ProviderItemBuildRecord, ProviderObservationFrameStageError, SyndicPointReadLimit,
    SyndicStorage, stage_provider_observation_frame,
};
use thiserror::Error;

pub(super) struct FrameCommitter<'a> {
    home: &'a HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    limit: SyndicPointReadLimit,
    cancelled: &'a AtomicBool,
    command: &'a crate::cas_projection::LiveCommandPermit,
}

impl<'a> FrameCommitter<'a> {
    pub(super) const fn new(
        home: &'a HomeStore,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
        limit: SyndicPointReadLimit,
        cancelled: &'a AtomicBool,
        command: &'a crate::cas_projection::LiveCommandPermit,
    ) -> Self {
        Self {
            home,
            home_id,
            home_generation,
            storage,
            limit,
            cancelled,
            command,
        }
    }

    fn begin(
        &self,
        prepared: &PreparedProviderObservationFrame,
    ) -> Result<ProviderItemBuildRecord, PersistenceCutError> {
        loop {
            let dispatch = self.execute_current(|| {
                self.home.execute_current(
                    self.storage
                        .current_begin_provider_observation_frame_build(prepared),
                )
            })?;
            match self.read_build(prepared.initial_build().item_id())? {
                Some(current) if &current == prepared.initial_build() => return Ok(current),
                None => match dispatch {
                    Err(error) if ambiguous_command_error(&error) => continue,
                    Err(error) => return Err(PersistenceCutError::Command(Box::new(error))),
                    Ok(_) => return Err(PersistenceCutError::ReportedSuccessWithoutAdvance),
                },
                Some(_) => return Err(PersistenceCutError::Collision),
            }
        }
    }

    fn dispatch_batch(&self, batch: &ProviderFrameStageBatch) -> Result<(), PersistenceCutError> {
        loop {
            let dispatch = self.execute_current(|| {
                self.home.execute_current(
                    self.storage
                        .current_stage_provider_frame_batch(batch.clone()),
                )
            })?;
            let durable = self.read_build(batch.expected_build().item_id())?;
            match durable {
                Some(current) if &current == batch.next_build() => return Ok(()),
                Some(current) if &current == batch.expected_build() => match dispatch {
                    Err(error) if ambiguous_command_error(&error) => continue,
                    Err(error) => return Err(PersistenceCutError::Command(Box::new(error))),
                    Ok(_) => return Err(PersistenceCutError::ReportedSuccessWithoutAdvance),
                },
                Some(_) | None => return Err(PersistenceCutError::Collision),
            }
        }
    }

    fn complete(
        &self,
        mut current: ProviderItemBuildRecord,
    ) -> Result<ProviderItemBuildRecord, PersistenceCutError> {
        while current.lifecycle() != ProviderItemBuildLifecycle::Sealed {
            let dispatch = self.execute_current(|| {
                self.home.execute_current(
                    self.storage
                        .current_compare_provider_completion(current.clone()),
                )
            })?;
            let Some(next) = self.read_build(current.item_id())? else {
                return Err(PersistenceCutError::Collision);
            };
            if next == current {
                match dispatch {
                    Err(error) if ambiguous_command_error(&error) => continue,
                    Err(error) => return Err(PersistenceCutError::Command(Box::new(error))),
                    Ok(_) => return Err(PersistenceCutError::ReportedSuccessWithoutAdvance),
                }
            }
            let Some(next_state) = next.completion_check().map(|check| check.state()) else {
                return Err(PersistenceCutError::Collision);
            };
            let expected = current
                .advance_completion(next_state)
                .map_err(|_| PersistenceCutError::Collision)?;
            if next != expected {
                return Err(PersistenceCutError::Collision);
            }
            current = next;
        }
        Ok(current)
    }

    fn execute_current<T>(
        &self,
        execute: impl FnOnce() -> Result<T, CommandError>,
    ) -> Result<Result<T, CommandError>, PersistenceCutError> {
        if self.home.home_id() != self.home_id {
            return Err(PersistenceCutError::HomeIdentity);
        }
        let verification = self
            .command
            .await_current_or_verification(self.home, self.home_id, self.home_generation)
            .map_err(PersistenceCutError::Authority)?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(PersistenceCutError::Cancelled);
        }
        let dispatch = execute();
        match verification.settle_after_operation() {
            Ok(settlement) if settlement.verified_current() => {}
            Ok(_) => {}
            Err(source) => return Err(PersistenceCutError::Authority(source)),
        }
        Ok(dispatch)
    }

    fn read_build(
        &self,
        item_id: SyndicItemId,
    ) -> Result<Option<ProviderItemBuildRecord>, PersistenceCutError> {
        loop {
            let verification = self
                .command
                .await_current_or_verification(self.home, self.home_id, self.home_generation)
                .map_err(PersistenceCutError::Authority)?;
            let current = self
                .storage
                .provider_item_build(self.home, item_id, self.limit);
            match verification.settle_after_operation() {
                Ok(settlement) if settlement.verified_current() => continue,
                Ok(_) => {
                    return current.map_err(|source| PersistenceCutError::Read(Box::new(source)));
                }
                Err(source) => return Err(PersistenceCutError::Authority(source)),
            }
        }
    }

    pub(super) fn commit(
        &self,
        prepared: &PreparedProviderObservationFrame,
    ) -> Result<ProviderItemBuildRecord, FrameCommitError> {
        let result = (|| {
            let current = self.begin(prepared).map_err(FrameCommitError::Begin)?;
            let mut callback = |batch: &ProviderFrameStageBatch| self.dispatch_batch(batch);
            let staged = stage_provider_observation_frame(
                &self.storage,
                self.home,
                prepared,
                current,
                self.limit,
                &mut callback,
            )?;
            self.complete(staged).map_err(FrameCommitError::Completion)
        })();
        if result
            .as_ref()
            .is_err_and(|error| error.authority().is_none())
        {
            let _ = self.command.observe_persistent_failure();
        }
        result
    }
}

fn ambiguous_command_error(error: &CommandError) -> bool {
    matches!(
        error,
        CommandError::HealthGate(_)
            | CommandError::RevisionRead { .. }
            | CommandError::ContributorAccess { .. }
            | CommandError::Commit { .. }
            | CommandError::Persistence { .. }
    )
}

#[derive(Debug, Error)]
pub(in crate::cas_projection::connection::provider_broker) enum FrameCommitError {
    #[error("provider-observation frame begin failed: {0}")]
    Begin(#[source] PersistenceCutError),
    #[error(transparent)]
    Stage(#[from] ProviderObservationFrameStageError<PersistenceCutError>),
    #[error("provider-observation frame completion failed: {0}")]
    Completion(#[source] PersistenceCutError),
}

impl FrameCommitError {
    pub(super) fn authority(&self) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
        match self {
            Self::Begin(source) | Self::Completion(source) => source.authority(),
            Self::Stage(syndic_storage::ProviderObservationFrameStageError::Callback(source)) => {
                source.authority()
            }
            Self::Stage(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub(in crate::cas_projection::connection::provider_broker) enum PersistenceCutError {
    #[error("provider-observation frame persistence was cancelled")]
    Cancelled,
    #[error("provider-observation frame home identity changed")]
    HomeIdentity,
    #[error("provider-observation frame command failed: {0}")]
    Command(#[source] Box<CommandError>),
    #[error("provider-observation frame lost exact verification authority: {0}")]
    Authority(#[source] crate::cas_projection::LiveCommandAdmissionError),
    #[error("provider-observation frame command reported success without a durable advance")]
    ReportedSuccessWithoutAdvance,
    #[error("provider-observation frame persistence found a conflicting durable frontier")]
    Collision,
    #[error("provider-observation frame reconciliation read failed: {0}")]
    Read(#[source] Box<syndic_storage::SyndicReadError>),
}

impl PersistenceCutError {
    fn authority(&self) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
        match self {
            Self::Authority(source) => Some(*source),
            _ => None,
        }
    }
}
