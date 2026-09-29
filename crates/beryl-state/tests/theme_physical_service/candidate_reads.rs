use super::*;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeRecoveryCandidate};
use beryl_state::{BerylState, ThemeFreshnessError};

fn max_manifest() -> NonZeroU64 {
    NonZeroU64::new(1024 * 1024).unwrap()
}

fn seeded() -> (
    tempfile::TempDir,
    HomeStore,
    FaultController,
    ThemeService,
    ThemeRepositoryObservation,
    InstalledThemeSelection,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let service = BerylState::register(&mut candidate).unwrap().themes();
    let store = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let (repository, selection) = install_fixture(&store, &service, VALID_DOCUMENT);
    (directory, store, faults, service, repository, selection)
}

fn recover(store: HomeStore, faults: &FaultController) -> (HomeRecoveryCandidate, ThemeService) {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let candidate = store.recover_same_home().unwrap();
    let service = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .themes();
    (candidate, service)
}

fn observe(
    service: &ThemeService,
    access: &HomeCandidateRecoveryAccess<'_>,
) -> (ThemeRepositoryObservation, InstalledThemeSelection) {
    let repository = service
        .observe_repository_candidate(access, max_manifest(), manifest_read_limits(), None)
        .unwrap();
    let mut session = service
        .open_manifest_candidate(access, &repository, max_manifest(), manifest_read_limits())
        .unwrap();
    assert_eq!(service.diagnostics().active_manifest_sessions(), 1);
    let page = session
        .read_page(
            ThemeManifestCursor::first(repository.manifest()),
            ThemePageLimits::new(NonZeroUsize::MIN, NonZeroUsize::new(4096).unwrap()).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 1);
    (repository, page.selection(0).unwrap())
}

#[test]
fn initial_candidate_loads_installed_theme_before_publication() {
    let (directory, store, _, _, _, _) = seeded();
    store.close().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = BerylState::register(&mut candidate).unwrap().themes();
    let mut candidate = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap();
    let ordinary = candidate.service_reference();
    let access = candidate.recovery_access().unwrap();
    let (repository, selection) = observe(&service, &access);
    let loaded = service
        .load_document_candidate(&access, &repository, &selection, None)
        .unwrap();
    assert_eq!(
        loaded.identity().digest(),
        ThemeDocumentDigest::of_bytes(VALID_DOCUMENT)
    );
    assert!(
        service
            .observe_repository(&ordinary, max_manifest(), manifest_read_limits(), None)
            .is_err()
    );
    assert!(
        service
            .open_manifest(
                &ordinary,
                &repository,
                max_manifest(),
                manifest_read_limits()
            )
            .is_err()
    );
    assert!(
        service
            .load_document(&ordinary, &repository, &selection, None)
            .is_err()
    );
    assert_eq!(service.diagnostics().active_manifest_sessions(), 0);
    assert_eq!(service.diagnostics().document_loads_in_flight(), 0);
    drop(ordinary);
    let store = candidate.publish().unwrap();
    service
        .load_document(&store, &repository, &selection, Some(loaded.identity()))
        .unwrap();
    store.close().unwrap();
}

#[test]
fn recovered_theme_reads_reject_stale_and_foreign_services_and_observations() {
    let (_directory, store, faults, stale_service, stale, stale_selection) = seeded();
    let (_foreign_directory, foreign_store, _, foreign_service, foreign, foreign_selection) =
        seeded();
    let (mut candidate, service) = recover(store, &faults);
    let access = candidate.recovery_access().unwrap();
    let (repository, selection) = observe(&service, &access);
    service
        .load_document_candidate(&access, &repository, &selection, None)
        .unwrap();
    for wrong in [&stale_service, &foreign_service] {
        assert!(matches!(
            wrong.observe_repository_candidate(
                &access,
                max_manifest(),
                manifest_read_limits(),
                None
            ),
            Err(ThemeRepositoryLoadError::Freshness(
                ThemeFreshnessError::StaleOrForeignHome
            ))
        ));
        assert!(
            wrong
                .open_manifest_candidate(
                    &access,
                    &repository,
                    max_manifest(),
                    manifest_read_limits()
                )
                .is_err()
        );
        assert!(
            wrong
                .load_document_candidate(&access, &repository, &selection, None)
                .is_err()
        );
    }
    for (wrong, wrong_selection) in [(&stale, &stale_selection), (&foreign, &foreign_selection)] {
        assert!(
            service
                .observe_repository_candidate(
                    &access,
                    max_manifest(),
                    manifest_read_limits(),
                    Some(wrong)
                )
                .is_err()
        );
        assert!(
            service
                .open_manifest_candidate(&access, wrong, max_manifest(), manifest_read_limits())
                .is_err()
        );
        assert!(
            service
                .load_document_candidate(&access, wrong, wrong_selection, None)
                .is_err()
        );
        assert!(
            service
                .load_document_candidate(&access, &repository, wrong_selection, None)
                .is_err()
        );
    }
    assert_eq!(service.diagnostics().document_loads_in_flight(), 0);
    candidate.abort().close().unwrap();
    foreign_store.close().unwrap();
}

#[test]
fn candidate_theme_reads_keep_manifest_limits_and_bounded_pages() {
    let (directory, store, faults, _, _, _) = seeded();
    let manifest = populated_manifest(3, 3);
    fs::write(directory.path().join("themes/manifest.toml"), &manifest).unwrap();
    let (mut candidate, service) = recover(store, &faults);
    let access = candidate.recovery_access().unwrap();
    assert!(matches!(
        service.observe_repository_candidate(
            &access,
            NonZeroU64::new(manifest.len() as u64 - 1).unwrap(),
            manifest_read_limits(),
            None,
        ),
        Err(ThemeRepositoryLoadError::Manifest(
            ThemeManifestDecodeError::LimitExceeded(ThemeManifestLimit::EncodedBytes)
        ))
    ));
    let repository = service
        .observe_repository_candidate(&access, max_manifest(), manifest_read_limits(), None)
        .unwrap();
    {
        let mut session = service
            .open_manifest_candidate(&access, &repository, max_manifest(), manifest_read_limits())
            .unwrap();
        let mut cursor = ThemeManifestCursor::first(repository.manifest());
        let mut count = 0;
        loop {
            let page = session
                .read_page(
                    cursor,
                    ThemePageLimits::new(NonZeroUsize::MIN, NonZeroUsize::new(4096).unwrap())
                        .unwrap(),
                )
                .unwrap();
            assert!(page.records().len() <= 1);
            count += page.records().len();
            match page.next() {
                Some(next) => cursor = next,
                None => break,
            }
        }
        assert_eq!(count, 3);
    }
    assert_eq!(service.diagnostics().active_manifest_sessions(), 0);
    candidate.abort().close().unwrap();
}

#[test]
fn candidate_reports_invalid_document_and_rejects_changed_manifest() {
    let (directory, store, faults, _, _, _) = seeded();
    let (mut candidate, service) = recover(store, &faults);
    let access = candidate.recovery_access().unwrap();
    let (repository, selection) = observe(&service, &access);
    let invalid = b"invalid compact theme";
    fs::write(
        directory.path().join("themes/installed/active.toml"),
        invalid,
    )
    .unwrap();
    match service.load_document_candidate(&access, &repository, &selection, None) {
        Err(ThemeDocumentLoadError::Invalid { identity, .. }) => {
            assert_eq!(identity.digest(), ThemeDocumentDigest::of_bytes(invalid));
        }
        other => panic!("expected invalid document, got {other:?}"),
    }
    fs::write(
        directory.path().join("themes/manifest.toml"),
        populated_manifest(1, 3),
    )
    .unwrap();
    assert!(
        service
            .open_manifest_candidate(&access, &repository, max_manifest(), manifest_read_limits())
            .is_err()
    );
    assert!(
        service
            .load_document_candidate(&access, &repository, &selection, None)
            .is_err()
    );
    assert_eq!(service.diagnostics().document_loads_in_flight(), 0);
    candidate.abort().close().unwrap();
}

#[test]
fn failed_candidate_rejects_all_new_typed_reads() {
    let (_directory, store, faults, _, _, _) = seeded();
    let (mut candidate, service) = recover(store, &faults);
    let access = candidate.recovery_access().unwrap();
    let (repository, selection) = observe(&service, &access);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(access.home_revision().is_err());
    assert!(
        service
            .observe_repository_candidate(&access, max_manifest(), manifest_read_limits(), None)
            .is_err()
    );
    assert!(
        service
            .open_manifest_candidate(&access, &repository, max_manifest(), manifest_read_limits())
            .is_err()
    );
    assert!(
        service
            .load_document_candidate(&access, &repository, &selection, None)
            .is_err()
    );
    assert_eq!(service.diagnostics().active_manifest_sessions(), 0);
    assert_eq!(service.diagnostics().document_loads_in_flight(), 0);
    candidate.abort().close().unwrap();
}

#[test]
fn empty_candidate_manifest_requires_current_healthy_candidate_access() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let service = BerylState::register(&mut candidate).unwrap().themes();
    let mut candidate = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap();
    let access = candidate.recovery_access().unwrap();
    let repository = service
        .observe_repository_candidate(&access, max_manifest(), manifest_read_limits(), None)
        .unwrap();
    assert!(!repository.is_initialized());
    {
        let mut session = service
            .open_manifest_candidate(&access, &repository, max_manifest(), manifest_read_limits())
            .unwrap();
        let page = session
            .read_page(
                ThemeManifestCursor::first(repository.manifest()),
                ThemePageLimits::new(NonZeroUsize::MIN, NonZeroUsize::new(4096).unwrap()).unwrap(),
            )
            .unwrap();
        assert!(page.records().is_empty());
        assert!(page.next().is_none());
    }
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(access.home_revision().is_err());
    assert!(
        service
            .open_manifest_candidate(&access, &repository, max_manifest(), manifest_read_limits())
            .is_err()
    );
    assert_eq!(service.diagnostics().active_manifest_sessions(), 0);
    candidate.close().unwrap();
}
