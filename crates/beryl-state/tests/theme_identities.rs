use std::num::NonZeroUsize;

use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_state::{
    InstalledThemeId, ThemeDocumentDigest, ThemeManifestGeneration, ThemePageLimits, ThemeService,
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
fn document_observations_are_service_scoped_and_advance_after_digest_reversion() {
    let (_directory, _store, service) = service();
    let manifest = service.manifest(ThemeManifestGeneration::INITIAL);
    let id = InstalledThemeId::new("operator-theme").unwrap();
    let digest_a = ThemeDocumentDigest::of_bytes(b"a");
    let digest_b = ThemeDocumentDigest::of_bytes(b"b");

    let first = service
        .observe_document(manifest, id.clone(), None, 1, digest_a)
        .unwrap();
    let duplicate = service
        .observe_document(manifest, id.clone(), Some(&first), 1, digest_a)
        .unwrap();
    let changed = service
        .observe_document(manifest, id.clone(), Some(&duplicate), 1, digest_b)
        .unwrap();
    let reverted = service
        .observe_document(manifest, id, Some(&changed), 1, digest_a)
        .unwrap();

    assert_eq!(first.revision(), duplicate.revision());
    assert!(changed.revision() > duplicate.revision());
    assert!(reverted.revision() > changed.revision());
    assert_eq!(first.digest(), reverted.digest());
}

#[test]
fn manifest_page_limits_are_explicitly_bounded() {
    let limits = ThemePageLimits::new(
        NonZeroUsize::new(2).unwrap(),
        NonZeroUsize::new(1024).unwrap(),
    )
    .unwrap();
    assert_eq!(limits.max_items(), 2);
    assert_eq!(limits.max_decoded_bytes(), 1024);
    assert!(
        ThemePageLimits::new(
            NonZeroUsize::new(129).unwrap(),
            NonZeroUsize::new(1024).unwrap(),
        )
        .is_err()
    );
}

#[test]
fn stable_ids_reject_path_syntax_and_invalid_edges() {
    for invalid in ["", "-theme", "theme-", "Theme", "theme.toml", "../theme"] {
        assert!(
            InstalledThemeId::new(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
    assert!(InstalledThemeId::new("theme-2").is_ok());
}
