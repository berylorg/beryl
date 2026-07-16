use thiserror::Error;

use super::GenerationExhausted;

/// Why draft persistence could not safely advance local publication state.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DraftPersistenceError {
    #[error(transparent)]
    GenerationExhausted(#[from] GenerationExhausted),
    #[error("reconciliation seed belongs to another Beryl home")]
    ForeignHome,
    #[error("reconciliation seed belongs to another thread or draft")]
    ForeignDraft,
    #[error("reconciliation seed changes immutable draft ownership")]
    ChangedImmutableDraft,
    #[error("reconciliation seed belongs to an older home generation")]
    StaleHomeGeneration,
    #[error("draft persistence is not suspended")]
    NotSuspended,
    #[error("editor update timestamp regressed behind current local or durable state")]
    RegressedEditTimestamp,
    #[error("ambiguous save did not reconcile to its whole old or whole new state")]
    UnexplainedAmbiguousState,
    #[error("conflicting save did not reconcile to its whole prior or requested next state")]
    UnexplainedConflictState,
}
