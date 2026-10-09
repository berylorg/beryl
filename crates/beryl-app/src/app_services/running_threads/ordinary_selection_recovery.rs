use super::*;
use crate::main_window::running_threads::activation::{
    RunningThreadActivation, RunningThreadActivationCommit, RunningThreadActivationOutcome,
};

pub(crate) enum RetiredOrdinaryClaimAdmission {
    NeverAdmitted(RunningThreadActivation),
    Original(RunningThreadActivationOutcome),
}

pub(crate) struct RetiredOrdinarySelectionOperation {
    pub(crate) saved: Option<crate::main_window::MainWindowRetiredClaimPredecessorSave>,
    admission: Option<Box<RetiredOrdinaryClaimAdmission>>,
    lease: Option<Arc<crate::window_acquisition::WindowSelectionLease>>,
    pub(super) exclusion: Option<crate::window_acquisition::RetiredWindowSelectionLease>,
    home: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    path: std::path::PathBuf,
}

impl RetiredOrdinarySelectionOperation {
    pub(crate) fn capture(
        reader: &PublishedRunningThreadsReader,
        lease: Arc<crate::window_acquisition::WindowSelectionLease>,
        admission: RetiredOrdinaryClaimAdmission,
        saved: Option<crate::main_window::MainWindowRetiredClaimPredecessorSave>,
    ) -> Result<
        Self,
        (
            RetiredOrdinaryClaimAdmission,
            Option<crate::main_window::MainWindowRetiredClaimPredecessorSave>,
            String,
        ),
    > {
        let health = reader.home.health();
        if health.state() != beryl_home_store::HomeHealthState::Failed {
            return Err((
                admission,
                saved,
                "ordinary selection retirement requires its original failed Home".into(),
            ));
        }
        let Some(generation) = health.generation() else {
            return Err((
                admission,
                saved,
                "original failed generation is missing".into(),
            ));
        };
        Ok(Self {
            saved,
            admission: Some(Box::new(admission)),
            lease: Some(lease),
            exclusion: None,
            home: reader.home.home_id(),
            generation,
            path: reader.home.canonical_path().to_owned(),
        })
    }

    pub(crate) fn retire_selection_exclusion(
        &mut self,
        home: &beryl_home_store::HomeStore,
        window: beryl_model::WindowId,
    ) -> Result<(), String> {
        if self.exclusion.is_some() {
            return Ok(());
        }
        if home.home_id() != self.home
            || home.canonical_path() != self.path
            || home.health().generation() != Some(self.generation)
        {
            return Err("ordinary selection exclusion source changed".into());
        }
        let lease = self
            .lease
            .take()
            .ok_or("original ordinary selection exclusion is missing")?;
        match lease.retire_failed_home(home, self.generation, window) {
            Ok(exclusion) => {
                self.exclusion = Some(exclusion);
                Ok(())
            }
            Err((lease, error)) => {
                self.lease = Some(lease);
                Err(error)
            }
        }
    }

    pub(super) fn qualify(
        &self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
    ) -> Result<(), String> {
        if access.home_id() != self.home
            || access.canonical_path() != self.path
            || access.generation() == self.generation
        {
            return Err("ordinary selection recovery candidate changed".into());
        }
        self.exclusion
            .as_ref()
            .ok_or("ordinary selection exclusion has not retired")?
            .validate_candidate(access)
    }

    pub(crate) fn claim_was_admitted(&self) -> bool {
        matches!(
            self.admission.as_deref(),
            Some(RetiredOrdinaryClaimAdmission::Original(_))
        )
    }

    pub(crate) fn committed(&self) -> Option<&RunningThreadActivationCommit> {
        match self.admission.as_deref() {
            Some(RetiredOrdinaryClaimAdmission::Original(
                RunningThreadActivationOutcome::Settled(commit),
            )) => Some(commit),
            _ => None,
        }
    }

    pub(crate) fn qualify_prior_candidate(
        &self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<(), String> {
        self.qualify(access)?;
        match self.admission.as_deref() {
            Some(RetiredOrdinaryClaimAdmission::NeverAdmitted(prepared)) => {
                prepared.validate_prior_candidate(access, state)
            }
            Some(RetiredOrdinaryClaimAdmission::Original(
                RunningThreadActivationOutcome::NotCommitted(rejected),
            )) => rejected.validate_prior_candidate(access, state),
            _ => return Err("ordinary selection has no proven original claim".into()),
        }
        .map_err(|error| error.to_string())
    }

    pub(crate) fn settle_claim(
        &mut self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<bool, String> {
        self.qualify(access)?;
        if matches!(
            self.admission.as_deref(),
            Some(RetiredOrdinaryClaimAdmission::Original(
                RunningThreadActivationOutcome::Pending(_)
            ))
        ) {
            let original = self.admission.take().unwrap();
            let RetiredOrdinaryClaimAdmission::Original(RunningThreadActivationOutcome::Pending(
                pending,
            )) = *original
            else {
                unreachable!()
            };
            self.admission = Some(Box::new(RetiredOrdinaryClaimAdmission::Original(
                pending.reconcile_candidate(access, state),
            )));
        }
        if let Some(committed) = self.committed() {
            committed
                .validate_candidate(access, state)
                .map_err(|error| error.to_string())?;
            Ok(true)
        } else {
            self.qualify_prior_candidate(access, state)?;
            Ok(false)
        }
    }
}
