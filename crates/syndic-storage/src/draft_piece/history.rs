mod append;
mod codec;
mod records;
mod references;

pub use append::canonical_empty_draft_edit_history_v1;
pub use records::*;
pub use references::*;

pub(crate) use append::append_ordinary_draft_edit_history_v1;
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
