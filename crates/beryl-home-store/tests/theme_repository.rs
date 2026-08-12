use std::{fs, io::Cursor, num::NonZeroUsize};

use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion, HomeStore, StableThemeFileId, ThemeFileIdentity,
    ThemeFileSelector, ThemeMutationOutcome, ThemeOperationLimits, ThemeReconciliationOutcome,
    ThemeRepositoryError,
    test_faults::{FaultController, FaultPoint},
};
use sha2::{Digest, Sha256};

fn limits() -> ThemeOperationLimits {
    ThemeOperationLimits::new(
        64 * 1024,
        NonZeroUsize::new(31).unwrap(),
        NonZeroUsize::new(2).unwrap(),
        NonZeroUsize::new(4).unwrap(),
        NonZeroUsize::new(512).unwrap(),
    )
    .unwrap()
}

fn identity(bytes: &[u8]) -> ThemeFileIdentity {
    ThemeFileIdentity::new(bytes.len() as u64, Sha256::digest(bytes).into())
}

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn stable_ids_enforce_the_exact_filename_grammar() {
    for valid in ["a", "0", "theme-2", "a1-b2", &"a".repeat(64)] {
        assert_eq!(StableThemeFileId::new(valid).unwrap().as_str(), valid);
    }
    for invalid in ["", "-a", "a-", "A", "a_b", "a.toml", "two--/bad"] {
        assert!(StableThemeFileId::new(invalid).is_err(), "{invalid}");
    }
    assert!(StableThemeFileId::new("a".repeat(65)).is_err());
}

#[test]
fn empty_snapshot_install_and_ranges_use_only_the_physical_theme_layout() {
    let directory = tempfile::tempdir().unwrap();
    let root_theme = directory.path().join("theme.toml");
    fs::write(&root_theme, b"outside-boundary").unwrap();
    let store = open(directory.path(), FaultController::new());
    let empty = store.theme_repository_snapshot(limits()).unwrap();
    assert_eq!(empty.manifest_identity(), None);

    let id = StableThemeFileId::new("ocean-dark").unwrap();
    let document = b"[colors]\nbackground = '#001122'\n";
    let manifest = b"opaque manifest bytes";
    let outcome = store
        .install_theme_document(
            &empty,
            &id,
            None,
            identity(document),
            &mut Cursor::new(document),
            identity(manifest),
            &mut Cursor::new(manifest),
            limits(),
        )
        .unwrap();
    let committed = match outcome {
        ThemeMutationOutcome::Committed(evidence) => evidence,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(fs::read(&root_theme).unwrap(), b"outside-boundary");
    assert_eq!(
        committed.snapshot().manifest_identity(),
        Some(identity(manifest))
    );
    assert_eq!(committed.document().unwrap().1, identity(document));
    assert_eq!(
        store
            .observe_theme_file(
                committed.snapshot(),
                &ThemeFileSelector::Document(id.clone()),
                limits(),
            )
            .unwrap(),
        identity(document)
    );

    let range = store
        .read_theme_file_range(
            committed.snapshot(),
            &ThemeFileSelector::Document(id.clone()),
            identity(document),
            3,
            NonZeroUsize::new(7).unwrap(),
            limits(),
        )
        .unwrap();
    assert_eq!(range.offset(), 3);
    assert_eq!(range.total_length(), document.len() as u64);
    assert_eq!(range.bytes(), &document[3..10]);
    assert!(!range.eof());
    assert!(matches!(
        store.read_theme_file_range(
            &empty,
            &ThemeFileSelector::Manifest,
            identity(manifest),
            0,
            NonZeroUsize::new(1).unwrap(),
            limits(),
        ),
        Err(ThemeRepositoryError::StaleSnapshot)
    ));

    let other = tempfile::tempdir().unwrap();
    let foreign = open(other.path(), FaultController::new());
    assert!(matches!(
        foreign.read_theme_file_range(
            committed.snapshot(),
            &ThemeFileSelector::Document(id),
            identity(document),
            0,
            NonZeroUsize::new(1).unwrap(),
            limits(),
        ),
        Err(ThemeRepositoryError::StaleSnapshot)
    ));
}

#[test]
fn staged_sources_reject_short_extra_digest_and_limit_mismatches() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path(), FaultController::new());
    let snapshot = store.theme_repository_snapshot(limits()).unwrap();
    let id = StableThemeFileId::new("source-check").unwrap();
    let expected = identity(b"abcd");

    for bytes in [&b"abc"[..], &b"abcde"[..], &b"abce"[..]] {
        assert!(matches!(
            store.replace_theme_document(
                &snapshot,
                &id,
                None,
                expected,
                &mut Cursor::new(bytes),
                limits(),
            ),
            Err(ThemeRepositoryError::SourceMismatch)
        ));
    }
    let too_large = ThemeFileIdentity::new(limits().max_source_bytes() + 1, [0; 32]);
    assert!(matches!(
        store.replace_theme_document(
            &snapshot,
            &id,
            None,
            too_large,
            &mut Cursor::new([]),
            limits(),
        ),
        Err(ThemeRepositoryError::LimitExceeded)
    ));
    assert_eq!(
        store
            .theme_repository_snapshot(limits())
            .unwrap()
            .manifest_identity(),
        None
    );
}

#[test]
fn document_only_and_manifest_only_replacements_keep_the_other_file_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path(), FaultController::new());
    let id = StableThemeFileId::new("retained").unwrap();
    let first_document = b"first";
    let first_manifest = b"listed";
    let empty = store.theme_repository_snapshot(limits()).unwrap();
    let installed = store
        .install_theme_document(
            &empty,
            &id,
            None,
            identity(first_document),
            &mut Cursor::new(first_document),
            identity(first_manifest),
            &mut Cursor::new(first_manifest),
            limits(),
        )
        .unwrap();
    let installed = match installed {
        ThemeMutationOutcome::Committed(value) => value,
        other => panic!("{other:?}"),
    };

    let second_document = b"second";
    let saved = store
        .replace_theme_document(
            installed.snapshot(),
            &id,
            Some(identity(first_document)),
            identity(second_document),
            &mut Cursor::new(second_document),
            limits(),
        )
        .unwrap();
    let saved = match saved {
        ThemeMutationOutcome::Committed(value) => value,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        saved.snapshot().manifest_identity(),
        Some(identity(first_manifest))
    );
    assert_eq!(
        fs::read(directory.path().join("themes/manifest.toml")).unwrap(),
        first_manifest
    );

    let empty_manifest = b"unlisted";
    let deleted = store
        .replace_theme_manifest(
            saved.snapshot(),
            identity(empty_manifest),
            &mut Cursor::new(empty_manifest),
            limits(),
        )
        .unwrap();
    assert!(matches!(deleted, ThemeMutationOutcome::Committed(_)));
    assert_eq!(
        fs::read(directory.path().join("themes/installed/retained.toml")).unwrap(),
        second_document
    );
}

#[test]
fn indeterminate_document_reconciles_after_a_fresh_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), faults.clone());
    let snapshot = store.theme_repository_snapshot(limits()).unwrap();
    let id = StableThemeFileId::new("reconcile-me").unwrap();
    faults.fail_next(FaultPoint::AfterThemeDocumentReplace);
    let bytes = b"published before later cut";
    let outcome = store
        .replace_theme_document(
            &snapshot,
            &id,
            None,
            identity(bytes),
            &mut Cursor::new(bytes),
            limits(),
        )
        .unwrap();
    let evidence = match outcome {
        ThemeMutationOutcome::Indeterminate(value) => value,
        other => panic!("{other:?}"),
    };
    let home_id = store.home_id();
    store.close().unwrap();

    let reopened = open(directory.path(), FaultController::new());
    assert_eq!(reopened.home_id(), home_id);
    let reconciled = reopened
        .reconcile_theme_mutation(&evidence, limits())
        .unwrap();
    let committed = match reconciled {
        ThemeReconciliationOutcome::ExactNew(value) => value,
        other => panic!("{other:?}"),
    };
    assert_eq!(committed.document().unwrap().1, identity(bytes));
    assert_eq!(committed.snapshot().home_id(), home_id);
}

#[test]
fn manifest_last_fault_leaves_the_new_document_inert() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), faults.clone());
    let snapshot = store.theme_repository_snapshot(limits()).unwrap();
    faults.fail_next(FaultPoint::BeforeThemeManifestReplace);
    let id = StableThemeFileId::new("inert").unwrap();
    let document = b"complete document";
    let manifest = b"would list inert";
    let outcome = store
        .install_theme_document(
            &snapshot,
            &id,
            None,
            identity(document),
            &mut Cursor::new(document),
            identity(manifest),
            &mut Cursor::new(manifest),
            limits(),
        )
        .unwrap();
    assert!(matches!(outcome, ThemeMutationOutcome::NotCommitted));
    assert_eq!(
        store
            .theme_repository_snapshot(limits())
            .unwrap()
            .manifest_identity(),
        None
    );
    assert_eq!(
        fs::read(directory.path().join("themes/installed/inert.toml")).unwrap(),
        document
    );
}

#[test]
fn delete_commits_manifest_first_and_cleanup_failure_remains_committed() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), faults.clone());
    let id = StableThemeFileId::new("delete-me").unwrap();
    let document = b"retained only on cleanup failure";
    let listed = b"listed";
    let empty = store.theme_repository_snapshot(limits()).unwrap();
    let installed = store
        .install_theme_document(
            &empty,
            &id,
            None,
            identity(document),
            &mut Cursor::new(document),
            identity(listed),
            &mut Cursor::new(listed),
            limits(),
        )
        .unwrap();
    let installed = match installed {
        ThemeMutationOutcome::Committed(value) => value,
        other => panic!("{other:?}"),
    };
    let unlisted = b"unlisted";
    faults.fail_next(FaultPoint::BeforeThemeDocumentRemove);
    let deleted = store
        .delete_theme_document(
            installed.snapshot(),
            &id,
            identity(document),
            identity(unlisted),
            &mut Cursor::new(unlisted),
            limits(),
        )
        .unwrap();
    let evidence = match deleted {
        ThemeMutationOutcome::Committed(value) => value,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        evidence.later_failure(),
        Some(beryl_home_store::ThemeRepositoryStage::DocumentRemove)
    );
    assert_eq!(
        fs::read(directory.path().join("themes/manifest.toml")).unwrap(),
        unlisted
    );
    assert_eq!(
        fs::read(directory.path().join("themes/installed/delete-me.toml")).unwrap(),
        document
    );
}

#[test]
fn deterministic_document_fault_cuts_classify_exactly() {
    let before_points = [
        FaultPoint::BeforeThemeDocumentWrite,
        FaultPoint::BeforeThemeDocumentSync,
        FaultPoint::BeforeThemeDocumentReplace,
    ];
    for point in before_points {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let store = open(directory.path(), faults.clone());
        let snapshot = store.theme_repository_snapshot(limits()).unwrap();
        let id = StableThemeFileId::new("fault-cut").unwrap();
        let bytes = b"new";
        faults.fail_next(point);
        let outcome = store
            .replace_theme_document(
                &snapshot,
                &id,
                None,
                identity(bytes),
                &mut Cursor::new(bytes),
                limits(),
            )
            .unwrap();
        assert!(
            matches!(outcome, ThemeMutationOutcome::NotCommitted),
            "{point:?}"
        );
    }

    for point in [
        FaultPoint::AfterThemeDocumentReplace,
        FaultPoint::BeforeThemeInstalledDirectorySync,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let store = open(directory.path(), faults.clone());
        let snapshot = store.theme_repository_snapshot(limits()).unwrap();
        let id = StableThemeFileId::new("fault-cut").unwrap();
        let bytes = b"new";
        faults.fail_next(point);
        let outcome = store
            .replace_theme_document(
                &snapshot,
                &id,
                None,
                identity(bytes),
                &mut Cursor::new(bytes),
                limits(),
            )
            .unwrap();
        assert!(
            matches!(outcome, ThemeMutationOutcome::Indeterminate(_)),
            "{point:?}"
        );
    }
}

#[test]
fn manifest_indeterminate_reconciles_exact_new_or_collision() {
    for collide in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let store = open(directory.path(), faults.clone());
        let snapshot = store.theme_repository_snapshot(limits()).unwrap();
        let manifest = b"intended";
        faults.fail_next(FaultPoint::AfterThemeManifestReplace);
        let outcome = store
            .replace_theme_manifest(
                &snapshot,
                identity(manifest),
                &mut Cursor::new(manifest),
                limits(),
            )
            .unwrap();
        let evidence = match outcome {
            ThemeMutationOutcome::Indeterminate(value) => value,
            other => panic!("{other:?}"),
        };
        store.close().unwrap();
        if collide {
            fs::write(directory.path().join("themes/manifest.toml"), b"external").unwrap();
        }
        let reopened = open(directory.path(), FaultController::new());
        let resolution = reopened
            .reconcile_theme_mutation(&evidence, limits())
            .unwrap();
        if collide {
            assert!(matches!(resolution, ThemeReconciliationOutcome::Collision));
        } else {
            assert!(matches!(
                resolution,
                ThemeReconciliationOutcome::ExactNew(_)
            ));
        }
    }
}
