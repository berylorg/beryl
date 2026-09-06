use std::{
    fs,
    io::Cursor,
    num::{NonZeroU64, NonZeroUsize},
};

use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion, HomeStore, ThemeFileIdentity, ThemeMutationOutcome,
    ThemeOperationLimits,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::DomainRevision;
use beryl_state::{
    DeleteTheme, InstallTheme, InstalledThemeId, InstalledThemeSelection, InstalledThemeSummary,
    RenameTheme, ReorderTheme, SaveTheme, SaveThemeAs, ThemeCommandFactError, ThemeDocument,
    ThemeDocumentDigest, ThemeDocumentDraft, ThemeDraftIdentity, ThemeDraftRevision,
    ThemeManifestContentIdentity, ThemeManifestCursor, ThemeManifestEncodeError,
    ThemeManifestLimit, ThemeManifestReadLimits, ThemeName, ThemePageLimits, ThemeParseMode,
    ThemeReconciliation, ThemeReferenceSnapshot, ThemeReferenceSnapshotProvider,
    ThemeReferenceSnapshotUnavailable, ThemeRepositoryCommand, ThemeRepositoryExecutionError,
    ThemeRepositoryLoadError, ThemeRepositoryObservation, ThemeRepositoryOperationOutcome,
    ThemeService, UpdateTheme,
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

fn physical_limits() -> ThemeOperationLimits {
    ThemeOperationLimits::new(
        1024 * 1024,
        NonZeroUsize::new(64 * 1024).unwrap(),
        NonZeroUsize::new(2).unwrap(),
        NonZeroUsize::new(4).unwrap(),
        NonZeroUsize::new(512).unwrap(),
    )
    .unwrap()
}

fn physical_identity(bytes: &[u8]) -> ThemeFileIdentity {
    ThemeFileIdentity::new(
        bytes.len() as u64,
        *ThemeDocumentDigest::of_bytes(bytes).as_bytes(),
    )
}

fn dense_manifest(entries: usize, generation: u64) -> Vec<u8> {
    let escaped_name = "\\\"".repeat(128);
    let mut manifest = format!("schema_version = 1\ngeneration = {generation}\n\n");
    for index in 0..entries {
        let (id, name) = if index == 0 {
            ("active".to_owned(), "Active".to_owned())
        } else {
            (format!("theme-{index}"), escaped_name.clone())
        };
        manifest.push_str(&format!("[[theme]]\nid = \"{id}\"\nname = \"{name}\"\n\n"));
    }
    manifest.into_bytes()
}

fn document_for(id: &str, background: &str) -> ThemeDocument {
    ThemeDocument::parse_bytes(
        format!(
            "schema = 1\nid = \"{id}\"\nname = \"{id}\"\n\n[[role]]\nid = \"app.window\"\nbackground = \"{background}\"\n"
        )
        .as_bytes(),
        ThemeParseMode::StrictCandidate,
    )
    .unwrap()
}

fn seed_dense_repository(
    store: &HomeStore,
    service: &ThemeService,
    entries: usize,
) -> ThemeRepositoryObservation {
    let empty = service
        .observe_repository(
            store,
            NonZeroU64::new(1024 * 1024).unwrap(),
            manifest_read_limits(),
            None,
        )
        .unwrap();
    let active = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            InstalledThemeId::new("active").unwrap(),
            ThemeName::new("Active").unwrap(),
            document_for("active", "#102030"),
        )
        .unwrap(),
    );
    service
        .execute_command(
            store,
            &empty,
            &active,
            NonZeroU64::new(1024 * 1024).unwrap(),
            &ReferencesUnavailable,
        )
        .unwrap();
    let active_repository = service
        .observe_repository(
            store,
            NonZeroU64::new(1024 * 1024).unwrap(),
            manifest_read_limits(),
            Some(&empty),
        )
        .unwrap();
    let companion = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            active_repository.manifest(),
            InstalledThemeId::new("theme-1").unwrap(),
            ThemeName::new("Theme one").unwrap(),
            document_for("theme-1", "#405060"),
        )
        .unwrap(),
    );
    service
        .execute_command(
            store,
            &active_repository,
            &companion,
            NonZeroU64::new(1024 * 1024).unwrap(),
            &ReferencesUnavailable,
        )
        .unwrap();
    let manifest = dense_manifest(entries, 4);
    assert!(manifest.len() > 256 * 1024);
    let snapshot = store.theme_repository_snapshot(physical_limits()).unwrap();
    assert!(matches!(
        store
            .replace_theme_manifest(
                &snapshot,
                physical_identity(&manifest),
                &mut Cursor::new(&manifest),
                physical_limits(),
            )
            .unwrap(),
        ThemeMutationOutcome::Committed(_)
    ));
    service
        .observe_repository(
            store,
            NonZeroU64::new(1024 * 1024).unwrap(),
            manifest_read_limits(),
            None,
        )
        .unwrap()
}

fn selection_for(
    service: &ThemeService,
    store: &HomeStore,
    repository: &ThemeRepositoryObservation,
    target: &str,
) -> (InstalledThemeSummary, InstalledThemeSelection) {
    let mut session = service
        .open_manifest(
            store,
            repository,
            NonZeroU64::new(1024 * 1024).unwrap(),
            manifest_read_limits(),
        )
        .unwrap();
    let mut cursor = ThemeManifestCursor::first(repository.manifest());
    loop {
        let page = session.read_page(cursor, page_limits()).unwrap();
        for (index, row) in page.records().iter().enumerate() {
            if row.id().as_str() == target {
                return (row.clone(), page.selection(index).unwrap());
            }
        }
        cursor = page.next().expect("target must be present");
    }
}

struct StaticReferences(ThemeReferenceSnapshot);

impl ThemeReferenceSnapshotProvider for StaticReferences {
    fn current_theme_references(
        &self,
    ) -> Result<ThemeReferenceSnapshot, ThemeReferenceSnapshotUnavailable> {
        Ok(self.0.clone())
    }
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
fn small_manifest_allowance_still_stages_a_legal_document() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let repository = service
        .observe_repository(
            &store,
            NonZeroU64::new(1024 * 1024).unwrap(),
            manifest_read_limits(),
            None,
        )
        .unwrap();
    let family = "a".repeat(256);
    let source = format!(
        r#"schema = 1
id = "active"
name = "Active"

[[role]]
id = "text"
font_family = "{family}"

[[role]]
id = "text.muted"
font_family = "{family}"

[[role]]
id = "text.subtle"
font_family = "{family}"
"#
    );
    let document =
        ThemeDocument::parse_bytes(source.as_bytes(), ThemeParseMode::StrictCandidate).unwrap();
    let command = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            repository.manifest(),
            InstalledThemeId::new("active").unwrap(),
            ThemeName::new("Active").unwrap(),
            document,
        )
        .unwrap(),
    );

    let outcome = service
        .execute_command(
            &store,
            &repository,
            &command,
            NonZeroU64::new(512).unwrap(),
            &ReferencesUnavailable,
        )
        .unwrap();
    let ThemeRepositoryOperationOutcome::Committed { publication, .. } = outcome else {
        panic!("small manifest allowance did not commit");
    };
    assert!(publication.affected_documents()[0].byte_length() > 512);
    let installed = service
        .observe_repository(
            &store,
            NonZeroU64::new(512).unwrap(),
            manifest_read_limits(),
            Some(&repository),
        )
        .unwrap();
    let (_, selection) = selection_for(&service, &store, &installed, "active");
    let loaded = service
        .load_document(&store, &installed, &selection, None)
        .unwrap();
    assert!(loaded.identity().byte_length() > 512);
}

#[test]
fn transformed_manifest_byte_cap_refuses_install_and_save_as_before_staging() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let small = NonZeroU64::new(512).unwrap();
    let quoted_name = ThemeName::new("\"".repeat(128)).unwrap();
    let empty = service
        .observe_repository(&store, small, manifest_read_limits(), None)
        .unwrap();
    let active = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            InstalledThemeId::new("active").unwrap(),
            quoted_name.clone(),
            document_for("active", "#102030"),
        )
        .unwrap(),
    );
    assert!(matches!(
        service
            .execute_command(&store, &empty, &active, small, &ReferencesUnavailable)
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let observed = service
        .observe_repository(&store, small, manifest_read_limits(), Some(&empty))
        .unwrap();
    let (_, selection) = selection_for(&service, &store, &observed, "active");
    let loaded = service
        .load_document(&store, &observed, &selection, None)
        .unwrap();
    let draft = ThemeDocumentDraft::new(
        ThemeDraftIdentity::new(NonZeroU64::new(13).unwrap()),
        ThemeDraftRevision::INITIAL,
        loaded.identity().clone(),
        loaded.document().clone(),
    )
    .unwrap();
    let install = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            observed.manifest(),
            InstalledThemeId::new("overflow").unwrap(),
            quoted_name.clone(),
            document_for("overflow", "#405060"),
        )
        .unwrap(),
    );
    assert!(matches!(
        service.execute_command(&store, &observed, &install, small, &ReferencesUnavailable),
        Err(ThemeRepositoryExecutionError::ManifestEncode(
            ThemeManifestEncodeError::LimitExceeded(ThemeManifestLimit::EncodedBytes)
        ))
    ));
    assert!(
        !directory
            .path()
            .join("themes/installed/overflow.toml")
            .exists()
    );
    let copied = InstalledThemeId::new("copy").unwrap();
    let save_as = ThemeRepositoryCommand::SaveAs(
        SaveThemeAs::new(
            observed.manifest(),
            draft.clone(),
            copied.clone(),
            quoted_name,
        )
        .unwrap(),
    );
    assert!(matches!(
        service.execute_command(&store, &observed, &save_as, small, &ReferencesUnavailable),
        Err(ThemeRepositoryExecutionError::ManifestEncode(
            ThemeManifestEncodeError::LimitExceeded(ThemeManifestLimit::EncodedBytes)
        ))
    ));
    assert_eq!(draft.binding(), loaded.identity());
    assert_eq!(draft.document(), loaded.document());
    assert!(!directory.path().join("themes/installed/copy.toml").exists());
    let unchanged = service
        .observe_repository(&store, small, manifest_read_limits(), Some(&observed))
        .unwrap();
    assert_eq!(unchanged.manifest(), observed.manifest());
}

#[test]
fn external_manifest_growth_preserves_typed_execution_byte_limit() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let small = NonZeroU64::new(512).unwrap();
    let empty = service
        .observe_repository(&store, small, manifest_read_limits(), None)
        .unwrap();
    let active = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            InstalledThemeId::new("active").unwrap(),
            ThemeName::new("Active").unwrap(),
            document_for("active", "#102030"),
        )
        .unwrap(),
    );
    service
        .execute_command(&store, &empty, &active, small, &ReferencesUnavailable)
        .unwrap();
    let observed = service
        .observe_repository(&store, small, manifest_read_limits(), Some(&empty))
        .unwrap();
    fs::write(
        directory.path().join("themes/manifest.toml"),
        vec![b'#'; 513],
    )
    .unwrap();
    let install = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            observed.manifest(),
            InstalledThemeId::new("overflow").unwrap(),
            ThemeName::new("Overflow").unwrap(),
            document_for("overflow", "#405060"),
        )
        .unwrap(),
    );
    assert!(matches!(
        service.execute_command(&store, &observed, &install, small, &ReferencesUnavailable),
        Err(ThemeRepositoryExecutionError::ManifestDecode(
            beryl_state::ThemeManifestDecodeError::LimitExceeded(ThemeManifestLimit::EncodedBytes)
        ))
    ));
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

#[test]
fn entry_cap_refuses_install_and_save_as_before_staging_or_draft_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let nearly_full = seed_dense_repository(&store, &service, 1023);
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let final_entry = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            nearly_full.manifest(),
            InstalledThemeId::new("last").unwrap(),
            ThemeName::new("Last").unwrap(),
            document_for("last", "#506070"),
        )
        .unwrap(),
    );
    assert!(matches!(
        service
            .execute_command(
                &store,
                &nearly_full,
                &final_entry,
                max,
                &ReferencesUnavailable,
            )
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let full = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&nearly_full))
        .unwrap();
    let (active, selection) = selection_for(&service, &store, &full, "active");
    assert_eq!(active.order(), 0);
    let loaded = service
        .load_document(&store, &full, &selection, None)
        .unwrap();
    let draft = ThemeDocumentDraft::new(
        ThemeDraftIdentity::new(NonZeroU64::new(11).unwrap()),
        ThemeDraftRevision::INITIAL,
        loaded.identity().clone(),
        loaded.document().clone(),
    )
    .unwrap();
    let overflow = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            full.manifest(),
            InstalledThemeId::new("overflow").unwrap(),
            ThemeName::new("Overflow").unwrap(),
            document_for("overflow", "#708090"),
        )
        .unwrap(),
    );
    assert!(matches!(
        service.execute_command(&store, &full, &overflow, max, &ReferencesUnavailable),
        Err(ThemeRepositoryExecutionError::ManifestEncode(
            ThemeManifestEncodeError::LimitExceeded(ThemeManifestLimit::InstalledEntries)
        ))
    ));
    assert!(
        !directory
            .path()
            .join("themes/installed/overflow.toml")
            .exists()
    );
    let copied = InstalledThemeId::new("copy").unwrap();
    let save_as = ThemeRepositoryCommand::SaveAs(
        SaveThemeAs::new(
            full.manifest(),
            draft.clone(),
            copied.clone(),
            ThemeName::new("Copy").unwrap(),
        )
        .unwrap(),
    );
    assert!(matches!(
        service.execute_command(&store, &full, &save_as, max, &ReferencesUnavailable),
        Err(ThemeRepositoryExecutionError::ManifestEncode(
            ThemeManifestEncodeError::LimitExceeded(ThemeManifestLimit::InstalledEntries)
        ))
    ));
    assert_eq!(draft.binding(), loaded.identity());
    assert_eq!(draft.document(), loaded.document());
    assert!(!directory.path().join("themes/installed/copy.toml").exists());
    let unchanged = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&full))
        .unwrap();
    assert_eq!(unchanged.manifest(), full.manifest());
}

#[test]
fn large_manifest_supports_document_and_repository_operations() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let initial = seed_dense_repository(&store, &service, 1024);
    let (active, active_selection) = selection_for(&service, &store, &initial, "active");
    assert_eq!(active.order(), 0);
    let loaded = service
        .load_document(&store, &initial, &active_selection, None)
        .unwrap();
    let update = ThemeRepositoryCommand::Update(
        UpdateTheme::new(loaded.identity().clone(), document_for("active", "#405060")).unwrap(),
    );
    assert!(matches!(
        service
            .execute_command(&store, &initial, &update, max, &ReferencesUnavailable)
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let after_update = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&initial))
        .unwrap();
    let (_, active_selection) = selection_for(&service, &store, &after_update, "active");
    let updated = service
        .load_document(
            &store,
            &after_update,
            &active_selection,
            Some(loaded.identity()),
        )
        .unwrap();
    let draft = ThemeDocumentDraft::new(
        ThemeDraftIdentity::new(NonZeroU64::new(12).unwrap()),
        ThemeDraftRevision::INITIAL,
        updated.identity().clone(),
        updated.document().clone(),
    )
    .unwrap();
    let save = ThemeRepositoryCommand::Save(SaveTheme::new(draft));
    assert!(matches!(
        service
            .execute_command(&store, &after_update, &save, max, &ReferencesUnavailable)
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let after_save = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&after_update))
        .unwrap();
    let (active, _) = selection_for(&service, &store, &after_save, "active");
    let rename = ThemeRepositoryCommand::Rename(RenameTheme::new(
        after_save.manifest(),
        active,
        ThemeName::new("Renamed").unwrap(),
    ));
    assert!(matches!(
        service
            .execute_command(&store, &after_save, &rename, max, &ReferencesUnavailable)
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let after_rename = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&after_save))
        .unwrap();
    let (active, _) = selection_for(&service, &store, &after_rename, "active");
    let reorder =
        ThemeRepositoryCommand::Reorder(ReorderTheme::new(after_rename.manifest(), active, 1023));
    assert!(matches!(
        service
            .execute_command(&store, &after_rename, &reorder, max, &ReferencesUnavailable)
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let after_reorder = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&after_rename))
        .unwrap();
    let (companion, companion_selection) =
        selection_for(&service, &store, &after_reorder, "theme-1");
    let companion_document = service
        .load_document(&store, &after_reorder, &companion_selection, None)
        .unwrap();
    let references = ThemeReferenceSnapshot::new(
        after_reorder.manifest(),
        service.settings_identity(DomainRevision::new(1).unwrap(), None),
        None,
        None,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let delete = ThemeRepositoryCommand::Delete(
        DeleteTheme::new(
            after_reorder.manifest(),
            companion,
            companion_document.identity().clone(),
            references.clone(),
        )
        .unwrap(),
    );
    assert!(matches!(
        service
            .execute_command(
                &store,
                &after_reorder,
                &delete,
                max,
                &StaticReferences(references),
            )
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let after_delete = service
        .observe_repository(&store, max, manifest_read_limits(), Some(&after_reorder))
        .unwrap();
    let install = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            after_delete.manifest(),
            InstalledThemeId::new("replacement").unwrap(),
            ThemeName::new("Replacement").unwrap(),
            document_for("replacement", "#506070"),
        )
        .unwrap(),
    );
    assert!(matches!(
        service
            .execute_command(&store, &after_delete, &install, max, &ReferencesUnavailable)
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
}

#[test]
fn large_manifest_reconciliation_uses_the_manifest_aware_envelope() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let service = ThemeService::acquire(&store).unwrap();
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let observed = seed_dense_repository(&store, &service, 1023);
    let (active, _) = selection_for(&service, &store, &observed, "active");
    let rename = ThemeRepositoryCommand::Rename(RenameTheme::new(
        observed.manifest(),
        active,
        ThemeName::new("Reconciled").unwrap(),
    ));
    faults.fail_next(FaultPoint::AfterThemeManifestReplace);
    let operation = match service
        .execute_command(&store, &observed, &rename, max, &ReferencesUnavailable)
        .unwrap()
    {
        ThemeRepositoryOperationOutcome::Indeterminate(operation) => operation.operation(),
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert!(matches!(
        service.reconcile_operation(&store, operation, max).unwrap(),
        ThemeReconciliation::ExactNew(_)
    ));
}
