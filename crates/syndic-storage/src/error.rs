use thiserror::Error;

use crate::{ProviderRevision, ResourceId, ThreadViewId, TranscriptViewPosition, TurnId};

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("fjall storage error: {0}")]
    Engine(#[from] fjall::Error),

    #[error("json encoding error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid {kind} id {value:?}")]
    InvalidId { kind: &'static str, value: String },

    #[error("missing {kind} {id}")]
    Missing { kind: &'static str, id: String },

    #[error("source event {event_id} conflicts with an existing durable event")]
    SourceEventConflict { event_id: String },

    #[error(
        "source event sequence conflict for turn {turn_id}: expected {expected}, received {received}"
    )]
    SourceEventSequence {
        turn_id: TurnId,
        expected: u64,
        received: u64,
    },

    #[error("requested limit {requested} exceeds maximum {max}")]
    LimitExceeded { requested: usize, max: usize },

    #[error("resource range {range:?} is out of bounds for {resource_id} length {byte_len}")]
    ResourceRangeOutOfBounds {
        resource_id: ResourceId,
        range: std::ops::Range<u64>,
        byte_len: u64,
    },

    #[error("resource range length {requested} exceeds maximum {max}")]
    ResourceRangeTooLarge { requested: u64, max: u64 },

    #[error("stale revision for view {view_id}: observed {observed:?}, current {current:?}")]
    StaleRevision {
        view_id: ThreadViewId,
        observed: Option<ProviderRevision>,
        current: ProviderRevision,
    },

    #[error("stale revision for {target}: observed {observed:?}, current {current:?}")]
    StaleRecordRevision {
        target: String,
        observed: Option<ProviderRevision>,
        current: ProviderRevision,
    },

    #[error("missing or invalid cursor {cursor}")]
    MissingCursor { cursor: String },

    #[error("invalid transcript page anchor at position {position:?}")]
    InvalidPageAnchor {
        position: Option<TranscriptViewPosition>,
    },

    #[error("source event payload contains secret-like field {field}")]
    SecretLikeField { field: String },
}
