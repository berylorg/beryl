use std::{
    error::Error,
    fmt,
    num::{NonZeroU64, NonZeroUsize},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use beryl_home_store::{
    HomeHealthState, HomeRecoveryCandidate, HomeStore, ThemeFileIdentity, ThemeFileSelector,
    ThemeRepositoryError, ThemeRepositorySnapshot, ThemeWatchError, ThemeWatchHint,
    ThemeWatchLimits, ThemeWatchSubscription,
};
use beryl_model::BerylHomeId;

use super::manifest::ThemeManifestDecoder;
mod loading;
mod preparation;
use super::{
    InstalledThemeId, InstalledThemeSelection, ThemeDocument, ThemeDocumentDigest,
    ThemeDocumentError, ThemeDocumentIdentity, ThemeDocumentRevision, ThemeHomeIdentity,
    ThemeIdentityError, ThemeManifestCursor, ThemeManifestDecodeError, ThemeManifestGeneration,
    ThemeManifestHeader, ThemeManifestIdentity, ThemeManifestLimit, ThemeManifestPage,
    ThemeManifestReadLimits, ThemePageLimits, ThemeRepositoryService, ThemeSettingsIdentity,
    physical::{
        PhysicalThemeLimits, PhysicalThemeReadErrors, PhysicalThemeReader, ThemeReadAccess,
        document_identity_parts, installed_theme_id, observe_file, repository_snapshot,
        stable_file_id,
    },
    runtime::{ThemeActivityGuard, ThemeActivityKind, ThemeOperationScope, ThemeServiceRuntime},
};
pub use preparation::PreparedThemeChangeSubscription;

/// Fresh generation-bound typed entry point for theme-domain work.
static NEXT_THEME_DOCUMENT_REVISION: AtomicU64 = AtomicU64::new(1);

/// Exact physical observation bound to one logical theme-service instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRepositoryObservation {
    home: ThemeHomeIdentity,
    snapshot: ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    max_manifest_bytes: NonZeroU64,
}

impl ThemeRepositoryObservation {
    #[must_use]
    pub const fn home(&self) -> ThemeHomeIdentity {
        self.home
    }

    #[must_use]
    pub const fn manifest(&self) -> ThemeManifestIdentity {
        self.manifest
    }

    #[must_use]
    pub const fn is_initialized(&self) -> bool {
        self.physical_manifest.is_some()
    }
}

/// One validated forward-only manifest enumeration session.
pub struct ThemeManifestSession<'store> {
    inner: ThemeManifestSessionInner<'store>,
    _activity: ThemeActivityGuard,
}

enum ThemeManifestSessionInner<'store> {
    Empty {
        manifest: ThemeManifestIdentity,
        consumed: bool,
    },
    Present(CheckedManifestDecoder<'store>),
}

struct CheckedManifestDecoder<'store> {
    decoder: ThemeManifestDecoder<PhysicalThemeReader<'store>>,
    errors: PhysicalThemeReadErrors,
}

impl CheckedManifestDecoder<'_> {
    fn header(&self) -> ThemeManifestHeader {
        self.decoder.header()
    }

    fn read_page(
        &mut self,
        cursor: ThemeManifestCursor,
        limits: ThemePageLimits,
    ) -> Result<ThemeManifestPage, ThemeRepositoryLoadError> {
        self.decoder.read_page(cursor, limits).map_err(|source| {
            self.errors.take().map_or(
                ThemeRepositoryLoadError::Manifest(source),
                ThemeRepositoryLoadError::Repository,
            )
        })
    }
}

impl ThemeManifestSession<'_> {
    #[must_use]
    pub fn header(&self) -> ThemeManifestHeader {
        match &self.inner {
            ThemeManifestSessionInner::Empty { manifest, .. } => {
                ThemeManifestHeader::new(*manifest)
            }
            ThemeManifestSessionInner::Present(decoder) => decoder.header(),
        }
    }

    pub fn read_page(
        &mut self,
        cursor: super::ThemeManifestCursor,
        limits: ThemePageLimits,
    ) -> Result<ThemeManifestPage, ThemeRepositoryLoadError> {
        match &mut self.inner {
            ThemeManifestSessionInner::Empty { manifest, consumed } => {
                if *consumed || cursor.manifest() != *manifest || cursor.next_order() != 0 {
                    return Err(ThemeRepositoryLoadError::Manifest(
                        ThemeManifestDecodeError::CursorMismatch,
                    ));
                }
                *consumed = true;
                ThemeManifestPage::checked(cursor, Vec::new(), false, limits).map_err(|source| {
                    ThemeRepositoryLoadError::Manifest(ThemeManifestDecodeError::Page(source))
                })
            }
            ThemeManifestSessionInner::Present(decoder) => decoder.read_page(cursor, limits),
        }
    }
}

/// One installed document observation and its validated typed contents.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeObservedDocument {
    identity: ThemeDocumentIdentity,
    document: ThemeDocument,
}

impl ThemeObservedDocument {
    #[must_use]
    pub const fn identity(&self) -> &ThemeDocumentIdentity {
        &self.identity
    }
    #[must_use]
    pub const fn document(&self) -> &ThemeDocument {
        &self.document
    }
}

#[derive(Clone, Debug)]
pub struct ThemeService {
    home: ThemeHomeIdentity,
    repository: ThemeRepositoryService,
    pub(super) runtime: Arc<ThemeServiceRuntime>,
}

impl ThemeService {
    pub fn acquire(store: &HomeStore) -> Result<Self, ThemeServiceError> {
        let health = store.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ThemeServiceError::HomeUnavailable(health.state()));
        }
        let generation = health
            .generation()
            .ok_or(ThemeServiceError::MissingHomeGeneration)?;
        Self::from_parts(store.home_id(), generation)
    }

    pub(crate) fn acquire_initial_candidate(
        candidate: &beryl_home_store::HomeOpenCandidate,
    ) -> Result<Self, ThemeServiceError> {
        Self::from_parts(candidate.home_id(), candidate.generation())
    }

    /// Constructs the fresh candidate service before the candidate stack is published.
    ///
    /// The caller supplies the already-proven durable home id retained by the same-home recovery
    /// composition. No prior service handle, cursor, descriptor, or preview state is adopted.
    #[must_use]
    pub fn reacquire_candidate(
        candidate: &HomeRecoveryCandidate,
    ) -> Result<Self, ThemeServiceError> {
        Self::from_parts(candidate.home_id(), candidate.generation())
    }

    fn from_parts(
        home_id: BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
    ) -> Result<Self, ThemeServiceError> {
        let home =
            ThemeHomeIdentity::fresh(home_id, generation).map_err(ThemeServiceError::Identity)?;
        Ok(Self {
            home,
            repository: ThemeRepositoryService::new(home),
            runtime: ThemeServiceRuntime::shared(),
        })
    }

    #[must_use]
    pub const fn home(&self) -> ThemeHomeIdentity {
        self.home
    }

    #[must_use]
    pub const fn repository(&self) -> ThemeRepositoryService {
        self.repository
    }

    #[must_use]
    pub const fn manifest(&self, generation: ThemeManifestGeneration) -> ThemeManifestIdentity {
        self.repository.manifest(generation)
    }

    /// Returns a content-free bounded diagnostic snapshot for this service generation.
    #[must_use]
    pub fn diagnostics(&self) -> super::ThemeServiceDiagnostics {
        self.runtime.diagnostics()
    }

    /// Observes the exact physical repository and validates its manifest header when present.
    pub fn observe_repository(
        &self,
        store: &HomeStore,
        max_manifest_bytes: NonZeroU64,
        read_limits: ThemeManifestReadLimits,
        previous: Option<&ThemeRepositoryObservation>,
    ) -> Result<ThemeRepositoryObservation, ThemeRepositoryLoadError> {
        self.observe_repository_with_access(store.into(), max_manifest_bytes, read_limits, previous)
    }

    /// Executes a typed mutation against one exact validated repository observation.
    pub fn execute_command(
        &self,
        store: &HomeStore,
        observation: &ThemeRepositoryObservation,
        command: &super::ThemeRepositoryCommand,
        max_manifest_source: NonZeroU64,
        references: &dyn super::ThemeReferenceSnapshotProvider,
    ) -> Result<super::ThemeRepositoryOperationOutcome, super::ThemeRepositoryExecutionError> {
        super::execution::execute_theme_command(
            self,
            store,
            &observation.snapshot,
            observation.manifest,
            observation.physical_manifest,
            command,
            max_manifest_source,
            references,
        )
    }

    /// Reconciles one ambiguous operation retained by this exact service generation.
    pub fn reconcile_operation(
        &self,
        store: &HomeStore,
        operation: NonZeroU64,
        max_manifest_source: NonZeroU64,
    ) -> Result<super::ThemeReconciliation, super::ThemeRepositoryExecutionError> {
        super::execution::reconcile_theme_operation(self, store, operation, max_manifest_source)
    }

    /// Opens a bounded forward-only enumeration over one exact manifest observation.
    pub fn open_manifest<'store>(
        &self,
        store: &'store HomeStore,
        observation: &ThemeRepositoryObservation,
        max_manifest_bytes: NonZeroU64,
        read_limits: ThemeManifestReadLimits,
    ) -> Result<ThemeManifestSession<'store>, ThemeRepositoryLoadError> {
        self.open_manifest_with_access(store.into(), observation, max_manifest_bytes, read_limits)
    }

    pub fn active_theme_from_setting(
        value: Option<&crate::SettingRecord>,
    ) -> Result<Option<InstalledThemeId>, ThemeIdentityError> {
        match value.and_then(|record| record.value().as_active_theme_id()) {
            Some(value) => InstalledThemeId::new(value).map(Some),
            None => Ok(None),
        }
    }

    pub fn settings_identity(
        &self,
        domain_revision: beryl_model::DomainRevision,
        record: Option<&crate::SettingRecord>,
    ) -> ThemeSettingsIdentity {
        ThemeSettingsIdentity::new(
            self.home,
            domain_revision,
            record.map(crate::SettingRecord::revision),
        )
    }

    /// Observes and incrementally validates one manifest-member document.
    pub fn load_document(
        &self,
        store: &HomeStore,
        repository: &ThemeRepositoryObservation,
        selection: &InstalledThemeSelection,
        previous: Option<&ThemeDocumentIdentity>,
    ) -> Result<ThemeObservedDocument, ThemeDocumentLoadError> {
        self.load_document_with_access(store.into(), repository, selection, previous)
    }

    /// Creates a fresh bounded generation-owned change-hint subscription.
    pub fn subscribe_changes(
        &self,
        store: &HomeStore,
        interval: Duration,
        queue_capacity: NonZeroUsize,
        max_entries_per_poll: NonZeroUsize,
        max_file_bytes: NonZeroU64,
    ) -> Result<ThemeChangeSubscription, ThemeChangeSubscriptionError> {
        if store.home_id() != self.home.home_id()
            || store.health().generation() != Some(self.home.home_generation())
        {
            return Err(ThemeChangeSubscriptionError::Freshness(
                super::ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        let io_buffer_bytes =
            NonZeroUsize::new(64 * 1024).ok_or(ThemeChangeSubscriptionError::InvalidLimits)?;
        let limits = ThemeWatchLimits::new(
            interval,
            queue_capacity,
            max_entries_per_poll,
            max_file_bytes.get(),
            io_buffer_bytes,
        )
        .map_err(|_| ThemeChangeSubscriptionError::InvalidLimits)?;
        store
            .subscribe_theme_changes(limits)
            .map(|inner| ThemeChangeSubscription {
                inner,
                runtime: Arc::clone(&self.runtime),
                _activity: self.runtime.begin_activity(ThemeActivityKind::Subscription),
            })
            .map_err(ThemeChangeSubscriptionError::Watcher)
    }

    /// Binds one coherent physical observation to the current manifest membership.
    ///
    /// An identical length/digest hint is idempotent. Any changed content receives a newer
    /// service-scoped observation revision, including bytes that return to an earlier digest.
    pub fn observe_document(
        &self,
        manifest: ThemeManifestIdentity,
        theme_id: InstalledThemeId,
        previous: Option<&ThemeDocumentIdentity>,
        byte_length: u64,
        digest: ThemeDocumentDigest,
    ) -> Result<ThemeDocumentIdentity, ThemeServiceError> {
        self.repository
            .check_manifest(manifest)
            .map_err(ThemeServiceError::Freshness)?;
        if let Some(previous) = previous {
            if previous.manifest().home() != self.home || previous.theme_id() != &theme_id {
                return Err(ThemeServiceError::Freshness(
                    super::ThemeFreshnessError::StaleDocument,
                ));
            }
            if previous.byte_length() == byte_length && previous.digest() == digest {
                return Ok(ThemeDocumentIdentity::new(
                    manifest,
                    theme_id,
                    previous.revision(),
                    byte_length,
                    digest,
                ));
            }
        }
        let revision = self.next_document_revision()?;
        Ok(ThemeDocumentIdentity::new(
            manifest,
            theme_id,
            revision,
            byte_length,
            digest,
        ))
    }

    fn next_document_revision(&self) -> Result<ThemeDocumentRevision, ThemeServiceError> {
        let raw = NEXT_THEME_DOCUMENT_REVISION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| ThemeServiceError::ObservationRevisionExhausted)?;
        let revision =
            NonZeroU64::new(raw).ok_or(ThemeServiceError::ObservationRevisionExhausted)?;
        Ok(ThemeDocumentRevision::new(revision))
    }

    fn check_snapshot(
        &self,
        snapshot: &ThemeRepositorySnapshot,
    ) -> Result<(), ThemeRepositoryLoadError> {
        if snapshot.home_id() != self.home.home_id()
            || snapshot.generation() != self.home.home_generation()
        {
            return Err(ThemeRepositoryLoadError::Freshness(
                super::ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        Ok(())
    }

    fn check_observation(
        &self,
        observation: &ThemeRepositoryObservation,
    ) -> Result<(), ThemeRepositoryLoadError> {
        if observation.home != self.home {
            return Err(ThemeRepositoryLoadError::Freshness(
                super::ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeServiceError {
    HomeUnavailable(HomeHealthState),
    MissingHomeGeneration,
    Identity(ThemeIdentityError),
    Freshness(super::ThemeFreshnessError),
    ObservationRevisionExhausted,
}

impl fmt::Display for ThemeServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ThemeServiceError {}

#[derive(Debug)]
pub enum ThemeRepositoryLoadError {
    InvalidLimits,
    ScopeGated,
    Repository(ThemeRepositoryError),
    Manifest(ThemeManifestDecodeError),
    Freshness(super::ThemeFreshnessError),
}

impl fmt::Display for ThemeRepositoryLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ThemeRepositoryLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(source) => Some(source),
            Self::Manifest(source) => Some(source),
            Self::InvalidLimits | Self::ScopeGated | Self::Freshness(_) => None,
        }
    }
}

#[derive(Debug)]
pub enum ThemeDocumentLoadError {
    InvalidStableId,
    RepositoryLoad(ThemeRepositoryLoadError),
    Repository(ThemeRepositoryError),
    Service(ThemeServiceError),
    Invalid {
        identity: ThemeDocumentIdentity,
        source: ThemeDocumentError,
    },
}

impl ThemeDocumentLoadError {
    #[must_use]
    pub const fn observed_identity(&self) -> Option<&ThemeDocumentIdentity> {
        match self {
            Self::Invalid { identity, .. } => Some(identity),
            _ => None,
        }
    }
}

impl fmt::Display for ThemeDocumentLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ThemeDocumentLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RepositoryLoad(source) => Some(source),
            Self::Repository(source) => Some(source),
            Self::Service(source) => Some(source),
            Self::Invalid { source, .. } => Some(source),
            Self::InvalidStableId => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeChangeHint {
    ManifestChanged,
    DocumentChanged(InstalledThemeId),
    Overflow,
}

pub struct ThemeChangeSubscription {
    inner: ThemeWatchSubscription,
    runtime: Arc<ThemeServiceRuntime>,
    _activity: ThemeActivityGuard,
}

impl ThemeChangeSubscription {
    pub fn try_recv(&self) -> Result<Option<ThemeChangeHint>, ThemeChangeSubscriptionError> {
        let hint = self
            .inner
            .try_recv()
            .map_err(ThemeChangeSubscriptionError::Watcher)?
            .map(convert_watch_hint)
            .transpose()?;
        if let Some(hint) = &hint {
            self.runtime
                .note_change_hint(matches!(hint, ThemeChangeHint::Overflow));
        }
        Ok(hint)
    }

    pub fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Option<ThemeChangeHint>, ThemeChangeSubscriptionError> {
        let hint = self
            .inner
            .recv_timeout(timeout)
            .map_err(ThemeChangeSubscriptionError::Watcher)?
            .map(convert_watch_hint)
            .transpose()?;
        if let Some(hint) = &hint {
            self.runtime
                .note_change_hint(matches!(hint, ThemeChangeHint::Overflow));
        }
        Ok(hint)
    }

    pub fn shutdown(self) {
        self.inner.shutdown();
    }
}

fn convert_watch_hint(
    hint: ThemeWatchHint,
) -> Result<ThemeChangeHint, ThemeChangeSubscriptionError> {
    match hint {
        ThemeWatchHint::ManifestChanged => Ok(ThemeChangeHint::ManifestChanged),
        ThemeWatchHint::DocumentChanged(id) => installed_theme_id(&id)
            .map(ThemeChangeHint::DocumentChanged)
            .map_err(|_| ThemeChangeSubscriptionError::InvalidStableId),
        ThemeWatchHint::Overflow => Ok(ThemeChangeHint::Overflow),
    }
}

#[derive(Debug)]
pub enum ThemeChangeSubscriptionError {
    InvalidLimits,
    InvalidStableId,
    Freshness(super::ThemeFreshnessError),
    Watcher(ThemeWatchError),
}

impl fmt::Display for ThemeChangeSubscriptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ThemeChangeSubscriptionError {}
