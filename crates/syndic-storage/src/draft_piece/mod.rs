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
mod tree;

pub use beryl_model::DraftMarkerCommitmentV1;
pub use builder_model::*;
pub use history::*;
pub use marker_commitment::canonical_empty_draft_marker_commitment_v1;
pub use marker_seal::*;
pub use materializer::*;
pub use model::*;
pub use mutation::{PreparedDraftPieceAdvanceV1, PreparedDraftPieceEditV1};
pub use publication::{
    DraftEditorCandidatePublicationCommandErrorV1, PreparedDraftEditorCandidatePublicationV1,
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
pub use tree::{
    DraftPiecePrepareErrorV1, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1, canonical_empty_draft_piece_root_v1,
    canonical_empty_marker_digest_v1, canonical_empty_root_digest_v1,
    draft_piece_fragment_chain_link_v1,
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
pub const DRAFT_PIECE_BUILD_WINDOW_MAX_READS: usize = 520;
pub const DRAFT_PIECE_BUILD_WINDOW_MAX_ENCODED_VALUE_BYTES: usize = 34_078_720;
pub const DRAFT_PIECE_BUILD_WINDOW_MAX_FRAGMENTS: usize = 256;
pub const DRAFT_PIECE_BUILD_WINDOW_MAX_INSERTED_UTF8_BYTES: usize = 65_536;
