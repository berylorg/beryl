mod adoption;
mod append;
mod codec;
mod records;
mod references;
mod retention;
mod witness;

pub use append::canonical_empty_draft_edit_history_v1;
pub use records::*;
pub use references::*;
pub use witness::*;

pub use adoption::*;
pub(crate) use adoption::{
    DraftHistoricalRootAdoptionsCodec, DraftHistoricalRootAdoptionsFamily,
    historical_candidate_session_is_exact, historical_candidate_session_is_exact_in_store,
};
#[cfg(feature = "test-faults")]
pub(crate) use append::{
    alternative_ordinary_draft_edit_history_for_test, draft_edit_history_overflow_errors_for_test,
    draft_edit_history_stored_charge_components_for_test,
};
pub(crate) use codec::{
    DraftEditHistoryFrontiersCodec, DraftEditHistoryFrontiersFamily,
    DraftEditHistoryTransitionsCodec, DraftEditHistoryTransitionsFamily,
    canonical_history_reference_bytes, dec_history_frontier, dec_history_reference,
    dec_history_transition, enc_history_frontier, enc_history_reference, enc_history_transition,
};
pub(crate) use retention::{
    DraftEditHistoryRetentionErrorV1, append_historical_draft_edit_history_with_retention_v1,
    append_ordinary_draft_edit_history_with_retention_v1,
    authenticate_draft_edit_history_frontier_v1, draft_edit_history_frontier_is_authenticated_v1,
    ordinary_draft_edit_history_adoption_is_locally_exact,
};
#[cfg(feature = "test-faults")]
pub(crate) use retention::{
    draft_edit_history_accounting_corruption_for_test,
    draft_edit_history_availability_corruption_for_test,
    draft_edit_history_first_transition_gap_for_test, draft_edit_history_no_head_gap_for_test,
    draft_edit_history_wrong_head_root_for_test,
};
