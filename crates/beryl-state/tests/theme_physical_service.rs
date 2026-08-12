use std::{
    fs,
    io::Cursor,
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
    thread,
    time::Duration,
};

use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion, HomeStore, StableThemeFileId, ThemeFileIdentity,
    ThemeMutationOutcome, ThemeOperationLimits,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    InstalledThemeSelection, ThemeDocumentDigest, ThemeDocumentLoadError, ThemeManifestCursor,
    ThemeManifestGeneration, ThemeManifestReadLimits, ThemePageLimits, ThemeRepositoryLoadError,
    ThemeRepositoryObservation, ThemeService,
};

const VALID_DOCUMENT: &[u8] = br##"schema = 1
id = "active"
name = "Active"

[[role]]
id = "app.window"
background = "#102030"
"##;

fn operation_limits() -> ThemeOperationLimits {
    ThemeOperationLimits::new(
        1024 * 1024,
        NonZeroUsize::new(64 * 1024).unwrap(),
        NonZeroUsize::new(2).unwrap(),
        NonZeroUsize::new(4).unwrap(),
        NonZeroUsize::new(512).unwrap(),
    )
    .unwrap()
}

fn manifest_read_limits() -> ThemeManifestReadLimits {
    ThemeManifestReadLimits::new(
        NonZeroUsize::new(4096).unwrap(),
        NonZeroUsize::new(16 * 1024).unwrap(),
        NonZeroUsize::new(256 * 1024).unwrap(),
    )
    .unwrap()
}

fn identity(bytes: &[u8]) -> ThemeFileIdentity {
    let digest = ThemeDocumentDigest::of_bytes(bytes);
    ThemeFileIdentity::new(bytes.len() as u64, *digest.as_bytes())
}

fn manifest_bytes(service: &ThemeService) -> Vec<u8> {
    let generation = ThemeManifestGeneration::INITIAL.checked_next().unwrap();
    assert_eq!(service.manifest(generation).generation(), generation);
    b"schema_version = 1\ngeneration = 2\n\n[[theme]]\nid = \"active\"\nname = \"Active\"\n"
        .to_vec()
}

fn install_fixture(
    store: &HomeStore,
    service: &ThemeService,
    document: &[u8],
) -> (ThemeRepositoryObservation, InstalledThemeSelection) {
    let snapshot = store.theme_repository_snapshot(operation_limits()).unwrap();
    let manifest = manifest_bytes(service);
    store
        .install_theme_document(
            &snapshot,
            &StableThemeFileId::new("active").unwrap(),
            None,
            identity(document),
            &mut Cursor::new(document),
            identity(&manifest),
            &mut Cursor::new(&manifest),
            operation_limits(),
        )
        .unwrap();
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let repository = service
        .observe_repository(store, max, manifest_read_limits(), None)
        .unwrap();
    let mut session = service
        .open_manifest(store, &repository, max, manifest_read_limits())
        .unwrap();
    let page = session
        .read_page(
            ThemeManifestCursor::first(repository.manifest()),
            ThemePageLimits::new(
                NonZeroUsize::new(8).unwrap(),
                NonZeroUsize::new(4096).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    (repository, page.selection(0).unwrap())
}

#[test]
fn state_service_streams_manifest_and_document_through_physical_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = beryl_state::BerylState::register(&mut store).unwrap();
    let service = state.themes();
    let max_manifest_bytes = NonZeroU64::new(1024 * 1024).unwrap();
    assert!(service.diagnostics().home_generation_present());
    assert!(!service.diagnostics().repository_generation_present());

    let empty = service
        .observe_repository(&store, max_manifest_bytes, manifest_read_limits(), None)
        .unwrap();
    assert!(service.diagnostics().repository_generation_present());
    assert!(!empty.is_initialized());
    assert_eq!(
        empty.manifest().generation(),
        ThemeManifestGeneration::INITIAL
    );

    let physical = store.theme_repository_snapshot(operation_limits()).unwrap();
    let manifest = manifest_bytes(&service);
    let mut document_source = Cursor::new(VALID_DOCUMENT);
    let mut manifest_source = Cursor::new(&manifest);
    let outcome = store
        .install_theme_document(
            &physical,
            &StableThemeFileId::new("active").unwrap(),
            None,
            identity(VALID_DOCUMENT),
            &mut document_source,
            identity(&manifest),
            &mut manifest_source,
            operation_limits(),
        )
        .unwrap();
    assert!(matches!(outcome, ThemeMutationOutcome::Committed(_)));

    let observed_repository = service
        .observe_repository(
            &store,
            max_manifest_bytes,
            manifest_read_limits(),
            Some(&empty),
        )
        .unwrap();
    assert!(observed_repository.is_initialized());
    assert_eq!(
        observed_repository.manifest().generation(),
        ThemeManifestGeneration::INITIAL.checked_next().unwrap(),
    );

    let mut manifest_session = service
        .open_manifest(
            &store,
            &observed_repository,
            max_manifest_bytes,
            manifest_read_limits(),
        )
        .unwrap();
    assert_eq!(service.diagnostics().active_manifest_sessions(), 1);
    let page_limits = ThemePageLimits::new(
        NonZeroUsize::new(8).unwrap(),
        NonZeroUsize::new(1024).unwrap(),
    )
    .unwrap();
    let page = manifest_session
        .read_page(
            beryl_state::ThemeManifestCursor::first(observed_repository.manifest()),
            page_limits,
        )
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert_eq!(page.records()[0].id().as_str(), "active");
    let selection = page.selection(0).unwrap();
    drop(manifest_session);
    assert_eq!(service.diagnostics().active_manifest_sessions(), 0);

    let loaded = service
        .load_document(&store, &observed_repository, &selection, None)
        .unwrap();
    assert_eq!(loaded.document().name(), Some("Active"));
    assert_eq!(loaded.identity().byte_length(), VALID_DOCUMENT.len() as u64);
    assert_eq!(service.diagnostics().document_loads_in_flight(), 0);
}

#[test]
fn subscription_diagnostics_release_after_orderly_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let subscription = service
        .subscribe_changes(
            &store,
            Duration::from_millis(20),
            NonZeroUsize::new(2).unwrap(),
            NonZeroUsize::new(8).unwrap(),
            NonZeroU64::new(1024 * 1024).unwrap(),
        )
        .unwrap();
    assert_eq!(service.diagnostics().active_subscriptions(), 1);
    subscription.shutdown();
    assert_eq!(service.diagnostics().active_subscriptions(), 0);
}

#[test]
fn document_replacement_during_streaming_cannot_publish() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = Arc::new(
        HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap(),
    );
    let service = ThemeService::acquire(&store).unwrap();
    let mut large = VALID_DOCUMENT.to_vec();
    for _ in 0..1_800 {
        large.extend_from_slice(b"# bounded padding for a second physical range\n");
    }
    let (repository, selection) = install_fixture(&store, &service, &large);
    let first_range = faults.block_next(FaultPoint::BeforeThemeRead);
    let second_range = faults.block_next(FaultPoint::BeforeThemeRead);
    let worker_store = Arc::clone(&store);
    let worker_service = service.clone();
    let worker = thread::spawn(move || {
        worker_service.load_document(&worker_store, &repository, &selection, None)
    });
    assert!(first_range.wait_until_reached(Duration::from_secs(10)));
    first_range.release();
    assert!(second_range.wait_until_reached(Duration::from_secs(10)));
    fs::write(
        directory.path().join("themes/installed/active.toml"),
        VALID_DOCUMENT,
    )
    .unwrap();
    second_range.release();
    assert!(matches!(
        worker.join().unwrap().unwrap_err(),
        ThemeDocumentLoadError::Repository(_)
    ));
    assert_eq!(service.diagnostics().document_loads_in_flight(), 0);
    assert_eq!(service.diagnostics().document_load_retry_rejections(), 1);
}

#[test]
fn manifest_replacement_after_document_streaming_cannot_publish() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = Arc::new(
        HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap(),
    );
    let service = ThemeService::acquire(&store).unwrap();
    let (repository, selection) = install_fixture(&store, &service, VALID_DOCUMENT);
    let document_range = faults.block_next(FaultPoint::BeforeThemeRead);
    let final_manifest_range = faults.block_next(FaultPoint::BeforeThemeRead);
    let worker_store = Arc::clone(&store);
    let worker_service = service.clone();
    let worker = thread::spawn(move || {
        worker_service.load_document(&worker_store, &repository, &selection, None)
    });
    assert!(document_range.wait_until_reached(Duration::from_secs(10)));
    document_range.release();
    assert!(final_manifest_range.wait_until_reached(Duration::from_secs(10)));
    fs::write(
        directory.path().join("themes/manifest.toml"),
        b"schema_version = 1\ngeneration = 3\n",
    )
    .unwrap();
    final_manifest_range.release();
    assert!(worker.join().unwrap().is_err());
    assert_eq!(service.diagnostics().document_loads_in_flight(), 0);
    assert_eq!(service.diagnostics().document_load_retry_rejections(), 1);
}

#[test]
fn invalid_physical_edit_reports_its_exact_observation_identity() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let max_manifest_bytes = NonZeroU64::new(1024 * 1024).unwrap();
    let initial_snapshot = store.theme_repository_snapshot(operation_limits()).unwrap();
    let manifest = manifest_bytes(&service);
    let mut document_source = Cursor::new(VALID_DOCUMENT);
    let mut manifest_source = Cursor::new(&manifest);
    store
        .install_theme_document(
            &initial_snapshot,
            &StableThemeFileId::new("active").unwrap(),
            None,
            identity(VALID_DOCUMENT),
            &mut document_source,
            identity(&manifest),
            &mut manifest_source,
            operation_limits(),
        )
        .unwrap();
    let repository = service
        .observe_repository(&store, max_manifest_bytes, manifest_read_limits(), None)
        .unwrap();
    let mut manifest_session = service
        .open_manifest(
            &store,
            &repository,
            max_manifest_bytes,
            manifest_read_limits(),
        )
        .unwrap();
    let page_limits = ThemePageLimits::new(
        NonZeroUsize::new(8).unwrap(),
        NonZeroUsize::new(1024).unwrap(),
    )
    .unwrap();
    let page = manifest_session
        .read_page(
            beryl_state::ThemeManifestCursor::first(repository.manifest()),
            page_limits,
        )
        .unwrap();
    let selection = page.selection(0).unwrap();
    let valid = service
        .load_document(&store, &repository, &selection, None)
        .unwrap();

    let invalid_bytes = b"schema = 1\n[[role]]\nid =";
    let physical_snapshot = store.theme_repository_snapshot(operation_limits()).unwrap();
    let mut invalid_source = Cursor::new(invalid_bytes);
    store
        .replace_theme_document(
            &physical_snapshot,
            &StableThemeFileId::new("active").unwrap(),
            Some(identity(VALID_DOCUMENT)),
            identity(invalid_bytes),
            &mut invalid_source,
            operation_limits(),
        )
        .unwrap();

    let error = service
        .load_document(&store, &repository, &selection, Some(valid.identity()))
        .unwrap_err();
    let ThemeDocumentLoadError::Invalid { identity, .. } = error else {
        panic!("invalid edit did not retain an exact observed identity");
    };
    assert!(identity.revision() > valid.identity().revision());
    assert_eq!(identity.byte_length(), invalid_bytes.len() as u64);
}

#[test]
fn duplicate_manifest_ids_are_rejected_without_materializing_the_collection() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let manifest = b"schema_version = 1\ngeneration = 2\n\n[[theme]]\nid = \"same\"\nname = \"One\"\n\n[[theme]]\nid = \"same\"\nname = \"Two\"\n";
    let snapshot = store.theme_repository_snapshot(operation_limits()).unwrap();
    let mut source = Cursor::new(manifest);
    store
        .replace_theme_manifest(
            &snapshot,
            identity(manifest),
            &mut source,
            operation_limits(),
        )
        .unwrap();

    let error = service
        .observe_repository(
            &store,
            NonZeroU64::new(1024 * 1024).unwrap(),
            manifest_read_limits(),
            None,
        )
        .unwrap_err();
    assert!(matches!(error, ThemeRepositoryLoadError::Manifest(_)));
}

#[test]
fn same_generation_manifest_rewrite_cannot_reuse_prior_page_identity() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let first = manifest_bytes(&service);
    let snapshot = store.theme_repository_snapshot(operation_limits()).unwrap();
    let mut first_source = Cursor::new(&first);
    store
        .replace_theme_manifest(
            &snapshot,
            identity(&first),
            &mut first_source,
            operation_limits(),
        )
        .unwrap();
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let prior = service
        .observe_repository(&store, max, manifest_read_limits(), None)
        .unwrap();
    let second =
        b"schema_version = 1\ngeneration = 2\n\n[[theme]]\nid = \"other\"\nname = \"Other\"\n";
    let snapshot = store.theme_repository_snapshot(operation_limits()).unwrap();
    let mut second_source = Cursor::new(second);
    store
        .replace_theme_manifest(
            &snapshot,
            identity(second),
            &mut second_source,
            operation_limits(),
        )
        .unwrap();

    let error = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&prior))
        .unwrap_err();
    assert!(matches!(error, ThemeRepositoryLoadError::Freshness(_)));
}
