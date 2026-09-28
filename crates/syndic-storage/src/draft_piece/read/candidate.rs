use super::*;
use beryl_home_store::HomeCandidateRecoveryAccess;

impl SyndicStorage {
    pub fn draft_piece_text_demand_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        root: DraftPieceRootReferenceV1,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    ) -> Result<DraftPieceTextDemandResultV1, DraftPieceRangeSourceErrorV1> {
        self.draft_piece_text_demand_with_access(
            ReadAccess::Candidate(store),
            root,
            demand,
            max_bytes,
        )
    }

    pub fn draft_piece_marker_demand_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        root: DraftPieceRootReferenceV1,
        demand: DraftPieceMarkerDemandV1,
    ) -> Result<DraftPieceMarkerDemandResultV1, DraftPieceRangeSourceErrorV1> {
        self.draft_piece_marker_demand_with_access(ReadAccess::Candidate(store), root, demand)
    }

    pub fn draft_piece_marker_edge_proof_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        root: DraftPieceRootReferenceV1,
        request: DraftPieceMarkerEdgeProofRequestV1,
        retained_byte_ceiling: usize,
    ) -> Result<Option<DraftPieceMarkerEdgeProofV1>, DraftPieceRangeSourceErrorV1> {
        self.draft_piece_marker_edge_proof_with_access(
            ReadAccess::Candidate(store),
            root,
            request,
            retained_byte_ceiling,
        )
    }

    pub fn validate_draft_piece_restoration_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        restoration: DraftPieceRestorationV1,
    ) -> Result<DraftPieceRestorationV1, DraftPiecePrepareErrorV1> {
        self.validate_draft_piece_restoration_with_access(ReadAccess::Candidate(store), restoration)
    }
}
