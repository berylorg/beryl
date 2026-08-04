use std::{sync::Arc, time::Duration};

use beryl_model::{CasLoadedSessionGeneration, SyndicThreadId};

use super::{
    ConnectionRegistryAuthority, LoadedProjectionReleaseError, LoadedProjectionReleaseOutcome,
    ProjectionConnection,
    registry::{
        self, ConnectionGeneration, LeaseToken, LoadedThreadKey, ReacquisitionAnchorToken,
        ReacquisitionReservationToken, ReleaseDisposition,
    },
    router::{LiveEventTargetRegistrationError, TargetRegistration, TargetTurnRegistration},
};
use crate::cas_projection::{
    ProjectionCoordinatorError,
    service_config::{ProjectionPreactivationSurrender, ProjectionPreactivationSurrenderIssuer},
};

mod recovery;

pub(in crate::cas_projection) use recovery::{
    DormantRecoveredProjectionLeaseOwner, LoadedLeaseRecoveryObservation,
    LocalLoadedRegistryDispositionOwner, PendingProjectionLeaseOwner,
    StableProjectionConnectionAuthentication, StableProjectionConnectionObservation,
};

pub(in crate::cas_projection) enum ExistingLease {
    Absent,
    Exact(LoadedProjectionLease),
    AnotherConnection,
    AnotherOwner { existing_owner: SyndicThreadId },
    Quarantined,
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
    preactivation_surrender: Option<ProjectionPreactivationSurrender>,
    preactivation_issuer: Option<ProjectionPreactivationSurrenderIssuer>,
    active: bool,
}

#[allow(
    dead_code,
    reason = "Phase 78 consumes raw loaded authority retained by the failure cut"
)]
pub(in crate::cas_projection) struct FailureRetainedRawLoadedLease {
    connection: Arc<ProjectionConnection>,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    preactivation_surrender: ProjectionPreactivationSurrender,
}

#[allow(
    dead_code,
    reason = "Phase 78 consumes raw quarantine authority retained by the failure cut"
)]
pub(in crate::cas_projection) struct FailureRetainedRawQuarantinedAnchor {
    connection: Arc<ProjectionConnection>,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: ReacquisitionAnchorToken,
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    preactivation_surrender: ProjectionPreactivationSurrender,
}

#[allow(
    dead_code,
    reason = "Phase 78 consumes raw replacement authority retained by the failure cut"
)]
pub(in crate::cas_projection) struct FailureRetainedRawReacquisitionReservation {
    connection: Arc<ProjectionConnection>,
    key: LoadedThreadKey,
    anchor_connection: ConnectionGeneration,
    owner: SyndicThreadId,
    anchor_generation: CasLoadedSessionGeneration,
    anchor_token: ReacquisitionAnchorToken,
    token: ReacquisitionReservationToken,
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    preactivation_surrender: Option<ProjectionPreactivationSurrender>,
}

/// Unwind owner installed before a loaded-registry mutation can publish a fresh lease token.
///
/// The seed lives outside the connection and master-gate guards. A successful registry commit arms
/// it while both guards are held; any later unwind therefore drops the guards first and then
/// settles the exact raw authority on the winning side of the persistent-failure cut.
pub(super) struct RawLoadedLeaseSeed {
    connection: Option<Arc<ProjectionConnection>>,
    key: Option<LoadedThreadKey>,
    owner: SyndicThreadId,
    unsubscribe_timeout: Duration,
    preactivation_surrender: Option<ProjectionPreactivationSurrender>,
    authority: Option<(CasLoadedSessionGeneration, LeaseToken)>,
}

enum RawQuarantineAuthority {
    Loaded(LeaseToken),
    Quarantined(ReacquisitionAnchorToken),
}

pub(super) struct RawQuarantinedAnchorSeed {
    connection: Option<Arc<ProjectionConnection>>,
    key: Option<LoadedThreadKey>,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    unsubscribe_timeout: Duration,
    preactivation_surrender: Option<ProjectionPreactivationSurrender>,
    authority: Option<RawQuarantineAuthority>,
}

pub(super) struct RawReacquisitionReservationSeed {
    connection: Option<Arc<ProjectionConnection>>,
    key: Option<LoadedThreadKey>,
    anchor_connection: ConnectionGeneration,
    owner: SyndicThreadId,
    anchor_generation: CasLoadedSessionGeneration,
    anchor_token: ReacquisitionAnchorToken,
    preactivation_surrender: Option<ProjectionPreactivationSurrender>,
    token: Option<ReacquisitionReservationToken>,
}

#[cfg(test)]
fn recovery_owner_settlement_observer()
-> &'static std::sync::Mutex<Option<std::sync::mpsc::SyncSender<()>>> {
    static OBSERVER: std::sync::OnceLock<
        std::sync::Mutex<Option<std::sync::mpsc::SyncSender<()>>>,
    > = std::sync::OnceLock::new();
    OBSERVER.get_or_init(|| std::sync::Mutex::new(None))
}

#[cfg(test)]
fn publish_recovery_owner_settlement_for_test() {
    if let Some(observer) = recovery_owner_settlement_observer()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .take()
    {
        let _ = observer.send(());
    }
}

impl FailureRetainedRawLoadedLease {
    pub(in crate::cas_projection) const fn identity(
        &self,
    ) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
        self.identity
    }
}

impl FailureRetainedRawQuarantinedAnchor {
    pub(in crate::cas_projection) const fn identity(
        &self,
    ) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
        self.identity
    }
}

impl FailureRetainedRawReacquisitionReservation {
    pub(in crate::cas_projection) const fn identity(
        &self,
    ) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
        self.identity
    }
}

impl RawLoadedLeaseSeed {
    pub(super) fn pending(
        connection: Arc<ProjectionConnection>,
        key: LoadedThreadKey,
        owner: SyndicThreadId,
        unsubscribe_timeout: Duration,
        preactivation_surrender: Option<ProjectionPreactivationSurrender>,
    ) -> Self {
        Self {
            connection: Some(connection),
            key: Some(key),
            owner,
            unsubscribe_timeout,
            preactivation_surrender,
            authority: None,
        }
    }

    pub(super) fn arm(&mut self, generation: CasLoadedSessionGeneration, token: LeaseToken) {
        let replaced = self.authority.replace((generation, token));
        debug_assert!(replaced.is_none());
    }

    pub(super) fn install_preactivation_surrender(
        &mut self,
        surrender: Option<ProjectionPreactivationSurrender>,
    ) {
        let replaced = std::mem::replace(&mut self.preactivation_surrender, surrender);
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
            self.preactivation_surrender.take(),
        ))
    }
}

impl RawQuarantinedAnchorSeed {
    fn take_loaded(lease: &mut LoadedProjectionLease) -> Self {
        let connection = Arc::clone(&lease.connection);
        let key = lease.key.clone();
        lease.active = false;
        Self {
            connection: Some(connection),
            key: Some(key),
            owner: lease.owner,
            generation: lease.generation,
            unsubscribe_timeout: lease.unsubscribe_timeout,
            preactivation_surrender: lease.preactivation_surrender.take(),
            authority: Some(RawQuarantineAuthority::Loaded(lease.token)),
        }
    }

    pub(super) fn arm_quarantined(&mut self, token: ReacquisitionAnchorToken) {
        let replaced = self
            .authority
            .replace(RawQuarantineAuthority::Quarantined(token));
        debug_assert!(matches!(replaced, Some(RawQuarantineAuthority::Loaded(_))));
    }

    fn into_anchor(mut self) -> Option<QuarantinedProjectionAnchor> {
        let Some(RawQuarantineAuthority::Quarantined(token)) = self.authority.take() else {
            return None;
        };
        Some(QuarantinedProjectionAnchor {
            connection: self
                .connection
                .take()
                .expect("armed quarantine seed retains its connection"),
            key: self
                .key
                .take()
                .expect("armed quarantine seed retains its thread key"),
            owner: self.owner,
            generation: self.generation,
            token,
            unsubscribe_timeout: self.unsubscribe_timeout,
            preactivation_surrender: self.preactivation_surrender.take(),
            state: QuarantinedAnchorState::Anchored,
            transfer_cleanup: None,
        })
    }
}

impl RawReacquisitionReservationSeed {
    #[allow(clippy::too_many_arguments)]
    fn pending(
        connection: Arc<ProjectionConnection>,
        key: LoadedThreadKey,
        anchor_connection: ConnectionGeneration,
        owner: SyndicThreadId,
        anchor_generation: CasLoadedSessionGeneration,
        anchor_token: ReacquisitionAnchorToken,
        preactivation_surrender: Option<ProjectionPreactivationSurrender>,
    ) -> Self {
        Self {
            connection: Some(connection),
            key: Some(key),
            anchor_connection,
            owner,
            anchor_generation,
            anchor_token,
            preactivation_surrender,
            token: None,
        }
    }

    pub(super) fn arm(&mut self, token: ReacquisitionReservationToken) {
        let replaced = self.token.replace(token);
        debug_assert!(replaced.is_none());
    }

    fn into_reservation(
        mut self,
        moves_source_surrender: bool,
    ) -> Option<ReacquisitionResumeReservation> {
        let token = self.token.take()?;
        Some(ReacquisitionResumeReservation {
            connection: self
                .connection
                .take()
                .expect("armed reacquisition seed retains its connection"),
            key: self
                .key
                .take()
                .expect("armed reacquisition seed retains its thread key"),
            anchor_connection: self.anchor_connection,
            owner: self.owner,
            anchor_generation: self.anchor_generation,
            anchor_token: self.anchor_token,
            token,
            preactivation_surrender: self.preactivation_surrender.take(),
            moves_source_surrender,
            state: ReacquisitionReservationState::Reserved,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuarantinedAnchorState {
    Anchored,
    TransferCleanupPending,
    Released,
}

#[derive(Debug)]
pub(in crate::cas_projection) struct QuarantinedProjectionAnchor {
    connection: Arc<ProjectionConnection>,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: ReacquisitionAnchorToken,
    unsubscribe_timeout: Duration,
    preactivation_surrender: Option<ProjectionPreactivationSurrender>,
    state: QuarantinedAnchorState,
    transfer_cleanup: Option<super::ConnectionCleanupOwner>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReacquisitionReservationState {
    Reserved,
    Transferred,
}

#[derive(Debug)]
pub(in crate::cas_projection) struct ReacquisitionResumeReservation {
    connection: Arc<ProjectionConnection>,
    key: LoadedThreadKey,
    anchor_connection: ConnectionGeneration,
    owner: SyndicThreadId,
    anchor_generation: CasLoadedSessionGeneration,
    anchor_token: ReacquisitionAnchorToken,
    token: ReacquisitionReservationToken,
    preactivation_surrender: Option<ProjectionPreactivationSurrender>,
    moves_source_surrender: bool,
    state: ReacquisitionReservationState,
}

impl LoadedProjectionLease {
    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_recovery_owner_settlement_for_test(
        observer: std::sync::mpsc::SyncSender<()>,
    ) {
        *recovery_owner_settlement_observer()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(observer);
    }

    pub(in crate::cas_projection) fn new(
        connection: Arc<ProjectionConnection>,
        key: LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        token: LeaseToken,
        unsubscribe_timeout: Duration,
        preactivation_surrender: Option<ProjectionPreactivationSurrender>,
    ) -> Self {
        let preactivation_issuer = preactivation_surrender
            .as_ref()
            .map(ProjectionPreactivationSurrender::issuer);
        Self {
            connection,
            key,
            owner,
            generation,
            token,
            unsubscribe_timeout,
            preactivation_surrender,
            preactivation_issuer,
            active: true,
        }
    }

    pub(in crate::cas_projection) const fn generation(&self) -> CasLoadedSessionGeneration {
        self.generation
    }

    pub(in crate::cas_projection) fn settle_recovery_owner_drop(
        self,
        retain: impl FnOnce(
            crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer,
            LoadedProjectionLease,
        ),
    ) {
        if !self.active || self.preactivation_surrender.is_none() {
            drop(self);
            return;
        }
        let connection = Arc::clone(&self.connection);
        let key = self.key.clone();
        let owner = self.owner;
        let generation = self.generation;
        let token = self.token;
        let retainer = self
            .preactivation_surrender
            .as_ref()
            .expect("recovery-eligible lease retains its surrender child")
            .retainer();
        #[cfg(test)]
        publish_recovery_owner_settlement_for_test();
        let authority = match connection.authority.lock() {
            Ok(authority) => authority,
            Err(_) => {
                drop(self);
                connection.request_ordinary_retirement();
                return;
            }
        };
        let lease = std::cell::RefCell::new(Some(self));
        let retain = std::cell::RefCell::new(Some(retain));
        let settlement = connection.settle_authority(
            || {
                let result = registry::release_exact(
                    &key,
                    connection.authority.generation,
                    owner,
                    generation,
                    token,
                );
                let mut lease = lease
                    .borrow_mut()
                    .take()
                    .expect("whole loaded authority settles exactly once");
                lease.active = false;
                drop(lease);
                RawAuthoritySettlement::Ordinary(result)
            },
            |_failure_generation| {
                let lease = lease
                    .borrow_mut()
                    .take()
                    .expect("whole loaded authority transfers exactly once");
                retain
                    .borrow_mut()
                    .take()
                    .expect("whole loaded authority retains one transfer")(
                    retainer.clone(), lease
                );
                RawAuthoritySettlement::PersistentFailure(Ok(ReleaseDisposition::Stale))
            },
            || {
                let result = registry::release_exact(
                    &key,
                    connection.authority.generation,
                    owner,
                    generation,
                    token,
                );
                let mut lease = lease
                    .borrow_mut()
                    .take()
                    .expect("closed loaded authority settles exactly once");
                lease.active = false;
                drop(lease);
                RawAuthoritySettlement::Closed(result)
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
        drop(self.preactivation_surrender.take());
        Ok((Arc::clone(&self.connection), registration))
    }

    pub(in crate::cas_projection) fn prepare_preactivation_surrender(
        &self,
    ) -> Result<Option<ProjectionPreactivationSurrender>, ProjectionCoordinatorError> {
        self.preactivation_issuer
            .as_ref()
            .map(ProjectionPreactivationSurrenderIssuer::try_mint)
            .transpose()
    }

    pub(in crate::cas_projection) fn install_preactivation_surrender(
        &mut self,
        surrender: Option<ProjectionPreactivationSurrender>,
    ) {
        debug_assert!(self.preactivation_surrender.is_none());
        self.preactivation_surrender = surrender;
    }

    pub(in crate::cas_projection) fn has_preactivation_surrender(&self) -> bool {
        self.preactivation_surrender.is_some()
    }

    pub(in crate::cas_projection) fn quarantine_for_reacquisition(
        mut self,
    ) -> Result<QuarantinedProjectionAnchor, LoadedProjectionReleaseError> {
        if !self.active {
            return Err(LoadedProjectionReleaseError::Registry(
                ProjectionCoordinatorError::ProjectionLeaseNotLive {
                    thread_id: self.key.cas_thread_id.clone(),
                },
            ));
        }
        let command = self
            .connection
            .authorize_command()
            .map_err(|_| LoadedProjectionReleaseError::Registry(self.connection.unavailable()))?;
        let mut seed = RawQuarantinedAnchorSeed::take_loaded(&mut self);
        let committed = self
            .connection
            .authority
            .quarantine_exact(
                &self.key,
                self.owner,
                self.generation,
                self.token,
                &command,
                &mut seed,
            )
            .map_err(LoadedProjectionReleaseError::Registry)?;
        let Some(()) = committed else {
            return Err(LoadedProjectionReleaseError::Registry(
                ProjectionCoordinatorError::ProjectionLeaseNotLive {
                    thread_id: self.key.cas_thread_id.clone(),
                },
            ));
        };
        seed.into_anchor().ok_or_else(|| {
            LoadedProjectionReleaseError::Registry(
                ProjectionCoordinatorError::ProjectionLeaseNotLive {
                    thread_id: self.key.cas_thread_id.clone(),
                },
            )
        })
    }

    pub(in crate::cas_projection) fn release(
        self,
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        self.release_inner(
            None::<
                fn(
                    crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer,
                    LoadedProjectionLease,
                ),
            >,
        )
    }

    pub(in crate::cas_projection) fn release_recovery_owner(
        self,
        retain: impl FnOnce(
            crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer,
            LoadedProjectionLease,
        ),
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        self.release_inner(Some(retain))
    }

    fn release_inner<F>(
        mut self,
        mut retain: Option<F>,
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError>
    where
        F: FnOnce(
            crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer,
            LoadedProjectionLease,
        ),
    {
        let connection = Arc::clone(&self.connection);
        let cleanup = match self.connection.acquire_cleanup_owner() {
            Ok(cleanup) => cleanup,
            Err(error) => {
                settle_uncommitted_loaded_release(self, retain.take());
                return Err(LoadedProjectionReleaseError::Registry(error));
            }
        };
        let Some(cleanup) = cleanup else {
            settle_uncommitted_loaded_release(self, retain.take());
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
                    settle_uncommitted_loaded_release(self, retain.take());
                    return cleanup
                        .finish()
                        .map_err(LoadedProjectionReleaseError::Registry)
                        .map(|()| LoadedProjectionReleaseOutcome::ConnectionRetired);
                }
                Err(error) => {
                    settle_uncommitted_loaded_release(self, retain.take());
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
            ReleaseDisposition::Last => self
                .connection
                .try_unsubscribe(&self.key.cas_thread_id, self.unsubscribe_timeout),
        };
        let cleanup_result = cleanup
            .finish()
            .map_err(LoadedProjectionReleaseError::Registry);
        outcome.and_then(|outcome| cleanup_result.map(|()| outcome))
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
            &mut self.preactivation_surrender,
        );
        self.active = false;
    }
}

fn settle_uncommitted_loaded_release<F>(lease: LoadedProjectionLease, retain: Option<F>)
where
    F: FnOnce(
        crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer,
        LoadedProjectionLease,
    ),
{
    if let Some(retain) = retain {
        lease.settle_recovery_owner_drop(retain);
    } else {
        drop(lease);
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
    preactivation_surrender: &mut Option<ProjectionPreactivationSurrender>,
) {
    let authority = match connection.authority.lock() {
        Ok(authority) => authority,
        Err(_) => {
            preactivation_surrender.take();
            connection.request_ordinary_retirement();
            return;
        }
    };
    let surrender = std::cell::RefCell::new(preactivation_surrender.take());
    let retainer = surrender
        .borrow()
        .as_ref()
        .map(ProjectionPreactivationSurrender::retainer);
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
        |failure_generation| {
            let Some(retainer) = retainer.as_ref() else {
                return RawAuthoritySettlement::PersistentFailure(registry::release_exact(
                    key,
                    connection.authority.generation,
                    owner,
                    generation,
                    token,
                ));
            };
            let preactivation_surrender = surrender
                .borrow_mut()
                .take()
                .expect("credentialed raw lease retains its surrender child");
            retainer.retain_raw_loaded_lease(FailureRetainedRawLoadedLease {
                connection: Arc::clone(connection),
                key: key.clone(),
                owner,
                generation,
                token,
                identity: retainer.cut_identity(failure_generation),
                preactivation_surrender,
            });
            RawAuthoritySettlement::PersistentFailure(Ok(ReleaseDisposition::Stale))
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
    surrender.borrow_mut().take();
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

#[allow(clippy::too_many_arguments)]
fn settle_raw_quarantined_authority(
    connection: &Arc<ProjectionConnection>,
    key: &LoadedThreadKey,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: ReacquisitionAnchorToken,
    preactivation_surrender: &mut Option<ProjectionPreactivationSurrender>,
) {
    let authority = match connection.authority.lock() {
        Ok(authority) => authority,
        Err(_) => {
            preactivation_surrender.take();
            connection.request_ordinary_retirement();
            return;
        }
    };
    let surrender = std::cell::RefCell::new(preactivation_surrender.take());
    let retainer = surrender
        .borrow()
        .as_ref()
        .map(ProjectionPreactivationSurrender::retainer);
    let settlement = connection.settle_authority(
        || {
            RawAuthoritySettlement::Ordinary(registry::abandon_quarantined(
                key,
                connection.authority.generation,
                owner,
                generation,
                token,
            ))
        },
        |failure_generation| {
            let Some(retainer) = retainer.as_ref() else {
                return RawAuthoritySettlement::PersistentFailure(registry::abandon_quarantined(
                    key,
                    connection.authority.generation,
                    owner,
                    generation,
                    token,
                ));
            };
            let preactivation_surrender = surrender
                .borrow_mut()
                .take()
                .expect("credentialed quarantine anchor retains its surrender child");
            retainer.retain_raw_quarantined_anchor(FailureRetainedRawQuarantinedAnchor {
                connection: Arc::clone(connection),
                key: key.clone(),
                owner,
                generation,
                token,
                identity: retainer.cut_identity(failure_generation),
                preactivation_surrender,
            });
            RawAuthoritySettlement::PersistentFailure(Ok(false))
        },
        || {
            RawAuthoritySettlement::Closed(registry::abandon_quarantined(
                key,
                connection.authority.generation,
                owner,
                generation,
                token,
            ))
        },
    );
    drop(authority);
    surrender.borrow_mut().take();
    if matches!(
        settlement,
        Ok(RawAuthoritySettlement::Ordinary(Ok(true)))
            | Ok(RawAuthoritySettlement::Ordinary(Err(_)))
            | Err(_)
    ) {
        connection.request_ordinary_retirement();
    }
}

#[allow(clippy::too_many_arguments)]
fn settle_raw_reacquisition_reservation(
    connection: &Arc<ProjectionConnection>,
    key: &LoadedThreadKey,
    anchor_connection: ConnectionGeneration,
    owner: SyndicThreadId,
    anchor_generation: CasLoadedSessionGeneration,
    anchor_token: ReacquisitionAnchorToken,
    token: ReacquisitionReservationToken,
    preactivation_surrender: &mut Option<ProjectionPreactivationSurrender>,
) {
    let retainer = connection
        .current_router()
        .and_then(|router| router.projection_retainer())
        .ok();
    let authority = match connection.authority.lock() {
        Ok(authority) => authority,
        Err(_) => {
            preactivation_surrender.take();
            connection.request_ordinary_retirement();
            return;
        }
    };
    let surrender = std::cell::RefCell::new(preactivation_surrender.take());
    let settlement = connection.settle_authority(
        || {
            RawAuthoritySettlement::Ordinary(registry::abandon_reacquisition_reservation(
                key,
                anchor_connection,
                connection.authority.generation,
                owner,
                anchor_generation,
                anchor_token,
                token,
            ))
        },
        |failure_generation| {
            let Some(retainer) = retainer.as_ref() else {
                return RawAuthoritySettlement::PersistentFailure(
                    registry::abandon_reacquisition_reservation(
                        key,
                        anchor_connection,
                        connection.authority.generation,
                        owner,
                        anchor_generation,
                        anchor_token,
                        token,
                    ),
                );
            };
            retainer.retain_raw_reacquisition_reservation(
                FailureRetainedRawReacquisitionReservation {
                    connection: Arc::clone(connection),
                    key: key.clone(),
                    anchor_connection,
                    owner,
                    anchor_generation,
                    anchor_token,
                    token,
                    identity: retainer.cut_identity(failure_generation),
                    preactivation_surrender: surrender.borrow_mut().take(),
                },
            );
            RawAuthoritySettlement::PersistentFailure(Ok(false))
        },
        || {
            RawAuthoritySettlement::Closed(registry::abandon_reacquisition_reservation(
                key,
                anchor_connection,
                connection.authority.generation,
                owner,
                anchor_generation,
                anchor_token,
                token,
            ))
        },
    );
    drop(authority);
    surrender.borrow_mut().take();
    if matches!(
        settlement,
        Ok(RawAuthoritySettlement::Ordinary(Ok(true)))
            | Ok(RawAuthoritySettlement::Ordinary(Err(_)))
            | Err(_)
    ) {
        connection.request_ordinary_retirement();
    }
}

impl QuarantinedProjectionAnchor {
    pub(in crate::cas_projection) const fn generation(&self) -> CasLoadedSessionGeneration {
        self.generation
    }

    pub(in crate::cas_projection) const fn unsubscribe_timeout(&self) -> Duration {
        self.unsubscribe_timeout
    }

    pub(in crate::cas_projection) fn settle_recovery_owner_drop(
        self,
        retain: impl FnOnce(
            crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer,
            QuarantinedProjectionAnchor,
        ),
    ) {
        if self.state != QuarantinedAnchorState::Anchored || self.preactivation_surrender.is_none()
        {
            drop(self);
            return;
        }
        let connection = Arc::clone(&self.connection);
        let key = self.key.clone();
        let owner = self.owner;
        let generation = self.generation;
        let token = self.token;
        let retainer = self
            .preactivation_surrender
            .as_ref()
            .expect("recovery-eligible anchor retains its surrender child")
            .retainer();
        #[cfg(test)]
        publish_recovery_owner_settlement_for_test();
        let authority = match connection.authority.lock() {
            Ok(authority) => authority,
            Err(_) => {
                drop(self);
                connection.request_ordinary_retirement();
                return;
            }
        };
        let anchor = std::cell::RefCell::new(Some(self));
        let retain = std::cell::RefCell::new(Some(retain));
        let settlement = connection.settle_authority(
            || {
                let result = registry::abandon_quarantined(
                    &key,
                    connection.authority.generation,
                    owner,
                    generation,
                    token,
                );
                let mut anchor = anchor
                    .borrow_mut()
                    .take()
                    .expect("whole quarantine authority settles exactly once");
                anchor.state = QuarantinedAnchorState::Released;
                drop(anchor);
                RawAuthoritySettlement::Ordinary(result)
            },
            |_failure_generation| {
                let anchor = anchor
                    .borrow_mut()
                    .take()
                    .expect("whole quarantine authority transfers exactly once");
                retain
                    .borrow_mut()
                    .take()
                    .expect("whole quarantine authority retains one transfer")(
                    retainer.clone(),
                    anchor,
                );
                RawAuthoritySettlement::PersistentFailure(Ok(false))
            },
            || {
                let result = registry::abandon_quarantined(
                    &key,
                    connection.authority.generation,
                    owner,
                    generation,
                    token,
                );
                let mut anchor = anchor
                    .borrow_mut()
                    .take()
                    .expect("closed quarantine authority settles exactly once");
                anchor.state = QuarantinedAnchorState::Released;
                drop(anchor);
                RawAuthoritySettlement::Closed(result)
            },
        );
        drop(authority);
        if matches!(
            settlement,
            Ok(RawAuthoritySettlement::Ordinary(Ok(true)))
                | Ok(RawAuthoritySettlement::Ordinary(Err(_)))
                | Err(_)
        ) {
            connection.request_ordinary_retirement();
        }
    }

    pub(in crate::cas_projection) fn has_preactivation_surrender(&self) -> bool {
        self.preactivation_surrender.is_some()
    }

    pub(in crate::cas_projection) fn is_live(&self) -> Result<bool, ProjectionCoordinatorError> {
        if self.state != QuarantinedAnchorState::Anchored || self.connection.authority.is_retired()
        {
            return Ok(false);
        }
        registry::contains_quarantined(
            &self.key,
            self.connection.authority.generation,
            self.owner,
            self.generation,
            self.token,
        )
    }

    pub(in crate::cas_projection) fn owns_connection(
        &self,
        connection: &Arc<ProjectionConnection>,
    ) -> bool {
        Arc::ptr_eq(&self.connection, connection)
    }

    pub(in crate::cas_projection) fn reserve_replacement(
        &self,
        connection: &Arc<ProjectionConnection>,
        preactivation_issuer: Option<&ProjectionPreactivationSurrenderIssuer>,
    ) -> Result<ReacquisitionResumeReservation, ProjectionCoordinatorError> {
        if self.state != QuarantinedAnchorState::Anchored
            || self.owns_connection(connection)
            || self.connection.authority.is_retired()
            || connection.authority.is_retired()
        {
            return Err(ProjectionCoordinatorError::ProjectionLeaseNotLive {
                thread_id: self.key.cas_thread_id.clone(),
            });
        }
        let target_service_generation = connection
            .current_router()?
            .projection_retainer()?
            .service_generation();
        let moves_source_surrender =
            self.preactivation_surrender
                .as_ref()
                .is_some_and(|surrender| {
                    surrender.retainer().service_generation() == target_service_generation
                });
        let preactivation_surrender = if moves_source_surrender {
            None
        } else {
            preactivation_issuer
                .map(ProjectionPreactivationSurrenderIssuer::try_mint)
                .transpose()?
        };
        if self.preactivation_surrender.is_some()
            && !moves_source_surrender
            && preactivation_surrender.is_none()
        {
            return Err(ProjectionCoordinatorError::PreactivationProjectionAdmissionUnavailable);
        }
        let old_command = self
            .connection
            .authorize_command()
            .map_err(|_| self.connection.unavailable())?;
        let new_command = connection
            .authorize_command()
            .map_err(|_| connection.unavailable())?;
        let old_router = self.connection.current_router()?;
        let new_router = connection.current_router()?;
        let mut seed = RawReacquisitionReservationSeed::pending(
            Arc::clone(connection),
            self.key.clone(),
            self.connection.authority.generation,
            self.owner,
            self.generation,
            self.token,
            preactivation_surrender,
        );
        let reservation = ConnectionRegistryAuthority::reserve_reacquisition(
            &self.connection.authority,
            &old_router,
            &old_command,
            &connection.authority,
            &new_router,
            &new_command,
            &self.key,
            self.owner,
            self.generation,
            self.token,
            &mut seed,
        );
        let Some(()) = (match reservation {
            Ok(committed) => committed,
            Err(error) => {
                connection.retire();
                return Err(error);
            }
        }) else {
            connection.retire();
            return Err(ProjectionCoordinatorError::ProjectionLeaseNotLive {
                thread_id: self.key.cas_thread_id.clone(),
            });
        };
        seed.into_reservation(moves_source_surrender)
            .ok_or_else(|| ProjectionCoordinatorError::ProjectionLeaseNotLive {
                thread_id: self.key.cas_thread_id.clone(),
            })
    }

    pub(in crate::cas_projection) fn transfer_to(
        &mut self,
        mut reservation: ReacquisitionResumeReservation,
    ) -> Result<LoadedProjectionLease, ProjectionCoordinatorError> {
        let connection = Arc::clone(&reservation.connection);
        if self.state != QuarantinedAnchorState::Anchored
            || reservation.state != ReacquisitionReservationState::Reserved
            || self.owns_connection(&connection)
            || reservation.key != self.key
            || reservation.anchor_connection != self.connection.authority.generation
            || reservation.owner != self.owner
            || reservation.anchor_generation != self.generation
            || reservation.anchor_token != self.token
            || self.connection.authority.is_retired()
        {
            return Err(ProjectionCoordinatorError::ProjectionLeaseNotLive {
                thread_id: self.key.cas_thread_id.clone(),
            });
        }
        let Some(transfer_cleanup) = self.connection.acquire_cleanup_owner()? else {
            return Err(ProjectionCoordinatorError::ProjectionLeaseNotLive {
                thread_id: self.key.cas_thread_id.clone(),
            });
        };
        let mut transfer_cleanup = Some(transfer_cleanup);
        let old_command = self
            .connection
            .authorize_command()
            .map_err(|_| self.connection.unavailable())?;
        let new_command = connection
            .authorize_command()
            .map_err(|_| connection.unavailable())?;
        let old_router = self.connection.current_router()?;
        let new_router = connection.current_router()?;
        let mut seed = RawLoadedLeaseSeed::pending(
            Arc::clone(&connection),
            self.key.clone(),
            self.owner,
            self.unsubscribe_timeout,
            None,
        );
        let transfer = ConnectionRegistryAuthority::transfer_quarantined(
            &self.connection.authority,
            &old_router,
            &old_command,
            &connection.authority,
            &new_router,
            &new_command,
            &self.key,
            self.owner,
            self.generation,
            self.token,
            reservation.token,
            &mut seed,
            |seed| {
                let preactivation_surrender = if reservation.moves_source_surrender {
                    self.preactivation_surrender.take()
                } else {
                    drop(self.preactivation_surrender.take());
                    reservation.preactivation_surrender.take()
                };
                seed.install_preactivation_surrender(preactivation_surrender);
                reservation.state = ReacquisitionReservationState::Transferred;
                self.state = QuarantinedAnchorState::TransferCleanupPending;
                self.transfer_cleanup = transfer_cleanup.take();
            },
        );
        let committed = match transfer {
            Ok(committed) => committed,
            Err(error) => {
                reservation.settle_implicit_drop();
                return Err(error);
            }
        };
        let Some(()) = committed else {
            reservation.settle_implicit_drop();
            return Err(ProjectionCoordinatorError::ProjectionLeaseNotLive {
                thread_id: self.key.cas_thread_id.clone(),
            });
        };
        seed.into_lease()
            .ok_or_else(|| ProjectionCoordinatorError::ProjectionLeaseNotLive {
                thread_id: self.key.cas_thread_id.clone(),
            })
    }

    pub(in crate::cas_projection) fn cleanup_after_transfer(
        mut self,
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        if self.state != QuarantinedAnchorState::TransferCleanupPending {
            return Ok(LoadedProjectionReleaseOutcome::AlreadyRevoked);
        }
        let cleanup = self
            .connection
            .try_unsubscribe(&self.key.cas_thread_id, self.unsubscribe_timeout);
        let owner = self
            .transfer_cleanup
            .take()
            .expect("transferred anchor retains its cleanup owner")
            .finish();
        self.state = QuarantinedAnchorState::Released;
        match (cleanup, owner) {
            (Err(error), _) => {
                self.connection.retire();
                Err(error)
            }
            (Ok(_), Err(error)) => {
                self.connection.retire();
                Err(LoadedProjectionReleaseError::Registry(error))
            }
            (Ok(outcome), Ok(())) => Ok(outcome),
        }
    }
}

impl ReacquisitionResumeReservation {
    pub(in crate::cas_projection) fn is_live(&self) -> Result<bool, ProjectionCoordinatorError> {
        if self.state != ReacquisitionReservationState::Reserved
            || self.connection.authority.is_retired()
        {
            return Ok(false);
        }
        registry::contains_reacquisition_reservation(
            &self.key,
            self.anchor_connection,
            self.connection.authority.generation,
            self.owner,
            self.anchor_generation,
            self.anchor_token,
            self.token,
        )
    }

    fn settle_implicit_drop(&mut self) {
        if self.state != ReacquisitionReservationState::Reserved {
            return;
        }
        self.state = ReacquisitionReservationState::Transferred;
        settle_raw_reacquisition_reservation(
            &self.connection,
            &self.key,
            self.anchor_connection,
            self.owner,
            self.anchor_generation,
            self.anchor_token,
            self.token,
            &mut self.preactivation_surrender,
        );
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
        let (Some(connection), Some(key)) = (self.connection.as_ref(), self.key.as_ref()) else {
            return;
        };
        settle_raw_loaded_authority(
            connection,
            key,
            self.owner,
            generation,
            token,
            &mut self.preactivation_surrender,
        );
    }
}

impl Drop for RawQuarantinedAnchorSeed {
    fn drop(&mut self) {
        let Some(authority) = self.authority.take() else {
            return;
        };
        let (Some(connection), Some(key)) = (self.connection.as_ref(), self.key.as_ref()) else {
            return;
        };
        match authority {
            RawQuarantineAuthority::Loaded(token) => settle_raw_loaded_authority(
                connection,
                key,
                self.owner,
                self.generation,
                token,
                &mut self.preactivation_surrender,
            ),
            RawQuarantineAuthority::Quarantined(token) => {
                settle_raw_quarantined_authority(
                    connection,
                    key,
                    self.owner,
                    self.generation,
                    token,
                    &mut self.preactivation_surrender,
                );
            }
        }
    }
}

impl Drop for RawReacquisitionReservationSeed {
    fn drop(&mut self) {
        let Some(token) = self.token.take() else {
            return;
        };
        let (Some(connection), Some(key)) = (self.connection.as_ref(), self.key.as_ref()) else {
            return;
        };
        settle_raw_reacquisition_reservation(
            connection,
            key,
            self.anchor_connection,
            self.owner,
            self.anchor_generation,
            self.anchor_token,
            token,
            &mut self.preactivation_surrender,
        );
    }
}

impl Drop for QuarantinedProjectionAnchor {
    fn drop(&mut self) {
        match self.state {
            QuarantinedAnchorState::Anchored => {
                self.state = QuarantinedAnchorState::Released;
                settle_raw_quarantined_authority(
                    &self.connection,
                    &self.key,
                    self.owner,
                    self.generation,
                    self.token,
                    &mut self.preactivation_surrender,
                );
            }
            QuarantinedAnchorState::TransferCleanupPending => {
                if let Some(cleanup) = self.transfer_cleanup.take() {
                    drop(cleanup);
                }
                self.state = QuarantinedAnchorState::Released;
                self.connection.request_ordinary_retirement();
            }
            QuarantinedAnchorState::Released => {}
        }
    }
}

impl Drop for ReacquisitionResumeReservation {
    fn drop(&mut self) {
        self.settle_implicit_drop();
    }
}
