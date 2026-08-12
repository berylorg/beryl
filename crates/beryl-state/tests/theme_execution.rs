use std::{
    fs,
    num::{NonZeroU64, NonZeroUsize},
};

use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    InstallTheme, InstalledThemeId, SaveThemeAs, ThemeCommandFactError, ThemeDocument,
    ThemeDocumentDraft, ThemeDraftIdentity, ThemeDraftRevision, ThemeManifestContentIdentity,
    ThemeManifestCursor, ThemeManifestReadLimits, ThemeName, ThemePageLimits, ThemeParseMode,
    ThemeReconciliation, ThemeReferenceSnapshot, ThemeReferenceSnapshotProvider,
    ThemeReferenceSnapshotUnavailable, ThemeRepositoryCommand, ThemeRepositoryExecutionError,
    ThemeRepositoryLoadError, ThemeRepositoryOperationOutcome, ThemeService, UpdateTheme,
};

const FIRST_DOCUMENT: &[u8] = br##"schema = 1
id = "active"
name = "Active"

[[role]]
id = "app.window"
background = "#102030"
"##;

const UPDATED_DOCUMENT: &[u8] = br##"schema = 1
id = "active"
name = "Active"

[[role]]
id = "app.window"
background = "#405060"
"##;

struct ReferencesUnavailable;

impl ThemeReferenceSnapshotProvider for ReferencesUnavailable {
    fn current_theme_references(
        &self,
    ) -> Result<ThemeReferenceSnapshot, ThemeReferenceSnapshotUnavailable> {
        Err(ThemeReferenceSnapshotUnavailable)
    }
}

fn manifest_read_limits() -> ThemeManifestReadLimits {
    ThemeManifestReadLimits::new(
        NonZeroUsize::new(4096).unwrap(),
        NonZeroUsize::new(16 * 1024).unwrap(),
        NonZeroUsize::new(256 * 1024).unwrap(),
    )
    .unwrap()
}

fn page_limits() -> ThemePageLimits {
    ThemePageLimits::new(
        NonZeroUsize::new(8).unwrap(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .unwrap()
}

#[test]
fn typed_install_and_update_publish_exact_repository_identities() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let max_manifest_bytes = NonZeroU64::new(1024 * 1024).unwrap();
    let empty = service
        .observe_repository(&store, max_manifest_bytes, manifest_read_limits(), None)
        .unwrap();
    let id = InstalledThemeId::new("active").unwrap();
    let document =
        ThemeDocument::parse_bytes(FIRST_DOCUMENT, ThemeParseMode::StrictCandidate).unwrap();
    let install = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            id.clone(),
            ThemeName::new("Active").unwrap(),
            document,
        )
        .unwrap(),
    );
    let install_outcome = service
        .execute_command(
            &store,
            &empty,
            &install,
            max_manifest_bytes,
            &ReferencesUnavailable,
        )
        .unwrap();
    let install_publication = match install_outcome {
        ThemeRepositoryOperationOutcome::Committed {
            publication,
            later_failure: None,
        } => publication,
        other => panic!("unexpected install outcome: {other:?}"),
    };
    let installed_manifest = install_publication.manifest().unwrap();
    assert_ne!(installed_manifest, empty.manifest());
    assert!(matches!(
        installed_manifest.content(),
        ThemeManifestContentIdentity::Present { .. }
    ));

    let observed = service
        .observe_repository(
            &store,
            max_manifest_bytes,
            manifest_read_limits(),
            Some(&empty),
        )
        .unwrap();
    assert_eq!(observed.manifest(), installed_manifest);
    let mut manifest = service
        .open_manifest(
            &store,
            &observed,
            max_manifest_bytes,
            manifest_read_limits(),
        )
        .unwrap();
    let page = manifest
        .read_page(
            ThemeManifestCursor::first(observed.manifest()),
            page_limits(),
        )
        .unwrap();
    let row = page.records()[0].clone();
    assert_eq!(row.id(), &id);
    let selection = page.selection(0).unwrap();

    let loaded = service
        .load_document(&store, &observed, &selection, None)
        .unwrap();
    let updated =
        ThemeDocument::parse_bytes(UPDATED_DOCUMENT, ThemeParseMode::StrictCandidate).unwrap();
    let update = ThemeRepositoryCommand::Update(
        UpdateTheme::new(loaded.identity().clone(), updated).unwrap(),
    );
    let update_outcome = service
        .execute_command(
            &store,
            &observed,
            &update,
            max_manifest_bytes,
            &ReferencesUnavailable,
        )
        .unwrap();
    let update_publication = match update_outcome {
        ThemeRepositoryOperationOutcome::Committed {
            publication,
            later_failure: None,
        } => publication,
        other => panic!("unexpected update outcome: {other:?}"),
    };
    assert_eq!(update_publication.manifest(), None);
    assert_eq!(update_publication.affected_documents().len(), 1);
    assert_eq!(
        update_publication.affected_documents()[0].manifest(),
        observed.manifest()
    );
    assert_ne!(
        update_publication.affected_documents()[0].digest(),
        loaded.identity().digest()
    );
}

#[test]
fn indeterminate_manifest_publication_reconciles_to_exact_new() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let max_manifest_bytes = NonZeroU64::new(1024 * 1024).unwrap();
    let empty = service
        .observe_repository(&store, max_manifest_bytes, manifest_read_limits(), None)
        .unwrap();
    let document =
        ThemeDocument::parse_bytes(FIRST_DOCUMENT, ThemeParseMode::StrictCandidate).unwrap();
    let command = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            InstalledThemeId::new("active").unwrap(),
            ThemeName::new("Active").unwrap(),
            document,
        )
        .unwrap(),
    );

    faults.fail_next(FaultPoint::AfterThemeManifestReplace);
    let outcome = service
        .execute_command(
            &store,
            &empty,
            &command,
            max_manifest_bytes,
            &ReferencesUnavailable,
        )
        .unwrap();
    let operation = match outcome {
        ThemeRepositoryOperationOutcome::Indeterminate(operation) => operation,
        other => panic!("unexpected faulted install outcome: {other:?}"),
    };
    let operation_id = operation.operation();
    assert_ne!(operation_id.get(), 0);
    drop(operation);
    assert_eq!(service.diagnostics().open_scopes(), 1);
    assert_eq!(service.diagnostics().mutations_indeterminate(), 1);
    let fresh = ThemeService::acquire(&store).unwrap();
    assert_eq!(fresh.diagnostics().open_scopes(), 0);
    assert!(matches!(
        fresh
            .reconcile_operation(&store, operation_id, max_manifest_bytes)
            .unwrap_err(),
        ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::UnknownReconciliationOperation
        )
    ));
    drop(fresh);
    assert_eq!(service.diagnostics().open_scopes(), 1);
    let clone = service.clone();
    assert!(matches!(
        clone
            .execute_command(
                &store,
                &empty,
                &command,
                max_manifest_bytes,
                &ReferencesUnavailable,
            )
            .unwrap_err(),
        ThemeRepositoryExecutionError::CommandFact(ThemeCommandFactError::ScopeGated)
    ));
    assert!(matches!(
        clone
            .observe_repository(&store, max_manifest_bytes, manifest_read_limits(), None)
            .unwrap_err(),
        ThemeRepositoryLoadError::ScopeGated
    ));
    assert!(matches!(
        clone
            .reconcile_operation(&store, operation_id, max_manifest_bytes)
            .unwrap(),
        ThemeReconciliation::ExactNew(_)
    ));
    let diagnostics = service.diagnostics();
    assert_eq!(diagnostics.open_scopes(), 0);
    assert_eq!(diagnostics.reconciliations_exact_new(), 1);
    clone
        .observe_repository(&store, max_manifest_bytes, manifest_read_limits(), None)
        .unwrap();
}

#[test]
fn exact_old_reconciliation_reopens_the_repository_scope() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let empty = service
        .observe_repository(&store, max, manifest_read_limits(), None)
        .unwrap();
    let command = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            InstalledThemeId::new("active").unwrap(),
            ThemeName::new("Active").unwrap(),
            ThemeDocument::parse_bytes(FIRST_DOCUMENT, ThemeParseMode::StrictCandidate).unwrap(),
        )
        .unwrap(),
    );
    faults.fail_next(FaultPoint::AfterThemeManifestReplace);
    let outcome = service
        .execute_command(&store, &empty, &command, max, &ReferencesUnavailable)
        .unwrap();
    let operation = match outcome {
        ThemeRepositoryOperationOutcome::Indeterminate(operation) => operation.operation(),
        other => panic!("unexpected faulted install outcome: {other:?}"),
    };
    fs::remove_file(directory.path().join("themes/manifest.toml")).unwrap();
    fs::remove_file(directory.path().join("themes/installed/active.toml")).unwrap();
    assert!(matches!(
        service.reconcile_operation(&store, operation, max).unwrap(),
        ThemeReconciliation::ExactOld
    ));
    assert_eq!(service.diagnostics().open_scopes(), 0);
    assert_eq!(service.diagnostics().reconciliations_exact_old(), 1);
    service
        .observe_repository(&store, max, manifest_read_limits(), None)
        .unwrap();
}

#[test]
fn collision_reconciliation_keeps_the_repository_scope_closed() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let empty = service
        .observe_repository(&store, max, manifest_read_limits(), None)
        .unwrap();
    let command = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            InstalledThemeId::new("active").unwrap(),
            ThemeName::new("Active").unwrap(),
            ThemeDocument::parse_bytes(FIRST_DOCUMENT, ThemeParseMode::StrictCandidate).unwrap(),
        )
        .unwrap(),
    );
    faults.fail_next(FaultPoint::AfterThemeManifestReplace);
    let outcome = service
        .execute_command(&store, &empty, &command, max, &ReferencesUnavailable)
        .unwrap();
    let operation = match outcome {
        ThemeRepositoryOperationOutcome::Indeterminate(operation) => operation.operation(),
        other => panic!("unexpected faulted install outcome: {other:?}"),
    };
    fs::write(
        directory.path().join("themes/manifest.toml"),
        b"external collision",
    )
    .unwrap();
    assert!(matches!(
        service.reconcile_operation(&store, operation, max).unwrap(),
        ThemeReconciliation::Collision
    ));
    let diagnostics = service.diagnostics();
    assert_eq!(diagnostics.open_scopes(), 0);
    assert_eq!(diagnostics.closed_collision_scopes(), 1);
    assert_eq!(diagnostics.reconciliations_collision(), 1);
    assert!(matches!(
        service
            .reconcile_operation(&store, operation, max)
            .unwrap_err(),
        ThemeRepositoryExecutionError::CommandFact(ThemeCommandFactError::CollisionScopeClosed)
    ));
    assert!(matches!(
        service
            .execute_command(&store, &empty, &command, max, &ReferencesUnavailable)
            .unwrap_err(),
        ThemeRepositoryExecutionError::CommandFact(ThemeCommandFactError::ScopeGated)
    ));
    assert!(matches!(
        service
            .observe_repository(&store, max, manifest_read_limits(), None)
            .unwrap_err(),
        ThemeRepositoryLoadError::ScopeGated
    ));
}

#[test]
fn manifest_admission_failure_is_proven_not_committed() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let max_manifest_bytes = NonZeroU64::new(1024 * 1024).unwrap();
    let empty = service
        .observe_repository(&store, max_manifest_bytes, manifest_read_limits(), None)
        .unwrap();
    let command = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            InstalledThemeId::new("active").unwrap(),
            ThemeName::new("Active").unwrap(),
            ThemeDocument::parse_bytes(FIRST_DOCUMENT, ThemeParseMode::StrictCandidate).unwrap(),
        )
        .unwrap(),
    );

    faults.fail_next(FaultPoint::BeforeThemeManifestReplace);
    assert!(matches!(
        service
            .execute_command(
                &store,
                &empty,
                &command,
                max_manifest_bytes,
                &ReferencesUnavailable,
            )
            .unwrap(),
        ThemeRepositoryOperationOutcome::NotCommitted { .. }
    ));
    let still_empty = service
        .observe_repository(
            &store,
            max_manifest_bytes,
            manifest_read_limits(),
            Some(&empty),
        )
        .unwrap();
    assert!(!still_empty.is_initialized());
}

#[test]
fn save_as_rewrites_the_published_id_without_mutating_the_bound_draft() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let empty = service
        .observe_repository(&store, max, manifest_read_limits(), None)
        .unwrap();
    let original = InstalledThemeId::new("active").unwrap();
    let install = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            original.clone(),
            ThemeName::new("Active").unwrap(),
            ThemeDocument::parse_bytes(FIRST_DOCUMENT, ThemeParseMode::StrictCandidate).unwrap(),
        )
        .unwrap(),
    );
    service
        .execute_command(&store, &empty, &install, max, &ReferencesUnavailable)
        .unwrap();
    let observed = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&empty))
        .unwrap();
    let mut session = service
        .open_manifest(&store, &observed, max, manifest_read_limits())
        .unwrap();
    let page = session
        .read_page(
            ThemeManifestCursor::first(observed.manifest()),
            page_limits(),
        )
        .unwrap();
    let original_selection = page.selection(0).unwrap();
    let loaded = service
        .load_document(&store, &observed, &original_selection, None)
        .unwrap();
    let draft = ThemeDocumentDraft::new(
        ThemeDraftIdentity::new(NonZeroU64::new(7).unwrap()),
        ThemeDraftRevision::INITIAL,
        loaded.identity().clone(),
        loaded.document().clone(),
    )
    .unwrap();
    assert_eq!(draft.document().id(), Some(&original));
    let copied = InstalledThemeId::new("active-copy").unwrap();
    let save_as = ThemeRepositoryCommand::SaveAs(
        SaveThemeAs::new(
            observed.manifest(),
            draft.clone(),
            copied.clone(),
            ThemeName::new("Active Copy").unwrap(),
        )
        .unwrap(),
    );
    assert!(matches!(
        service
            .execute_command(&store, &observed, &save_as, max, &ReferencesUnavailable)
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    assert_eq!(draft.document().id(), Some(&original));

    let copied_repository = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&observed))
        .unwrap();
    let mut session = service
        .open_manifest(&store, &copied_repository, max, manifest_read_limits())
        .unwrap();
    let page = session
        .read_page(
            ThemeManifestCursor::first(copied_repository.manifest()),
            page_limits(),
        )
        .unwrap();
    let copied_index = page
        .records()
        .iter()
        .position(|row| row.id() == &copied)
        .unwrap();
    let copied_document = service
        .load_document(
            &store,
            &copied_repository,
            &page.selection(copied_index).unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(copied_document.document().id(), Some(&copied));
}
