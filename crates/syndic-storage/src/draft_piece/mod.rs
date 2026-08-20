mod codec;
mod history;
mod materializer;
mod model;
mod mutation;
mod persistent;
mod read;
mod session;
mod tree;

pub use history::*;
pub use materializer::*;
pub use model::*;
pub use mutation::{PreparedDraftPieceAdvanceV1, PreparedDraftPieceEditV1};
pub use read::DraftPieceCommandReconciliationErrorV1;
pub use session::{
    DraftEditorCandidateSessionCommandErrorV1, PreparedDraftEditorCandidateSessionOpenV1,
};
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
    canonical_history_reference_bytes, dec_history_frontier, dec_history_reference,
    dec_history_transition, enc_history_frontier, enc_history_reference, enc_history_transition,
};
pub(crate) use persistent::*;
pub(crate) use tree::*;

pub const DRAFT_PIECE_MAX_CHILDREN: usize = 128;
pub const DRAFT_PIECE_MAX_HEIGHT: u8 = 64;
pub const DRAFT_PIECE_PAGE_MAX_RECORDS: usize = 256;
pub const DRAFT_PIECE_PAGE_MAX_BYTES: usize = 65_536;
pub const DRAFT_PIECE_STAGE_MAX_RECORDS: usize = 256;
pub const DRAFT_PIECE_TEXT_LEAF_MAX_BYTES: usize = 32_768;
