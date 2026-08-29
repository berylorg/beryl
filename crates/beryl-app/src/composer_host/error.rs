use beryl_home_store::{
    CommandBuildError, HomeGeneration, HomeHealthState, ReadError, ReconciliationFailure,
};
use beryl_model::BerylHomeId;
use beryl_state::{AssetOwnerHeadUpdateError, AssetOwnerHeadValidationError, AssetReadError};
use syndic_storage::{
    DraftEditorCandidatePublicationCommandErrorV1, DraftEditorCandidateSessionCommandErrorV1,
    DraftHistoricalRootAdoptionPrepareErrorV1, DraftHistoricalRootAdoptionReconciliationErrorV1,
    DraftMutationStagingErrorV1, DraftPieceCommandReconciliationErrorV1, DraftPiecePrepareErrorV1,
    DraftPieceRangeSourceErrorV1, SyndicMutationError, SyndicReadError,
};

use crate::composer_marker_seal::DraftMarkerSealServiceError;

#[derive(Debug, thiserror::Error)]
pub enum ComposerHostError {
    #[error("Beryl home is unavailable: {0:?}")]
    HomeUnavailable(HomeHealthState),
    #[error("composer host belongs to another Beryl home")]
    ForeignHome {
        expected: BerylHomeId,
        actual: BerylHomeId,
    },
    #[error("Beryl-home generation changed")]
    HomeGenerationChanged {
        expected: HomeGeneration,
        actual: Option<HomeGeneration>,
    },
    #[error("composer host generation is exhausted")]
    GenerationExhausted,
    #[error("activation has too many initial demands")]
    TooManyInitialDemands,
    #[error("activation reuses or misorders an initial request identity")]
    InvalidInitialRequestOrder,
    #[error("the selected thread has no current draft")]
    MissingCurrentDraft,
    #[error("the selected thread has no image-label authority head")]
    MissingImageLabelAuthority,
    #[error("restoration does not match the activated root and logical extent")]
    RestorationBindingMismatch,
    #[error("request belongs to an inactive or replaced binding")]
    OldBinding,
    #[error("request identity is duplicate, stale, or out of order")]
    StaleRequestIdentity,
    #[error("the bounded pending-request window is full")]
    PendingRequestLimit,
    #[error("the request was cancelled, released, completed, or never admitted")]
    RequestNotPending,
    #[error("the executed result does not match the admitted request")]
    RequestMismatch,
    #[error("a composer mutation is already pending")]
    MutationPending,
    #[error("the bounded composer mutation work quantum is exhausted")]
    MutationWorkPending,
    #[error("the mutation is not pending for this active binding")]
    MutationNotPending,
    #[error("the range-widget mutation cannot be represented exactly")]
    MutationMalformed,
    #[error("the range-widget mutation identity collides with different authored intent")]
    MutationIdentityCollision,
    #[error("the composer mutation path is unavailable")]
    MutationUnavailable,
    #[error("the shared composer settlement-custody capacity is exhausted")]
    SettlementCustodyLimit,
    #[error("a composer history selection is already pending")]
    HistoryPending,
    #[error("the composer history selection is not pending")]
    HistoryNotPending,
    #[error("the range-widget history intent cannot be represented exactly")]
    HistoryMalformed,
    #[error("the composer history path is terminally unavailable")]
    HistoryUnavailable,
    #[error("a composer publication or disposal command is already pending")]
    PublicationPending,
    #[error("no composer publication or disposal command is pending")]
    PublicationNotPending,
    #[error("the publication or disposal callback generation is stale")]
    StalePublicationGeneration,
    #[error("the composer publication path is terminally unavailable")]
    PublicationUnavailable,
    #[error("the durable draft selector changed before publication capture")]
    DurableSelectorChanged,
    #[error("the captured candidate binding changed before publication capture")]
    CandidateBindingChanged,
    #[error("changed-marker publication requires the original marker-seal authority")]
    MarkerSealAuthorityRequired,
    #[error("unchanged-marker publication cannot consume marker-seal authority")]
    UnexpectedMarkerSealAuthority,
    #[error("the bounded marker-seal flight capacity is full")]
    MarkerSealCapacity,
    #[error("the marker-seal operation identity is occupied by different authority")]
    MarkerSealIdentityCollision,
    #[error("composer lifecycle release requires a clean, settled publication state")]
    LifecycleBlocked,
    #[error("the supplied Asset ownership case does not match the captured draft root")]
    PublicationAssetMismatch,
    #[error("candidate publication or disposal failed: {0}")]
    Publication(#[from] DraftEditorCandidatePublicationCommandErrorV1),
    #[error("asset read failed: {0}")]
    AssetRead(#[from] AssetReadError),
    #[error("asset owner-head update failed: {0}")]
    AssetOwnerUpdate(#[from] AssetOwnerHeadUpdateError),
    #[error("asset owner-head validation failed: {0}")]
    AssetOwnerValidation(#[from] AssetOwnerHeadValidationError),
    #[error("draft marker sealing failed: {0}")]
    MarkerSeal(#[from] DraftMarkerSealServiceError),
    #[error("home read failed: {0}")]
    HomeRead(#[from] ReadError),
    #[error("home command construction failed: {0}")]
    CommandBuild(#[from] CommandBuildError),
    #[error("home command reconciliation failed: {0}")]
    HomeReconciliation(#[from] ReconciliationFailure),
    #[error("Syndic read failed: {0}")]
    SyndicRead(#[from] SyndicReadError),
    #[error("Syndic mutation failed: {0}")]
    SyndicMutation(#[from] SyndicMutationError),
    #[error("candidate-session command failed: {0}")]
    SessionCommand(#[from] DraftEditorCandidateSessionCommandErrorV1),
    #[error("draft range source failed: {0}")]
    Range(#[from] DraftPieceRangeSourceErrorV1),
    #[error("draft restoration validation failed: {0}")]
    Restoration(#[from] DraftPiecePrepareErrorV1),
    #[error("draft-piece command reconciliation failed: {0}")]
    Reconciliation(#[from] DraftPieceCommandReconciliationErrorV1),
    #[error("draft mutation staging failed: {0}")]
    MutationStaging(#[from] DraftMutationStagingErrorV1),
    #[error("historical-root selection preparation failed: {0}")]
    HistoryPrepare(#[from] DraftHistoricalRootAdoptionPrepareErrorV1),
    #[error("historical-root selection reconciliation failed: {0}")]
    HistoryReconciliation(#[from] DraftHistoricalRootAdoptionReconciliationErrorV1),
}
