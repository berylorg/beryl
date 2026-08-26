use std::num::NonZeroUsize;

use beryl_home_store::{CommandBuildError, HomeHealthState, ReadError, ReconciliationFailure};
use beryl_model::{AssetProofError, SealedAssetReferenceSetProof};
use beryl_state::{
    ASSET_REFERENCE_PAGE_MAX_ENTRIES, AssetReadError, AssetReferencePageError,
    AssetReferenceSetStagingAuthority,
};
use syndic_storage::{
    DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS, DraftEditorCandidateActivationBindingV1,
    DraftMarkerSealCustodyReleaseV1, DraftMarkerSealErrorV1, DraftMarkerSealFailureReasonV1,
    DraftMarkerSealOperationIdV1, DraftMarkerSealProofV1, DraftMarkerSealRequestV1,
    SyndicReadError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealServiceLimits {
    pub(super) max_concurrent_flights: NonZeroUsize,
    pub(super) markers_per_page: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DraftMarkerSealServiceConstructionError {
    #[error("the requested marker seal service generation differs from the shared home generation")]
    HomeGenerationMismatch,
    #[error(
        "the requested marker seal service domain authority differs from the shared home authority"
    )]
    DomainAuthorityMismatch,
    #[error("the requested marker seal service limits differ from the shared home limits")]
    LimitsMismatch,
}

impl DraftMarkerSealServiceLimits {
    pub fn new(
        max_concurrent_flights: NonZeroUsize,
        markers_per_page: NonZeroUsize,
    ) -> Result<Self, DraftMarkerSealServiceError> {
        if markers_per_page.get() > DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS {
            return Err(DraftMarkerSealServiceError::InvalidPageLimit {
                configured: markers_per_page.get(),
                maximum: DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS,
            });
        }
        Ok(Self {
            max_concurrent_flights,
            markers_per_page: NonZeroUsize::new(
                markers_per_page.get().min(ASSET_REFERENCE_PAGE_MAX_ENTRIES),
            )
            .expect("a nonzero configured page limit remains nonzero after the Asset clamp"),
        })
    }

    pub const fn max_concurrent_flights(self) -> NonZeroUsize {
        self.max_concurrent_flights
    }

    pub const fn markers_per_page(self) -> NonZeroUsize {
        self.markers_per_page
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct DraftMarkerSealFlightRequest {
    pub(super) candidate: DraftEditorCandidateActivationBindingV1,
    pub(super) operation_id: DraftMarkerSealOperationIdV1,
    pub(super) staging: AssetReferenceSetStagingAuthority,
}

impl DraftMarkerSealFlightRequest {
    pub const fn new(
        candidate: DraftEditorCandidateActivationBindingV1,
        operation_id: DraftMarkerSealOperationIdV1,
        staging: AssetReferenceSetStagingAuthority,
    ) -> Self {
        Self {
            candidate,
            operation_id,
            staging,
        }
    }

    pub const fn candidate(self) -> DraftEditorCandidateActivationBindingV1 {
        self.candidate
    }

    pub const fn operation_id(self) -> DraftMarkerSealOperationIdV1 {
        self.operation_id
    }

    pub const fn staging_authority(self) -> AssetReferenceSetStagingAuthority {
        self.staging
    }

    pub(super) const fn seal_request(self) -> DraftMarkerSealRequestV1 {
        DraftMarkerSealRequestV1::new(self.candidate.root(), self.operation_id)
    }

    pub(super) const fn is_empty(self) -> bool {
        self.candidate.root().marker_commitment().marker_count() == 0
    }

    pub(super) fn is_coherent(self) -> bool {
        let root = self.candidate.root();
        let history = self.candidate.history();
        self.candidate.draft_id() == root.key().draft_id()
            && history.root() == root
            && history.candidate_generation() == self.candidate.candidate_generation()
            && self.candidate.logical_extent() == root.summary().logical_extent()
            && self.candidate.session_generation() > self.candidate.candidate_generation()
    }
}

impl std::fmt::Debug for DraftMarkerSealFlightRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DraftMarkerSealFlightRequest")
            .field("candidate", &self.candidate)
            .field("operation_id", &self.operation_id)
            .field("staging_authority", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealFlight {
    pub(super) serial: u64,
    pub(super) request: DraftMarkerSealFlightRequest,
}

impl DraftMarkerSealFlight {
    pub const fn request(self) -> DraftMarkerSealFlightRequest {
        self.request
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealAdmission {
    Admitted(DraftMarkerSealFlight),
    Coalesced(DraftMarkerSealFlight),
    CancelledBeforeAdmission,
    Saturated,
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealCommandStage {
    Begin,
    Page,
    AssetSeal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealDriveOutcome {
    Progress,
    NotCommitted(DraftMarkerSealCommandStage),
    ChangedNonempty {
        syndic: DraftMarkerSealProofV1,
        assets: SealedAssetReferenceSetProof,
    },
    ChangedToEmpty {
        syndic: DraftMarkerSealProofV1,
    },
    TerminalSettlementPending(DraftMarkerSealReleaseIntent),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealReleaseIntent {
    Cancelled,
    Failed(DraftMarkerSealFailureReasonV1),
    Superseded {
        successor_operation_id: DraftMarkerSealOperationIdV1,
        successor: DraftEditorCandidateActivationBindingV1,
    },
    SessionDisposed,
    ServiceDisposed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealReleaseOutcome {
    DeferredByActiveDrive(DraftMarkerSealReleaseIntent),
    NotCommitted(DraftMarkerSealReleaseIntent),
    Settled {
        intent: DraftMarkerSealReleaseIntent,
        release: DraftMarkerSealCustodyReleaseV1,
    },
    ReleasedWithoutDurableSeal(DraftMarkerSealReleaseIntent),
    ReleasedAfterSeal(DraftMarkerSealReleaseIntent),
    ReleasedAfterOtherTerminal {
        requested: DraftMarkerSealReleaseIntent,
        observed: DraftMarkerSealObservedTerminal,
    },
    ConflictingIntent {
        active: DraftMarkerSealReleaseIntent,
        requested: DraftMarkerSealReleaseIntent,
    },
    AlreadyReleased,
    HomeGenerationRetired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealObservedTerminal {
    Cancelled,
    Failed(DraftMarkerSealFailureReasonV1),
    Superseded(DraftMarkerSealOperationIdV1),
    Sealed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerSealDisposeOutcome {
    Progress {
        remaining: usize,
        release: DraftMarkerSealReleaseOutcome,
    },
    WaitingForDrive {
        remaining: usize,
    },
    Disposed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealRetireOutcome {
    pub(super) released: usize,
    pub(super) settling_drives: usize,
}

impl DraftMarkerSealRetireOutcome {
    pub const fn released(self) -> usize {
        self.released
    }

    pub const fn settling_drives(self) -> usize {
        self.settling_drives
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerSealServiceDiagnostics {
    pub(super) configured_flight_limit: usize,
    pub(super) current_flights: usize,
    pub(super) high_water_flights: usize,
    pub(super) admission_denials: u64,
    pub(super) coalesced_admissions: u64,
    pub(super) conflicts: u64,
    pub(super) driving_flights: usize,
    pub(super) terminalizing_flights: usize,
    pub(super) retained_draft_sized_bytes: usize,
}

impl DraftMarkerSealServiceDiagnostics {
    pub const fn configured_flight_limit(self) -> usize {
        self.configured_flight_limit
    }

    pub const fn current_flights(self) -> usize {
        self.current_flights
    }

    pub const fn high_water_flights(self) -> usize {
        self.high_water_flights
    }

    pub const fn admission_denials(self) -> u64 {
        self.admission_denials
    }

    pub const fn coalesced_admissions(self) -> u64 {
        self.coalesced_admissions
    }

    pub const fn conflicts(self) -> u64 {
        self.conflicts
    }

    pub const fn driving_flights(self) -> usize {
        self.driving_flights
    }

    pub const fn terminalizing_flights(self) -> usize {
        self.terminalizing_flights
    }

    pub const fn retained_draft_sized_bytes(self) -> usize {
        self.retained_draft_sized_bytes
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DraftMarkerSealServiceError {
    #[cfg(feature = "test-faults")]
    #[error("injected marker seal operational failure")]
    InjectedOperationalFailure,
    #[error("marker seal page limit {configured} exceeds the lower-layer maximum {maximum}")]
    InvalidPageLimit { configured: usize, maximum: usize },
    #[error("the marker seal service is disposed")]
    ServiceDisposed,
    #[error("the marker seal flight is stale or already released")]
    StaleFlight,
    #[error("the marker seal flight is already being driven")]
    FlightBusy,
    #[error("marker seal flight serial space is exhausted")]
    SerialExhausted,
    #[error("the marker seal candidate binding is incoherent")]
    InvalidCandidateBinding,
    #[error("the marker seal candidate session is absent")]
    CandidateSessionAbsent,
    #[error("the marker seal candidate session is disposed")]
    CandidateSessionDisposed,
    #[error("the marker seal candidate session changed during authentication")]
    CandidateSessionConcurrentChange,
    #[error("the marker seal candidate session failed invariant validation")]
    CandidateSessionInvariant,
    #[error("the marker seal candidate binding is not the active session head")]
    StaleCandidateBinding,
    #[error("the marker seal flight has a pending durable terminal settlement")]
    TerminalSettlementRequired,
    #[error("the marker seal service is disposing")]
    ServiceDisposing,
    #[error("the supplied store belongs to a different home")]
    ForeignHome,
    #[error("the retained home is unavailable: {0:?}")]
    HomeUnavailable(HomeHealthState),
    #[error("the retained home generation changed")]
    HomeGenerationChanged,
    #[error("the durable marker seal is in an unexpected terminal state")]
    DurableTerminal,
    #[error("the Syndic and Asset staging frontiers disagree")]
    FrontierMismatch,
    #[error("the home command reconciliation classified a collision")]
    ReconciliationCollision,
    #[error("Syndic marker seal failed: {0}")]
    Syndic(#[from] DraftMarkerSealErrorV1),
    #[error("Asset staging read failed: {0}")]
    AssetRead(#[from] AssetReadError),
    #[error("Asset marker page construction failed: {0}")]
    AssetPage(#[from] AssetReferencePageError),
    #[error("Asset sealed proof construction failed: {0}")]
    AssetProof(#[from] AssetProofError),
    #[error("home command construction failed: {0}")]
    CommandBuild(#[from] CommandBuildError),
    #[error("home read failed: {0}")]
    HomeRead(#[from] ReadError),
    #[error("home command reconciliation failed: {0}")]
    Reconciliation(#[from] ReconciliationFailure),
    #[error("Syndic candidate-session read failed: {0}")]
    SyndicRead(#[from] SyndicReadError),
}
