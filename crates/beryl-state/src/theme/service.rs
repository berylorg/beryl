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
use super::{
    InstalledThemeId, InstalledThemeSelection, ThemeDocument, ThemeDocumentDigest,
    ThemeDocumentError, ThemeDocumentIdentity, ThemeDocumentRevision, ThemeHomeIdentity,
    ThemeIdentityError, ThemeManifestCursor, ThemeManifestDecodeError, ThemeManifestGeneration,
    ThemeManifestHeader, ThemeManifestIdentity, ThemeManifestPage, ThemeManifestReadLimits,
    ThemePageLimits, ThemeRepositoryService, ThemeSettingsIdentity,
    physical::{
        PhysicalThemeLimits, PhysicalThemeReadErrors, PhysicalThemeReader, document_identity_parts,
        installed_theme_id, observe_file, repository_snapshot, stable_file_id,
    },
    runtime::{ThemeActivityGuard, ThemeActivityKind, ThemeOperationScope, ThemeServiceRuntime},
};

/// Fresh generation-bound typed entry point for theme-domain work.
static NEXT_THEME_DOCUMENT_REVISION: AtomicU64 = AtomicU64::new(1);

/// Exact physical observation bound to one logical theme-service instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRepositoryObservation {
    home: ThemeHomeIdentity,
    snapshot: ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
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
        if self
            .runtime
            .refresh_is_gated(&ThemeOperationScope::Repository)
        {
            return Err(ThemeRepositoryLoadError::ScopeGated);
        }
        let limits = PhysicalThemeLimits::manifest(max_manifest_bytes)
            .map_err(|_| ThemeRepositoryLoadError::InvalidLimits)?;
        let snapshot =
            repository_snapshot(store, limits).map_err(ThemeRepositoryLoadError::Repository)?;
        self.check_snapshot(&snapshot)?;
        let physical_manifest = snapshot.manifest_identity();
        let manifest = match physical_manifest {
            None => self.manifest(ThemeManifestGeneration::INITIAL),
            Some(expected) => {
                let decoder = open_manifest_decoder(
                    store,
                    &snapshot,
                    expected,
                    limits,
                    self.home,
                    read_limits,
                    None,
                )?;
                let parsed = decoder.header().identity();
                let identity = ThemeManifestIdentity::observed(
                    self.home,
                    parsed.generation(),
                    expected.length(),
                    ThemeDocumentDigest::from_bytes(expected.sha256()),
                );
                validate_manifest_unique(
                    store,
                    &snapshot,
                    expected,
                    limits,
                    self.home,
                    read_limits,
                    identity,
                )?;
                identity
            }
        };
        if let Some(previous) = previous {
            self.check_observation(previous)?;
            let physical_changed = previous.physical_manifest != physical_manifest;
            if physical_changed {
                let successor = previous
                    .manifest
                    .generation()
                    .checked_next()
                    .is_ok_and(|generation| generation == manifest.generation());
                if !successor {
                    return Err(ThemeRepositoryLoadError::Freshness(
                        super::ThemeFreshnessError::StaleManifest,
                    ));
                }
            } else if previous.manifest != manifest {
                return Err(ThemeRepositoryLoadError::Freshness(
                    super::ThemeFreshnessError::StaleManifest,
                ));
            }
        }
        self.runtime.note_repository_observed();
        Ok(ThemeRepositoryObservation {
            home: self.home,
            snapshot,
            manifest,
            physical_manifest,
        })
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
        if self
            .runtime
            .refresh_is_gated(&ThemeOperationScope::Repository)
        {
            return Err(ThemeRepositoryLoadError::ScopeGated);
        }
        self.check_observation(observation)?;
        let inner = match observation.physical_manifest {
            None => ThemeManifestSessionInner::Empty {
                manifest: observation.manifest,
                consumed: false,
            },
            Some(expected) => {
                let limits = PhysicalThemeLimits::manifest(max_manifest_bytes)
                    .map_err(|_| ThemeRepositoryLoadError::InvalidLimits)?;
                let decoder = open_manifest_decoder(
                    store,
                    &observation.snapshot,
                    expected,
                    limits,
                    self.home,
                    read_limits,
                    Some(observation.manifest),
                )?;
                if decoder.header().identity() != observation.manifest {
                    return Err(ThemeRepositoryLoadError::Freshness(
                        super::ThemeFreshnessError::StaleManifest,
                    ));
                }
                ThemeManifestSessionInner::Present(decoder)
            }
        };
        Ok(ThemeManifestSession {
            inner,
            _activity: self
                .runtime
                .begin_activity(ThemeActivityKind::ManifestSession),
        })
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
        let _activity = self.runtime.begin_activity(ThemeActivityKind::DocumentLoad);
        self.check_observation(repository)
            .map_err(ThemeDocumentLoadError::RepositoryLoad)?;
        if selection.manifest() != repository.manifest {
            return Err(ThemeDocumentLoadError::RepositoryLoad(
                ThemeRepositoryLoadError::Freshness(super::ThemeFreshnessError::StaleManifest),
            ));
        }
        let theme_id = selection.id().clone();
        let scope = ThemeOperationScope::Document(theme_id.clone());
        if self.runtime.refresh_is_gated(&scope) {
            self.runtime.note_document_load_retry_rejection();
            return Err(ThemeDocumentLoadError::RepositoryLoad(
                ThemeRepositoryLoadError::ScopeGated,
            ));
        }
        let limits = PhysicalThemeLimits::document().map_err(|_| {
            ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::InvalidLimits)
        })?;
        let stable_id =
            stable_file_id(&theme_id).map_err(|_| ThemeDocumentLoadError::InvalidStableId)?;
        let selector = ThemeFileSelector::Document(stable_id);
        let physical = observe_file(store, &repository.snapshot, &selector, limits)
            .map_err(ThemeDocumentLoadError::Repository)?;
        let (byte_length, digest) = document_identity_parts(physical);
        let identity = self
            .observe_document(repository.manifest, theme_id, previous, byte_length, digest)
            .map_err(ThemeDocumentLoadError::Service)?;
        let reader = PhysicalThemeReader::new(
            store,
            &repository.snapshot,
            selector.clone(),
            physical,
            limits,
        )
        .map_err(ThemeDocumentLoadError::Repository)?;
        let read_errors = reader.errors();
        match ThemeDocument::parse_reader(reader, super::ThemeParseMode::InstalledLoad) {
            Ok(document) => {
                if let Err(source) =
                    self.confirm_document_fresh(store, repository, selection, &selector, physical)
                {
                    self.runtime.note_document_load_retry_rejection();
                    return Err(source);
                }
                if self.runtime.refresh_is_gated(&scope) {
                    self.runtime.note_document_load_retry_rejection();
                    return Err(ThemeDocumentLoadError::RepositoryLoad(
                        ThemeRepositoryLoadError::ScopeGated,
                    ));
                }
                Ok(ThemeObservedDocument { identity, document })
            }
            Err(source) => match read_errors.take() {
                Some(repository) => {
                    self.runtime.note_document_load_retry_rejection();
                    Err(ThemeDocumentLoadError::Repository(repository))
                }
                None => Err(ThemeDocumentLoadError::Invalid { identity, source }),
            },
        }
    }

    fn confirm_document_fresh(
        &self,
        store: &HomeStore,
        repository: &ThemeRepositoryObservation,
        selection: &InstalledThemeSelection,
        selector: &ThemeFileSelector,
        expected_document: ThemeFileIdentity,
    ) -> Result<(), ThemeDocumentLoadError> {
        let expected_manifest = repository.physical_manifest.ok_or({
            ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::Freshness(
                super::ThemeFreshnessError::StaleManifest,
            ))
        })?;
        let max_manifest_bytes = NonZeroU64::new(
            expected_manifest
                .length()
                .max(super::THEME_DOCUMENT_MAX_BYTES as u64)
                .max(1),
        )
        .ok_or({
            ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::InvalidLimits)
        })?;
        let limits = PhysicalThemeLimits::manifest(max_manifest_bytes).map_err(|_| {
            ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::InvalidLimits)
        })?;
        let snapshot =
            repository_snapshot(store, limits).map_err(ThemeDocumentLoadError::Repository)?;
        if snapshot != repository.snapshot {
            self.runtime.note_document_load_retry_rejection();
            return Err(ThemeDocumentLoadError::RepositoryLoad(
                ThemeRepositoryLoadError::Freshness(super::ThemeFreshnessError::StaleManifest),
            ));
        }
        let read_limits = ThemeManifestReadLimits::new(
            NonZeroUsize::new(super::THEME_MANIFEST_LINE_MAX_BYTES).expect("nonzero limit"),
            NonZeroUsize::new(super::THEME_MANIFEST_HEADER_MAX_BYTES).expect("nonzero limit"),
            NonZeroUsize::new(super::THEME_MANIFEST_PAGE_MAX_ENCODED_BYTES).expect("nonzero limit"),
        )
        .map_err(|source| {
            ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::Manifest(source))
        })?;
        let mut decoder = open_manifest_decoder(
            store,
            &snapshot,
            expected_manifest,
            limits,
            self.home,
            read_limits,
            Some(repository.manifest),
        )
        .map_err(ThemeDocumentLoadError::RepositoryLoad)?;
        let page_limits = ThemePageLimits::new(
            NonZeroUsize::new(super::THEME_MANIFEST_PAGE_MAX_ITEMS).expect("nonzero limit"),
            NonZeroUsize::new(super::THEME_MANIFEST_PAGE_MAX_DECODED_BYTES).expect("nonzero limit"),
        )
        .map_err(|_| {
            ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::InvalidLimits)
        })?;
        let mut cursor = ThemeManifestCursor::first(repository.manifest);
        let mut found = false;
        loop {
            let page = decoder
                .read_page(cursor, page_limits)
                .map_err(|source| match source {
                    ThemeRepositoryLoadError::Repository(source) => {
                        ThemeDocumentLoadError::Repository(source)
                    }
                    other => ThemeDocumentLoadError::RepositoryLoad(other),
                })?;
            if page.records().iter().any(|row| row == selection.summary()) {
                found = true;
            }
            match page.next() {
                Some(next) if !found => cursor = next,
                _ => break,
            }
        }
        if !found {
            self.runtime.note_document_load_retry_rejection();
            return Err(ThemeDocumentLoadError::RepositoryLoad(
                ThemeRepositoryLoadError::Freshness(super::ThemeFreshnessError::StaleManifest),
            ));
        }
        let final_document = observe_file(
            store,
            &snapshot,
            selector,
            PhysicalThemeLimits::document().map_err(|_| {
                ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::InvalidLimits)
            })?,
        )
        .map_err(ThemeDocumentLoadError::Repository)?;
        let final_snapshot =
            repository_snapshot(store, limits).map_err(ThemeDocumentLoadError::Repository)?;
        if final_document != expected_document || final_snapshot != snapshot {
            self.runtime.note_document_load_retry_rejection();
            return Err(ThemeDocumentLoadError::RepositoryLoad(
                ThemeRepositoryLoadError::Freshness(super::ThemeFreshnessError::StaleDocument),
            ));
        }
        Ok(())
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

fn open_manifest_decoder<'store>(
    store: &'store HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    expected: ThemeFileIdentity,
    physical_limits: PhysicalThemeLimits,
    home: ThemeHomeIdentity,
    read_limits: ThemeManifestReadLimits,
    bind: Option<ThemeManifestIdentity>,
) -> Result<CheckedManifestDecoder<'store>, ThemeRepositoryLoadError> {
    let reader = PhysicalThemeReader::new(
        store,
        snapshot,
        ThemeFileSelector::Manifest,
        expected,
        physical_limits,
    )
    .map_err(ThemeRepositoryLoadError::Repository)?;
    let errors = reader.errors();
    let mut decoder = ThemeManifestDecoder::open(reader, home, read_limits).map_err(|source| {
        errors.take().map_or(
            ThemeRepositoryLoadError::Manifest(source),
            ThemeRepositoryLoadError::Repository,
        )
    })?;
    if let Some(identity) = bind {
        decoder
            .bind_identity(identity)
            .map_err(ThemeRepositoryLoadError::Manifest)?;
    }
    Ok(CheckedManifestDecoder { decoder, errors })
}

/// Proves stable-id uniqueness with bounded memory by comparing each row only to its predecessors.
///
/// The manifest is logically unbounded, so this deliberately trades repeated exact range scans
/// for a constant one-row working set rather than retaining an unbounded identity index.
fn validate_manifest_unique(
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    expected: ThemeFileIdentity,
    physical_limits: PhysicalThemeLimits,
    home: ThemeHomeIdentity,
    read_limits: ThemeManifestReadLimits,
    manifest: ThemeManifestIdentity,
) -> Result<(), ThemeRepositoryLoadError> {
    let page_limits = ThemePageLimits::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(InstalledThemeId::MAX_BYTES + super::THEME_NAME_MAX_BYTES + 8)
            .ok_or(ThemeRepositoryLoadError::InvalidLimits)?,
    )
    .map_err(|source| ThemeRepositoryLoadError::Manifest(ThemeManifestDecodeError::Page(source)))?;
    let mut outer = open_manifest_decoder(
        store,
        snapshot,
        expected,
        physical_limits,
        home,
        read_limits,
        Some(manifest),
    )?;
    let mut outer_cursor = ThemeManifestCursor::first(manifest);
    loop {
        let page = outer.read_page(outer_cursor, page_limits)?;
        let Some(current) = page.records().first() else {
            return Ok(());
        };
        if current.order() > 0 {
            let mut prior = open_manifest_decoder(
                store,
                snapshot,
                expected,
                physical_limits,
                home,
                read_limits,
                Some(manifest),
            )?;
            let mut prior_cursor = ThemeManifestCursor::first(manifest);
            while prior_cursor.next_order() < current.order() {
                let prior_page = prior.read_page(prior_cursor, page_limits)?;
                let earlier = prior_page.records().first().ok_or_else(|| {
                    ThemeRepositoryLoadError::Manifest(ThemeManifestDecodeError::CursorMismatch)
                })?;
                if earlier.id() == current.id() {
                    return Err(ThemeRepositoryLoadError::Manifest(
                        ThemeManifestDecodeError::DuplicateThemeId {
                            id: current.id().clone(),
                        },
                    ));
                }
                prior_cursor = prior_page.next().ok_or_else(|| {
                    ThemeRepositoryLoadError::Manifest(ThemeManifestDecodeError::CursorMismatch)
                })?;
            }
        }
        let Some(next) = page.next() else {
            return Ok(());
        };
        outer_cursor = next;
    }
}

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
