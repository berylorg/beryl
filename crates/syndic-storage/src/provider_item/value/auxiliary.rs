use super::super::ProviderTextV1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderWebSearchActionV1 {
    Search {
        query: Option<ProviderTextV1>,
        queries: Option<Vec<ProviderTextV1>>,
    },
    OpenPage {
        url: Option<ProviderTextV1>,
    },
    FindInPage {
        url: Option<ProviderTextV1>,
        pattern: Option<ProviderTextV1>,
    },
    /// Pinned backend `serde(other)` marker; it cannot support complete history.
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderWebSearchV1 {
    pub query: ProviderTextV1,
    pub action: Option<ProviderWebSearchActionV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderImageViewV1 {
    pub path: ProviderTextV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderSleepV1 {
    pub duration_ms: u64,
}

/// Closed status vocabulary emitted by the pinned standalone image generator.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderImageGenerationStatusV1 {
    InProgress,
    Failed,
    Completed,
}

/// Standalone `image_gen.imagegen` metadata after bounded ingress removed `result`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderImageGenerationV1 {
    pub status: ProviderImageGenerationStatusV1,
    pub revised_prompt: Option<ProviderTextV1>,
    pub saved_path: Option<ProviderTextV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEnteredReviewModeV1 {
    pub review: ProviderTextV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderExitedReviewModeV1 {
    pub review: ProviderTextV1,
}
