use std::{error::Error, fmt};

use beryl_home_store::{DomainCallbackError, DomainCallbackSource, ReadError};
use beryl_model::SyndicDraftMarkerId;

/// Why a bounded record value could not be constructed.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SyndicRecordError {
    #[error("{kind} exceeds the addressable count or byte range")]
    LengthOverflow { kind: &'static str },
    #[error("{kind} must be a non-empty ordered byte range, got {start}..{end}")]
    InvalidByteRange {
        kind: &'static str,
        start: u64,
        end: u64,
    },
    #[error("{kind} must be non-zero")]
    ZeroValue { kind: &'static str },
    #[error("Markdown heading level must be in 1..=6, got {level}")]
    InvalidMarkdownHeadingLevel { level: u8 },
    #[error("projection source range has {range_bytes} bytes but payload has {source_bytes}")]
    ProjectionSourceLengthMismatch { range_bytes: u64, source_bytes: u64 },
    #[error("{kind} maps {logical_bytes} logical bytes to {encoded_bytes} encoded bytes")]
    MappedByteLengthMismatch {
        kind: &'static str,
        logical_bytes: u64,
        encoded_bytes: u64,
    },
    #[error("only code and table blocks may own a textual projection resource")]
    InvalidProjectionResourceKind,
    #[error(
        "resource preview {preview_start}..{preview_end} exceeds {resource_bytes} resource bytes"
    )]
    InvalidResourcePreviewRange {
        resource_bytes: u64,
        preview_start: u64,
        preview_end: u64,
    },
    #[error("resource kind and structural metadata disagree")]
    InvalidResourceStructure,
    #[error("chunked content has an invalid or noncanonical encoding")]
    InvalidContentEncoding,
    #[error("{kind} must not exceed {maximum} bytes, got {actual}")]
    BytesTooLong {
        kind: &'static str,
        maximum: usize,
        actual: usize,
    },
    #[error("{kind} must not exceed {maximum} entries, got {actual}")]
    TooManyEntries {
        kind: &'static str,
        maximum: usize,
        actual: usize,
    },
    #[error("{kind} repeats image marker {marker_id}")]
    DuplicateImageMarker {
        kind: &'static str,
        marker_id: SyndicDraftMarkerId,
    },
    #[error("source-event CAS turn and item identities must be supplied together")]
    SourceIdentityMismatch,
    #[error("provider item kind and durable disposition disagree")]
    InvalidProviderItemDisposition,
    #[error("provider item lifecycle and source frontier disagree")]
    InvalidProviderItemLifecycle,
    #[error("turn capture aggregates or incomplete reason disagree")]
    InvalidTurnCaptureFrontier,
    #[error("context-compaction authority or fixed observation frontiers disagree")]
    InvalidCompactionOperation,
    #[error("thread image-label inherited frontier exceeds its current frontier")]
    InvalidImageLabelFrontier,
    #[error("draft image-label protection head is invalid")]
    InvalidDraftImageLabelProtection,
    #[error("activity-query running-row count exceeds its logical row count")]
    InvalidActivityQueryFrontier,
    #[error("activity-query ordering and provider lifecycle disagree")]
    InvalidActivityQueryLifecycle,
    #[error("accepted-route interval is empty, reversed, or only partially present")]
    InvalidAcceptedRouteInterval,
    #[error("accepted-route interval and checked aggregates disagree")]
    InvalidAcceptedRouteAggregates,
    #[error("selected accepted-route generation exceeds its allocator high-water")]
    InvalidAcceptedRouteSelection,
    #[error("accepted-input admission source and replacement draft identities collide")]
    AcceptedInputAdmissionDraftCollision,
    #[error("accepted-input identity is not derived from its admission source draft")]
    AcceptedInputIdentityMismatch,
    #[error("provider-observation build, validator, or chunk frontier disagrees")]
    InvalidProviderObservationFrontier,
    #[error("{kind} must not be empty")]
    Empty { kind: &'static str },
    #[error("{kind} contains a NUL byte at offset {index}")]
    NulByte { kind: &'static str, index: usize },
}

/// Why one public typed read could not publish a coherent bounded result.
#[derive(Debug)]
pub enum SyndicReadError {
    Read(ReadError),
    ConcurrentChange {
        operation: &'static str,
    },
    Invariant(&'static str),
    ContentTextRequiresSealed,
    ContentTextContainsImageMarkers {
        actual: u64,
    },
    InvalidContentTextOffset {
        content_bytes: u64,
        offset: u64,
    },
    InvalidContentTextSegmentCursor {
        piece_count: u64,
        after_piece: u64,
    },
    InvalidContentTextSegmentOffset {
        segment_start: u64,
        segment_end: u64,
        offset: u64,
    },
    InvalidContentTextReadLimit {
        maximum: usize,
        actual: usize,
    },
    ContentTextReadLimitTooSmall {
        offset: u64,
        actual: usize,
    },
    InvalidResourceRange {
        resource_bytes: u64,
        start: u64,
        end: u64,
    },
    InvalidResourceReadLimit {
        maximum: usize,
        actual: usize,
    },
    ResourceHasNoTextBacking,
    CaptureItemHasNoTextContent,
    CatalogSummaryRevisionExhausted,
    StaleThreadLineage,
    InvalidThreadLineageCursor,
    StaleActivityQuery,
    InvalidActivityQueryCursor,
    ActivityQueryIsStale,
    StaleAcceptedRoute,
    InvalidAcceptedRouteCursor,
    StaleAcceptedReadySourceScan,
    InvalidAcceptedReadySourceCursor,
    StaleAcceptedReadyCandidateSource,
    InvalidAcceptedReadyCandidateCursor,
    StaleAcceptedNextSourceScan,
    InvalidAcceptedNextSourceCursor,
    StaleAcceptedNextCandidateSource,
    InvalidAcceptedNextCandidateSource,
    InvalidAcceptedNextCandidateCursor,
    InvalidDeliveryRecoveryStartupCursor,
    StaleRecoveredPendingScan,
    InvalidRecoveredPendingCursor,
}

impl fmt::Display for SyndicReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::ConcurrentChange { operation } => {
                write!(formatter, "Syndic changed concurrently during {operation}")
            }
            Self::Invariant(message) => formatter.write_str(message),
            Self::ContentTextRequiresSealed => {
                formatter.write_str("logical content text reads require sealed content")
            }
            Self::ContentTextContainsImageMarkers { actual } => write!(
                formatter,
                "logical content text reads require marker-free content, got {actual} markers"
            ),
            Self::InvalidContentTextOffset {
                content_bytes,
                offset,
            } => write!(
                formatter,
                "logical content text offset {offset} is not a UTF-8 boundary in {content_bytes} bytes"
            ),
            Self::InvalidContentTextSegmentCursor {
                piece_count,
                after_piece,
            } => write!(
                formatter,
                "content text segment cursor must name one image marker in {piece_count} pieces, got {after_piece}"
            ),
            Self::InvalidContentTextSegmentOffset {
                segment_start,
                segment_end,
                offset,
            } => write!(
                formatter,
                "logical content text offset {offset} lies outside segment {segment_start}..{segment_end}"
            ),
            Self::InvalidContentTextReadLimit { maximum, actual } => write!(
                formatter,
                "logical content text read limit must be in 1..={maximum} bytes, got {actual}"
            ),
            Self::ContentTextReadLimitTooSmall { offset, actual } => write!(
                formatter,
                "logical content text read limit {actual} cannot include the UTF-8 scalar at offset {offset}"
            ),
            Self::InvalidResourceRange {
                resource_bytes,
                start,
                end,
            } => write!(
                formatter,
                "resource range {start}..{end} lies outside {resource_bytes} bytes"
            ),
            Self::InvalidResourceReadLimit { maximum, actual } => write!(
                formatter,
                "resource read limit must be in 1..={maximum} bytes, got {actual}"
            ),
            Self::ResourceHasNoTextBacking => {
                formatter.write_str("resource does not have canonical text backing")
            }
            Self::CaptureItemHasNoTextContent => {
                formatter.write_str("captured item does not have canonical text content")
            }
            Self::CatalogSummaryRevisionExhausted => {
                formatter.write_str("thread-catalog summary revision is exhausted")
            }
            Self::StaleThreadLineage => formatter.write_str("thread-lineage revision is stale"),
            Self::InvalidThreadLineageCursor => {
                formatter.write_str("thread-lineage cursor does not belong to this query")
            }
            Self::StaleActivityQuery => formatter.write_str("activity-query revision is stale"),
            Self::InvalidActivityQueryCursor => {
                formatter.write_str("activity-query cursor does not belong to this query")
            }
            Self::ActivityQueryIsStale => formatter.write_str("activity-query projection is stale"),
            Self::StaleAcceptedRoute => {
                formatter.write_str("accepted-route generation revision is stale")
            }
            Self::InvalidAcceptedRouteCursor => formatter
                .write_str("accepted-route cursor does not belong to this generation revision"),
            Self::StaleAcceptedReadySourceScan => {
                formatter.write_str("accepted-ready source scan revision is stale")
            }
            Self::InvalidAcceptedReadySourceCursor => formatter
                .write_str("accepted-ready source cursor does not belong to this scan revision"),
            Self::StaleAcceptedReadyCandidateSource => {
                formatter.write_str("accepted-ready candidate source is stale")
            }
            Self::InvalidAcceptedReadyCandidateCursor => formatter.write_str(
                "accepted-ready candidate cursor does not belong to this source revision",
            ),
            Self::StaleAcceptedNextSourceScan => {
                formatter.write_str("accepted-next source scan revision is stale")
            }
            Self::InvalidAcceptedNextSourceCursor => formatter
                .write_str("accepted-next source cursor does not belong to this scan revision"),
            Self::StaleAcceptedNextCandidateSource => {
                formatter.write_str("accepted-next candidate source is stale")
            }
            Self::InvalidAcceptedNextCandidateSource => formatter.write_str(
                "accepted-next candidate source is not the earliest current source for its thread",
            ),
            Self::InvalidAcceptedNextCandidateCursor => formatter.write_str(
                "accepted-next candidate cursor does not belong to this source revision",
            ),
            Self::InvalidDeliveryRecoveryStartupCursor => {
                formatter.write_str("delivery-recovery startup cursor belongs to another home")
            }
            Self::StaleRecoveredPendingScan => {
                formatter.write_str("recovered-pending scan revision is stale")
            }
            Self::InvalidRecoveredPendingCursor => formatter
                .write_str("recovered-pending cursor does not belong to this scan revision"),
        }
    }
}

impl Error for SyndicReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::ConcurrentChange { .. }
            | Self::Invariant(_)
            | Self::ContentTextRequiresSealed
            | Self::ContentTextContainsImageMarkers { .. }
            | Self::InvalidContentTextOffset { .. }
            | Self::InvalidContentTextSegmentCursor { .. }
            | Self::InvalidContentTextSegmentOffset { .. }
            | Self::InvalidContentTextReadLimit { .. }
            | Self::ContentTextReadLimitTooSmall { .. }
            | Self::InvalidResourceRange { .. }
            | Self::InvalidResourceReadLimit { .. }
            | Self::ResourceHasNoTextBacking
            | Self::CaptureItemHasNoTextContent
            | Self::CatalogSummaryRevisionExhausted => None,
            Self::StaleThreadLineage
            | Self::InvalidThreadLineageCursor
            | Self::StaleActivityQuery
            | Self::InvalidActivityQueryCursor
            | Self::ActivityQueryIsStale
            | Self::StaleAcceptedRoute
            | Self::InvalidAcceptedRouteCursor
            | Self::StaleAcceptedReadySourceScan
            | Self::InvalidAcceptedReadySourceCursor
            | Self::StaleAcceptedReadyCandidateSource
            | Self::InvalidAcceptedReadyCandidateCursor
            | Self::StaleAcceptedNextSourceScan
            | Self::InvalidAcceptedNextSourceCursor
            | Self::StaleAcceptedNextCandidateSource
            | Self::InvalidAcceptedNextCandidateSource
            | Self::InvalidAcceptedNextCandidateCursor
            | Self::InvalidDeliveryRecoveryStartupCursor
            | Self::StaleRecoveredPendingScan
            | Self::InvalidRecoveredPendingCursor => None,
        }
    }
}

impl From<ReadError> for SyndicReadError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

/// Independent recovery ceiling that made an exact projection unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryBudgetKind {
    /// Maximum number of ordered canonical recovery items.
    ItemCount,
    /// Maximum total canonical UTF-8 payload bytes.
    Utf8Bytes,
}

/// Why storage could not prepare one exact recovery-item projection.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryProjectionError {
    #[error("exact selected-model context-window metadata is missing")]
    MissingModelContextWindow,
    #[error("selected-model context-window metadata must be nonzero")]
    ZeroModelContextWindow,
    #[error("the supplied selected-path proof is not the thread's exact current selected path")]
    StaleSelectedPath,
    #[error("the selected tail is not one pending ordinary-user turn")]
    CurrentTailNotPendingOrdinaryUser,
    #[error("required recovery history is missing its {record} record")]
    MissingHistory { record: &'static str },
    /// Required history is not recovery-complete.
    ///
    /// Recovery-complete includes `Complete`, `Interrupted`, and `Failed` turns only when their
    /// finalized item frontier equals their item frontier. It explicitly excludes `Incomplete`.
    #[error("required recovery history is not recovery-complete: {reason}")]
    IncompleteHistory { reason: &'static str },
    #[error("required recovery history has no supported lossless shape: {reason}")]
    UnsupportedHistory { reason: &'static str },
    #[error("required recovery history contains unsupported media: {reason}")]
    MediaHistory { reason: &'static str },
    #[error("required recovery history contains an empty canonical item")]
    EmptyHistoryItem,
    #[error(
        "recovery {kind:?} budget is {maximum}, but the complete prefix requires at least {actual}"
    )]
    BudgetOverflow {
        kind: RecoveryBudgetKind,
        maximum: u64,
        actual: u64,
    },
    #[error("Syndic changed concurrently during recovery projection preparation")]
    ConcurrentChange,
    #[error("the recovery cursor has already emitted its terminal sequence page")]
    CursorTerminal,
    #[error("recovery cursor page limit must be nonzero, got {actual}")]
    InvalidCursorPageLimit { actual: usize },
    #[error(
        "recovery cursor page limit {actual} at item offset {offset} cannot fit the next UTF-8 scalar"
    )]
    CursorPageLimitTooSmall { offset: u64, actual: usize },
    #[error("recovery cursor replay disagreed with its compact proof: {reason}")]
    CursorMismatch { reason: &'static str },
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error("recovery projection invariant failed: {0}")]
    Invariant(&'static str),
}

impl From<SyndicReadError> for RecoveryProjectionError {
    fn from(source: SyndicReadError) -> Self {
        match source {
            SyndicReadError::Read(source) => Self::Read(source),
            SyndicReadError::ConcurrentChange { .. } => Self::ConcurrentChange,
            SyndicReadError::Invariant(message) => Self::Invariant(message),
            SyndicReadError::ContentTextRequiresSealed
            | SyndicReadError::ContentTextContainsImageMarkers { .. }
            | SyndicReadError::InvalidContentTextOffset { .. }
            | SyndicReadError::InvalidContentTextSegmentCursor { .. }
            | SyndicReadError::InvalidContentTextSegmentOffset { .. }
            | SyndicReadError::InvalidContentTextReadLimit { .. }
            | SyndicReadError::ContentTextReadLimitTooSmall { .. }
            | SyndicReadError::InvalidResourceRange { .. }
            | SyndicReadError::InvalidResourceReadLimit { .. }
            | SyndicReadError::ResourceHasNoTextBacking
            | SyndicReadError::CaptureItemHasNoTextContent
            | SyndicReadError::CatalogSummaryRevisionExhausted
            | SyndicReadError::StaleThreadLineage
            | SyndicReadError::InvalidThreadLineageCursor
            | SyndicReadError::StaleActivityQuery
            | SyndicReadError::InvalidActivityQueryCursor
            | SyndicReadError::ActivityQueryIsStale
            | SyndicReadError::StaleAcceptedRoute
            | SyndicReadError::InvalidAcceptedRouteCursor
            | SyndicReadError::StaleAcceptedReadySourceScan
            | SyndicReadError::InvalidAcceptedReadySourceCursor
            | SyndicReadError::StaleAcceptedReadyCandidateSource
            | SyndicReadError::InvalidAcceptedReadyCandidateCursor
            | SyndicReadError::StaleAcceptedNextSourceScan
            | SyndicReadError::InvalidAcceptedNextSourceCursor
            | SyndicReadError::StaleAcceptedNextCandidateSource
            | SyndicReadError::InvalidAcceptedNextCandidateSource
            | SyndicReadError::InvalidAcceptedNextCandidateCursor
            | SyndicReadError::InvalidDeliveryRecoveryStartupCursor
            | SyndicReadError::StaleRecoveredPendingScan
            | SyndicReadError::InvalidRecoveredPendingCursor => Self::Invariant(
                "a recovery read unexpectedly used a public content/resource range boundary",
            ),
        }
    }
}

#[derive(Debug)]
pub(crate) enum SyndicValidationError {
    Read(ReadError),
    Invariant(&'static str),
}

impl fmt::Display for SyndicValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for SyndicValidationError {}

impl From<ReadError> for SyndicValidationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl DomainCallbackError for SyndicValidationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}
