use super::*;
use crate::theme;
use beryl_home_store::HomeCandidateRecoveryAccess;

impl ThemeService {
    pub fn observe_repository_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        max_manifest_bytes: NonZeroU64,
        read_limits: ThemeManifestReadLimits,
        previous: Option<&ThemeRepositoryObservation>,
    ) -> Result<ThemeRepositoryObservation, ThemeRepositoryLoadError> {
        self.check_candidate_read(access)?;
        self.observe_repository_with_access(
            access.into(),
            max_manifest_bytes,
            read_limits,
            previous,
        )
    }

    pub fn open_manifest_candidate<'a>(
        &self,
        access: &'a HomeCandidateRecoveryAccess<'_>,
        observation: &ThemeRepositoryObservation,
        max_manifest_bytes: NonZeroU64,
        read_limits: ThemeManifestReadLimits,
    ) -> Result<ThemeManifestSession<'a>, ThemeRepositoryLoadError> {
        self.check_candidate_read(access)?;
        self.open_manifest_with_access(access.into(), observation, max_manifest_bytes, read_limits)
    }

    pub fn load_document_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        repository: &ThemeRepositoryObservation,
        selection: &InstalledThemeSelection,
        previous: Option<&ThemeDocumentIdentity>,
    ) -> Result<ThemeObservedDocument, ThemeDocumentLoadError> {
        self.check_candidate_read(access)
            .map_err(ThemeDocumentLoadError::RepositoryLoad)?;
        self.load_document_with_access(access.into(), repository, selection, previous)
    }

    fn check_candidate_read(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
    ) -> Result<(), ThemeRepositoryLoadError> {
        if access.home_id() != self.home.home_id()
            || access.generation() != self.home.home_generation()
        {
            return Err(ThemeRepositoryLoadError::Freshness(
                theme::ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        Ok(())
    }

    pub(super) fn observe_repository_with_access(
        &self,
        store: ThemeReadAccess<'_>,
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
        let physical_limits = PhysicalThemeLimits::manifest(
            NonZeroU64::new(theme::THEME_MANIFEST_MAX_BYTES).expect("hard limit is nonzero"),
        )
        .map_err(|_| ThemeRepositoryLoadError::InvalidLimits)?;
        let snapshot = repository_snapshot(store, physical_limits)
            .map_err(ThemeRepositoryLoadError::Repository)?;
        self.check_snapshot(&snapshot)?;
        let physical_manifest = snapshot.manifest_identity();
        let manifest = match physical_manifest {
            None => self.manifest(ThemeManifestGeneration::INITIAL),
            Some(expected) => {
                if expected.length() > limits.operations().max_source_bytes() {
                    return Err(ThemeRepositoryLoadError::Manifest(
                        ThemeManifestDecodeError::LimitExceeded(ThemeManifestLimit::EncodedBytes),
                    ));
                }
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
                validate_manifest_complete(
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
                        theme::ThemeFreshnessError::StaleManifest,
                    ));
                }
            } else if previous.manifest != manifest {
                return Err(ThemeRepositoryLoadError::Freshness(
                    theme::ThemeFreshnessError::StaleManifest,
                ));
            }
        }
        self.runtime.note_repository_observed();
        Ok(ThemeRepositoryObservation {
            home: self.home,
            snapshot,
            manifest,
            physical_manifest,
            max_manifest_bytes: NonZeroU64::new(limits.operations().max_source_bytes())
                .expect("physical manifest limit is nonzero"),
        })
    }

    pub(super) fn open_manifest_with_access<'store>(
        &self,
        store: ThemeReadAccess<'store>,
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
            None => {
                let limits = PhysicalThemeLimits::manifest(max_manifest_bytes)
                    .map_err(|_| ThemeRepositoryLoadError::InvalidLimits)?;
                let current = repository_snapshot(store, limits)
                    .map_err(ThemeRepositoryLoadError::Repository)?;
                if current != observation.snapshot {
                    return Err(ThemeRepositoryLoadError::Freshness(
                        theme::ThemeFreshnessError::StaleManifest,
                    ));
                }
                ThemeManifestSessionInner::Empty {
                    manifest: observation.manifest,
                    consumed: false,
                }
            }
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
                        theme::ThemeFreshnessError::StaleManifest,
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

    pub(super) fn load_document_with_access(
        &self,
        store: ThemeReadAccess<'_>,
        repository: &ThemeRepositoryObservation,
        selection: &InstalledThemeSelection,
        previous: Option<&ThemeDocumentIdentity>,
    ) -> Result<ThemeObservedDocument, ThemeDocumentLoadError> {
        let _activity = self.runtime.begin_activity(ThemeActivityKind::DocumentLoad);
        self.check_observation(repository)
            .map_err(ThemeDocumentLoadError::RepositoryLoad)?;
        if selection.manifest() != repository.manifest {
            return Err(ThemeDocumentLoadError::RepositoryLoad(
                ThemeRepositoryLoadError::Freshness(theme::ThemeFreshnessError::StaleManifest),
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
        let document_limits = PhysicalThemeLimits::document().map_err(|_| {
            ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::InvalidLimits)
        })?;
        let limits =
            PhysicalThemeLimits::repository(repository.max_manifest_bytes).map_err(|_| {
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
        if byte_length > document_limits.operations().max_source_bytes() {
            return Err(ThemeDocumentLoadError::Invalid {
                identity,
                source: theme::ThemeDocumentError::DocumentTooLarge,
            });
        }
        let reader = PhysicalThemeReader::new(
            store,
            &repository.snapshot,
            selector.clone(),
            physical,
            limits,
        )
        .map_err(ThemeDocumentLoadError::Repository)?;
        let read_errors = reader.errors();
        match ThemeDocument::parse_reader(reader, theme::ThemeParseMode::InstalledLoad) {
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
        store: ThemeReadAccess<'_>,
        repository: &ThemeRepositoryObservation,
        selection: &InstalledThemeSelection,
        selector: &ThemeFileSelector,
        expected_document: ThemeFileIdentity,
    ) -> Result<(), ThemeDocumentLoadError> {
        let expected_manifest = repository.physical_manifest.ok_or({
            ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::Freshness(
                theme::ThemeFreshnessError::StaleManifest,
            ))
        })?;
        let max_manifest_bytes = repository.max_manifest_bytes;
        let limits = PhysicalThemeLimits::manifest(max_manifest_bytes).map_err(|_| {
            ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::InvalidLimits)
        })?;
        let snapshot =
            repository_snapshot(store, limits).map_err(ThemeDocumentLoadError::Repository)?;
        if snapshot != repository.snapshot {
            self.runtime.note_document_load_retry_rejection();
            return Err(ThemeDocumentLoadError::RepositoryLoad(
                ThemeRepositoryLoadError::Freshness(theme::ThemeFreshnessError::StaleManifest),
            ));
        }
        let read_limits = ThemeManifestReadLimits::new(
            NonZeroUsize::new(theme::THEME_MANIFEST_LINE_MAX_BYTES).expect("nonzero limit"),
            NonZeroUsize::new(theme::THEME_MANIFEST_HEADER_MAX_BYTES).expect("nonzero limit"),
            NonZeroUsize::new(theme::THEME_MANIFEST_PAGE_MAX_ENCODED_BYTES).expect("nonzero limit"),
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
            NonZeroUsize::new(theme::THEME_MANIFEST_PAGE_MAX_ITEMS).expect("nonzero limit"),
            NonZeroUsize::new(theme::THEME_MANIFEST_PAGE_MAX_DECODED_BYTES).expect("nonzero limit"),
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
                ThemeRepositoryLoadError::Freshness(theme::ThemeFreshnessError::StaleManifest),
            ));
        }
        let final_document = observe_file(
            store,
            &snapshot,
            selector,
            PhysicalThemeLimits::repository(repository.max_manifest_bytes).map_err(|_| {
                ThemeDocumentLoadError::RepositoryLoad(ThemeRepositoryLoadError::InvalidLimits)
            })?,
        )
        .map_err(ThemeDocumentLoadError::Repository)?;
        let final_snapshot =
            repository_snapshot(store, limits).map_err(ThemeDocumentLoadError::Repository)?;
        if final_document != expected_document || final_snapshot != snapshot {
            self.runtime.note_document_load_retry_rejection();
            return Err(ThemeDocumentLoadError::RepositoryLoad(
                ThemeRepositoryLoadError::Freshness(theme::ThemeFreshnessError::StaleDocument),
            ));
        }
        Ok(())
    }
}

fn open_manifest_decoder<'store>(
    store: ThemeReadAccess<'store>,
    snapshot: &ThemeRepositorySnapshot,
    expected: ThemeFileIdentity,
    physical_limits: PhysicalThemeLimits,
    home: ThemeHomeIdentity,
    read_limits: ThemeManifestReadLimits,
    bind: Option<ThemeManifestIdentity>,
) -> Result<CheckedManifestDecoder<'store>, ThemeRepositoryLoadError> {
    if expected.length() > physical_limits.operations().max_source_bytes() {
        return Err(ThemeRepositoryLoadError::Manifest(
            ThemeManifestDecodeError::LimitExceeded(ThemeManifestLimit::EncodedBytes),
        ));
    }
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

fn validate_manifest_complete(
    store: ThemeReadAccess<'_>,
    snapshot: &ThemeRepositorySnapshot,
    expected: ThemeFileIdentity,
    physical_limits: PhysicalThemeLimits,
    home: ThemeHomeIdentity,
    read_limits: ThemeManifestReadLimits,
    manifest: ThemeManifestIdentity,
) -> Result<(), ThemeRepositoryLoadError> {
    let page_limits = ThemePageLimits::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(InstalledThemeId::MAX_BYTES + theme::THEME_NAME_MAX_BYTES + 8)
            .ok_or(ThemeRepositoryLoadError::InvalidLimits)?,
    )
    .map_err(|source| ThemeRepositoryLoadError::Manifest(ThemeManifestDecodeError::Page(source)))?;
    let mut decoder = open_manifest_decoder(
        store,
        snapshot,
        expected,
        physical_limits,
        home,
        read_limits,
        Some(manifest),
    )?;
    let mut cursor = ThemeManifestCursor::first(manifest);
    loop {
        let page = decoder.read_page(cursor, page_limits)?;
        if page.records().is_empty() {
            return Ok(());
        }
        let Some(next) = page.next() else {
            return Ok(());
        };
        cursor = next;
    }
}
