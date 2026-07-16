use std::{sync::Arc, time::Duration};

use beryl_model::{CasLoadedSessionGeneration, SyndicThreadId};

use super::{
    LoadedProjectionReleaseError, LoadedProjectionReleaseOutcome, ProjectionConnection,
    registry::{self, LeaseToken, LoadedThreadKey, ReleaseDisposition},
    router::{LiveEventTargetRegistrationError, TargetRegistration},
};
use crate::cas_projection::ProjectionCoordinatorError;

pub(in crate::cas_projection) enum ExistingLease {
    Absent,
    Exact(LoadedProjectionLease),
    AnotherConnection,
    AnotherOwner { existing_owner: SyndicThreadId },
}

pub(in crate::cas_projection) enum ThreadRetirement {
    Absent,
    Retired,
    AnotherConnection,
    AnotherOwner { existing_owner: SyndicThreadId },
}

#[derive(Debug)]
pub(in crate::cas_projection) struct LoadedProjectionLease {
    connection: Arc<ProjectionConnection>,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
    unsubscribe_timeout: Duration,
    active: bool,
}

impl LoadedProjectionLease {
    pub(in crate::cas_projection) fn new(
        connection: Arc<ProjectionConnection>,
        key: LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        token: LeaseToken,
        unsubscribe_timeout: Duration,
    ) -> Self {
        Self {
            connection,
            key,
            owner,
            generation,
            token,
            unsubscribe_timeout,
            active: true,
        }
    }

    pub(in crate::cas_projection) const fn generation(&self) -> CasLoadedSessionGeneration {
        self.generation
    }

    pub(in crate::cas_projection) fn is_live(&self) -> Result<bool, ProjectionCoordinatorError> {
        if !self.active || self.connection.authority.is_retired() {
            return Ok(false);
        }
        registry::contains_exact(
            &self.key,
            self.connection.authority.generation,
            self.owner,
            self.generation,
            self.token,
        )
    }

    pub(in crate::cas_projection) fn register_event_target(
        &self,
        turn_id: Option<beryl_model::CasTurnId>,
    ) -> Result<(Arc<ProjectionConnection>, TargetRegistration), LiveEventTargetRegistrationError>
    {
        if !self.active {
            return Err(LiveEventTargetRegistrationError::ProjectionNotLive);
        }
        let registration = self.connection.register_event_target(
            &self.key,
            self.owner,
            self.generation,
            self.token,
            turn_id,
        )?;
        Ok((Arc::clone(&self.connection), registration))
    }

    pub(in crate::cas_projection) fn release(
        mut self,
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        match self.revoke_local()? {
            ReleaseDisposition::Stale => Ok(LoadedProjectionReleaseOutcome::AlreadyRevoked),
            ReleaseDisposition::Shared => {
                Ok(LoadedProjectionReleaseOutcome::SharedSubscriptionRemains)
            }
            ReleaseDisposition::Last => self
                .connection
                .try_unsubscribe(&self.key.cas_thread_id, self.unsubscribe_timeout),
        }
    }

    fn revoke_local(&mut self) -> Result<ReleaseDisposition, LoadedProjectionReleaseError> {
        if !self.active {
            return Ok(ReleaseDisposition::Stale);
        }
        self.active = false;
        registry::release_exact(
            &self.key,
            self.connection.authority.generation,
            self.owner,
            self.generation,
            self.token,
        )
        .map_err(|error| {
            self.connection.retire();
            LoadedProjectionReleaseError::Registry(error)
        })
    }
}

impl Drop for LoadedProjectionLease {
    fn drop(&mut self) {
        match self.revoke_local() {
            Ok(ReleaseDisposition::Last) | Err(_) => self.connection.retire(),
            Ok(ReleaseDisposition::Stale | ReleaseDisposition::Shared) => {}
        }
    }
}
