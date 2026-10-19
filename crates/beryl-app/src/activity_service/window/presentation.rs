use super::*;
use beryl_home_store::{
    CommandBuildError, CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    HomeCommand, ReadError, ReconciliationHandle,
};
use beryl_state::{ActivityPanelHeight, SessionMutationError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ActivityHeightError {
    #[error("Activity window is unavailable: {0}")]
    Activity(#[from] ActivityReadError),
    #[error("Activity height read failed: {0}")]
    Read(#[from] ReadError),
    #[error("Activity height preparation failed: {0}")]
    State(#[from] SessionMutationError),
    #[error("Activity height command failed: {0}")]
    Command(#[from] CommandBuildError),
}

pub(crate) struct ActivityHeightRead {
    selection: ActivityWindowSelection,
    observation: HomeMutationObservation,
    height: Option<ActivityPanelHeight>,
}

impl ActivityHeightRead {
    pub(crate) fn with_current<T>(
        &self,
        publish: impl FnOnce(Option<ActivityPanelHeight>) -> T,
    ) -> Result<T, ActivityReadError> {
        self.selection
            .publish_observed(&self.observation, || publish(self.height))
    }
}

#[derive(Debug)]
pub(crate) enum ActivityHeightCommit {
    NotCommitted {
        evidence: CommandError,
    },
    Committed {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate {
        failure: CommandError,
        handle: ReconciliationHandle,
    },
}

impl ActivityWindowSelection {
    pub(crate) fn read_height(&self) -> Result<ActivityHeightRead, ActivityHeightError> {
        let facts = self.observe(None)?;
        let operation = self.begin()?;
        let height = self
            .reader
            .session
            .window_presentation(&operation.resources().home, self.window)?
            .map(|record| record.activity_panel_height());
        self.publish_observed(&facts.observation, || ())?;
        Ok(ActivityHeightRead {
            selection: self.clone(),
            observation: facts.observation,
            height,
        })
    }

    pub(crate) fn commit_height(
        &self,
        height: ActivityPanelHeight,
    ) -> Result<ActivityHeightCommit, ActivityHeightError> {
        self.commit_height_elected(height, None)
    }

    pub(super) fn commit_height_elected(
        &self,
        height: ActivityPanelHeight,
        input: Option<&ActivityInteractionGate>,
    ) -> Result<ActivityHeightCommit, ActivityHeightError> {
        let operation = self.begin()?;
        let home = &operation.resources().home;
        let before = home.home_revision()?;
        if let Some(input) = input {
            input.with_current(|| ())?;
        }
        let facts = self.observe(None)?;
        let update =
            self.reader
                .session
                .prepare_activity_panel_height(home, self.window, height)?;
        let mut command = HomeCommand::new(before);
        update.add_to(&mut command)?;
        self.publish_observed(&facts.observation, || ())?;
        let outcome = match input {
            Some(input) => input.with_runtime_current(|| home.execute(command))?,
            None => home.execute(command),
        };
        Ok(match outcome {
            CommandOutcome::NotCommitted { evidence } => {
                ActivityHeightCommit::NotCommitted { evidence }
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => ActivityHeightCommit::Committed {
                receipt,
                later_failure,
                local_finalization,
            },
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => ActivityHeightCommit::Indeterminate {
                failure,
                handle: reconciliation.install_and_handle(),
            },
        })
    }
}
