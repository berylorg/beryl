use std::num::NonZeroU64;

use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::DomainRevision;
use beryl_state::{
    BuiltinFallback, InstalledThemeId, PreparedThemeAppearance, ThemeAppearanceSource,
    ThemeCommandError, ThemeDeleteGuard, ThemeDocumentDigest, ThemeDocumentIdentity,
    ThemeDocumentRevision, ThemeLiveEditFailure, ThemeLiveEditOutcome, ThemeManifestGeneration,
    ThemeReferenceSnapshot, ThemeRefreshInput, ThemeRepositoryCommit, ThemeService,
    ThemeStartupOutcome, builtin_fallback_appearance,
};

fn service() -> (tempfile::TempDir, HomeStore, ThemeService) {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    (directory, store, service)
}

#[test]
fn live_edit_rejects_other_manifest_and_refresh_cannot_replace_installed_with_fallback() {
    let (_directory, _store, service) = service();
    let settings = service.settings_identity(DomainRevision::new(1).unwrap(), None);
    let active = InstalledThemeId::new("active").unwrap();
    let current = document(
        &service,
        ThemeManifestGeneration::INITIAL,
        active.as_str(),
        1,
        b"current",
    );
    let prior = PreparedThemeAppearance::installed(
        settings,
        &active,
        current,
        builtin_fallback_appearance(),
    )
    .unwrap();
    let next_generation = ThemeManifestGeneration::INITIAL.checked_next().unwrap();
    let stale_completion = document(
        &service,
        next_generation,
        active.as_str(),
        2,
        b"other-manifest",
    );
    assert!(
        ThemeLiveEditOutcome::valid(
            prior.clone(),
            stale_completion,
            builtin_fallback_appearance(),
        )
        .is_err()
    );

    let refresh = ThemeRefreshInput::new(
        service.manifest(ThemeManifestGeneration::INITIAL),
        prior,
        service.manifest(next_generation),
    )
    .unwrap();
    assert!(
        refresh
            .accept(PreparedThemeAppearance::fallback(settings))
            .is_err()
    );
}

fn document(
    service: &ThemeService,
    generation: ThemeManifestGeneration,
    id: &str,
    revision: u64,
    bytes: &[u8],
) -> ThemeDocumentIdentity {
    ThemeDocumentIdentity::new(
        service.manifest(generation),
        InstalledThemeId::new(id).unwrap(),
        ThemeDocumentRevision::new(NonZeroU64::new(revision).unwrap()),
        bytes.len() as u64,
        ThemeDocumentDigest::of_bytes(bytes),
    )
}

#[test]
fn reference_snapshot_rejects_every_delete_conflict() {
    let (_directory, _store, service) = service();
    let manifest = service.manifest(ThemeManifestGeneration::INITIAL);
    let settings = service.settings_identity(DomainRevision::new(1).unwrap(), None);
    let target = InstalledThemeId::new("active").unwrap();

    for (durable, staged, drafts, operations, expected) in [
        (
            Some(target.clone()),
            None,
            vec![],
            vec![],
            ThemeDeleteGuard::DurableActive,
        ),
        (
            None,
            Some(target.clone()),
            vec![],
            vec![],
            ThemeDeleteGuard::SettingsStagedActive,
        ),
        (
            None,
            None,
            vec![target.clone()],
            vec![],
            ThemeDeleteGuard::OpenDocumentDraft,
        ),
        (
            None,
            None,
            vec![],
            vec![target.clone()],
            ThemeDeleteGuard::RepositoryOperation,
        ),
    ] {
        let snapshot =
            ThemeReferenceSnapshot::new(manifest, settings, durable, staged, drafts, operations)
                .unwrap();
        assert_eq!(snapshot.delete_guard(&target), Err(expected));
    }
}

#[test]
fn document_only_commit_requires_one_exact_manifest_binding() {
    let (_directory, _store, service) = service();
    let first = document(&service, ThemeManifestGeneration::INITIAL, "one", 1, b"one");
    let next = ThemeManifestGeneration::INITIAL.checked_next().unwrap();
    let second = document(&service, next, "two", 2, b"two");

    assert_eq!(
        ThemeRepositoryCommit::checked(None, vec![first, second]),
        Err(ThemeCommandError::Freshness(
            beryl_state::ThemeFreshnessError::StaleDocument,
        )),
    );
}

#[test]
fn invalid_startup_uses_fallback_and_invalid_live_edit_retains_prior() {
    let (_directory, _store, service) = service();
    let settings = service.settings_identity(DomainRevision::new(1).unwrap(), None);
    let active = InstalledThemeId::new("active").unwrap();
    let startup = ThemeStartupOutcome::evaluate(
        settings,
        Some(&active),
        Err(beryl_state::ThemeLoadFailure::DocumentInvalid),
    )
    .unwrap();
    assert_eq!(
        startup.appearance().source(),
        &ThemeAppearanceSource::BuiltinFallback(BuiltinFallback),
    );
    assert_eq!(
        startup.failure(),
        Some(&beryl_state::ThemeLoadFailure::DocumentInvalid),
    );

    let current = document(
        &service,
        ThemeManifestGeneration::INITIAL,
        active.as_str(),
        1,
        b"valid",
    );
    let prior = PreparedThemeAppearance::installed(
        settings,
        &active,
        current,
        builtin_fallback_appearance(),
    )
    .unwrap();
    let observed = document(
        &service,
        ThemeManifestGeneration::INITIAL,
        active.as_str(),
        2,
        b"invalid",
    );
    let retained = ThemeLiveEditOutcome::invalid(
        prior.clone(),
        Some(observed.clone()),
        ThemeLiveEditFailure::InvalidDocument,
    )
    .unwrap();
    assert_eq!(
        retained,
        ThemeLiveEditOutcome::Retained {
            coherent: prior,
            observed: Some(observed),
            failure: ThemeLiveEditFailure::InvalidDocument,
        },
    );
}
