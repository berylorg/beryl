use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

use beryl_home_store::{DomainHandleError, HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::{BerylState, BerylStateReacquireError};
use syndic_storage::SyndicStorage;
use thiserror::Error;

use super::{ProjectionConnectionService, ProjectionConnectionServiceCloseError};
use crate::cas_projection::{
    ProjectionCoordinatorError, ProjectionServiceConfig, ProjectionServiceGeneration,
    ScheduledOrdinaryExecutionProvider,
    accepted_input_scheduler::StartupRecoveryDiagnostics,
    connection::{
        AdoptedConnectionEpochAttachment, BoundConnectionEpoch, ConnectionEpochIdentity,
        DriverParkError, InertConnectionEpochAttachment, OldConnectionIngesterJoinError,
        ParkedDriver, PreparedConnectionEpoch, PreparedConnectionEpochBindError,
        PreparedConnectionEpochError, ProjectionConnection, ProviderBrokerAdoptionStopped,
    },
    persistent_failure::{
        PendingProjectionAdoptionTopology, PersistentFailureAdoptionFence,
        PersistentFailureCutIdentity, PersistentFailurePendingProjectionQuarantine,
        PersistentFailurePendingProjectionQuarantineReason, PersistentFailureRecoveryInventory,
    },
    service_config::{ProjectionPreactivationRecoveryHold, ProjectionWorkerPermit},
    service_startup::ServiceStartupGate,
};

mod publication;
mod reauthentication;
mod transaction;

pub use publication::{
    RecoveredServicePublicationError, RecoveredServicePublicationMetadata,
    RecoveredServicePublicationReason,
};

pub use reauthentication::{
    AdoptedProjectionCandidateReauthenticationLedger,
    CandidateSetConvergedAdoptedProjectionConnectionService, ProjectionCandidateDispositionOutcome,
    ProjectionCandidateId, ProjectionCandidateLedgerAccessError, ProjectionCandidateLedgerMetadata,
    ProjectionCandidateLedgerSealError, ProjectionCandidateLedgerSealFailure,
    ProjectionCandidateLedgerSealReason, ProjectionCandidateMetadata,
    ProjectionCandidateReauthenticationOutcome, ProjectionCandidateReauthenticationReason,
    ProjectionCandidateReauthenticationStatus, RecoveredProjectionCandidateMetadata,
    TerminalAdoptedProjectionConnectionService, TerminalAdoptedProjectionConnectionServiceReason,
};
pub(in crate::cas_projection) use reauthentication::{
    RecoveredProjectionLaneStagingError, StagedRecoveredProjectionConnectionService,
};

use transaction::commit_fenced;
#[cfg(test)]
use transaction::retain_late_authority_before_commit_if_armed;

#[cfg(test)]
static LATE_AUTHORITY_BEFORE_ADOPTION_COMMIT: OnceLock<Mutex<Option<BerylHomeId>>> =
    OnceLock::new();

#[cfg(test)]
pub(in crate::cas_projection) fn retain_late_authority_before_next_adoption_commit_for_test(
    home_id: BerylHomeId,
) {
    *LATE_AUTHORITY_BEFORE_ADOPTION_COMMIT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("the adoption late-authority test hook is usable") = Some(home_id);
}

/// Content-free identity of one never-published replacement service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnpublishedProjectionConnectionServiceMetadata {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
}

/// Content-free identity and exact bounded counts for one service-epoch adoption attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentFailureServiceAdoptionMetadata {
    home_id: BerylHomeId,
    old_home_generation: HomeGeneration,
    new_home_generation: HomeGeneration,
    old_service_generation: ProjectionServiceGeneration,
    new_service_generation: ProjectionServiceGeneration,
    connection_count: usize,
    candidate_count: usize,
    group_count: usize,
    local_disposition_count: usize,
}

/// Content-free reason why a consuming service-epoch adoption became inert.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFailureServiceAdoptionReason {
    QuarantineNotPromotable,
    HomeInstanceMismatch,
    HomeIdentityMismatch,
    HomeGenerationNotNewer,
    ReplacementHomeNotHealthy,
    ServiceGenerationNotNewer,
    ServiceConfigMismatch,
    ReplacementStartupFenceUnavailable,
    ReplacementCommandGateClosed,
    OldCommandGateMismatch,
    QuarantineCheckout(PersistentFailurePendingProjectionQuarantineReason),
    ConnectionSetMismatch,
    DuplicateConnection,
    ConnectionUnavailable,
    ReplacementRegistryUnavailable,
    ReplacementRegistryNotEmpty,
    ReplacementCapacityUnavailable,
    ConnectionPreparation,
    DriverPark,
    DriverBind,
    OldIngesterJoin,
    ForwardingHubUnavailable,
    LatePublication(PersistentFailurePendingProjectionQuarantineReason),
    UnexpectedUnwind,
}

/// Owning terminal failure from one consuming all-or-nothing adoption attempt.
#[must_use = "the error owns both services, the quarantine, and every reached epoch attachment"]
pub struct PersistentFailureServiceAdoptionError {
    reason: PersistentFailureServiceAdoptionReason,
    attempt: Option<Box<ServiceAdoptionAttempt>>,
}

/// Non-cloneable adopted service that remains unpublished behind its shared startup fence.
#[must_use = "the adopted service remains inert until later recovery publication consumes it"]
pub struct AdoptedUnpublishedProjectionConnectionService {
    attempt: Option<Box<ServiceAdoptionAttempt>>,
}

/// Construction failure before a replacement service can become adoption authority.
#[derive(Debug, Error)]
pub enum UnpublishedProjectionConnectionServiceBuildError {
    #[error("the recovered Beryl handle set could not be reacquired")]
    BerylState(#[source] BerylStateReacquireError),
    #[error("the recovered Syndic handle could not be reacquired")]
    SyndicStorage(#[source] DomainHandleError),
    #[error(transparent)]
    Service(#[from] ProjectionCoordinatorError),
}

/// Non-cloneable service typestate whose workers are all held behind one startup fence.
#[must_use = "the unpublished service owns its recovered home and dormant worker topology"]
pub struct UnpublishedProjectionConnectionService {
    service: Option<ProjectionConnectionService>,
    beryl_state: Option<BerylState>,
    startup_gate: Option<Arc<ServiceStartupGate>>,
}

struct UnpublishedServiceStartupGuard {
    gate: Option<Arc<ServiceStartupGate>>,
}

pub(in crate::cas_projection) struct ServiceAdoptionAttempt {
    metadata: PersistentFailureServiceAdoptionMetadata,
    quarantine_promotable: bool,
    cut: PersistentFailureCutIdentity,
    inventory: Option<PersistentFailureRecoveryInventory>,
    replacement: Option<UnpublishedProjectionConnectionService>,
    topology: Option<PendingProjectionAdoptionTopology>,
    fence: Option<PersistentFailureAdoptionFence>,
    connections: Vec<Arc<ProjectionConnection>>,
    rejected_connections: Vec<Arc<ProjectionConnection>>,
    connection_check_scratch: Vec<Arc<ProjectionConnection>>,
    connection_states: Vec<ConnectionAdoptionState>,
    inert_attachments: Vec<InertConnectionEpochAttachment>,
    replacement_candidate_holds: Vec<ProjectionPreactivationRecoveryHold>,
    failed_candidate_permits: Vec<ProjectionWorkerPermit>,
    old_candidate_holds: Vec<ProjectionPreactivationRecoveryHold>,
    adopted_service: Option<ProjectionConnectionService>,
    adopted_beryl_state: Option<BerylState>,
    adopted_startup_gate: Option<Arc<ServiceStartupGate>>,
    publication_committed: bool,
    #[cfg(test)]
    retain_late_after_publication_fence: bool,
}

enum ConnectionAdoptionState {
    Stable,
    Prepared(PreparedConnectionEpoch),
    PreparationFailed(PreparedConnectionEpochError),
    Parked(PreparedConnectionEpoch, ParkedDriver),
    ParkFailed(PreparedConnectionEpoch, DriverParkError),
    Bound(BoundConnectionEpoch),
    BindFailed(PreparedConnectionEpochBindError),
    Joined(BoundConnectionEpoch, ProviderBrokerAdoptionStopped),
    JoinFailed(BoundConnectionEpoch, OldConnectionIngesterJoinError),
    Adopted(AdoptedConnectionEpochAttachment),
    Vacant,
}

impl UnpublishedProjectionConnectionServiceMetadata {
    #[must_use]
    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }

    #[must_use]
    pub const fn service_generation(self) -> ProjectionServiceGeneration {
        self.service_generation
    }
}

impl PersistentFailureServiceAdoptionMetadata {
    #[must_use]
    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn old_home_generation(self) -> HomeGeneration {
        self.old_home_generation
    }

    #[must_use]
    pub const fn new_home_generation(self) -> HomeGeneration {
        self.new_home_generation
    }

    #[must_use]
    pub const fn old_service_generation(self) -> ProjectionServiceGeneration {
        self.old_service_generation
    }

    #[must_use]
    pub const fn new_service_generation(self) -> ProjectionServiceGeneration {
        self.new_service_generation
    }

    #[must_use]
    pub const fn connection_count(self) -> usize {
        self.connection_count
    }

    #[must_use]
    pub const fn candidate_count(self) -> usize {
        self.candidate_count
    }

    #[must_use]
    pub const fn group_count(self) -> usize {
        self.group_count
    }

    #[must_use]
    pub const fn local_disposition_count(self) -> usize {
        self.local_disposition_count
    }
}

impl UnpublishedServiceStartupGuard {
    fn new() -> Self {
        Self {
            gate: Some(ServiceStartupGate::closed_gate()),
        }
    }

    fn gate(&self) -> &Arc<ServiceStartupGate> {
        self.gate
            .as_ref()
            .expect("an armed unpublished-service constructor retains its startup gate")
    }

    fn disarm(mut self) -> Arc<ServiceStartupGate> {
        self.gate
            .take()
            .expect("a complete unpublished service takes its startup gate once")
    }
}

impl Drop for UnpublishedServiceStartupGuard {
    fn drop(&mut self) {
        if let Some(gate) = self.gate.take() {
            gate.cancel();
        }
    }
}

impl UnpublishedProjectionConnectionService {
    /// Reacquires the complete current-generation handle set from one retained same-home store and
    /// constructs every service worker behind one shared closed startup fence.
    pub(in crate::cas_projection) fn from_recovered_home(
        home: Arc<HomeStore>,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Result<Self, UnpublishedProjectionConnectionServiceBuildError> {
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            }
            .into());
        }
        let beryl_state = BerylState::reacquire(&home)
            .map_err(UnpublishedProjectionConnectionServiceBuildError::BerylState)?;
        let storage = SyndicStorage::reacquire(&home)
            .map_err(UnpublishedProjectionConnectionServiceBuildError::SyndicStorage)?;
        Self::from_recovered_handles(
            home,
            beryl_state,
            storage,
            config,
            scheduled_ordinary_provider,
        )
    }

    /// Constructs one dormant replacement from the complete handles reacquired by the process
    /// supervisor for the exact recovered generation.
    pub(in crate::cas_projection) fn from_recovered_handles(
        home: Arc<HomeStore>,
        beryl_state: BerylState,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Result<Self, UnpublishedProjectionConnectionServiceBuildError> {
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            }
            .into());
        }
        let startup_guard = UnpublishedServiceStartupGuard::new();
        let service = match ProjectionConnectionService::new_dormant_with_startup_gate(
            home,
            storage,
            config,
            scheduled_ordinary_provider,
            Arc::clone(startup_guard.gate()),
        ) {
            Ok(service) => service,
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            service: Some(service),
            beryl_state: Some(beryl_state),
            startup_gate: Some(startup_guard.disarm()),
        })
    }

    #[must_use]
    pub fn metadata(&self) -> UnpublishedProjectionConnectionServiceMetadata {
        let service = self
            .service
            .as_ref()
            .expect("an unpublished service retains its dormant service topology");
        UnpublishedProjectionConnectionServiceMetadata {
            home_id: service.home_id(),
            home_generation: service.home_generation(),
            service_generation: service.service_generation(),
        }
    }

    pub(super) fn service(&self) -> &ProjectionConnectionService {
        self.service
            .as_ref()
            .expect("an unpublished service retains its dormant service topology")
    }

    pub(super) fn take_service(&mut self) -> ProjectionConnectionService {
        self.service
            .take()
            .expect("an unpublished service transfers its dormant service topology once")
    }

    pub(super) fn take_beryl_state(&mut self) -> BerylState {
        self.beryl_state
            .take()
            .expect("an unpublished service transfers its recovered Beryl handles once")
    }

    pub(super) fn startup_gate(&self) -> &Arc<ServiceStartupGate> {
        self.startup_gate
            .as_ref()
            .expect("an unpublished service retains its shared startup fence")
    }

    pub(super) fn take_startup_gate(&mut self) -> Arc<ServiceStartupGate> {
        self.startup_gate
            .take()
            .expect("an unpublished service transfers its shared startup fence once")
    }

    pub(in crate::cas_projection) fn attach_recovery_supervisor(
        &self,
        signal: std::sync::mpsc::SyncSender<()>,
    ) -> Result<(), ()> {
        self.service().attach_recovery_supervisor(signal)
    }
}

impl PersistentFailurePendingProjectionQuarantine {
    /// Consumes this complete quarantine and one never-published recovered service into a single
    /// adopted-but-unpublished authority or a terminal inert owner.
    pub fn adopt_unpublished_service(
        self,
        replacement: UnpublishedProjectionConnectionService,
    ) -> Result<AdoptedUnpublishedProjectionConnectionService, PersistentFailureServiceAdoptionError>
    {
        let mut attempt = Box::new(ServiceAdoptionAttempt::new(self, replacement));
        let outcome = catch_unwind(AssertUnwindSafe(|| attempt.run()));
        match outcome {
            Ok(Ok(())) => Ok(AdoptedUnpublishedProjectionConnectionService {
                attempt: Some(attempt),
            }),
            Ok(Err(reason)) => {
                attempt.make_inert();
                Err(PersistentFailureServiceAdoptionError {
                    reason,
                    attempt: Some(attempt),
                })
            }
            Err(_panic) => {
                attempt.make_inert();
                Err(PersistentFailureServiceAdoptionError {
                    reason: PersistentFailureServiceAdoptionReason::UnexpectedUnwind,
                    attempt: Some(attempt),
                })
            }
        }
    }
}

impl AdoptedUnpublishedProjectionConnectionService {
    #[must_use]
    pub fn metadata(&self) -> PersistentFailureServiceAdoptionMetadata {
        self.attempt
            .as_ref()
            .expect("an adopted unpublished service retains its complete adoption owner")
            .metadata
    }

    /// Converges recovered durable startup state while every replacement worker remains behind
    /// the unpublished service's shared startup gate.
    pub(in crate::cas_projection) fn converge_recovered_startup(
        &mut self,
    ) -> Result<StartupRecoveryDiagnostics, ProjectionCoordinatorError> {
        let attempt = self
            .attempt
            .as_mut()
            .expect("an adopted unpublished owner retains its complete adoption attempt");
        let service = attempt
            .adopted_service
            .as_mut()
            .expect("an adopted unpublished owner retains its dormant replacement service");
        service.converge_dormant_startup()
    }

    pub(in crate::cas_projection) fn dispose_after_recovery_failure(
        mut self,
    ) -> Result<(), ProjectionConnectionServiceCloseError> {
        let Some(mut attempt) = self.attempt.take() else {
            return Ok(());
        };
        attempt.make_inert();
        attempt.dispose_inert()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn startup_fence_is_closed_for_test(&self) -> bool {
        self.attempt
            .as_ref()
            .and_then(|attempt| attempt.adopted_startup_gate.as_ref())
            .is_some_and(|startup| startup.is_closed())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn adopted_connection_count_for_test(&self) -> usize {
        let Some(service) = self
            .attempt
            .as_ref()
            .and_then(|attempt| attempt.adopted_service.as_ref())
        else {
            return 0;
        };
        service
            .connections
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .len()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replacement_worker_diagnostics_for_test(
        &self,
    ) -> crate::cas_projection::ProjectionWorkerPoolDiagnostics {
        self.attempt
            .as_ref()
            .and_then(|attempt| attempt.adopted_service.as_ref())
            .expect("an adopted unpublished owner retains its replacement service")
            .worker_pool_diagnostics()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retain_late_adoption_authority_for_test(&self) {
        self.attempt
            .as_ref()
            .and_then(|attempt| attempt.inventory.as_ref())
            .expect("an adopted unpublished owner retains its old-cut inventory")
            .retain_late_adoption_authority_for_test();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retire_adoption_fence_for_test(
        &mut self,
    ) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
        let attempt = self
            .attempt
            .as_mut()
            .expect("an adopted unpublished owner retains its complete adoption attempt");
        let fence = attempt
            .fence
            .take()
            .expect("the adoption publication fence is consumed once");
        match attempt
            .inventory
            .as_ref()
            .expect("an adopted unpublished owner retains its old-cut inventory")
            .retire_pending_quarantine_adoption(fence)
        {
            Ok(witness) => {
                drop(witness);
                Ok(())
            }
            Err(error) => {
                let reason = error.reason();
                attempt.fence = Some(error.into_fence());
                Err(reason)
            }
        }
    }
}

impl PersistentFailureServiceAdoptionError {
    #[must_use]
    pub const fn reason(&self) -> PersistentFailureServiceAdoptionReason {
        self.reason
    }

    #[must_use]
    pub fn metadata(&self) -> PersistentFailureServiceAdoptionMetadata {
        self.attempt
            .as_ref()
            .expect("an adoption error retains its inert owning attempt")
            .metadata
    }

    /// Consumes the terminal inert owner through explicit cancellation and joining of every
    /// reached worker and endpoint. It cannot yield retry or publication authority, and reports
    /// the first typed shutdown failure only after attempting complete cleanup.
    pub fn dispose(mut self) -> Result<(), ProjectionConnectionServiceCloseError> {
        if let Some(mut attempt) = self.attempt.take() {
            attempt.dispose_inert()
        } else {
            Ok(())
        }
    }
}

impl fmt::Debug for AdoptedUnpublishedProjectionConnectionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdoptedUnpublishedProjectionConnectionService")
            .field("metadata", &self.metadata())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for PersistentFailureServiceAdoptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentFailureServiceAdoptionError")
            .field("reason", &self.reason)
            .field("metadata", &self.metadata())
            .finish_non_exhaustive()
    }
}

impl fmt::Display for PersistentFailureServiceAdoptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "service-epoch adoption became inert: {:?}",
            self.reason
        )
    }
}

impl std::error::Error for PersistentFailureServiceAdoptionError {}

impl ServiceAdoptionAttempt {
    fn new(
        quarantine: PersistentFailurePendingProjectionQuarantine,
        replacement: UnpublishedProjectionConnectionService,
    ) -> Self {
        let quarantine_metadata = quarantine.metadata();
        let replacement_metadata = replacement.metadata();
        let cut = PersistentFailureCutIdentity::new(
            quarantine.home_id(),
            quarantine.home_generation(),
            quarantine.service_generation(),
            quarantine.failure_generation(),
        );
        let metadata = PersistentFailureServiceAdoptionMetadata {
            home_id: quarantine.home_id(),
            old_home_generation: quarantine.home_generation(),
            new_home_generation: replacement_metadata.home_generation(),
            old_service_generation: quarantine.service_generation(),
            new_service_generation: replacement_metadata.service_generation(),
            connection_count: quarantine_metadata.retained_connection_count(),
            candidate_count: quarantine_metadata.candidate_count(),
            group_count: quarantine_metadata.group_count(),
            local_disposition_count: quarantine_metadata.local_disposition_count(),
        };
        let inventory = quarantine.into_inventory();
        Self {
            metadata,
            quarantine_promotable: quarantine_metadata.is_promotable(),
            cut,
            inventory: Some(inventory),
            replacement: Some(replacement),
            topology: None,
            fence: None,
            connections: Vec::new(),
            rejected_connections: Vec::new(),
            connection_check_scratch: Vec::new(),
            connection_states: Vec::new(),
            inert_attachments: Vec::new(),
            replacement_candidate_holds: Vec::new(),
            failed_candidate_permits: Vec::new(),
            old_candidate_holds: Vec::new(),
            adopted_service: None,
            adopted_beryl_state: None,
            adopted_startup_gate: None,
            publication_committed: false,
            #[cfg(test)]
            retain_late_after_publication_fence: false,
        }
    }

    fn run(&mut self) -> Result<(), PersistentFailureServiceAdoptionReason> {
        self.snapshot_retained_connections()?;
        self.validate_service_pair()?;
        self.checkout_topology()?;
        self.validate_and_order_connections()?;
        let mut barriers = Vec::new();
        barriers
            .try_reserve_exact(self.connections.len())
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.preallocate_attempt_storage()?;
        self.reserve_replacement_topology()?;
        self.park_and_bind_drivers()?;
        self.join_old_ingesters()?;
        #[cfg(test)]
        retain_late_authority_before_commit_if_armed(
            self.metadata.home_id,
            self.inventory
                .as_ref()
                .expect("the adoption attempt retains its old-cut inventory"),
        );
        commit_fenced(
            self.cut,
            &self.connections,
            &mut self.connection_states,
            self.topology
                .as_mut()
                .expect("adoption topology remains owned through commit"),
            &mut self.replacement_candidate_holds,
            &mut self.old_candidate_holds,
            &mut self.connection_check_scratch,
            self.inventory
                .as_ref()
                .expect("adoption inventory remains owned through commit"),
            self.fence
                .as_ref()
                .expect("adoption publication fence remains owned through commit"),
            self.replacement
                .as_mut()
                .expect("replacement service remains owned through commit"),
            &mut self.adopted_service,
            &mut self.adopted_beryl_state,
            &mut self.adopted_startup_gate,
            &mut barriers,
        )
    }

    fn snapshot_retained_connections(
        &mut self,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        let inventory = self
            .inventory
            .as_ref()
            .expect("an active adoption attempt retains its quarantine inventory");
        self.inert_attachments
            .try_reserve_exact(self.metadata.connection_count)
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.connections
            .try_reserve_exact(self.metadata.connection_count)
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.connections.extend(
            inventory
                .retained_service_connection_slice()
                .iter()
                .cloned(),
        );
        if self.connections.len() != self.metadata.connection_count {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }
        Ok(())
    }

    fn validate_service_pair(&self) -> Result<(), PersistentFailureServiceAdoptionReason> {
        if !self.quarantine_promotable {
            return Err(PersistentFailureServiceAdoptionReason::QuarantineNotPromotable);
        }
        let inventory = self
            .inventory
            .as_ref()
            .expect("an active adoption attempt retains its quarantine inventory");
        let replacement = self
            .replacement
            .as_ref()
            .expect("an active adoption attempt retains its replacement service");
        let service = replacement.service();
        let home = service
            .home
            .as_ref()
            .ok_or(PersistentFailureServiceAdoptionReason::HomeInstanceMismatch)?;
        if !Arc::ptr_eq(inventory.retained_home(), home) {
            return Err(PersistentFailureServiceAdoptionReason::HomeInstanceMismatch);
        }
        if inventory.retained_home().canonical_identity() != home.canonical_identity()
            || inventory.retained_home().canonical_path() != home.canonical_path()
            || inventory.home_id() != service.home_id()
        {
            return Err(PersistentFailureServiceAdoptionReason::HomeIdentityMismatch);
        }
        if service.home_generation() <= inventory.home_generation() {
            return Err(PersistentFailureServiceAdoptionReason::HomeGenerationNotNewer);
        }
        let health = home.health();
        if health.state() != HomeHealthState::Healthy
            || health.generation() != Some(service.home_generation())
        {
            return Err(PersistentFailureServiceAdoptionReason::ReplacementHomeNotHealthy);
        }
        if service.service_generation() <= inventory.service_generation() {
            return Err(PersistentFailureServiceAdoptionReason::ServiceGenerationNotNewer);
        }
        if service.config != inventory.retained_service_config() {
            return Err(PersistentFailureServiceAdoptionReason::ServiceConfigMismatch);
        }
        if !replacement.startup_gate().is_closed() {
            return Err(PersistentFailureServiceAdoptionReason::ReplacementStartupFenceUnavailable);
        }
        if !service.command_authorizer.is_open() {
            return Err(PersistentFailureServiceAdoptionReason::ReplacementCommandGateClosed);
        }
        if !inventory.old_gate_matches_cut() {
            return Err(PersistentFailureServiceAdoptionReason::OldCommandGateMismatch);
        }
        Ok(())
    }

    fn checkout_topology(&mut self) -> Result<(), PersistentFailureServiceAdoptionReason> {
        let checkout = self
            .inventory
            .as_ref()
            .expect("an active adoption attempt retains its quarantine inventory")
            .checkout_pending_quarantine_for_adoption()
            .map_err(PersistentFailureServiceAdoptionReason::QuarantineCheckout)?;
        let (topology, fence) = checkout.into_parts();
        self.topology = Some(topology);
        self.fence = Some(fence);
        Ok(())
    }
}

impl ServiceAdoptionAttempt {
    fn park_and_bind_drivers(&mut self) -> Result<(), PersistentFailureServiceAdoptionReason> {
        for index in 0..self.connections.len() {
            let park = self.connections[index].park_driver_for_adoption(self.cut);
            let state = std::mem::replace(
                &mut self.connection_states[index],
                ConnectionAdoptionState::Vacant,
            );
            match (state, park) {
                (ConnectionAdoptionState::Prepared(prepared), Ok(parked)) => {
                    self.connection_states[index] =
                        ConnectionAdoptionState::Parked(prepared, parked);
                }
                (ConnectionAdoptionState::Prepared(prepared), Err(failure)) => {
                    self.connection_states[index] =
                        ConnectionAdoptionState::ParkFailed(prepared, failure);
                    return Err(PersistentFailureServiceAdoptionReason::DriverPark);
                }
                (state, _) => {
                    self.connection_states[index] = state;
                    return Err(PersistentFailureServiceAdoptionReason::ConnectionPreparation);
                }
            }
        }

        for index in 0..self.connections.len() {
            let state = std::mem::replace(
                &mut self.connection_states[index],
                ConnectionAdoptionState::Vacant,
            );
            let ConnectionAdoptionState::Parked(prepared, parked) = state else {
                self.connection_states[index] = state;
                return Err(PersistentFailureServiceAdoptionReason::ConnectionPreparation);
            };
            match prepared.bind_parked_driver(parked) {
                Ok(bound) => {
                    self.connection_states[index] = ConnectionAdoptionState::Bound(bound);
                }
                Err(failure) => {
                    self.connection_states[index] = ConnectionAdoptionState::BindFailed(failure);
                    return Err(PersistentFailureServiceAdoptionReason::DriverBind);
                }
            }
        }
        Ok(())
    }

    fn join_old_ingesters(&mut self) -> Result<(), PersistentFailureServiceAdoptionReason> {
        let old_identity = ConnectionEpochIdentity::new(
            self.cut.home_id,
            self.cut.home_generation,
            self.cut.service_generation,
        );
        for index in 0..self.connections.len() {
            let joined =
                self.connections[index].join_old_ingester_for_adoption(old_identity, self.cut);
            let state = std::mem::replace(
                &mut self.connection_states[index],
                ConnectionAdoptionState::Vacant,
            );
            let ConnectionAdoptionState::Bound(bound) = state else {
                self.connection_states[index] = state;
                return Err(PersistentFailureServiceAdoptionReason::ConnectionPreparation);
            };
            match joined {
                Ok(stopped) => {
                    self.connection_states[index] = ConnectionAdoptionState::Joined(bound, stopped);
                }
                Err(failure) => {
                    self.connection_states[index] =
                        ConnectionAdoptionState::JoinFailed(bound, failure);
                    return Err(PersistentFailureServiceAdoptionReason::OldIngesterJoin);
                }
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn retain_late_after_publication_fence_for_test(&mut self) {
        self.retain_late_after_publication_fence = true;
    }

    #[cfg(test)]
    fn fail_replacement_ingester_arm_for_test(&mut self, index: usize) {
        let ConnectionAdoptionState::Adopted(attachment) = &mut self.connection_states[index]
        else {
            panic!("a converged publication test retains adopted connection attachments");
        };
        attachment.fail_replacement_ingester_arm_for_test();
    }

    fn make_inert(&mut self) {
        if self.publication_committed {
            return;
        }
        if let Some(replacement) = self.replacement.as_ref() {
            if let Some(startup_gate) = replacement.startup_gate.as_ref() {
                startup_gate.cancel();
            }
        }
        if let Some(startup_gate) = self.adopted_startup_gate.as_ref() {
            startup_gate.cancel();
        }
        if self.connections.is_empty()
            && let Some(inventory) = self.inventory.as_ref()
        {
            for connection in inventory.retained_service_connection_slice() {
                let attachment = connection.make_adoption_inert_in_place(self.cut);
                if !attachment.is_empty() {
                    self.inert_attachments.push(attachment);
                }
            }
        }
        for connection in self.connections.iter().chain(&self.rejected_connections) {
            let attachment = connection.make_adoption_inert_in_place(self.cut);
            if !attachment.is_empty() {
                self.inert_attachments.push(attachment);
            }
        }
    }

    fn dispose_inert(&mut self) -> Result<(), ProjectionConnectionServiceCloseError> {
        self.make_inert();
        let mut connection_failed = false;
        for state in self.connection_states.drain(..) {
            connection_failed |= state.dispose_after_adoption_failure().is_err();
        }
        for attachment in self.inert_attachments.drain(..) {
            connection_failed |= attachment.dispose_after_adoption_failure().is_err();
        }
        if self.connections.is_empty()
            && let Some(inventory) = self.inventory.as_ref()
        {
            for connection in inventory.retained_service_connection_slice() {
                connection_failed |= connection
                    .dispose_inert_driver_after_adoption_failure()
                    .is_err();
            }
        }
        for connection in self.connections.iter().chain(&self.rejected_connections) {
            connection_failed |= connection
                .dispose_inert_driver_after_adoption_failure()
                .is_err();
        }
        let mut service_failure = None;
        if let Some(replacement) = self.replacement.as_mut()
            && let Some(service) = replacement.service.as_mut()
        {
            service_failure = service.dispose_unpublished_inert().err();
        }
        if let Some(service) = self.adopted_service.as_mut() {
            let adopted_failure = service.dispose_unpublished_inert().err();
            if service_failure.is_none() {
                service_failure = adopted_failure;
            }
        }
        if connection_failed {
            Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
        } else if let Some(error) = service_failure {
            Err(error)
        } else {
            Ok(())
        }
    }
}

impl ConnectionAdoptionState {
    fn dispose_after_adoption_failure(self) -> Result<(), ProjectionCoordinatorError> {
        match self {
            Self::Stable | Self::Vacant => Ok(()),
            Self::Prepared(prepared) => prepared.dispose_after_adoption_failure(),
            Self::PreparationFailed(failure) => {
                failure.dispose_after_adoption_failure();
                Ok(())
            }
            Self::Parked(prepared, parked) => {
                drop(parked);
                prepared.dispose_after_adoption_failure()
            }
            Self::ParkFailed(prepared, failure) => {
                drop(failure);
                prepared.dispose_after_adoption_failure()
            }
            Self::Bound(bound) => bound.dispose_after_adoption_failure(),
            Self::BindFailed(failure) => failure.dispose_after_adoption_failure(),
            Self::Joined(bound, stopped) => {
                drop(stopped);
                bound.dispose_after_adoption_failure()
            }
            Self::JoinFailed(bound, failure) => {
                let replacement_result = bound.dispose_after_adoption_failure();
                let old_result = failure.dispose_after_adoption_failure();
                replacement_result.and(old_result)
            }
            Self::Adopted(adopted) => adopted.dispose_after_adoption_failure(),
        }
    }
}

impl Drop for ServiceAdoptionAttempt {
    fn drop(&mut self) {
        self.make_inert();
    }
}

impl std::fmt::Debug for UnpublishedProjectionConnectionService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UnpublishedProjectionConnectionService")
            .field("metadata", &self.metadata())
            .finish_non_exhaustive()
    }
}

impl Drop for UnpublishedProjectionConnectionService {
    fn drop(&mut self) {
        if let Some(startup_gate) = self.startup_gate.as_ref() {
            startup_gate.cancel();
        }
    }
}
