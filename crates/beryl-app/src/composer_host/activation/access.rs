use super::*;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeGeneration};
use syndic_storage::*;

#[derive(Clone, Copy)]
pub(super) enum InitialActivationAccess<'a, 'b> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'b>),
}

impl InitialActivationAccess<'_, '_> {
    pub(super) fn home_id(self) -> beryl_model::BerylHomeId {
        match self {
            Self::Ordinary(store) => store.home_id(),
            Self::Candidate(access) => access.home_id(),
        }
    }

    pub(super) fn validate_generation(
        self,
        expected: HomeGeneration,
    ) -> Result<(), ComposerHostError> {
        match self {
            Self::Ordinary(store) => {
                let health = store.health();
                if health.state() != HomeHealthState::Healthy {
                    return Err(ComposerHostError::HomeUnavailable(health.state()));
                }
                if health.generation() != Some(expected) {
                    return Err(ComposerHostError::HomeGenerationChanged {
                        expected,
                        actual: health.generation(),
                    });
                }
            }
            Self::Candidate(access) if access.generation() != expected => {
                return Err(ComposerHostError::HomeGenerationChanged {
                    expected,
                    actual: Some(access.generation()),
                });
            }
            Self::Candidate(_) => {}
        }
        Ok(())
    }

    pub(super) fn validate_restoration(
        self,
        storage: &SyndicStorage,
        restoration: DraftPieceRestorationV1,
    ) -> Result<DraftPieceRestorationV1, DraftPiecePrepareErrorV1> {
        match self {
            Self::Ordinary(store) => storage.validate_draft_piece_restoration(store, restoration),
            Self::Candidate(access) => {
                storage.validate_draft_piece_restoration_candidate(access, restoration)
            }
        }
    }

    pub(super) fn text(
        self,
        storage: &SyndicStorage,
        binding: DraftEditorCandidateActivationBindingV1,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    ) -> Result<
        DraftPieceCandidateRangeResultV1<DraftPieceTextDemandResultV1>,
        DraftPieceRangeSourceErrorV1,
    > {
        match self {
            Self::Ordinary(store) => {
                storage.candidate_draft_piece_text_demand(store, binding, demand, max_bytes)
            }
            Self::Candidate(access) => storage
                .candidate_draft_piece_text_demand_candidate(access, binding, demand, max_bytes),
        }
    }

    pub(super) fn markers(
        self,
        storage: &SyndicStorage,
        binding: DraftEditorCandidateActivationBindingV1,
        demand: DraftPieceMarkerDemandV1,
    ) -> Result<
        DraftPieceCandidateRangeResultV1<DraftPieceMarkerDemandResultV1>,
        DraftPieceRangeSourceErrorV1,
    > {
        match self {
            Self::Ordinary(store) => {
                storage.candidate_draft_piece_marker_demand(store, binding, demand)
            }
            Self::Candidate(access) => {
                storage.candidate_draft_piece_marker_demand_candidate(access, binding, demand)
            }
        }
    }

    pub(super) fn marker_proof(
        self,
        storage: &SyndicStorage,
        binding: DraftEditorCandidateActivationBindingV1,
        request: DraftPieceMarkerEdgeProofRequestV1,
        ceiling: usize,
    ) -> Result<
        DraftPieceCandidateRangeResultV1<Option<DraftPieceMarkerEdgeProofV1>>,
        DraftPieceRangeSourceErrorV1,
    > {
        match self {
            Self::Ordinary(store) => {
                storage.candidate_draft_piece_marker_edge_proof(store, binding, request, ceiling)
            }
            Self::Candidate(access) => storage.candidate_draft_piece_marker_edge_proof_candidate(
                access, binding, request, ceiling,
            ),
        }
    }
}
