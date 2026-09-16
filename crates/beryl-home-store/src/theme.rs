//! Physical installed-theme repository access.

mod operations;
mod platform;
mod preparation;
mod types;
mod watcher;

pub use preparation::PreparedThemeWatchSubscription;

pub use types::{
    StableThemeFileId, StableThemeFileIdError, ThemeCommitEvidence, ThemeFileIdentity,
    ThemeFileRange, ThemeFileSelector, ThemeMutationOutcome, ThemeOperationLimits,
    ThemeOperationLimitsError, ThemeReconciliationEvidence, ThemeReconciliationOutcome,
    ThemeRepositoryError, ThemeRepositorySnapshot, ThemeRepositoryStage, ThemeWatchError,
    ThemeWatchHint, ThemeWatchLimits, ThemeWatchLimitsError, ThemeWatchSubscription,
};

pub(crate) use watcher::ThemeWatcherCoordinator;
