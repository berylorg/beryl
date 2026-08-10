use std::{fmt, sync::Arc};

use beryl_home_store::HomeGeneration;
use beryl_model::BerylHomeId;
use beryl_state::BerylState;

#[cfg(test)]
use super::super::super::persistent_failure::PendingProjectionAdoptionCheckout;
#[cfg(test)]
use super::super::super::service_config::ProjectionWorkerPoolDiagnostics;
use super::super::super::{
    accepted_input_scheduler::StartupRecoveryDiagnostics,
    connection::{
        AdoptedConnectionEpochAttachment, BoundConnectionEpoch, DriverParkError,
        InertConnectionEpochAttachment, OldConnectionIngesterJoinError, ParkedDriver,
        PreparedConnectionEpoch, PreparedConnectionEpochBindError, PreparedConnectionEpochError,
        ProjectionConnection, ProviderBrokerAdoptionStopped,
    },
    error::ProjectionCoordinatorError,
    persistent_failure::{
        PendingProjectionAdoptionTopology, PersistentFailureAdoptionFence,
        PersistentFailureCutIdentity, PersistentFailurePendingProjectionQuarantine,
        PersistentFailurePendingProjectionQuarantineReason, PersistentFailureRecoveryInventory,
        ProjectionServiceGeneration,
    },
    service_config::{ProjectionPreactivationRecoveryHold, ProjectionWorkerPermit},
    service_startup::ServiceStartupGate,
};
use super::super::ProjectionConnectionServiceCloseError;
#[cfg(test)]
use super::unpublished::ReplacementResourceFailureSelectorForTest;
use super::{ProjectionConnectionService, UnpublishedProjectionConnectionService};

mod preflight;
#[cfg(test)]
mod test_support;

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
    pub(super) attempt: Option<Box<ServiceAdoptionAttempt>>,
}

pub(in crate::cas_projection) struct ServiceAdoptionAttempt {
    pub(super) metadata: PersistentFailureServiceAdoptionMetadata,
    pub(super) quarantine_promotable: bool,
    pub(super) cut: PersistentFailureCutIdentity,
    pub(super) inventory: Option<PersistentFailureRecoveryInventory>,
    pub(super) replacement: Option<UnpublishedProjectionConnectionService>,
    pub(super) topology: Option<PendingProjectionAdoptionTopology>,
    pub(super) fence: Option<PersistentFailureAdoptionFence>,
    #[cfg(test)]
    pub(super) adversarial_checkout: Option<PendingProjectionAdoptionCheckout>,
    pub(super) connections: Vec<Arc<ProjectionConnection>>,
    pub(super) rejected_connections: Vec<Arc<ProjectionConnection>>,
    pub(super) connection_check_scratch: Vec<Arc<ProjectionConnection>>,
    pub(super) connection_states: Vec<ConnectionAdoptionState>,
    pub(super) inert_attachments: Vec<InertConnectionEpochAttachment>,
    pub(super) inert_attachment_limit: Option<usize>,
    pub(super) inert_connections_terminalized: bool,
    pub(super) replacement_candidate_holds: Vec<ProjectionPreactivationRecoveryHold>,
    pub(super) failed_candidate_permits: Vec<ProjectionWorkerPermit>,
    pub(super) old_candidate_holds: Vec<ProjectionPreactivationRecoveryHold>,
    pub(super) adopted_service: Option<ProjectionConnectionService>,
    pub(super) adopted_beryl_state: Option<BerylState>,
    pub(super) adopted_startup_gate: Option<Arc<ServiceStartupGate>>,
    pub(super) publication_committed: bool,
    #[cfg(test)]
    pub(super) replacement_resource_failure: Option<ReplacementResourceFailureSelectorForTest>,
    #[cfg(test)]
    pub(super) retain_late_after_publication_fence: bool,
}

pub(super) enum ConnectionAdoptionState {
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

impl PersistentFailurePendingProjectionQuarantine {
    /// Consumes this complete quarantine and one never-published recovered service into a single
    /// adopted-but-unpublished authority or a terminal inert owner.
    pub fn adopt_unpublished_service(
        self,
        replacement: UnpublishedProjectionConnectionService,
    ) -> Result<AdoptedUnpublishedProjectionConnectionService, PersistentFailureServiceAdoptionError>
    {
        let mut attempt = Box::new(ServiceAdoptionAttempt::new(self, replacement));
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| attempt.run())) {
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
            Err(_) => {
                attempt.make_inert();
                Err(PersistentFailureServiceAdoptionError {
                    reason: PersistentFailureServiceAdoptionReason::UnexpectedUnwind,
                    attempt: Some(attempt),
                })
            }
        }
    }
}

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
        #[cfg(test)]
        let mut replacement = replacement;
        #[cfg(test)]
        let replacement_resource_failure = replacement.take_replacement_resource_failure_for_test();
        Self {
            metadata,
            quarantine_promotable: quarantine_metadata.is_promotable(),
            cut,
            inventory: Some(quarantine.into_inventory()),
            replacement: Some(replacement),
            topology: None,
            fence: None,
            #[cfg(test)]
            adversarial_checkout: None,
            connections: Vec::new(),
            rejected_connections: Vec::new(),
            connection_check_scratch: Vec::new(),
            connection_states: Vec::new(),
            inert_attachments: Vec::new(),
            inert_attachment_limit: None,
            inert_connections_terminalized: false,
            replacement_candidate_holds: Vec::new(),
            failed_candidate_permits: Vec::new(),
            old_candidate_holds: Vec::new(),
            adopted_service: None,
            adopted_beryl_state: None,
            adopted_startup_gate: None,
            publication_committed: false,
            #[cfg(test)]
            replacement_resource_failure,
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
        super::transaction::panic_after_old_ingester_join_if_armed(self.metadata.home_id);
        #[cfg(test)]
        super::transaction::pause_before_commit_if_armed(self.metadata.home_id);
        #[cfg(test)]
        super::transaction::retain_late_authority_before_commit_if_armed(
            self.metadata.home_id,
            self.inventory
                .as_ref()
                .expect("the adoption attempt retains its old-cut inventory"),
        );
        super::transaction::commit_fenced(
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
        self.attempt
            .as_mut()
            .expect("an adopted unpublished owner retains its complete adoption attempt")
            .adopted_service
            .as_mut()
            .expect("an adopted unpublished owner retains its dormant replacement service")
            .converge_dormant_startup()
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
    pub(in crate::cas_projection) fn startup_gate_for_test(&self) -> Arc<ServiceStartupGate> {
        Arc::clone(
            self.attempt
                .as_ref()
                .and_then(|attempt| attempt.adopted_startup_gate.as_ref())
                .expect("an adopted unpublished owner retains its startup gate"),
        )
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
    ) -> ProjectionWorkerPoolDiagnostics {
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
    #[cfg(test)]
    pub(in crate::cas_projection) fn inventory_reescrow_is_disarmed_for_test(&self) -> bool {
        self.attempt
            .as_ref()
            .and_then(|attempt| attempt.inventory.as_ref())
            .is_some_and(|inventory| inventory.reescrow_is_disarmed_for_test())
    }

    /// Consumes the terminal inert owner through explicit cancellation and joining of every
    /// reached worker and endpoint.
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
