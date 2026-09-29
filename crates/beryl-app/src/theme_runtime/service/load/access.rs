use super::*;
use beryl_home_store::HomeCandidateRecoveryAccess;
use beryl_state::ThemeManifestSession;

#[derive(Clone, Copy)]
pub(crate) enum ThemeLoadAccess<'a> {
    Published(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

impl<'a> From<&'a HomeStore> for ThemeLoadAccess<'a> {
    fn from(store: &'a HomeStore) -> Self {
        Self::Published(store)
    }
}

impl<'a> ThemeLoadAccess<'a> {
    pub(crate) fn observe(
        self,
        service: &ThemeService,
        config: ThemeRuntimeConfig,
    ) -> Result<ThemeRepositoryObservation, ThemeRepositoryLoadError> {
        match self {
            Self::Published(store) => service.observe_repository(
                store,
                config.max_manifest_bytes,
                config.manifest_read,
                None,
            ),
            Self::Candidate(access) => service.observe_repository_candidate(
                access,
                config.max_manifest_bytes,
                config.manifest_read,
                None,
            ),
        }
    }

    pub(super) fn open_manifest(
        self,
        service: &ThemeService,
        repository: &ThemeRepositoryObservation,
        config: ThemeRuntimeConfig,
    ) -> Result<ThemeManifestSession<'a>, ThemeRepositoryLoadError> {
        match self {
            Self::Published(store) => service.open_manifest(
                store,
                repository,
                config.max_manifest_bytes,
                config.manifest_read,
            ),
            Self::Candidate(access) => service.open_manifest_candidate(
                access,
                repository,
                config.max_manifest_bytes,
                config.manifest_read,
            ),
        }
    }

    pub(super) fn load_document(
        self,
        service: &ThemeService,
        repository: &ThemeRepositoryObservation,
        selection: &InstalledThemeSelection,
        previous: Option<&ThemeDocumentIdentity>,
    ) -> Result<ThemeObservedDocument, ThemeDocumentLoadError> {
        match self {
            Self::Published(store) => service.load_document(store, repository, selection, previous),
            Self::Candidate(access) => {
                service.load_document_candidate(access, repository, selection, previous)
            }
        }
    }
}
