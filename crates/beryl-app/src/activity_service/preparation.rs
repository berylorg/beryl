use beryl_home_store::{
    HomeCandidateError, HomeCandidateRecoveryAccess, HomeOpenPublication, HomeRecoveryCandidate,
    ReadError,
};

use super::*;

pub(crate) struct PreparedActivityService {
    service: ActivityService,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ActivityPreparationError {
    #[error("Activity candidate access failed: {0}")]
    Candidate(#[from] HomeCandidateError),
    #[error("Activity storage provenance failed: {0}")]
    Storage(#[from] ReadError),
    #[error("Activity runtime source belongs to another candidate")]
    Identity,
}

impl PreparedActivityService {
    #[cfg(test)]
    pub(super) fn service_for_test(&self) -> &ActivityService {
        &self.service
    }

    pub(crate) fn prepare(
        candidate: &mut HomeOpenPublication,
        storage: SyndicStorage,
        runtime: RuntimeActivityReadSource,
        limits: ActivityServiceLimits,
    ) -> Result<Self, ActivityPreparationError> {
        let home = candidate.service_reference();
        Self::prepare_candidate(
            &candidate.recovery_access()?,
            home,
            storage,
            runtime,
            limits,
        )
    }

    pub(crate) fn prepare_recovery(
        candidate: &mut HomeRecoveryCandidate,
        storage: SyndicStorage,
        runtime: RuntimeActivityReadSource,
        limits: ActivityServiceLimits,
    ) -> Result<Self, ActivityPreparationError> {
        let home = candidate.service_reference();
        Self::prepare_candidate(
            &candidate.recovery_access()?,
            home,
            storage,
            runtime,
            limits,
        )
    }

    fn prepare_candidate(
        access: &HomeCandidateRecoveryAccess<'_>,
        home: HomeServiceReference,
        storage: SyndicStorage,
        runtime: RuntimeActivityReadSource,
        limits: ActivityServiceLimits,
    ) -> Result<Self, ActivityPreparationError> {
        if access.home_id() != runtime.home_id() || access.generation() != runtime.home_generation()
        {
            return Err(ActivityPreparationError::Identity);
        }
        storage.revision_candidate(access)?;
        Ok(Self {
            service: ActivityService::dormant(
                Resources {
                    home,
                    storage,
                    runtime,
                },
                limits,
                None,
            ),
        })
    }

    pub(crate) fn into_service(
        mut self,
        home: &HomeStore,
        identity: beryl_home_store::HomeGenerationIdentity,
    ) -> ActivityService {
        let shared =
            Arc::get_mut(&mut self.service.shared).expect("unshared prepared Activity service");
        shared.identity = Some(identity);
        let state = shared.state.get_mut().expect("unobserved Activity state");
        state
            .resources
            .as_mut()
            .expect("prepared Activity resources")
            .home = home.service_reference();
        state.live = true;
        self.service
    }

    #[cfg(test)]
    pub(crate) fn admit_published(
        self,
        home: &HomeStore,
    ) -> Result<ActivityService, ActivityReadError> {
        if home.home_id() != self.service.shared.home_id {
            return Err(ActivityReadError::Identity);
        }
        let identity = home.generation_identity().map_err(SyndicReadError::from)?;
        home.try_elect_coherent(self.service.shared.generation, || {
            self.into_service(home, identity)
        })
        .map_err(ActivityReadError::from)
    }
}
