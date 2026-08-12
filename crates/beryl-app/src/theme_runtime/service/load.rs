use beryl_home_store::HomeStore;
use beryl_state::{
    InstalledThemeId, InstalledThemeSelection, PreparedThemeAppearance, ThemeAppearanceSource,
    ThemeDocumentIdentity, ThemeDocumentLoadError, ThemeLiveEditFailure, ThemeLoadFailure,
    ThemeManifestCursor, ThemeObservedDocument, ThemeRepositoryLoadError,
    ThemeRepositoryObservation, ThemeResolver, ThemeService, ThemeSettingsIdentity,
};

use super::{ThemeRuntimeConfig, ThemeRuntimeFailureClass, ThemeRuntimeStartError};
use crate::theme_runtime::DurablePublicationError;

pub(super) enum ActiveLoadError {
    Missing,
    Repository,
    Document(ThemeDocumentLoadError),
}

impl ActiveLoadError {
    pub(super) fn observed_identity(&self) -> Option<&ThemeDocumentIdentity> {
        match self {
            Self::Document(error) => error.observed_identity(),
            Self::Missing | Self::Repository => None,
        }
    }
}

pub(super) fn load_prepared(
    service: &ThemeService,
    store: &HomeStore,
    repository: &ThemeRepositoryObservation,
    active: &InstalledThemeId,
    settings: ThemeSettingsIdentity,
    previous: Option<&ThemeDocumentIdentity>,
    config: ThemeRuntimeConfig,
    pages_read: &mut u64,
) -> Result<(PreparedThemeAppearance, ThemeDocumentIdentity), ThemeLoadFailure> {
    let observed = load_observed(
        service, store, repository, active, previous, config, pages_read,
    )
    .map_err(|error| match error {
        ActiveLoadError::Missing => ThemeLoadFailure::DocumentMissing,
        ActiveLoadError::Repository => ThemeLoadFailure::RepositoryUnavailable,
        ActiveLoadError::Document(ThemeDocumentLoadError::Invalid { .. }) => {
            ThemeLoadFailure::DocumentInvalid
        }
        ActiveLoadError::Document(_) => ThemeLoadFailure::DocumentUnreadable,
    })?;
    let resolver = ThemeResolver::new(observed.document().definition())
        .map_err(|_| ThemeLoadFailure::DocumentInvalid)?;
    let identity = observed.identity().clone();
    let prepared =
        PreparedThemeAppearance::installed(settings, active, identity.clone(), resolver.resolve())
            .map_err(|_| ThemeLoadFailure::ApplicationUnavailable)?;
    Ok((prepared, identity))
}

pub(super) fn load_observed(
    service: &ThemeService,
    store: &HomeStore,
    repository: &ThemeRepositoryObservation,
    active: &InstalledThemeId,
    previous: Option<&ThemeDocumentIdentity>,
    config: ThemeRuntimeConfig,
    pages_read: &mut u64,
) -> Result<ThemeObservedDocument, ActiveLoadError> {
    let selection = find_selection(service, store, repository, active, config, pages_read)
        .map_err(|_| ActiveLoadError::Repository)?
        .ok_or(ActiveLoadError::Missing)?;
    service
        .load_document(store, repository, &selection, previous)
        .map_err(ActiveLoadError::Document)
}

fn find_selection(
    service: &ThemeService,
    store: &HomeStore,
    repository: &ThemeRepositoryObservation,
    active: &InstalledThemeId,
    config: ThemeRuntimeConfig,
    pages_read: &mut u64,
) -> Result<Option<InstalledThemeSelection>, ThemeRepositoryLoadError> {
    let mut session = service.open_manifest(
        store,
        repository,
        config.max_manifest_bytes,
        config.manifest_read,
    )?;
    let mut cursor = ThemeManifestCursor::first(session.header().identity());
    loop {
        let page = session.read_page(cursor, config.page)?;
        *pages_read = pages_read.saturating_add(1);
        if let Some(index) = page
            .records()
            .iter()
            .position(|record| record.id() == active)
        {
            return Ok(page.selection(index));
        }
        let Some(next) = page.next() else {
            return Ok(None);
        };
        cursor = next;
    }
}

pub(super) fn installed_identity(
    prepared: &PreparedThemeAppearance,
) -> Option<&ThemeDocumentIdentity> {
    match prepared.source() {
        ThemeAppearanceSource::Installed(identity) => Some(identity),
        ThemeAppearanceSource::BuiltinFallback(_) => None,
    }
}

pub(super) fn prepared_matches_active(
    prepared: &PreparedThemeAppearance,
    active: Option<&InstalledThemeId>,
) -> bool {
    match (prepared.source(), active) {
        (ThemeAppearanceSource::BuiltinFallback(_), None) => true,
        (ThemeAppearanceSource::Installed(document), Some(active)) => document.theme_id() == active,
        _ => false,
    }
}

pub(super) const fn start_error(class: ThemeRuntimeFailureClass) -> ThemeRuntimeStartError {
    ThemeRuntimeStartError { class }
}

pub(super) fn map_startup_failure(failure: &ThemeLoadFailure) -> ThemeRuntimeFailureClass {
    match failure {
        ThemeLoadFailure::RepositoryUnavailable => ThemeRuntimeFailureClass::Repository,
        ThemeLoadFailure::ActiveIdentityMissing | ThemeLoadFailure::DocumentMissing => {
            ThemeRuntimeFailureClass::DocumentMissing
        }
        ThemeLoadFailure::DocumentUnreadable => ThemeRuntimeFailureClass::DocumentUnreadable,
        ThemeLoadFailure::DocumentInvalid => ThemeRuntimeFailureClass::DocumentInvalid,
        ThemeLoadFailure::ApplicationUnavailable => ThemeRuntimeFailureClass::Resolution,
    }
}

pub(super) fn map_live_load_failure(error: &ActiveLoadError) -> ThemeLiveEditFailure {
    match error {
        ActiveLoadError::Missing => ThemeLiveEditFailure::Missing,
        ActiveLoadError::Repository => ThemeLiveEditFailure::Unreadable,
        ActiveLoadError::Document(ThemeDocumentLoadError::Invalid { .. }) => {
            ThemeLiveEditFailure::InvalidDocument
        }
        ActiveLoadError::Document(_) => ThemeLiveEditFailure::Unreadable,
    }
}

pub(super) const fn map_live_failure(failure: ThemeLiveEditFailure) -> ThemeRuntimeFailureClass {
    match failure {
        ThemeLiveEditFailure::Missing => ThemeRuntimeFailureClass::DocumentMissing,
        ThemeLiveEditFailure::Unreadable | ThemeLiveEditFailure::OverLimit => {
            ThemeRuntimeFailureClass::DocumentUnreadable
        }
        ThemeLiveEditFailure::InvalidDocument => ThemeRuntimeFailureClass::DocumentInvalid,
        ThemeLiveEditFailure::ResolutionFailed => ThemeRuntimeFailureClass::Resolution,
        ThemeLiveEditFailure::Superseded => ThemeRuntimeFailureClass::Publication,
    }
}

pub(super) fn map_publication_error(_: DurablePublicationError) -> ThemeRuntimeFailureClass {
    ThemeRuntimeFailureClass::Publication
}
