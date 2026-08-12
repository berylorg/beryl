use std::{error::Error, fmt};

mod command;
mod document;
mod execution;
mod identity;
mod manifest;
mod model;
mod physical;
mod repository;
mod resolver;
mod runtime;
mod schema;
mod service;
mod startup;

pub use command::{
    DeleteTheme, InstallTheme, RenameTheme, ReorderTheme, SaveTheme, SaveThemeAs,
    THEME_RECONCILIATION_MAX_DOCUMENTS, THEME_REFERENCE_GUARD_MAX_ITEMS, ThemeCommandError,
    ThemeDeleteGuard, ThemeDocumentDraft, ThemeReconciliation, ThemeReferenceSnapshot,
    ThemeRepositoryCommand, ThemeRepositoryCommit, UpdateTheme,
};
pub(crate) use command::{ThemeNaturalRepositoryIdentity, ThemeReconciliationDescriptor};
pub use document::{
    THEME_DOCUMENT_MAX_BYTES, THEME_DOCUMENT_MAX_LINE_BYTES, THEME_DOCUMENT_MAX_LINES,
    THEME_DOCUMENT_MAX_OUTPUT_BYTES, THEME_DOCUMENT_MAX_PROPERTY_ENTRIES, THEME_DOCUMENT_MAX_ROLES,
    THEME_DOCUMENT_NAME_MAX_BYTES, THEME_DOCUMENT_SCHEMA_VERSION, ThemeDocument,
    ThemeDocumentError, ThemeParseMode,
};
pub use execution::{
    ThemeCommandFactError, ThemeIndeterminateOperation, ThemeReferenceSnapshotProvider,
    ThemeReferenceSnapshotUnavailable, ThemeRepositoryExecutionError,
    ThemeRepositoryOperationFailure, ThemeRepositoryOperationOutcome,
    ThemeRepositoryOperationStage,
};
pub use identity::{
    InstalledThemeId, ThemeDocumentDigest, ThemeDocumentIdentity, ThemeDocumentRevision,
    ThemeDraftIdentity, ThemeDraftRevision, ThemeHomeIdentity, ThemeManifestContentIdentity,
    ThemeManifestGeneration, ThemeManifestIdentity, ThemeSettingsIdentity,
};
pub use manifest::{
    THEME_MANIFEST_HEADER_MAX_BYTES, THEME_MANIFEST_LINE_MAX_BYTES,
    THEME_MANIFEST_PAGE_MAX_ENCODED_BYTES, THEME_MANIFEST_SCHEMA_VERSION, ThemeManifestDecodeError,
    ThemeManifestEncodeError, ThemeManifestHeader, ThemeManifestLimit, ThemeManifestReadLimits,
};
pub(crate) use manifest::{ThemeManifestDecoder, ThemeManifestEncoder, ThemeManifestEncoding};
pub use model::*;
pub use repository::{
    InstalledThemeSelection, InstalledThemeSummary, THEME_DOCUMENT_RANGE_MAX_BYTES,
    THEME_MANIFEST_PAGE_MAX_DECODED_BYTES, THEME_MANIFEST_PAGE_MAX_ITEMS, THEME_NAME_MAX_BYTES,
    ThemeDocumentRange, ThemeFreshnessError, ThemeManifestCursor, ThemeManifestPage, ThemeName,
    ThemePageError, ThemePageLimits, ThemeRangeError, ThemeRepositoryService,
};
pub use resolver::{
    THEME_VALIDATION_MAX_DIAGNOSTICS, ThemeDiagnostic, ThemeDiagnosticKind, ThemeResolver,
    ThemeValidationDiagnostics, builtin_fallback_appearance,
};
pub use runtime::{THEME_RETAINED_OPERATION_MAX, ThemeServiceDiagnostics};
pub use schema::{
    CANONICAL_THEME_PROPERTY_ENTRY_COUNT_MAX, CANONICAL_THEME_ROLE_COUNT, THEME_SOURCE_KEYWORDS,
    canonical_theme_schema,
};
pub use service::{
    ThemeChangeHint, ThemeChangeSubscription, ThemeChangeSubscriptionError, ThemeDocumentLoadError,
    ThemeManifestSession, ThemeObservedDocument, ThemeRepositoryLoadError,
    ThemeRepositoryObservation, ThemeService, ThemeServiceError,
};
pub use startup::{
    BuiltinFallback, PreparedThemeAppearance, ThemeAppearanceSource, ThemeLiveEditFailure,
    ThemeLiveEditOutcome, ThemeLoadFailure, ThemeRefreshInput, ThemeStartupError,
    ThemeStartupOutcome,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeIdentityError {
    InvalidInstalledThemeId,
    InvalidThemeName,
    ServiceInstanceExhausted,
    RevisionExhausted(&'static str),
}

impl fmt::Display for ThemeIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInstalledThemeId => formatter.write_str("invalid installed theme id"),
            Self::InvalidThemeName => formatter.write_str("invalid theme name"),
            Self::ServiceInstanceExhausted => {
                formatter.write_str("theme service instance identity is exhausted")
            }
            Self::RevisionExhausted(name) => write!(formatter, "{name} is exhausted"),
        }
    }
}

impl Error for ThemeIdentityError {}
