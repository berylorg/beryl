use std::{sync::Arc, time::Duration};

use beryl_model::{CasLoadedSessionGeneration, SyndicThreadId};

use super::{
    LoadedProjectionReleaseError, LoadedProjectionReleaseOutcome, ProjectionConnection,
    registry::{self, LeaseToken, LoadedThreadKey, ReleaseDisposition},
    router::{LiveEventTargetRegistrationError, TargetRegistration, TargetTurnRegistration},
};
use crate::cas_projection::ProjectionCoordinatorError;

mod terminal_disposition;

pub(in crate::cas_projection) use terminal_disposition::{
    LocalLoadedRegistryDispositionOwner, TerminalLoadedLeaseDispositionOwner,
};

pub(in crate::cas_projection) enum ExistingLease {
    Absent,
    Exact(LoadedProjectionLease),
    AnotherConnection,
    AnotherOwner { existing_owner: SyndicThreadId },
}

pub(in crate::cas_projection) enum ThreadRetirement {
    Absent,
    Retired {
        generation: CasLoadedSessionGeneration,
        release_error: Option<LoadedProjectionReleaseError>,
    },
    AnotherConnection,
    AnotherOwner {
        existing_owner: SyndicThreadId,
    },
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

/// Unwind owner installed before a loaded-registry mutation publishes a fresh lease token.
pub(super) struct RawLoadedLeaseSeed {
    connection: Option<Arc<ProjectionConnection>>,
    key: Option<LoadedThreadKey>,
    owner: SyndicThreadId,
    unsubscribe_timeout: Duration,
    authority: Option<(CasLoadedSessionGeneration, LeaseToken)>,
}

impl RawLoadedLeaseSeed {
    pub(super) fn pending(
        connection: Arc<ProjectionConnection>,
        key: LoadedThreadKey,
        owner: SyndicThreadId,
        unsubscribe_timeout: Duration,
    ) -> Self {
        Self {
            connection: Some(connection),
            key: Some(key),
            owner,
            unsubscribe_timeout,
            authority: None,
        }
    }

    pub(super) fn arm(&mut self, generation: CasLoadedSessionGeneration, token: LeaseToken) {
        let replaced = self.authority.replace((generation, token));
        debug_assert!(replaced.is_none());
    }

    pub(super) fn into_lease(mut self) -> Option<LoadedProjectionLease> {
        let (generation, token) = self.authority.take()?;
        Some(LoadedProjectionLease::new(
            self.connection
                .take()
                .expect("armed raw lease seed retains its connection"),
            self.key
                .take()
                .expect("armed raw lease seed retains its thread key"),
            self.owner,
            generation,
            token,
            self.unsubscribe_timeout,
        ))
    }
}

impl LoadedProjectionLease {
    pub(in crate::cas_projection) fn bind_context(
        &self,
        previous: Option<beryl_model::BindingRevision>,
        revision: beryl_model::BindingRevision,
        proven_continuity: bool,
    ) {
        if !self.active
            || registry::bind_context(
                &self.key,
                self.connection.authority.generation,
                self.owner,
                self.generation,
                self.token,
                previous,
                revision,
                proven_continuity,
            )
            .is_err()
        {
            self.connection.retire();
        }
    }
    pub(in crate::cas_projection) fn runtime_interest(
        &self,
    ) -> Option<Arc<crate::cas_projection::RuntimeInterest>> {
        self.connection.runtime_interest()
    }

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

    pub(in crate::cas_projection) fn try_with_live_authority<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Option<T> {
        if !self.active || self.connection.authority.is_retired() {
            return None;
        }
        let boundary = &self.connection.authority.work_boundary;
        let observation = boundary.try_observe().ok()?;
        if !self.connection.try_forwarding_attached() {
            return None;
        }
        registry::try_with_exact_lease(
            &self.key,
            self.connection.authority.generation,
            self.owner,
            self.generation,
            self.token,
            || {
                boundary
                    .try_elect(&observation, || {
                        if self.connection.authority.is_retired() {
                            None
                        } else {
                            Some(publish())
                        }
                    })
                    .ok()
                    .flatten()
            },
        )
        .flatten()
    }

    pub(in crate::cas_projection) fn observed_metadata(
        &self,
    ) -> Result<Option<beryl_backend::ThreadSessionMetadata>, ProjectionCoordinatorError> {
        if !self.active || self.connection.authority.is_retired() {
            return Ok(None);
        }
        registry::observed_metadata(
            &self.key,
            self.connection.authority.generation,
            self.owner,
            self.generation,
            self.token,
        )
    }

    pub(in crate::cas_projection) fn admit_ordinary_execution(
        &self,
    ) -> Result<
        crate::cas_projection::acquisition::OrdinaryExecutionCustody,
        ProjectionCoordinatorError,
    > {
        if !self.is_live()? {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        self.connection.admit_ordinary_execution()
    }

    pub(in crate::cas_projection) fn invalidate_observed_metadata(
        &self,
    ) -> Result<(), ProjectionCoordinatorError> {
        if !self.active || self.connection.authority.is_retired() {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        registry::invalidate_metadata(
            &self.key,
            self.connection.authority.generation,
            self.owner,
            self.generation,
            self.token,
        )
    }

    pub(in crate::cas_projection) fn register_event_target(
        &mut self,
        home_generation: u64,
        turn: TargetTurnRegistration,
    ) -> Result<(Arc<ProjectionConnection>, TargetRegistration), LiveEventTargetRegistrationError>
    {
        if !self.active {
            return Err(LiveEventTargetRegistrationError::ProjectionNotLive);
        }
        let registration = self.connection.register_event_target(
            &self.key,
            self.owner,
            self.generation,
            home_generation,
            self.unsubscribe_timeout,
            self.token,
            turn,
        )?;
        Ok((Arc::clone(&self.connection), registration))
    }

    pub(in crate::cas_projection) fn release(
        self,
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        self.release_inner(|| {})
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn release_with_completion_observer(
        self,
        after_cleanup: impl FnOnce(),
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        self.release_inner(after_cleanup)
    }

    fn release_inner(
        mut self,
        after_cleanup: impl FnOnce(),
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        let connection = Arc::clone(&self.connection);
        let cleanup = match connection.acquire_cleanup_owner() {
            Ok(cleanup) => cleanup,
            Err(error) => {
                drop(self);
                return Err(LoadedProjectionReleaseError::Registry(error));
            }
        };
        let Some(cleanup) = cleanup else {
            drop(self);
            return Ok(LoadedProjectionReleaseOutcome::ConnectionRetired);
        };
        let disposition =
            match cleanup.commit_loaded_release(&self.key, self.owner, self.generation, self.token)
            {
                Ok(Some(disposition)) => {
                    self.active = false;
                    disposition
                }
                Ok(None) => {
                    drop(self);
                    return cleanup
                        .finish()
                        .map_err(LoadedProjectionReleaseError::Registry)
                        .map(|()| LoadedProjectionReleaseOutcome::ConnectionRetired);
                }
                Err(error) => {
                    drop(self);
                    connection.request_ordinary_retirement();
                    let cleanup_error = cleanup.finish().err();
                    connection.retire();
                    return Err(LoadedProjectionReleaseError::Registry(
                        cleanup_error.unwrap_or(error),
                    ));
                }
            };
        let outcome = match disposition {
            ReleaseDisposition::Stale => Ok(LoadedProjectionReleaseOutcome::AlreadyRevoked),
            ReleaseDisposition::Shared => {
                Ok(LoadedProjectionReleaseOutcome::SharedSubscriptionRemains)
            }
            ReleaseDisposition::Last => {
                connection.try_unsubscribe(&self.key.cas_thread_id, self.unsubscribe_timeout)
            }
        };
        if outcome.is_err() {
            connection.request_ordinary_retirement();
        }
        let cleanup_result = cleanup
            .finish()
            .map_err(LoadedProjectionReleaseError::Registry);
        after_cleanup();
        let result = outcome.and_then(|outcome| cleanup_result.map(|()| outcome));
        if result.is_err() {
            connection.retire();
        }
        result
    }

    fn settle_implicit_drop(&mut self) {
        if !self.active {
            return;
        }
        settle_raw_loaded_authority(
            &self.connection,
            &self.key,
            self.owner,
            self.generation,
            self.token,
        );
        self.active = false;
    }
}

enum RawAuthoritySettlement<T> {
    Ordinary(T),
    PersistentFailure(T),
    Closed(T),
}

fn settle_raw_loaded_authority(
    connection: &Arc<ProjectionConnection>,
    key: &LoadedThreadKey,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
) {
    let authority = match connection.authority.lock() {
        Ok(authority) => authority,
        Err(_) => {
            connection.request_ordinary_retirement();
            return;
        }
    };
    let settlement = connection.settle_authority(
        || {
            RawAuthoritySettlement::Ordinary(registry::release_exact(
                key,
                connection.authority.generation,
                owner,
                generation,
                token,
            ))
        },
        |_failure_generation| {
            RawAuthoritySettlement::PersistentFailure(registry::release_exact(
                key,
                connection.authority.generation,
                owner,
                generation,
                token,
            ))
        },
        || {
            RawAuthoritySettlement::Closed(registry::release_exact(
                key,
                connection.authority.generation,
                owner,
                generation,
                token,
            ))
        },
    );
    drop(authority);
    if matches!(
        settlement,
        Ok(RawAuthoritySettlement::Ordinary(Ok(
            ReleaseDisposition::Last
        ))) | Ok(RawAuthoritySettlement::Ordinary(Err(_)))
            | Err(_)
    ) {
        connection.request_ordinary_retirement();
    }
}

impl Drop for LoadedProjectionLease {
    fn drop(&mut self) {
        self.settle_implicit_drop();
    }
}

impl Drop for RawLoadedLeaseSeed {
    fn drop(&mut self) {
        let Some((generation, token)) = self.authority.take() else {
            return;
        };
        let connection = self
            .connection
            .as_ref()
            .expect("armed raw lease seed retains its connection");
        let key = self
            .key
            .as_ref()
            .expect("armed raw lease seed retains its thread key");
        settle_raw_loaded_authority(connection, key, self.owner, generation, token);
    }
}
