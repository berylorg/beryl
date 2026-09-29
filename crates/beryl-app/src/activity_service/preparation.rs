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
            ),
        })
    }

    pub(crate) fn into_service(mut self) -> ActivityService {
        let shared =
            Arc::get_mut(&mut self.service.shared).expect("unshared prepared Activity service");
        shared
            .state
            .get_mut()
            .expect("unobserved Activity state")
            .live = true;
        self.service
    }

    #[cfg(test)]
    pub(crate) fn admit_published(
        self,
        home: &HomeStore,
    ) -> Result<ActivityService, ActivityReadError> {
        let shared = &self.service.shared;
        if home.home_id() != shared.home_id {
            return Err(ActivityReadError::Identity);
        }
        home.try_elect_coherent(shared.generation, || {
            let mut state = shared
                .state
                .lock()
                .map_err(|_| ActivityReadError::Unavailable)?;
            if state.resources.is_none() {
                return Err(ActivityReadError::Unavailable);
            }
            state.live = true;
            Ok(())
        })??;
        Ok(self.service)
    }
}
