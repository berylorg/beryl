include!("phase154_durable_builder/support.rs");

#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{reset_syndic_point_read_count, syndic_point_read_count};
use syndic_storage::{
    DRAFT_COMPOSER_INPUT_MAX_BYTES, DRAFT_COMPOSER_READ_MAX_RECORDS,
    DRAFT_COMPOSER_RESIDENT_MAX_BYTES, DRAFT_COMPOSER_WRITE_MAX_RECORDS,
    DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS, DRAFT_MUTATION_STAGING_BATCH_MAX_BYTES,
    DRAFT_MUTATION_STAGING_BATCH_MAX_ITEMS, DRAFT_MUTATION_STAGING_BATCH_MAX_PAGES,
    DRAFT_PIECE_BUILD_WINDOW_MAX_ENCODED_VALUE_BYTES, DRAFT_PIECE_BUILD_WINDOW_MAX_FRAGMENTS,
    DRAFT_PIECE_BUILD_WINDOW_MAX_INSERTED_UTF8_BYTES, DRAFT_PIECE_BUILD_WINDOW_MAX_PAGES,
    DRAFT_PIECE_BUILD_WINDOW_MAX_READS, DRAFT_PIECE_MAX_HEIGHT, DRAFT_PIECE_PAGE_MAX_BYTES,
    DRAFT_PIECE_PAGE_MAX_RECORDS, DRAFT_PIECE_STAGE_MAX_RECORDS, DraftComposerBuildKeyV1,
    DraftComposerFormatV1, DraftComposerMaterializationOperationIdV1,
    DraftComposerMaterializationStatusV1, DraftEditHistoryPolicyV1,
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidatePublicationEvidenceV1,
    DraftEditorCandidatePublicationOutcomeV1, DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1, DraftHistoricalRootDirectionV1,
    DraftHistoricalRootSelectionIntentV1, DraftHistoricalRootSelectionV1,
    DraftMarkerSealOperationIdV1, DraftMarkerSealRequestV1, DraftMarkerSealStatusV1,
    DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1, DraftPieceMarkerEdgeProofRequestV1,
    DraftPieceMarkerEdgeProofV1, DraftPieceMarkerScopeV1, DraftRootHistoryPairV1,
};

const LARGE_CHUNK_BYTES: usize = 32_768;
const LARGE_CHUNK_COUNT: usize = 96;
const LARGE_DRAFT_BYTES: u64 = (LARGE_CHUNK_BYTES * LARGE_CHUNK_COUNT) as u64;
const MARKER_COUNT: usize = 128;
const MARKER_PAGE: usize = 31;

include!("phase184_large_draft_storage/support.rs");

#[path = "phase184_large_draft_storage/large_text_history.rs"]
mod large_text_history;
#[path = "phase184_large_draft_storage/marker_acquisition.rs"]
mod marker_acquisition;
