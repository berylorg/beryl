use super::*;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeStore, ThemeRepositorySnapshot};

const DOCUMENT: &[u8] = b"bounded installed theme bytes";
const MANIFEST: &[u8] = b"bounded manifest bytes";

fn selector() -> ThemeFileSelector {
    ThemeFileSelector::Document(StableThemeFileId::new("candidate-theme").unwrap())
}

fn seeded() -> (
    tempfile::TempDir,
    HomeStore,
    FaultController,
    ThemeRepositorySnapshot,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), faults.clone())
        .prepare_publication(HomeDomainRequirements::new())
        .unwrap()
        .publish()
        .unwrap();
    let empty = store.theme_repository_snapshot(limits()).unwrap();
    assert!(matches!(
        store
            .install_theme_document(
                &empty,
                &StableThemeFileId::new("candidate-theme").unwrap(),
                None,
                identity(DOCUMENT),
                &mut Cursor::new(DOCUMENT),
                identity(MANIFEST),
                &mut Cursor::new(MANIFEST),
                limits(),
            )
            .unwrap(),
        ThemeMutationOutcome::Committed(_)
    ));
    let snapshot = store.theme_repository_snapshot(limits()).unwrap();
    (directory, store, faults, snapshot)
}

fn verify_private_reads(access: &HomeCandidateRecoveryAccess<'_>, ordinary: &HomeStore) {
    let snapshot = access.theme_repository_snapshot(limits()).unwrap();
    assert_eq!(snapshot.manifest_identity(), Some(identity(MANIFEST)));
    for (selector, bytes) in [
        (ThemeFileSelector::Manifest, MANIFEST),
        (selector(), DOCUMENT),
    ] {
        assert_eq!(
            access
                .observe_theme_file(&snapshot, &selector, limits())
                .unwrap(),
            identity(bytes)
        );
        let range = access
            .read_theme_file_range(
                &snapshot,
                &selector,
                identity(bytes),
                3,
                NonZeroUsize::new(7).unwrap(),
                limits(),
            )
            .unwrap();
        assert_eq!(range.bytes(), &bytes[3..10]);
        assert!(!range.eof());
        assert!(
            ordinary
                .observe_theme_file(&snapshot, &selector, limits())
                .is_err()
        );
        assert!(
            ordinary
                .read_theme_file_range(
                    &snapshot,
                    &selector,
                    identity(bytes),
                    0,
                    NonZeroUsize::new(7).unwrap(),
                    limits(),
                )
                .is_err()
        );
    }
    assert!(ordinary.theme_repository_snapshot(limits()).is_err());
}

#[test]
fn initial_candidate_reads_existing_theme_files_without_ordinary_admission() {
    let (directory, store, _, _) = seeded();
    store.close().unwrap();
    let mut candidate = open(directory.path(), FaultController::new())
        .prepare_publication(HomeDomainRequirements::new())
        .unwrap();
    let ordinary = candidate.service_reference();
    verify_private_reads(&candidate.recovery_access().unwrap(), &ordinary);
    let store = candidate.publish().unwrap();
    assert_eq!(
        store
            .theme_repository_snapshot(limits())
            .unwrap()
            .manifest_identity(),
        Some(identity(MANIFEST))
    );
    drop(ordinary);
    store.close().unwrap();
}

#[test]
fn recovered_candidate_rejects_old_and_foreign_theme_snapshots() {
    let (_directory, store, faults, stale) = seeded();
    let (_foreign_directory, foreign, _, foreign_snapshot) = seeded();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut candidate = store.recover_same_home().unwrap();
    let ordinary = candidate.service_reference();
    let access = candidate.recovery_access().unwrap();
    verify_private_reads(&access, &ordinary);
    for snapshot in [&stale, &foreign_snapshot] {
        assert!(matches!(
            access.observe_theme_file(snapshot, &selector(), limits()),
            Err(ThemeRepositoryError::StaleSnapshot)
        ));
        assert!(matches!(
            access.read_theme_file_range(
                snapshot,
                &selector(),
                identity(DOCUMENT),
                0,
                NonZeroUsize::new(7).unwrap(),
                limits()
            ),
            Err(ThemeRepositoryError::StaleSnapshot)
        ));
    }
    drop(ordinary);
    candidate.abort().close().unwrap();
    foreign.close().unwrap();
}

#[test]
fn candidate_theme_reads_preserve_limits_and_changed_file_rejection() {
    let (directory, store, faults, _) = seeded();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut candidate = store.recover_same_home().unwrap();
    let access = candidate.recovery_access().unwrap();
    let snapshot = access.theme_repository_snapshot(limits()).unwrap();
    for (offset, count) in [(0, 64 * 1024 + 1), (DOCUMENT.len() as u64 + 1, 7)] {
        assert!(matches!(
            access.read_theme_file_range(
                &snapshot,
                &selector(),
                identity(DOCUMENT),
                offset,
                NonZeroUsize::new(count).unwrap(),
                limits()
            ),
            Err(ThemeRepositoryError::LimitExceeded)
        ));
    }
    let eof = access
        .read_theme_file_range(
            &snapshot,
            &selector(),
            identity(DOCUMENT),
            DOCUMENT.len() as u64,
            NonZeroUsize::new(7).unwrap(),
            limits(),
        )
        .unwrap();
    assert!(eof.eof());
    assert!(eof.bytes().is_empty());
    fs::write(
        directory
            .path()
            .join("themes/installed/candidate-theme.toml"),
        b"changed",
    )
    .unwrap();
    assert!(matches!(
        access.read_theme_file_range(
            &snapshot,
            &selector(),
            identity(DOCUMENT),
            0,
            NonZeroUsize::new(7).unwrap(),
            limits()
        ),
        Err(ThemeRepositoryError::IdentityMismatch)
    ));
    fs::write(
        directory.path().join("themes/manifest.toml"),
        b"changed manifest",
    )
    .unwrap();
    assert!(matches!(
        access.observe_theme_file(&snapshot, &selector(), limits()),
        Err(ThemeRepositoryError::StaleSnapshot)
    ));
    candidate.abort().close().unwrap();
}

#[test]
fn failed_candidate_rejects_theme_reads_through_retained_access() {
    let (_directory, store, faults, _) = seeded();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut candidate = store.recover_same_home().unwrap();
    let access = candidate.recovery_access().unwrap();
    let snapshot = access.theme_repository_snapshot(limits()).unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(access.home_revision().is_err());
    assert!(access.theme_repository_snapshot(limits()).is_err());
    assert!(
        access
            .observe_theme_file(&snapshot, &selector(), limits())
            .is_err()
    );
    assert!(
        access
            .read_theme_file_range(
                &snapshot,
                &selector(),
                identity(DOCUMENT),
                0,
                NonZeroUsize::new(7).unwrap(),
                limits()
            )
            .is_err()
    );
    candidate.abort().close().unwrap();
}
