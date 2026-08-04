/// Why a provider-frame storage record was structurally inconsistent.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderStorageRecordError {
    #[error("provider-item build key and value item identities disagree")]
    BuildKeyMismatch,
    #[error("provider narrative-span key and value identities disagree")]
    NarrativeSpanKeyMismatch,
    #[error("provider narrative span must have matching nonempty logical and source ranges")]
    InvalidNarrativeSpan,
    #[error("provider narrative span and logical-byte frontiers must both be empty or nonempty")]
    InvalidNarrativeSummary,
    #[error("a sealed provider frame must reference ProviderItemV1 content")]
    InvalidContentEncoding,
    #[error("provider frame encoded end {frame_end} does not equal content frontier {content_end}")]
    FrameContentFrontierMismatch { frame_end: u64, content_end: u64 },
    #[error("provider frame range {start}..{end} lies outside content frontier {content_end}")]
    FrameOutsideContent {
        start: u64,
        end: u64,
        content_end: u64,
    },
    #[error(
        "ProviderItemV1 content must keep generic piece, text, atom, and marker frontiers empty"
    )]
    InvalidProviderContentSummary,
    #[error("provider stream state does not agree with its sealed frame identity")]
    StreamStateFrameMismatch,
    #[error("provider stream state does not agree with its sealed frame observation")]
    StreamStateObservationMismatch,
    #[error("provider narrative presence does not agree with the exact provider item kind")]
    NarrativePresenceMismatch,
    #[error("provider narrative content does not agree with the sealed ProviderItemV1 content")]
    NarrativeContentMismatch,
    #[error("an empty provider narrative does not carry its canonical chain seed")]
    EmptyNarrativeChainDigestMismatch,
    #[error("provider narrative frontiers do not equal the selected frame contribution")]
    NarrativeFrameFrontierMismatch,
    #[error("provider narrative frontier arithmetic overflowed")]
    NarrativeFrontierOverflow,
    #[error("provider build target does not belong to its CAS item source")]
    TargetCasItemMismatch,
    #[error("provider build prior and target content identities differ")]
    PriorContentMismatch,
    #[error("provider build prior and target CAS item identities differ")]
    PriorCasItemMismatch,
    #[error("provider build prior and target item kinds differ")]
    PriorItemKindMismatch,
    #[error("provider build target frame is not immediately after its prior frame")]
    PriorFrameOrdinalMismatch,
    #[error("provider build prior content frontier does not equal the target frame start")]
    PriorContentFrontierMismatch,
    #[error("provider build target content revision does not immediately follow its prior")]
    PriorContentRevisionMismatch,
    #[error("provider build stream state does not extend its prior state")]
    PriorStreamStateMismatch,
    #[error("provider build cumulative history support regressed")]
    HistorySupportRegression,
    #[error("an initial narrative build must use the first item-owned generation")]
    InitialNarrativeGenerationMismatch,
    #[error("a provider delta must retain its prior item-owned narrative generation")]
    AppendNarrativeGenerationMismatch,
    #[error("a provider delta is missing its prior selected narrative")]
    MissingPriorNarrative,
    #[error("a narrative provider build is missing its staged narrative frontier")]
    MissingStagedNarrative,
    #[error("a nonnarrative provider build carries a staged narrative frontier")]
    UnexpectedStagedNarrative,
    #[error("provider build staged narrative identity differs from its target")]
    StagedNarrativeIdentityMismatch,
    #[error("provider build staged narrative presence changed while advancing")]
    StagedNarrativePresenceChanged,
    #[error("provider build staged narrative span and byte frontiers disagree")]
    InvalidStagedNarrativeFrontier,
    #[error("provider build staged narrative does not equal its exact seed frontier")]
    StagedNarrativeSeedMismatch,
    #[error("provider build completed narrative frontier has the wrong chain digest")]
    StagedNarrativeChainDigestMismatch,
    #[error("a narrative completion build is missing its equality check")]
    MissingNarrativeCompletionCheck,
    #[error("a noncompletion build carries a narrative completion check")]
    UnexpectedNarrativeCompletionCheck,
    #[error("provider completion narrative source evidence disagrees with its frame")]
    InvalidNarrativeCompletionSource,
    #[error("provider completion comparison frontier is structurally invalid")]
    InvalidNarrativeComparisonFrontier,
    #[error("provider completion terminal comparison disposition is structurally invalid")]
    InvalidNarrativeCompletionDisposition,
    #[error("provider completion comparison is already terminal")]
    NarrativeCompletionAlreadyTerminal,
    #[error("a provider build without a prior frame must target the first frame")]
    InitialFrameOrdinalMismatch,
    #[error("a provider build without a prior frame must begin at encoded byte zero")]
    InitialFrameStartMismatch,
    #[error("a provider build without a prior frame must use content revision one")]
    InitialContentRevisionMismatch,
    #[error("provider build initial stream lifecycle is invalid")]
    InitialStreamStateMismatch,
    #[error("provider build {kind} frontier {actual} lies outside {minimum}..={maximum}")]
    StagedFrontierOutOfRange {
        kind: &'static str,
        minimum: u64,
        maximum: u64,
        actual: u64,
    },
    #[error("provider build chain digest does not equal the exact target digest")]
    StagedChainDigestMismatch,
    #[error("provider build lifecycle does not agree with all staged frontiers")]
    BuildLifecycleMismatch,
    #[error("a sealed provider build cannot advance")]
    BuildAlreadySealed,
    #[error("provider build {kind} frontier regressed from {previous} to {actual}")]
    StagedFrontierRegression {
        kind: &'static str,
        previous: u64,
        actual: u64,
    },
    #[error("provider-item build revision is exhausted")]
    BuildRevisionExhausted,
}
