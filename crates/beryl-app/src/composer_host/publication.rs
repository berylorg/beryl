mod disposal;
mod execution;
mod lane;

use beryl_home_store::{CommandCancellation, HomeStore, ReconciliationHandle};
use beryl_model::{SealedAssetReferenceSetProof, SyndicDraftId};
use beryl_state::{
    AssetOwner, AssetOwnerHeadExpectation, AssetReferenceSetStagingAuthority, AssetState,
};
use syndic_storage::{
    CapturedDraftEditorCandidatePublicationSourceV1, DraftEditorCandidateActivationBindingV1,
    DraftEditorCandidatePublicationEvidenceV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1, DraftEditorCurrentSelectorV1,
    DraftMarkerSealFailureReasonV1, DraftMarkerSealOperationIdV1, DraftPieceOperationIdV1,
    DraftRootHistoryPairV1, PreparedDraftEditorCandidatePublicationV1,
    PreparedDraftEditorCandidateSessionDisposeV1, SyndicPointReadLimit, SyndicTimestamp,
};

use crate::composer_marker_seal::{
    DraftMarkerSealAdmission, DraftMarkerSealCommandStage, DraftMarkerSealDriveOutcome,
    DraftMarkerSealFlight, DraftMarkerSealFlightRequest, DraftMarkerSealReleaseIntent,
    DraftMarkerSealReleaseOutcome, DraftMarkerSealService,
};

use super::{ComposerHostBinding, ComposerHostError, SyndicComposerHost, request::validate_store};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostPublicationTicket {
    host_generation: super::ComposerHostGeneration,
    lane_generation: u64,
    candidate_generation: u64,
}

impl ComposerHostPublicationTicket {
    pub const fn lane_generation(self) -> u64 {
        self.lane_generation
    }

    pub const fn candidate_generation(self) -> u64 {
        self.candidate_generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostDisposalTicket {
    host_generation: super::ComposerHostGeneration,
    lane_generation: u64,
}

impl ComposerHostDisposalTicket {
    pub const fn lane_generation(self) -> u64 {
        self.lane_generation
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ComposerHostMarkerSealAuthority {
    operation_id: DraftMarkerSealOperationIdV1,
    staging: AssetReferenceSetStagingAuthority,
}

impl std::fmt::Debug for ComposerHostMarkerSealAuthority {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ComposerHostMarkerSealAuthority")
            .field("operation_id", &self.operation_id)
            .field("staging", &"[redacted]")
            .finish()
    }
}

impl ComposerHostMarkerSealAuthority {
    pub const fn new(
        operation_id: DraftMarkerSealOperationIdV1,
        staging: AssetReferenceSetStagingAuthority,
    ) -> Self {
        Self {
            operation_id,
            staging,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostPublicationCapture {
    CleanNoOp,
    CancelledBeforeAdmission,
    Captured(ComposerHostPublicationTicket),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostPublicationDrive {
    Progress,
    NotCommitted(DraftMarkerSealCommandStage),
    Ready,
    ReleasePending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostPublicationReleaseReason {
    Cancelled,
    Failed,
    Superseded {
        successor_operation_id: DraftMarkerSealOperationIdV1,
    },
    SessionDisposed,
    ServiceDisposed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostPublicationReleaseCompletion {
    Pending,
    Released,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostPublicationCompletion {
    ReconciliationPending,
    Published,
    ExactReplay,
    Superseded,
    NotCommitted,
    CancelledBeforeAdmission,
    DurableBaseConflict,
    SessionDisposed,
    OccupiedIdentityCollision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostDisposalCompletion {
    ReconciliationPending,
    Disposed,
    ExactReplay,
    AlreadyDisposed,
    NotCommitted,
    CancelledBeforeAdmission,
    DirtyConflict,
    OccupiedIdentityCollision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostPublicationUnavailable {
    DurableBaseConflict,
    SessionDisposed,
    IdentityCollision,
    ReconciliationCollision,
    DisposalDirtyConflict,
}

pub(super) struct ComposerHostPublicationCoordinator {
    lane_generation: u64,
    pub(super) lane: Option<ComposerHostPublicationLane>,
    #[cfg(feature = "test-faults")]
    convergence_read_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, syndic_storage::SyndicStorage) + Send>>,
}

impl ComposerHostPublicationCoordinator {
    pub(super) const fn new() -> Self {
        Self {
            lane_generation: 0,
            lane: None,
            #[cfg(feature = "test-faults")]
            convergence_read_fault: None,
        }
    }
}

pub(super) enum ComposerHostPublicationLane {
    Publication(PendingPublication),
    Disposal(PendingDisposal),
}

struct PublicationIntent {
    binding: ComposerHostBinding,
    selector: syndic_storage::DraftEditorCurrentSelectorV1,
    candidate: DraftEditorCandidateActivationBindingV1,
    candidate_pair: DraftRootHistoryPairV1,
    marker_authority: Option<ComposerHostMarkerSealAuthority>,
    source: Option<CapturedDraftEditorCandidatePublicationSourceV1>,
    assets: AssetState,
    cancellation: CommandCancellation,
}

pub(super) struct PendingPublication {
    ticket: ComposerHostPublicationTicket,
    intent: PublicationIntent,
    stage: PublicationStage,
}

enum PublicationStage {
    Sealing {
        service: DraftMarkerSealService,
        flight: DraftMarkerSealFlight,
    },
    Releasing {
        service: DraftMarkerSealService,
        flight: DraftMarkerSealFlight,
        intent: DraftMarkerSealReleaseIntent,
    },
    Sealed(DraftEditorCandidatePublicationEvidenceV1),
    Ready(PreparedPublication),
    Reconciling {
        prepared: PreparedPublication,
        handle: ReconciliationHandle,
    },
    Terminal {
        prepared: Option<PreparedPublication>,
        reason: ComposerHostPublicationUnavailable,
    },
}

#[derive(Clone)]
struct PreparedPublication {
    syndic: PreparedDraftEditorCandidatePublicationV1,
    asset: PublicationAssetPlan,
}

#[derive(Clone, Copy)]
enum PublicationAssetPlan {
    Replace {
        draft: SyndicDraftId,
        expected: Option<AssetOwnerHeadExpectation>,
        proof: SealedAssetReferenceSetProof,
    },
    Remove {
        draft: SyndicDraftId,
        expected: AssetOwnerHeadExpectation,
    },
    Validate {
        draft: SyndicDraftId,
        expected: Option<AssetOwnerHeadExpectation>,
    },
}

pub(super) struct PendingDisposal {
    ticket: ComposerHostDisposalTicket,
    binding: ComposerHostBinding,
    prepared: PreparedDraftEditorCandidateSessionDisposeV1,
    cancellation: CommandCancellation,
    reconciliation: Option<ReconciliationHandle>,
    terminal: Option<ComposerHostPublicationUnavailable>,
}

fn authenticate_capture(
    host: &SyndicComposerHost,
    store: &HomeStore,
    active: &super::ActiveComposerHost,
) -> Result<(), ComposerHostError> {
    let current = host
        .storage
        .current_draft(store, active.thread_id, publication_point_limit())?
        .ok_or(ComposerHostError::MissingCurrentDraft)?;
    if current_selector(&current) != active.durable_selector {
        return Err(ComposerHostError::DurableSelectorChanged);
    }
    let head = match host.storage.draft_editor_candidate_session(
        store,
        active.storage_candidate.draft_id(),
        active.storage_candidate.session_id(),
    )? {
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Disposed(_) => {
            return Err(ComposerHostError::PublicationUnavailable);
        }
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Absent => {
            return Err(ComposerHostError::CandidateBindingChanged);
        }
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange
        | syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure => {
            return Err(ComposerHostError::CandidateBindingChanged);
        }
    };
    if DraftEditorCandidateActivationBindingV1::from_head(&head) != active.storage_candidate {
        return Err(ComposerHostError::CandidateBindingChanged);
    }
    Ok(())
}

fn current_selector(current: &syndic_storage::SyndicCurrentDraft) -> DraftEditorCurrentSelectorV1 {
    DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    )
}

fn publication_point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).expect("publication point-read bound is nonzero")
}

fn unchanged_evidence(
    store: &HomeStore,
    assets: AssetState,
    candidate: DraftRootHistoryPairV1,
) -> Result<DraftEditorCandidatePublicationEvidenceV1, ComposerHostError> {
    let owner = AssetOwner::CurrentDraft(candidate.root().key().draft_id());
    let head = assets.owner_head(store, owner)?;
    if candidate.root().summary().marker_count() == 0 {
        if head.is_some() {
            return Err(ComposerHostError::PublicationAssetMismatch);
        }
        Ok(DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty)
    } else {
        let proof = head
            .as_ref()
            .ok_or(ComposerHostError::PublicationAssetMismatch)?
            .set();
        assets.sealed_reference_set_manifest(store, proof)?;
        Ok(DraftEditorCandidatePublicationEvidenceV1::UnchangedNonempty { asset_proof: proof })
    }
}

fn prepare_publication(
    store: &HomeStore,
    storage: syndic_storage::SyndicStorage,
    intent: &mut PublicationIntent,
    evidence: DraftEditorCandidatePublicationEvidenceV1,
) -> Result<PreparedPublication, ComposerHostError> {
    let asset = prepare_asset_plan(store, intent.assets, intent.candidate_pair, evidence)?;
    let source = intent
        .source
        .take()
        .ok_or(ComposerHostError::PublicationUnavailable)?;
    let syndic = match storage.prepare_draft_editor_candidate_publication(store, source, evidence) {
        Ok(prepared) => prepared,
        Err(failure) => {
            let (source, error) = failure.into_parts();
            intent.source = Some(source);
            return Err(error.into());
        }
    };
    Ok(PreparedPublication { syndic, asset })
}

fn prepare_asset_plan(
    store: &HomeStore,
    assets: AssetState,
    candidate: DraftRootHistoryPairV1,
    evidence: DraftEditorCandidatePublicationEvidenceV1,
) -> Result<PublicationAssetPlan, ComposerHostError> {
    let draft = candidate.root().key().draft_id();
    let owner = AssetOwner::CurrentDraft(draft);
    let current = assets.owner_head(store, owner)?;
    match evidence {
        DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty { asset_proof, .. } => {
            assets.sealed_reference_set_manifest(store, asset_proof)?;
            let expected = current.as_ref().map(|head| head.expectation());
            if expected.is_some_and(|value| value.set() == asset_proof) {
                return Err(ComposerHostError::PublicationAssetMismatch);
            }
            Ok(PublicationAssetPlan::Replace {
                draft,
                expected,
                proof: asset_proof,
            })
        }
        DraftEditorCandidatePublicationEvidenceV1::ChangedEmpty { .. } => {
            let expected = current
                .as_ref()
                .ok_or(ComposerHostError::PublicationAssetMismatch)?
                .expectation();
            Ok(PublicationAssetPlan::Remove { draft, expected })
        }
        DraftEditorCandidatePublicationEvidenceV1::UnchangedNonempty { asset_proof } => {
            assets.sealed_reference_set_manifest(store, asset_proof)?;
            let expected = current
                .as_ref()
                .filter(|head| head.set() == asset_proof)
                .ok_or(ComposerHostError::PublicationAssetMismatch)?
                .expectation();
            Ok(PublicationAssetPlan::Validate {
                draft,
                expected: Some(expected),
            })
        }
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty => {
            if current.is_some() {
                return Err(ComposerHostError::PublicationAssetMismatch);
            }
            Ok(PublicationAssetPlan::Validate {
                draft,
                expected: None,
            })
        }
    }
}

fn release_intent(
    reason: ComposerHostPublicationReleaseReason,
    successor: Option<DraftEditorCandidateActivationBindingV1>,
) -> Result<DraftMarkerSealReleaseIntent, ComposerHostError> {
    Ok(match reason {
        ComposerHostPublicationReleaseReason::Cancelled => DraftMarkerSealReleaseIntent::Cancelled,
        ComposerHostPublicationReleaseReason::Failed => {
            DraftMarkerSealReleaseIntent::Failed(DraftMarkerSealFailureReasonV1::Operational)
        }
        ComposerHostPublicationReleaseReason::Superseded {
            successor_operation_id,
        } => DraftMarkerSealReleaseIntent::Superseded {
            successor_operation_id,
            successor: successor.ok_or(ComposerHostError::OldBinding)?,
        },
        ComposerHostPublicationReleaseReason::SessionDisposed => {
            DraftMarkerSealReleaseIntent::SessionDisposed
        }
        ComposerHostPublicationReleaseReason::ServiceDisposed => {
            DraftMarkerSealReleaseIntent::ServiceDisposed
        }
    })
}

pub(super) fn same_session(left: ComposerHostBinding, right: ComposerHostBinding) -> bool {
    left.home_id() == right.home_id()
        && left.home_generation() == right.home_generation()
        && left.host_generation() == right.host_generation()
        && left.candidate().draft_id() == right.candidate().draft_id()
        && left.candidate().session_id() == right.candidate().session_id()
}

fn next(value: u64) -> Result<u64, ComposerHostError> {
    value
        .checked_add(1)
        .ok_or(ComposerHostError::GenerationExhausted)
}
