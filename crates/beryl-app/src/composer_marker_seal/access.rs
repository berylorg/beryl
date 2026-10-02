use super::*;
use beryl_home_store::{CommandOutcome, HomeCandidateRecoveryAccess, HomeCommand, ReadError};
use beryl_model::{
    DomainRevision, HomeRevision, OrderedMarkerAssetSummaryV1, SequentialMarkerSummaryV1,
};
use beryl_state::{AssetReferenceSetCompletion, AssetReferenceSetManifest};
use syndic_storage::*;

#[derive(Clone, Copy)]
pub(super) enum MarkerAccess<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

impl MarkerAccess<'_> {
    pub(super) fn validate(
        self,
        state: &mut ServiceState,
    ) -> Result<(), DraftMarkerSealServiceError> {
        match self {
            Self::Ordinary(store) => durability::validate_store(state, store),
            Self::Candidate(access)
                if access.home_id() == state.home_id
                    && access.generation() == state.home_generation
                    && state.recovery_owned
                    && matches!(state.lifecycle, ServiceLifecycle::Recovering) =>
            {
                if state.orphans.iter().any(|flight| flight.collision) {
                    return Err(DraftMarkerSealServiceError::ReconciliationCollision);
                }
                access.home_revision()?;
                Ok(())
            }
            Self::Candidate(_) => Err(DraftMarkerSealServiceError::HomeGenerationChanged),
        }
    }
    pub(super) fn authenticate(
        self,
        storage: &SyndicStorage,
        expected: DraftEditorCandidateActivationBindingV1,
    ) -> Result<(), DraftMarkerSealServiceError> {
        match self {
            Self::Ordinary(store) => admission::authenticate_candidate(storage, store, expected),
            Self::Candidate(access) => {
                Ok(storage.validate_draft_editor_candidate_candidate(access, expected)?)
            }
        }
    }
    pub(super) fn is_candidate(self) -> bool {
        matches!(self, Self::Candidate(_))
    }
    pub(super) fn home_revision(self) -> Result<HomeRevision, ReadError> {
        match self {
            Self::Ordinary(store) => store.home_revision(),
            Self::Candidate(access) => access.home_revision(),
        }
    }
    pub(super) fn syndic_revision(
        self,
        storage: &SyndicStorage,
    ) -> Result<DomainRevision, ReadError> {
        match self {
            Self::Ordinary(store) => storage.revision(store),
            Self::Candidate(access) => storage.revision_candidate(access),
        }
    }
    pub(super) fn asset_revision(self, assets: &AssetState) -> Result<DomainRevision, ReadError> {
        match self {
            Self::Ordinary(store) => assets.revision(store),
            Self::Candidate(access) => assets.revision_candidate(access),
        }
    }
    pub(super) fn execute(self, command: HomeCommand, fault: CommandFault) -> CommandOutcome {
        match self {
            Self::Ordinary(store) => {
                fault.run(store);
                store.execute(command)
            }
            Self::Candidate(access) => access.execute(command),
        }
    }
    pub(super) fn status(
        self,
        storage: &SyndicStorage,
        key: DraftMarkerSealKeyV1,
    ) -> Result<DraftMarkerSealStatusV1, DraftMarkerSealErrorV1> {
        match self {
            Self::Ordinary(store) => storage.draft_marker_seal_status(store, key),
            Self::Candidate(access) => storage.draft_marker_seal_status_candidate(access, key),
        }
    }
    pub(super) fn begin(
        self,
        storage: &SyndicStorage,
        request: DraftMarkerSealRequestV1,
    ) -> Result<PreparedDraftMarkerSealBeginV1, DraftMarkerSealErrorV1> {
        match self {
            Self::Ordinary(store) => storage.prepare_draft_marker_seal_begin(store, request),
            Self::Candidate(access) => {
                storage.prepare_draft_marker_seal_begin_candidate(access, request)
            }
        }
    }
    pub(super) fn advance(
        self,
        storage: &SyndicStorage,
        key: DraftMarkerSealKeyV1,
        limit: usize,
    ) -> Result<Option<PreparedDraftMarkerSealAdvanceV1>, DraftMarkerSealErrorV1> {
        match self {
            Self::Ordinary(store) => {
                storage.prepare_draft_marker_seal_advance_with_limit(store, key, limit)
            }
            Self::Candidate(access) => {
                storage.prepare_draft_marker_seal_advance_with_limit_candidate(access, key, limit)
            }
        }
    }
    pub(super) fn cancel(
        self,
        storage: &SyndicStorage,
        key: DraftMarkerSealKeyV1,
    ) -> Result<PreparedDraftMarkerSealCancelV1, DraftMarkerSealErrorV1> {
        match self {
            Self::Ordinary(store) => storage.prepare_draft_marker_seal_cancel(store, key),
            Self::Candidate(access) => {
                storage.prepare_draft_marker_seal_cancel_candidate(access, key)
            }
        }
    }
    pub(super) fn fail(
        self,
        storage: &SyndicStorage,
        key: DraftMarkerSealKeyV1,
        reason: DraftMarkerSealFailureReasonV1,
    ) -> Result<PreparedDraftMarkerSealFailV1, DraftMarkerSealErrorV1> {
        match self {
            Self::Ordinary(store) => storage.prepare_draft_marker_seal_fail(store, key, reason),
            Self::Candidate(access) => {
                storage.prepare_draft_marker_seal_fail_candidate(access, key, reason)
            }
        }
    }
    pub(super) fn supersede(
        self,
        storage: &SyndicStorage,
        key: DraftMarkerSealKeyV1,
        successor: DraftMarkerSealOperationIdV1,
    ) -> Result<PreparedDraftMarkerSealSupersedeV1, DraftMarkerSealErrorV1> {
        match self {
            Self::Ordinary(store) => {
                storage.prepare_draft_marker_seal_supersede(store, key, successor)
            }
            Self::Candidate(access) => {
                storage.prepare_draft_marker_seal_supersede_candidate(access, key, successor)
            }
        }
    }
    pub(super) fn building(
        self,
        assets: &AssetState,
        staging: AssetReferenceSetStagingAuthority,
    ) -> Result<AssetReferenceSetManifest, beryl_state::AssetReadError> {
        match self {
            Self::Ordinary(store) => assets.staged_reference_set_manifest(store, staging),
            Self::Candidate(access) => {
                assets.staged_reference_set_manifest_candidate(access, staging)
            }
        }
    }
    pub(super) fn completion(
        self,
        assets: &AssetState,
        staging: AssetReferenceSetStagingAuthority,
        sequential: SequentialMarkerSummaryV1,
        ordered: OrderedMarkerAssetSummaryV1,
    ) -> Result<AssetReferenceSetCompletion, beryl_state::AssetReadError> {
        match self {
            Self::Ordinary(store) => {
                assets.complete_reference_set(store, staging, sequential, ordered)
            }
            Self::Candidate(access) => {
                assets.complete_reference_set_candidate(access, staging, sequential, ordered)
            }
        }
    }
}
