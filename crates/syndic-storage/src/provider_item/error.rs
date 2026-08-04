/// Rejection raised by the pure provider-item grammar before persistence.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderItemValidationError {
    #[error("provider text reference range must be non-empty, got {start}..{end}")]
    InvalidTextReference { start: u64, end: u64 },
    #[error("provider text reference {start}..{end} exceeds the prior content frontier {frontier}")]
    TextReferenceBeyondFrontier { start: u64, end: u64, frontier: u64 },
    #[error("provider structured value exceeds the maximum container depth {maximum}")]
    StructuredDepthExceeded { maximum: usize },
    #[error("provider floating-point number must be finite")]
    NonFiniteNumber,
    #[error("typed MCP inline image content requires an admitted asset reference")]
    McpInlineImageRequiresAsset,
    #[error("dynamic-tool data image URL requires an admitted asset reference")]
    DynamicImageDataUrlRequiresAsset,
    #[error("dynamic-tool image locator must be a well-formed non-data absolute URI")]
    InvalidDynamicImageLocator,
    #[error("typed MCP content cannot reuse an unresolved content-type discriminator")]
    McpContentTypeReference,
    #[error("MCP inline-image asset metadata must not contain raw {field} payload")]
    McpImageMetadataContainsBytes { field: &'static str },
    #[error("submitted-user correlation must reference ComposerV1 content")]
    SubmittedContentMustBeComposer,
    #[error("provider frame ordinal must be non-zero")]
    ZeroFrameOrdinal,
    #[error("provider frame ordinal is exhausted")]
    FrameOrdinalExhausted,
    #[error("provider item stream expected frame ordinal {expected}, got {actual}")]
    FrameOrdinalConflict { expected: u64, actual: u64 },
    #[error("provider item stream state has invalid next ordinal {actual}")]
    InvalidStreamStateOrdinal { actual: u64 },
    #[error("provider item stream state lifecycle is incoherent for kind {kind:?}")]
    InvalidStreamStateLifecycle { kind: crate::ProviderItemKind },
    #[error("provider item stream changed item identity")]
    ItemIdentityMismatch,
    #[error("provider item stream changed kind from {expected:?} to {actual:?}")]
    ItemKindMismatch {
        expected: crate::ProviderItemKind,
        actual: crate::ProviderItemKind,
    },
    #[error("provider delta kind {actual:?} does not match item kind {expected:?}")]
    DeltaKindMismatch {
        expected: crate::ProviderItemKind,
        actual: crate::ProviderItemKind,
    },
    #[error("paired provider item must start before delta or completion")]
    MissingItemStart,
    #[error("provider item received a duplicate start")]
    DuplicateItemStart,
    #[error("provider item received an event after completion")]
    EventAfterCompletion,
    #[error("completion-only provider item cannot appear in a start frame")]
    CompletionOnlyItemStarted,
    #[error("provider completion timestamp {completed} precedes start timestamp {started}")]
    CompletionBeforeStart { started: u64, completed: u64 },
    #[error("completed provider frame retains an in-progress item status")]
    CompletionStatusInProgress,
    #[error("provider frame byte or logical-text arithmetic overflowed")]
    FrameLengthOverflow,
    #[error("provider encoded frame range must be non-empty, got {start}..{end}")]
    InvalidFrameRange { start: u64, end: u64 },
    #[error("provider text span ranges must be non-empty and equally sized")]
    InvalidFrameTextSpan,
    #[error("provider text span ordinal does not match its frame")]
    FrameTextSpanOrdinalMismatch,
    #[error("provider text spans are not contiguous at logical byte {expected}")]
    FrameTextSpanFrontierConflict { expected: u64 },
    #[error("provider frame text-span count or logical frontier disagrees with its reference")]
    FrameTextSpanSummaryMismatch,
}
