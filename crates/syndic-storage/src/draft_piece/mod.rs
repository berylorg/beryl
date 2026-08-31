mod admission;
mod builder_model;
mod codec;
mod history;
mod marker_commitment;
mod marker_seal;
mod materializer;
mod model;
mod mutation;
mod persistent;
mod publication;
mod read;
mod session;
mod staging;
mod staging_model;
#[cfg(feature = "test-faults")]
mod test_unadmitted_marker;
mod tree;

pub use admission::*;
pub use beryl_model::DraftMarkerCommitmentV1;
pub use builder_model::*;
pub use history::*;
pub use marker_commitment::canonical_empty_draft_marker_commitment_v1;
pub use marker_seal::*;
pub use materializer::*;
pub use model::*;
pub use mutation::{PreparedDraftPieceAdvanceV1, PreparedDraftPieceEditV1};
pub use publication::{
    CapturedDraftEditorCandidatePublicationSourceV1, DraftEditorCandidatePublicationCommandErrorV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1,
    DraftEditorCandidatePublicationSourcePreparationErrorV1,
    PreparedDraftEditorCandidatePublicationV1, PreparedDraftEditorCandidateSessionAbandonFreshV1,
    PreparedDraftEditorCandidateSessionDisposeV1,
};
pub use read::DraftPieceCommandReconciliationErrorV1;
pub use session::{
    DraftEditorCandidateSessionCommandErrorV1, PreparedDraftEditorCandidateSessionOpenV1,
};
pub use staging::{
    DraftMutationStagingErrorV1, DraftPieceDurableBuildWindowLimitsV1,
    PreparedDraftMutationStagingBatchV1, PreparedDraftMutationStagingCommandV1,
    PreparedDraftMutationTransferV1, PreparedDraftPieceStagingWindowV1,
};
pub use staging_model::*;
#[cfg(feature = "test-faults")]
pub use test_unadmitted_marker::DraftPieceUnadmittedMarkerBuilderForTest;
#[cfg(feature = "test-faults")]
pub(crate) use test_unadmitted_marker::unadmitted_marker_builder_is_authorized_for_test;
#[cfg(not(feature = "test-faults"))]
pub(crate) const fn unadmitted_marker_builder_is_authorized_for_test(
    _key: DraftPieceSettlementKeyV1,
) -> bool {
    false
}
pub use tree::{
    DraftPiecePrepareErrorV1, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1, canonical_empty_draft_piece_root_v1,
    canonical_empty_marker_digest_v1, canonical_empty_root_digest_v1,
    draft_piece_fragment_chain_link_v1,
};

#[cfg(feature = "test-faults")]
pub use codec::test_candidate_disposal_receipt_codec_accepts;
#[cfg(feature = "test-faults")]
pub use publication::test_abandon_fresh_reconciliation_resolution;

pub(crate) use admission::{
    DraftMarkerAdmissionCapacityCodec, DraftMarkerAdmissionCapacityFamily,
    DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
    DraftMarkerAdmissionNodesCodec, DraftMarkerAdmissionNodesFamily,
    DraftMarkerAdmissionReceiptsCodec, DraftMarkerAdmissionReceiptsFamily,
};
pub(crate) use codec::*;
pub(crate) use history::{
    DraftEditHistoryFrontiersCodec, DraftEditHistoryFrontiersFamily,
    DraftEditHistoryTransitionsCodec, DraftEditHistoryTransitionsFamily,
    DraftHistoricalRootAdoptionsCodec, canonical_history_reference_bytes, dec_history_frontier,
    dec_history_reference, dec_history_transition, enc_history_frontier, enc_history_reference,
    enc_history_transition, historical_candidate_session_is_exact,
    historical_candidate_session_is_exact_in_store,
};
pub(crate) use marker_commitment::*;
pub(crate) use marker_seal::DraftMarkerSealsCodec;
pub(crate) use persistent::*;
#[cfg(feature = "test-faults")]
pub(crate) use staging::head_digest;
pub(crate) use tree::*;

pub const DRAFT_PIECE_MAX_CHILDREN: usize = 128;
pub const DRAFT_PIECE_MAX_HEIGHT: u8 = 64;
pub const DRAFT_PIECE_PAGE_MAX_RECORDS: usize = 256;
pub const DRAFT_PIECE_PAGE_MAX_BYTES: usize = 65_536;
pub const DRAFT_MUTATION_STAGING_BATCH_MAX_PAGES: usize = 257;
pub const DRAFT_MUTATION_STAGING_BATCH_MAX_ITEMS: usize = 65_792;
pub const DRAFT_MUTATION_STAGING_BATCH_MAX_BYTES: usize = 16_842_752;
pub const DRAFT_PIECE_STAGE_MAX_RECORDS: usize = 256;
pub const DRAFT_PIECE_TEXT_LEAF_MAX_BYTES: usize = 32_768;
pub const DRAFT_PIECE_BUILD_WINDOW_MAX_PAGES: usize = 256;
const DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_BASE_READS: usize = 6;
const DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_RECEIPT_ENDPOINT_READS: usize = 1;
const DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_RECEIPT_ROOT_READS: usize = 3;
const DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_RECEIPT_READS: usize =
    DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_RECEIPT_ENDPOINT_READS
        .checked_add(DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_RECEIPT_ROOT_READS)
        .expect("staging-window receipt acquisition shape fits usize");
const DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_FIXED_READS: usize =
    DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_BASE_READS
        .checked_add(
            DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_RECEIPT_READS
                .checked_mul(2)
                .expect("staging-window receipt count fits usize"),
        )
        .expect("staging-window fixed acquisition shape fits usize");
const DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_PAGE_READS: usize = 2;
pub const DRAFT_PIECE_BUILD_WINDOW_MAX_READS: usize =
    DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_FIXED_READS
        .checked_add(
            DRAFT_PIECE_BUILD_WINDOW_ACQUISITION_PAGE_READS
                .checked_mul(DRAFT_PIECE_BUILD_WINDOW_MAX_PAGES)
                .expect("staging-window page acquisition shape fits usize"),
        )
        .expect("staging-window acquisition shape fits usize");
pub const DRAFT_PIECE_BUILD_WINDOW_MAX_ENCODED_VALUE_BYTES: usize =
    DRAFT_PIECE_BUILD_WINDOW_MAX_READS
        .checked_mul(DRAFT_PIECE_PAGE_MAX_BYTES)
        .expect("staging-window acquisition byte ceiling fits usize");
pub const DRAFT_PIECE_BUILD_WINDOW_MAX_FRAGMENTS: usize = 256;
pub const DRAFT_PIECE_BUILD_WINDOW_MAX_INSERTED_UTF8_BYTES: usize = 65_536;
