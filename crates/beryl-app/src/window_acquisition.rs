use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    CursorReadLimits, HomeCommand, HomeServiceReference, HomeStore, ReconciliationFailure,
    ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::{ExecutionBinding, SyndicDraftId, SyndicThreadId, WindowId, WindowPlacement};
use beryl_state::{
    BerylState, CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimReplacementAudit,
    CatalogClaimReplacementRow, CatalogClaimSummary, CatalogExecutionSummary, CatalogFacts,
    CatalogLineageSummary, CatalogResolvedTitle, CatalogSourceRevisions, CreateClaimedWindow,
    MAX_RESTORABLE_WINDOWS, PublishCatalogClaimReplacement, RememberedTarget, UnixMillis,
    WindowAcquisitionAuditError, WindowAcquisitionNaturalState,
};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, EligibleEmptyThreadCandidate,
    EligibleEmptyThreadOutcome, PristineThreadAudit, SyndicStorage, SyndicTimestamp,
    ThreadArchiveState, ThreadCatalogSummaryRecord, ThreadCatalogTitleSource, ThreadLineageDepth,
};

use crate::catalog_projection::{
    CatalogProjectionBuildError, ThreadCatalogProjectionPreparation,
    prepare_thread_catalog_projection,
};

mod abandonment;
mod close_admission;
mod eligible_source;
mod natural_state;
mod preparation;
mod selection_admission;

pub use abandonment::*;
pub(crate) use close_admission::*;
#[cfg(feature = "test-faults")]
pub use selection_admission::WindowSelectionAdmissionTestProbe;
pub(crate) use selection_admission::*;

const CATALOG_REPAIR_BUDGET: usize = MAX_RESTORABLE_WINDOWS;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RuntimeBackedWindowAcquisitionRequestError {
    #[error("fallback thread execution does not match the requested runtime/root target")]
    FallbackTargetMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBackedWindowAcquisitionRequest {
    window_id: WindowId,
    target: RememberedTarget,
    placement: WindowPlacement,
    fallback_thread_id: SyndicThreadId,
    fallback_draft_id: SyndicDraftId,
    fallback_execution: ExecutionBinding,
    fallback_created_at: SyndicTimestamp,
    fallback_history_policy: DraftEditHistoryPolicyV1,
}

impl RuntimeBackedWindowAcquisitionRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window_id: WindowId,
        target: RememberedTarget,
        placement: WindowPlacement,
        fallback_thread_id: SyndicThreadId,
        fallback_draft_id: SyndicDraftId,
        fallback_execution: ExecutionBinding,
        fallback_created_at: SyndicTimestamp,
        fallback_history_policy: DraftEditHistoryPolicyV1,
    ) -> Result<Self, RuntimeBackedWindowAcquisitionRequestError> {
        if fallback_execution.runtime_id() != target.runtime_id()
            || fallback_execution.root_id() != target.root_id()
        {
            return Err(RuntimeBackedWindowAcquisitionRequestError::FallbackTargetMismatch);
        }
        Ok(Self {
            window_id,
            target,
            placement,
            fallback_thread_id,
            fallback_draft_id,
            fallback_execution,
            fallback_created_at,
            fallback_history_policy,
        })
    }

    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn target(&self) -> RememberedTarget {
        self.target
    }

    #[must_use]
    pub const fn placement(&self) -> &WindowPlacement {
        &self.placement
    }

    #[must_use]
    pub const fn fallback_thread_id(&self) -> SyndicThreadId {
        self.fallback_thread_id
    }

    #[must_use]
    pub const fn fallback_draft_id(&self) -> SyndicDraftId {
        self.fallback_draft_id
    }

    #[must_use]
    pub const fn fallback_execution(&self) -> &ExecutionBinding {
        &self.fallback_execution
    }

    #[must_use]
    pub const fn fallback_created_at(&self) -> SyndicTimestamp {
        self.fallback_created_at
    }

    #[must_use]
    pub const fn fallback_history_policy(&self) -> DraftEditHistoryPolicyV1 {
        self.fallback_history_policy
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeBackedWindowAcquisitionDisposition {
    Reused,
    Created,
}

#[derive(Debug, Eq, PartialEq)]
pub struct RuntimeBackedWindowAcquisition {
    home_id: beryl_model::BerylHomeId,
    window_id: WindowId,
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    target: RememberedTarget,
    placement: WindowPlacement,
    disposition: RuntimeBackedWindowAcquisitionDisposition,
    window_revision: beryl_state::RecordRevision,
    fallback_thread_id: SyndicThreadId,
    fallback_draft_id: SyndicDraftId,
    fallback_execution: ExecutionBinding,
    fallback_created_at: SyndicTimestamp,
    reused_source: Option<EligibleEmptyThreadOutcome>,
    catalog_audit: Option<CatalogClaimReplacementAudit>,
}

impl RuntimeBackedWindowAcquisition {
    pub(crate) fn window_revision(&self) -> beryl_state::RecordRevision {
        self.window_revision
    }
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }

    #[must_use]
    pub const fn target(&self) -> RememberedTarget {
        self.target
    }

    #[must_use]
    pub const fn placement(&self) -> &WindowPlacement {
        &self.placement
    }

    #[must_use]
    pub const fn disposition(&self) -> RuntimeBackedWindowAcquisitionDisposition {
        self.disposition
    }

    #[must_use]
    pub const fn fallback_thread_id(&self) -> SyndicThreadId {
        self.fallback_thread_id
    }

    #[must_use]
    pub const fn fallback_draft_id(&self) -> SyndicDraftId {
        self.fallback_draft_id
    }

    #[must_use]
    pub const fn fallback_execution(&self) -> &ExecutionBinding {
        &self.fallback_execution
    }

    #[must_use]
    pub const fn fallback_created_at(&self) -> SyndicTimestamp {
        self.fallback_created_at
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeBackedWindowAcquisitionNotCommitted {
    #[error("window lifecycle mutation is already admitted")]
    MutationInProgress,
    #[error("another acquisition already owns this window identity")]
    DuplicateWindowIdentity,
    #[error("the bounded acquisition-flight capacity is full")]
    FlightCapacity,
    #[error("the durable window identity collides with the requested acquisition")]
    WindowIdentityCollision,
    #[error("the durable session is not initialized")]
    SessionNotInitialized,
    #[error("the durable main-window capacity is full")]
    WindowCapacity,
    #[error("window acquisition was cancelled before writer admission")]
    Cancelled,
    #[error("the bounded stale-catalog repair budget was exhausted")]
    CatalogRepairBudgetExhausted,
    #[error("the stale catalog row has no repairable Syndic source")]
    CatalogRepairSourceMissing,
    #[error("the natural-state audit requires catalog repair for {0}")]
    CatalogRepairNeeded(SyndicThreadId),
    #[error("catalog repair did not commit: {0}")]
    CatalogRepair(CommandError),
    #[error("window acquisition preparation failed: {0}")]
    Preparation(Box<dyn std::error::Error + Send + Sync>),
    #[error("window acquisition command construction failed: {0}")]
    CommandBuild(beryl_home_store::CommandBuildError),
    #[error("window acquisition did not commit: {0}")]
    Command(CommandError),
}

#[derive(Debug)]
pub enum RuntimeBackedWindowAcquisitionOutcome {
    NotCommitted {
        window_id: WindowId,
        evidence: RuntimeBackedWindowAcquisitionNotCommitted,
    },
    Committed {
        acquisition: RuntimeBackedWindowAcquisition,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    ExactCommitted {
        acquisition: RuntimeBackedWindowAcquisition,
    },
    Indeterminate {
        window_id: WindowId,
        failure: CommandError,
        reconciliation: RuntimeBackedWindowAcquisitionReconciliation,
    },
    RepairIndeterminate {
        window_id: WindowId,
        failure: CommandError,
        reconciliation: RuntimeBackedWindowAcquisitionRepairReconciliation,
    },
}

#[derive(Debug)]
pub enum RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome {
    ExactOld {
        window_id: WindowId,
    },
    ExactCommitted {
        acquisition: RuntimeBackedWindowAcquisition,
    },
    Collision {
        window_id: WindowId,
    },
    NotCommitted {
        window_id: WindowId,
        evidence: RuntimeBackedWindowAcquisitionNotCommitted,
    },
}

#[derive(Debug)]
pub enum RuntimeBackedWindowAcquisitionReconciliationOutcome {
    Pending {
        failure: ReconciliationFailure,
        reconciliation: RuntimeBackedWindowAcquisitionReconciliation,
    },
    ExactOld {
        window_id: WindowId,
    },
    ExactNew {
        acquisition: RuntimeBackedWindowAcquisition,
        receipt: CommitReceipt,
    },
    Collision {
        window_id: WindowId,
    },
}

#[derive(Debug)]
pub struct RuntimeBackedWindowAcquisitionReconciliation {
    acquisition: RuntimeBackedWindowAcquisition,
    handle: ReconciliationHandle,
    _flight: AcquisitionFlight,
}

impl RuntimeBackedWindowAcquisitionReconciliation {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.acquisition.window_id
    }

    #[must_use]
    pub fn reconcile(
        self,
        store: &HomeStore,
    ) -> RuntimeBackedWindowAcquisitionReconciliationOutcome {
        match store.reconcile(&self.handle) {
            Ok(ReconciliationResolution::ExactOld) => {
                RuntimeBackedWindowAcquisitionReconciliationOutcome::ExactOld {
                    window_id: self.acquisition.window_id,
                }
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                RuntimeBackedWindowAcquisitionReconciliationOutcome::ExactNew {
                    acquisition: self.acquisition,
                    receipt,
                }
            }
            Ok(ReconciliationResolution::ExactSuccessor { .. })
            | Ok(ReconciliationResolution::Collision) => {
                let window_id = self.acquisition.window_id;
                RuntimeBackedWindowAcquisitionReconciliationOutcome::Collision { window_id }
            }
            Err(failure) => RuntimeBackedWindowAcquisitionReconciliationOutcome::Pending {
                failure,
                reconciliation: self,
            },
        }
    }
}

#[derive(Debug)]
pub enum RuntimeBackedWindowAcquisitionRepairReconciliationOutcome {
    Pending {
        failure: ReconciliationFailure,
        reconciliation: RuntimeBackedWindowAcquisitionRepairReconciliation,
    },
    ExactOld {
        window_id: WindowId,
    },
    ExactNew {
        window_id: WindowId,
        receipt: CommitReceipt,
    },
    Collision {
        window_id: WindowId,
    },
}

#[derive(Debug)]
pub struct RuntimeBackedWindowAcquisitionRepairReconciliation {
    window_id: WindowId,
    handle: ReconciliationHandle,
    _flight: AcquisitionFlight,
}

impl RuntimeBackedWindowAcquisitionRepairReconciliation {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub fn reconcile(
        self,
        store: &HomeStore,
    ) -> RuntimeBackedWindowAcquisitionRepairReconciliationOutcome {
        match store.reconcile(&self.handle) {
            Ok(ReconciliationResolution::ExactOld) => {
                RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::ExactOld {
                    window_id: self.window_id,
                }
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::ExactNew {
                    window_id: self.window_id,
                    receipt,
                }
            }
            Ok(ReconciliationResolution::ExactSuccessor { .. })
            | Ok(ReconciliationResolution::Collision) => {
                RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::Collision {
                    window_id: self.window_id,
                }
            }
            Err(failure) => RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::Pending {
                failure,
                reconciliation: self,
            },
        }
    }
}

#[derive(Clone)]
pub struct RuntimeBackedWindowAcquisitionService {
    process_admission: crate::process_admission::ProcessAdmissionGate,
    store: Arc<HomeServiceReference>,
    state: BerylState,
    syndic: SyndicStorage,
    flights: Arc<Mutex<AcquisitionFlights>>,
    catalog_repair_budget: usize,
    #[cfg(feature = "test-faults")]
    before_execute: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

#[derive(Clone)]
pub struct RuntimeBackedWindowProcessRegistry {
    process_admission: crate::process_admission::ProcessAdmissionGate,
    flights: Arc<Mutex<AcquisitionFlights>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeBackedWindowMainWindowReservationError {
    ProcessAdmission(crate::process_admission::ProcessAdmissionError),
    CloseInProgress,
    SelectionInProgress,
    DuplicateWindowIdentity,
    Capacity,
}

pub struct RuntimeBackedWindowMainWindowReservation {
    process_admission: crate::process_admission::ProcessAdmissionGate,
    flights: Arc<Mutex<AcquisitionFlights>>,
    window_id: WindowId,
}

impl std::fmt::Debug for RuntimeBackedWindowMainWindowReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeBackedWindowMainWindowReservation")
            .field("window_id", &self.window_id)
            .finish_non_exhaustive()
    }
}

impl RuntimeBackedWindowProcessRegistry {
    #[must_use]
    pub fn new(process_admission: crate::process_admission::ProcessAdmissionGate) -> Self {
        Self {
            process_admission,
            flights: Arc::new(Mutex::new(AcquisitionFlights::default())),
        }
    }

    pub fn reserve_main_window(
        &self,
        window_id: WindowId,
    ) -> Result<
        RuntimeBackedWindowMainWindowReservation,
        RuntimeBackedWindowMainWindowReservationError,
    > {
        self.process_admission
            .admit(|| {
                let mut registry = self
                    .flights
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if registry.selection_owner.is_some() {
                    return Err(RuntimeBackedWindowMainWindowReservationError::SelectionInProgress);
                }
                if registry.close_owner.is_some() {
                    return Err(RuntimeBackedWindowMainWindowReservationError::CloseInProgress);
                }
                if registry.main_window_reservations.contains(&window_id) {
                    return Err(
                        RuntimeBackedWindowMainWindowReservationError::DuplicateWindowIdentity,
                    );
                }
                if registry.main_window_reservations.len() >= MAX_RESTORABLE_WINDOWS {
                    return Err(RuntimeBackedWindowMainWindowReservationError::Capacity);
                }
                registry.main_window_reservations.insert(window_id);
                registry.membership_revision = Arc::new(());
                Ok(())
            })
            .map_err(RuntimeBackedWindowMainWindowReservationError::ProcessAdmission)??;
        Ok(RuntimeBackedWindowMainWindowReservation {
            process_admission: self.process_admission.clone(),
            flights: Arc::clone(&self.flights),
            window_id,
        })
    }

    #[must_use]
    pub fn main_window_occupancy(&self) -> usize {
        self.flights
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .main_window_reservations
            .len()
    }

    pub(crate) fn try_main_window_occupancy(&self) -> Option<usize> {
        self.flights
            .try_lock()
            .ok()
            .map(|registry| registry.main_window_reservations.len())
    }

    pub(crate) fn selection_pending(&self) -> bool {
        self.flights
            .try_lock()
            .map_or(true, |registry| registry.selection_owner.is_some())
    }
}

impl RuntimeBackedWindowMainWindowReservation {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }
}

impl Drop for RuntimeBackedWindowMainWindowReservation {
    fn drop(&mut self) {
        self.process_admission.settle(|| {
            let mut registry = self
                .flights
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if registry.main_window_reservations.remove(&self.window_id) {
                registry.membership_revision = Arc::new(());
            }
        });
    }
}

impl RuntimeBackedWindowAcquisitionService {
    pub fn process_registry(&self) -> RuntimeBackedWindowProcessRegistry {
        RuntimeBackedWindowProcessRegistry {
            process_admission: self.process_admission.clone(),
            flights: Arc::clone(&self.flights),
        }
    }

    #[must_use]
    pub fn new(
        process: &RuntimeBackedWindowProcessRegistry,
        store: Arc<HomeServiceReference>,
        state: BerylState,
        syndic: SyndicStorage,
    ) -> Self {
        Self {
            process_admission: process.process_admission.clone(),
            store,
            state,
            syndic,
            flights: Arc::clone(&process.flights),
            catalog_repair_budget: CATALOG_REPAIR_BUDGET,
            #[cfg(feature = "test-faults")]
            before_execute: Arc::new(Mutex::new(None)),
        }
    }

    pub fn home_reference(&self) -> Arc<HomeServiceReference> {
        self.store.clone()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_set_before_execute(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self
            .before_execute
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(hook));
    }

    #[cfg(feature = "test-faults")]
    #[must_use]
    pub fn test_with_catalog_repair_budget(mut self, budget: usize) -> Self {
        self.catalog_repair_budget = budget;
        self
    }

    #[must_use]
    pub fn acquire(
        &self,
        request: RuntimeBackedWindowAcquisitionRequest,
        cancellation: CommandCancellation,
    ) -> RuntimeBackedWindowAcquisitionOutcome {
        let window_id = request.window_id;
        let flight = match AcquisitionFlight::acquire(Arc::clone(&self.flights), window_id) {
            Ok(flight) => flight,
            Err(evidence) => {
                return RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
                    window_id,
                    evidence,
                };
            }
        };
        self.acquire_in_flight(request, cancellation, flight)
    }

    #[must_use]
    pub fn reconcile_natural_state(
        &self,
        request: &RuntimeBackedWindowAcquisitionRequest,
    ) -> RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome {
        let window_id = request.window_id;
        let _flight = match AcquisitionFlight::acquire(Arc::clone(&self.flights), window_id) {
            Ok(flight) => flight,
            Err(evidence) => {
                return RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::NotCommitted {
                    window_id,
                    evidence,
                };
            }
        };
        self.reconcile_natural_state_in_flight(request, &CommandCancellation::new())
    }

    fn acquire_in_flight(
        &self,
        request: RuntimeBackedWindowAcquisitionRequest,
        cancellation: CommandCancellation,
        flight: AcquisitionFlight,
    ) -> RuntimeBackedWindowAcquisitionOutcome {
        let window_id = request.window_id;
        for repair_count in 0..=self.catalog_repair_budget {
            if cancellation.is_cancelled() {
                return not_committed(
                    window_id,
                    RuntimeBackedWindowAcquisitionNotCommitted::Cancelled,
                );
            }
            let repair_thread = match self
                .reconcile_natural_state_in_flight(&request, &cancellation)
            {
                RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactOld { .. } => {
                    match self.prepare_acquisition(&request, &cancellation) {
                        Ok(prepared) => {
                            return self.execute_acquisition(prepared, cancellation, flight);
                        }
                        Err(PreparationFailure::NotCommitted(evidence)) => {
                            return not_committed(window_id, evidence);
                        }
                    }
                }
                RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactCommitted {
                    acquisition,
                } => return RuntimeBackedWindowAcquisitionOutcome::ExactCommitted { acquisition },
                RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::Collision {
                    window_id,
                } => {
                    return not_committed(
                        window_id,
                        RuntimeBackedWindowAcquisitionNotCommitted::WindowIdentityCollision,
                    );
                }
                RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::NotCommitted {
                    evidence:
                        RuntimeBackedWindowAcquisitionNotCommitted::CatalogRepairNeeded(thread_id),
                    ..
                } => thread_id,
                RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::NotCommitted {
                    window_id,
                    evidence,
                } => return not_committed(window_id, evidence),
            };
            if repair_count == self.catalog_repair_budget {
                return not_committed(
                    window_id,
                    RuntimeBackedWindowAcquisitionNotCommitted::CatalogRepairBudgetExhausted,
                );
            }
            match self.repair_catalog(repair_thread, cancellation.clone()) {
                Ok(()) => {}
                Err(CatalogRepairFailure::NotCommitted(evidence)) => {
                    return not_committed(window_id, evidence);
                }
                Err(CatalogRepairFailure::Indeterminate {
                    failure,
                    reconciliation,
                }) => {
                    return RuntimeBackedWindowAcquisitionOutcome::RepairIndeterminate {
                        window_id,
                        failure,
                        reconciliation: RuntimeBackedWindowAcquisitionRepairReconciliation {
                            window_id,
                            handle: reconciliation,
                            _flight: flight,
                        },
                    };
                }
            }
        }
        unreachable!("bounded repair loop returns at its upper bound")
    }

    fn execute_acquisition(
        &self,
        prepared: PreparedAcquisition,
        _cancellation: CommandCancellation,
        flight: AcquisitionFlight,
    ) -> RuntimeBackedWindowAcquisitionOutcome {
        let window_id = prepared.acquisition.window_id;
        #[cfg(feature = "test-faults")]
        let hook = {
            self.before_execute
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        };
        #[cfg(feature = "test-faults")]
        if let Some(hook) = hook {
            hook();
        }
        match self.store.execute(prepared.command) {
            CommandOutcome::NotCommitted { evidence } => not_committed(
                window_id,
                RuntimeBackedWindowAcquisitionNotCommitted::Command(evidence),
            ),
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => RuntimeBackedWindowAcquisitionOutcome::Committed {
                acquisition: prepared.acquisition,
                receipt,
                later_failure,
                local_finalization,
            },
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => RuntimeBackedWindowAcquisitionOutcome::Indeterminate {
                window_id,
                failure,
                reconciliation: RuntimeBackedWindowAcquisitionReconciliation {
                    acquisition: prepared.acquisition,
                    handle: reconciliation.install_and_handle(),
                    _flight: flight,
                },
            },
        }
    }

    fn repair_catalog(
        &self,
        thread_id: SyndicThreadId,
        cancellation: CommandCancellation,
    ) -> Result<(), CatalogRepairFailure> {
        let preparation =
            prepare_thread_catalog_projection(&self.store, &self.syndic, &self.state, thread_id)
                .map_err(|error| {
                    CatalogRepairFailure::NotCommitted(
                        RuntimeBackedWindowAcquisitionNotCommitted::Preparation(Box::new(error)),
                    )
                })?;
        let command = match preparation {
            ThreadCatalogProjectionPreparation::ThreadMissing => {
                return Err(CatalogRepairFailure::NotCommitted(
                    RuntimeBackedWindowAcquisitionNotCommitted::CatalogRepairSourceMissing,
                ));
            }
            ThreadCatalogProjectionPreparation::ExactCurrent => return Ok(()),
            ThreadCatalogProjectionPreparation::Publish(command) => {
                command.with_cancellation(cancellation)
            }
        };
        match self.store.execute(command) {
            CommandOutcome::NotCommitted { evidence } => Err(CatalogRepairFailure::NotCommitted(
                RuntimeBackedWindowAcquisitionNotCommitted::CatalogRepair(evidence),
            )),
            CommandOutcome::Committed { .. } => Ok(()),
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => Err(CatalogRepairFailure::Indeterminate {
                failure,
                reconciliation: reconciliation.install_and_handle(),
            }),
        }
    }
}

struct PreparedAcquisition {
    acquisition: RuntimeBackedWindowAcquisition,
    command: HomeCommand,
}

struct ReuseCandidate {
    candidate: EligibleEmptyThreadCandidate,
    job_guard: beryl_state::ThreadReuseJobGuard,
}

enum AcquisitionIntent {
    Reuse(ReuseCandidate),
    Create(CreateThread),
}

enum NaturalAcquisitionAudit {
    ExactOld,
    ExactCommitted(RuntimeBackedWindowAcquisition),
    Collision,
}

enum PreparationFailure {
    NotCommitted(RuntimeBackedWindowAcquisitionNotCommitted),
}

enum CatalogRepairFailure {
    NotCommitted(RuntimeBackedWindowAcquisitionNotCommitted),
    Indeterminate {
        failure: CommandError,
        reconciliation: ReconciliationHandle,
    },
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct AcquisitionInvariant(&'static str);

fn preparation<E>(error: E) -> PreparationFailure
where
    E: std::error::Error + Send + Sync + 'static,
{
    PreparationFailure::NotCommitted(RuntimeBackedWindowAcquisitionNotCommitted::Preparation(
        Box::new(error),
    ))
}

fn command_build(error: beryl_home_store::CommandBuildError) -> PreparationFailure {
    PreparationFailure::NotCommitted(RuntimeBackedWindowAcquisitionNotCommitted::CommandBuild(
        error,
    ))
}

fn not_committed(
    window_id: WindowId,
    evidence: RuntimeBackedWindowAcquisitionNotCommitted,
) -> RuntimeBackedWindowAcquisitionOutcome {
    RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
        window_id,
        evidence,
    }
}

fn natural_reconciliation_not_committed<E>(
    window_id: WindowId,
    error: E,
) -> RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome
where
    E: std::error::Error + Send + Sync + 'static,
{
    RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::NotCommitted {
        window_id,
        evidence: RuntimeBackedWindowAcquisitionNotCommitted::Preparation(Box::new(error)),
    }
}

fn validate_execution(
    binding: &ExecutionBinding,
    source: &beryl_state::RuntimeRootCatalogSource,
) -> Result<(), CatalogProjectionBuildError> {
    if binding.runtime_id() != source.runtime().runtime_id()
        || binding.root_id() != source.root().root_id()
        || source.root().runtime_id() != source.runtime().runtime_id()
        || binding.root_path() != source.root().canonical_path()
    {
        return Err(CatalogProjectionBuildError::ExecutionBindingMismatch);
    }
    Ok(())
}

pub(crate) fn project_unclaimed_facts(
    summary: &ThreadCatalogSummaryRecord,
    source: &beryl_state::RuntimeRootCatalogSource,
) -> Result<CatalogFacts, CatalogProjectionBuildError> {
    let title = match summary.title() {
        None => CatalogResolvedTitle::absent(),
        Some(title) => match title.source() {
            ThreadCatalogTitleSource::Generated => CatalogResolvedTitle::generated(title.text())?,
            ThreadCatalogTitleSource::HistoryDerived => {
                CatalogResolvedTitle::history_derived(title.text())?
            }
        },
    };
    let archive = match summary.archive() {
        ThreadArchiveState::Ordinary => CatalogArchiveSummary::Ordinary,
        ThreadArchiveState::BranchDiscussionOpen => CatalogArchiveSummary::BranchDiscussionOpen,
        ThreadArchiveState::BranchDiscussionArchived { .. } => {
            CatalogArchiveSummary::BranchDiscussionArchived
        }
    };
    let lineage = match (summary.parent_thread_id(), summary.lineage_depth()) {
        (None, depth) if depth == ThreadLineageDepth::FIRST => CatalogLineageSummary::TopLevel,
        (Some(parent), depth) if depth != ThreadLineageDepth::FIRST => {
            CatalogLineageSummary::descendant(parent, depth.get(), summary.lineage_digest())?
        }
        (None, _) | (Some(_), _) => return Err(CatalogProjectionBuildError::LineageMismatch),
    };
    let runtime = source.runtime();
    let root = source.root();
    let execution = CatalogExecutionSummary::new(
        runtime.runtime_id(),
        root.root_id(),
        runtime.environment_label(),
        runtime.canonical_executable().clone(),
        root.display_path().clone(),
        CatalogAvailabilitySummary::new(
            runtime.availability().availability(),
            root.availability().availability(),
        ),
    )?;
    CatalogFacts::new(
        title,
        execution,
        archive,
        UnixMillis::new(summary.last_activity_at().unix_millis()),
        summary.complete(),
        CatalogClaimSummary::Unclaimed,
        lineage,
    )
    .map_err(Into::into)
}

#[derive(Default)]
struct AcquisitionFlights {
    active: HashSet<WindowId>,
    main_window_reservations: HashSet<WindowId>,
    membership_revision: Arc<()>,
    close_owner: Option<Arc<()>>,
    selection_owner: Option<Arc<()>>,
}

struct AcquisitionFlight {
    flights: Arc<Mutex<AcquisitionFlights>>,
    window_id: WindowId,
}

impl std::fmt::Debug for AcquisitionFlight {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AcquisitionFlight")
            .field("window_id", &self.window_id)
            .finish_non_exhaustive()
    }
}

impl AcquisitionFlight {
    fn acquire(
        flights: Arc<Mutex<AcquisitionFlights>>,
        window_id: WindowId,
    ) -> Result<Self, RuntimeBackedWindowAcquisitionNotCommitted> {
        {
            let mut registry = flights
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if registry.selection_owner.is_some() || registry.close_owner.is_some() {
                return Err(RuntimeBackedWindowAcquisitionNotCommitted::MutationInProgress);
            }
            if registry.active.contains(&window_id) {
                return Err(RuntimeBackedWindowAcquisitionNotCommitted::DuplicateWindowIdentity);
            }
            if registry.active.len() >= MAX_RESTORABLE_WINDOWS {
                return Err(RuntimeBackedWindowAcquisitionNotCommitted::FlightCapacity);
            }
            registry.active.insert(window_id);
        }
        Ok(Self { flights, window_id })
    }
}

impl Drop for AcquisitionFlight {
    fn drop(&mut self) {
        self.flights
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .remove(&self.window_id);
    }
}
