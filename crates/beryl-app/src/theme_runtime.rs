//! Process-wide orchestration and publication of immutable theme appearances.
//!
//! Repository access, parsing, validation, resolution, and mutation remain in
//! `beryl-state`. [`ThemeRuntime`] composes that typed state service with one
//! bounded change subscription and the pure [`AppearanceCoordinator`]. All
//! appearances cross a fallible-prepare/infallible-commit window barrier.
//! A confirmed durable base survives adapter rejection and can be retried
//! without reinterpreting the durable operation or advancing current appearance.
//! External repository candidates are discarded on adapter rejection and must
//! be reread before another publication attempt.

mod adapter;
mod coordinator;
mod error;
mod identity;
mod publication;
mod service;

pub use adapter::{AppearanceWindowAdapter, PreparedWindowAppearance, WindowAdapterId};
pub use coordinator::{
    AppearanceCoordinator, AppearanceCoordinatorConfig, AppearanceDiagnostics,
    DurablePublicationOutcome, DurablePublicationRequest, DurableRetryOutcome,
    PreparedPreviewAppearance, PreviewDiagnostic, PreviewPublicationRequest,
    PreviewPublicationResult, StopPreviewResult,
};
pub use error::{
    AdapterFailureClass, AdapterRegistrationError, DurablePublicationError, GenerationExhausted,
    PreviewPublicationError, PreviewSequenceExhausted, PreviewSourceError, PublicationFailureClass,
    StalePublicationReason, WindowEpochExhausted,
};
pub use identity::{
    AppearanceGeneration, AppearanceGenerationNumber, AppearancePublication,
    DurablePublicationIdentity, PreviewCandidateIdentity, PreviewSequence, PreviewSource,
    PreviewSourceIdentity, PreviewSourceKind, WindowSetEpoch,
};
pub use service::{
    ConfirmedSettingsTheme, RepositoryAppearanceResult, SettingsThemeOutcome, SettingsThemeResult,
    ThemeRepositoryReconciliationResult, ThemeRepositoryRequest, ThemeRepositoryRequestOrigin,
    ThemeRepositoryRequestResult, ThemeRuntime, ThemeRuntimeConfig, ThemeRuntimeDiagnostics,
    ThemeRuntimeFailureClass, ThemeRuntimeStartError, ThemeWatchDrainResult,
};
