//! Non-GPUI persistence coordination for one exact durable current draft.
//!
//! The service owns scheduling and asynchronous-result correlation only. The
//! caller-facing editor payload remains its local user state, while one exact
//! durable base is retained solely for dirty comparison and recovery. Storage
//! execution stays behind Syndic's typed current-draft/update boundary.

mod error;
mod executor;
mod generation;
mod model;
mod outcome;
mod request;
mod seed;
mod service;
mod time;

pub use error::DraftPersistenceError;
pub use executor::{DraftSaveExecution, DraftSaveExecutionFailure, execute_draft_save};
pub use generation::{
    DraftBindingGeneration, DraftEditGeneration, DraftRequestGeneration, DraftTimerGeneration,
    GenerationExhausted,
};
pub use model::{DraftPersistenceBinding, DraftPersistenceSeed};
pub use outcome::{
    DraftAutosaveAction, DraftCompletionAction, DraftFlushAction, DraftKnownUnchanged,
    DraftReconciliationAction, DraftSaveOutcome, DraftSuspensionCause,
};
pub use request::{DraftSaveRequest, DraftSaveToken};
pub use seed::{DraftSeedReadError, read_draft_persistence_seed};
pub use service::DraftPersistenceService;
pub use time::{
    DEFAULT_AUTOSAVE_SECONDS, DraftAutosaveInterval, DraftAutosaveIntervalError,
    DraftAutosavePublication, DraftAutosavePublicationAction, DraftPersistenceTime,
    MAX_AUTOSAVE_SECONDS, MIN_AUTOSAVE_SECONDS,
};

pub(crate) use model::{DurableDraftBase, ImmutableDraftShape};
