use super::*;
use crate::same_window_thread_acquisition::{SameWindowThreadOutcome, SameWindowThreadUnadmitted};

pub(crate) enum RetiredThreadClaimAdmission {
    NeverPrepared,
    NeverAdmitted(SameWindowThreadUnadmitted),
    Original(SameWindowThreadOutcome),
}

pub(crate) struct RetiredSameWindowThreadOperation {
    pub(crate) saved: Option<crate::main_window::MainWindowRetiredThreadPredecessorSave>,
    pub(crate) admission: RetiredThreadClaimAdmission,
    lease: Option<Arc<crate::window_acquisition::WindowSelectionLease>>,
    pub(crate) exclusion: Option<crate::window_acquisition::RetiredWindowSelectionLease>,
    pub(crate) home: beryl_model::BerylHomeId,
    pub(crate) generation: beryl_home_store::HomeGeneration,
    pub(crate) path: std::path::PathBuf,
}

fn qualify_failed_reader(
    reader: &PublishedRunningThreadsReader,
) -> Result<beryl_home_store::HomeGeneration, String> {
    let health = reader.home.health();
    if health.state() != beryl_home_store::HomeHealthState::Failed {
        return Err("thread creation retirement requires its original failed home".into());
    }
    health
        .generation()
        .ok_or("thread creation failed generation is missing".into())
}

impl PublishedSameWindowThreadOperation {
    pub(crate) fn retire_failed_home(
        mut self,
    ) -> Result<RetiredSameWindowThreadOperation, (Self, String)> {
        let generation = match qualify_failed_reader(&self.reader) {
            Ok(generation) => generation,
            Err(error) => return Err((self, error)),
        };
        let saved = match self.saved.take() {
            Some(saved) => match saved.retire_failed_home() {
                Ok(saved) => Some(saved),
                Err((saved, error)) => {
                    self.saved = Some(saved);
                    return Err((self, error));
                }
            },
            None if matches!(self.outcome, Some(SameWindowThreadOutcome::Settled(_))) => {
                self.adopted_save.take()
            }
            None => {
                return Err((
                    self,
                    "thread creation original save custody is missing".into(),
                ));
            }
        };
        let admission = match self.prepared.take() {
            Some(prepared) => {
                RetiredThreadClaimAdmission::NeverAdmitted(prepared.retire_unadmitted())
            }
            None => RetiredThreadClaimAdmission::Original(
                self.outcome.take().expect("original claim outcome"),
            ),
        };
        Ok(RetiredSameWindowThreadOperation {
            saved,
            admission,
            lease: Some(self.lease),
            exclusion: None,
            home: self.reader.home.home_id(),
            generation,
            path: self.reader.home.canonical_path().to_owned(),
        })
    }
}

impl PublishedSameWindowThreadPreparationFailure {
    pub(crate) fn retire_failed_home(
        mut self,
    ) -> Result<RetiredSameWindowThreadOperation, (Self, String)> {
        let generation = match qualify_failed_reader(&self.reader) {
            Ok(generation) => generation,
            Err(error) => return Err((self, error)),
        };
        let Some(saved) = self.saved.take() else {
            return Err((
                self,
                "thread creation original save custody is missing".into(),
            ));
        };
        let saved = match saved.retire_failed_home() {
            Ok(saved) => saved,
            Err((saved, error)) => {
                self.saved = Some(saved);
                return Err((self, error));
            }
        };
        Ok(RetiredSameWindowThreadOperation {
            saved: Some(saved),
            admission: RetiredThreadClaimAdmission::NeverPrepared,
            lease: Some(self.lease),
            exclusion: None,
            home: self.reader.home.home_id(),
            generation,
            path: self.reader.home.canonical_path().to_owned(),
        })
    }
}

impl PublishedSameWindowThreadCurrent {
    pub(crate) fn retire_failed_home(
        self,
    ) -> Result<RetiredSameWindowThreadOperation, (Self, String)> {
        let Self {
            window,
            claim,
            draft,
            custody,
        } = self;
        custody.retire_failed_home().map_err(|(custody, error)| {
            (
                Self {
                    window,
                    claim,
                    draft,
                    custody,
                },
                error,
            )
        })
    }
}

impl RetiredSameWindowThreadOperation {
    pub(crate) fn claim_was_admitted(&self) -> bool {
        matches!(
            self.admission,
            RetiredThreadClaimAdmission::Original(
                SameWindowThreadOutcome::Settled(_)
                    | SameWindowThreadOutcome::Pending(_)
                    | SameWindowThreadOutcome::Unavailable(_)
            )
        )
    }
    pub(crate) fn qualify_prior_candidate(
        &self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        state: &beryl_state::BerylState,
        prior: crate::main_window::MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        self.exclusion
            .as_ref()
            .ok_or("original selection exclusion has not retired")?
            .validate_candidate(access)?;
        if prior.binding().home_id() != self.home
            || prior.binding().home_generation() != self.generation
        {
            return Err("original prior belongs to another failed Home".into());
        }
        let source = state
            .session()
            .window_claim_catalog_source_candidate(access, prior.window_id())
            .map_err(|error| error.to_string())?;
        let snapshot = state
            .session()
            .minimal_bootstrap_candidate(access)
            .map_err(|error| error.to_string())?
            .ok_or("original noncommit Running session is missing")?;
        if snapshot
            .windows()
            .iter()
            .find(|window| window.window_id() == prior.window_id())
            .is_none_or(|window| window.selected_thread() != Some(prior.claim()))
            || source.claim().is_none_or(|claim| {
                claim.thread_id() != prior.claim().thread_id()
                    || claim.generation() != prior.claim().generation()
                    || claim.revision() != prior.claim().revision()
                    || claim.state() != beryl_state::ThreadClaimState::Active
            })
        {
            return Err("original noncommit prior window and paired claim changed".into());
        }
        Ok(())
    }
    pub(crate) fn before_preparation(
        home: &beryl_home_store::HomeStore,
        generation: beryl_home_store::HomeGeneration,
        lease: Arc<crate::window_acquisition::WindowSelectionLease>,
    ) -> Result<Self, String> {
        if home.health().state() != beryl_home_store::HomeHealthState::Failed
            || home.health().generation() != Some(generation)
        {
            return Err("thread creation source does not match failed capture".into());
        }
        Ok(Self {
            saved: None,
            admission: RetiredThreadClaimAdmission::NeverPrepared,
            lease: Some(lease),
            exclusion: None,
            home: home.home_id(),
            generation,
            path: home.canonical_path().to_owned(),
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
            return Err("thread creation exclusion failed-home transfer changed".into());
        }
        let lease = self
            .lease
            .take()
            .ok_or("thread creation selection exclusion is missing")?;
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
    pub(crate) fn settle_claim(
        &mut self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        state: &beryl_state::BerylState,
    ) -> Result<bool, String> {
        if access.home_id() != self.home
            || access.canonical_path() != self.path
            || access.generation() == self.generation
        {
            return Err("thread creation recovery candidate changed".into());
        }
        self.exclusion
            .as_ref()
            .ok_or("thread creation exclusion has not retired")?
            .validate_candidate(access)?;
        match &self.admission {
            RetiredThreadClaimAdmission::NeverPrepared => return Ok(false),
            RetiredThreadClaimAdmission::NeverAdmitted(prepared) => {
                prepared
                    .qualify_candidate_original(access, state)
                    .map_err(|error| error.to_string())?;
                return Ok(false);
            }
            RetiredThreadClaimAdmission::Original(SameWindowThreadOutcome::Settled(commit)) => {
                commit
                    .validate_candidate(access, state)
                    .map_err(|error| error.to_string())?;
                return Ok(true);
            }
            RetiredThreadClaimAdmission::Original(SameWindowThreadOutcome::NotCommitted(_)) => {
                return Ok(false);
            }
            RetiredThreadClaimAdmission::Original(SameWindowThreadOutcome::Unavailable(_)) => {
                return Err("original thread creation remains unavailable".into());
            }
            RetiredThreadClaimAdmission::Original(SameWindowThreadOutcome::Pending(_)) => {}
        }
        let RetiredThreadClaimAdmission::Original(SameWindowThreadOutcome::Pending(pending)) =
            std::mem::replace(
                &mut self.admission,
                RetiredThreadClaimAdmission::NeverPrepared,
            )
        else {
            unreachable!()
        };
        self.admission =
            RetiredThreadClaimAdmission::Original(pending.reconcile_candidate(access, state));
        match &self.admission {
            RetiredThreadClaimAdmission::Original(SameWindowThreadOutcome::Settled(_)) => Ok(true),
            RetiredThreadClaimAdmission::Original(SameWindowThreadOutcome::NotCommitted(_)) => {
                Ok(false)
            }
            _ => Err("original thread creation settlement retains uncertainty".into()),
        }
    }
    pub(crate) fn committed(
        &self,
    ) -> Option<&crate::same_window_thread_acquisition::SameWindowThreadCommit> {
        match &self.admission {
            RetiredThreadClaimAdmission::Original(SameWindowThreadOutcome::Settled(commit)) => {
                Some(commit)
            }
            _ => None,
        }
    }
}
