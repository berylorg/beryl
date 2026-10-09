use super::*;
use crate::main_window::running_threads::activation::RunningThreadActivationCommit;
use crate::same_window_thread_acquisition::SameWindowThreadCommit;
use beryl_home_store::HomeCandidateRecoveryAccess;

pub(crate) enum RetiredClaimOperation {
    Creation(Box<RetiredSameWindowThreadOperation>),
    Ordinary(Box<RetiredOrdinarySelectionOperation>),
}

#[derive(Clone, Copy)]
pub(crate) enum RetiredClaimCommit<'a> {
    Creation(&'a SameWindowThreadCommit),
    Ordinary(&'a RunningThreadActivationCommit),
}

impl RetiredClaimCommit<'_> {
    pub(crate) fn window(&self) -> &beryl_state::SessionWindowRecord {
        match self {
            Self::Creation(commit) => &commit.window,
            Self::Ordinary(commit) => &commit.window,
        }
    }
    pub(crate) fn selection(&self) -> beryl_state::WindowClaimSelection {
        match self {
            Self::Creation(commit) => commit.selection,
            Self::Ordinary(commit) => commit.selection,
        }
    }
    pub(crate) fn validate_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<(), String> {
        match self {
            Self::Creation(commit) => commit
                .validate_candidate(access, state)
                .map_err(|error| error.to_string()),
            Self::Ordinary(commit) => commit
                .validate_candidate(access, state)
                .map_err(|error| error.to_string()),
        }
    }
}

impl RetiredClaimOperation {
    pub(crate) fn exclusion(
        &self,
    ) -> Result<&crate::window_acquisition::RetiredWindowSelectionLease, String> {
        match self {
            Self::Creation(operation) => operation.exclusion.as_ref(),
            Self::Ordinary(operation) => operation.exclusion.as_ref(),
        }
        .ok_or("original selection exclusion is missing".into())
    }
    pub(crate) fn take_saved(
        &mut self,
    ) -> Option<crate::main_window::MainWindowRetiredClaimPredecessorSave> {
        match self {
            Self::Creation(operation) => operation.saved.take(),
            Self::Ordinary(operation) => operation.saved.take(),
        }
    }
    pub(crate) fn committed(&self) -> Option<RetiredClaimCommit<'_>> {
        match self {
            Self::Creation(operation) => operation.committed().map(RetiredClaimCommit::Creation),
            Self::Ordinary(operation) => operation.committed().map(RetiredClaimCommit::Ordinary),
        }
    }
    pub(crate) fn claim_was_admitted(&self) -> bool {
        match self {
            Self::Creation(operation) => operation.claim_was_admitted(),
            Self::Ordinary(operation) => operation.claim_was_admitted(),
        }
    }
    pub(crate) fn retire_selection_exclusion(
        &mut self,
        home: &HomeStore,
        window: beryl_model::WindowId,
    ) -> Result<(), String> {
        match self {
            Self::Creation(operation) => operation.retire_selection_exclusion(home, window),
            Self::Ordinary(operation) => operation.retire_selection_exclusion(home, window),
        }
    }
    pub(crate) fn validate_exclusion(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
    ) -> Result<(), String> {
        match self {
            Self::Creation(operation) => operation
                .exclusion
                .as_ref()
                .ok_or("original selection exclusion is missing")?
                .validate_candidate(access),
            Self::Ordinary(operation) => operation.qualify(access),
        }
    }
    pub(crate) fn settle_claim(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<bool, String> {
        match self {
            Self::Creation(operation) => operation.settle_claim(access, state),
            Self::Ordinary(operation) => operation.settle_claim(access, state),
        }
    }
    pub(crate) fn qualify_prior_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
        prior: crate::main_window::MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        match self {
            Self::Creation(operation) => operation.qualify_prior_candidate(access, state, prior),
            Self::Ordinary(operation) => operation.qualify_prior_candidate(access, state),
        }
    }
}
