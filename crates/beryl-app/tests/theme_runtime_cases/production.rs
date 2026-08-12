use std::{
    fs,
    io::Cursor,
    num::{NonZeroU64, NonZeroUsize},
    thread,
    time::Duration,
};

use beryl_app::theme_runtime::{
    AppearanceCoordinatorConfig, ConfirmedSettingsTheme, PreparedPreviewAppearance,
    PreviewCandidateIdentity, PreviewSource, PreviewSourceIdentity, RepositoryAppearanceResult,
    SettingsThemeOutcome, SettingsThemeResult, ThemeRepositoryRequest,
    ThemeRepositoryRequestOrigin, ThemeRepositoryRequestResult, ThemeRuntime, ThemeRuntimeConfig,
    ThemeRuntimeFailureClass,
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore, StableThemeFileId,
    ThemeFileIdentity, ThemeFileSelector, ThemeMutationOutcome, ThemeOperationLimits,
};
use beryl_model::DomainRevision;
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, InstallTheme, InstalledThemeId,
    SaveThemeAs, SettingKey, SettingUpdate, SettingValue, ThemeChangeHint, ThemeDocument,
    ThemeDocumentDigest, ThemeDocumentDraft, ThemeDraftIdentity, ThemeDraftRevision,
    ThemeManifestCursor, ThemeManifestReadLimits, ThemeName, ThemePageLimits, ThemeParseMode,
    ThemeReferenceSnapshot, ThemeReferenceSnapshotProvider, ThemeReferenceSnapshotUnavailable,
    ThemeRepositoryCommand, ThemeRepositoryOperationOutcome,
};
use sha2::{Digest, Sha256};

use crate::support::TestAdapter;

#[path = "production/corrections.rs"]
mod corrections;

const ACTIVE_DOCUMENT: &[u8] = br##"schema = 1
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

const INVALID_DOCUMENT: &[u8] = b"schema = not-a-number\n";

const OTHER_DOCUMENT: &[u8] = br##"schema = 1
id = "other"
name = "Other"

[[role]]
id = "app.window"
background = "#708090"
"##;

struct NoReferences;

impl ThemeReferenceSnapshotProvider for NoReferences {
    fn current_theme_references(
        &self,
    ) -> Result<ThemeReferenceSnapshot, ThemeReferenceSnapshotUnavailable> {
        Err(ThemeReferenceSnapshotUnavailable)
    }
}

struct RuntimeFixture {
    directory: tempfile::TempDir,
    store: HomeStore,
    state: BerylState,
}

impl RuntimeFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut store).unwrap();
        Self {
            directory,
            store,
            state,
        }
    }

    fn active_record(&self) -> beryl_state::SettingRecord {
        self.state
            .settings()
            .setting(&self.store, SettingKey::ActiveThemeId)
            .unwrap()
            .unwrap()
    }
}

fn config() -> ThemeRuntimeConfig {
    ThemeRuntimeConfig::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
        NonZeroU64::new(1024 * 1024).unwrap(),
        ThemeManifestReadLimits::new(
            NonZeroUsize::new(4096).unwrap(),
            NonZeroUsize::new(16 * 1024).unwrap(),
            NonZeroUsize::new(256 * 1024).unwrap(),
        )
        .unwrap(),
        ThemePageLimits::new(
            NonZeroUsize::new(8).unwrap(),
            NonZeroUsize::new(4096).unwrap(),
        )
        .unwrap(),
        Duration::from_millis(10),
        NonZeroUsize::new(8).unwrap(),
        NonZeroUsize::new(32).unwrap(),
        NonZeroU64::new(1024 * 1024).unwrap(),
        NonZeroUsize::new(4).unwrap(),
    )
}

fn execute_settings(fixture: &RuntimeFixture, value: SettingValue) {
    let revision = fixture.state.settings().revision(&fixture.store).unwrap();
    let contribution = fixture.state.settings().apply(
        revision,
        ApplySettings::new(vec![SettingUpdate::new(
            SettingKey::ActiveThemeId,
            ExpectedSettingRevision::Absent,
            value,
        )])
        .unwrap(),
    );
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed { .. }
    ));
}

fn install_active(fixture: &RuntimeFixture) {
    install_theme(fixture, "active", "Active", ACTIVE_DOCUMENT);
}

fn install_theme(fixture: &RuntimeFixture, id: &str, name: &str, bytes: &[u8]) {
    let service = fixture.state.themes();
    let observation = service
        .observe_repository(
            &fixture.store,
            config_max_manifest(),
            manifest_limits(),
            None,
        )
        .unwrap();
    let document = ThemeDocument::parse_bytes(bytes, ThemeParseMode::StrictCandidate).unwrap();
    let command = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            observation.manifest(),
            InstalledThemeId::new(id).unwrap(),
            ThemeName::new(name).unwrap(),
            document,
        )
        .unwrap(),
    );
    assert!(matches!(
        service
            .execute_command(
                &fixture.store,
                &observation,
                &command,
                config_max_manifest(),
                &NoReferences,
            )
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
}

fn config_max_manifest() -> NonZeroU64 {
    NonZeroU64::new(1024 * 1024).unwrap()
}

fn manifest_limits() -> ThemeManifestReadLimits {
    ThemeManifestReadLimits::new(
        NonZeroUsize::new(4096).unwrap(),
        NonZeroUsize::new(16 * 1024).unwrap(),
        NonZeroUsize::new(256 * 1024).unwrap(),
    )
    .unwrap()
}

fn physical_limits() -> ThemeOperationLimits {
    ThemeOperationLimits::new(
        1024 * 1024,
        NonZeroUsize::new(64 * 1024).unwrap(),
        NonZeroUsize::new(2).unwrap(),
        NonZeroUsize::new(4).unwrap(),
        NonZeroUsize::new(4096).unwrap(),
    )
    .unwrap()
}

fn replace_active_bytes(fixture: &RuntimeFixture, bytes: &[u8]) {
    let limits = physical_limits();
    let snapshot = fixture.store.theme_repository_snapshot(limits).unwrap();
    let id = StableThemeFileId::new("active").unwrap();
    let selector = ThemeFileSelector::Document(id.clone());
    let expected = fixture
        .store
        .observe_theme_file(&snapshot, &selector, limits)
        .unwrap();
    let intended = ThemeFileIdentity::new(bytes.len() as u64, Sha256::digest(bytes).into());
    assert!(matches!(
        fixture
            .store
            .replace_theme_document(
                &snapshot,
                &id,
                Some(expected),
                intended,
                &mut Cursor::new(bytes),
                limits,
            )
            .unwrap(),
        ThemeMutationOutcome::Committed(_)
    ));
}

fn await_appearance(
    runtime: &mut ThemeRuntime,
    fixture: &RuntimeFixture,
) -> RepositoryAppearanceResult {
    for _ in 0..100 {
        let result = runtime.drain_change_hints(&fixture.store).unwrap();
        if result.received() != 0 {
            return result.appearance();
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("theme watcher did not publish a hint")
}

#[test]
fn startup_uses_fallback_for_missing_active_and_releases_on_retire() {
    let fixture = RuntimeFixture::new();
    execute_settings(&fixture, SettingValue::active_theme_id("missing").unwrap());
    let revision = fixture.state.settings().revision(&fixture.store).unwrap();
    let record = fixture.active_record();
    let mut runtime = ThemeRuntime::start(
        &fixture.store,
        fixture.state.themes(),
        revision,
        Some(&record),
        config(),
    )
    .unwrap();

    assert!(matches!(
        runtime.current().unwrap().prepared().source(),
        beryl_state::ThemeAppearanceSource::BuiltinFallback(_)
    ));
    let diagnostic = runtime.diagnostics();
    assert!(diagnostic.home_generation_present());
    assert!(diagnostic.repository_generation_present());
    assert_eq!(
        diagnostic.last_failure(),
        Some(ThemeRuntimeFailureClass::DocumentMissing)
    );
    assert_eq!(diagnostic.worker_count(), 0);
    assert_eq!(diagnostic.state().active_subscriptions(), 1);

    runtime.retire();
    let diagnostic = runtime.diagnostics();
    assert!(diagnostic.retired());
    assert!(!diagnostic.home_generation_present());
    assert!(!diagnostic.repository_generation_present());
    assert_eq!(diagnostic.state().active_subscriptions(), 0);
    assert!(runtime.current().is_none());
}

#[test]
fn startup_publishes_the_exact_installed_active_document() {
    let fixture = RuntimeFixture::new();
    install_active(&fixture);
    execute_settings(&fixture, SettingValue::active_theme_id("active").unwrap());
    let revision = fixture.state.settings().revision(&fixture.store).unwrap();
    let record = fixture.active_record();
    let runtime = ThemeRuntime::start(
        &fixture.store,
        fixture.state.themes(),
        revision,
        Some(&record),
        config(),
    )
    .unwrap();

    let current = runtime.current().unwrap();
    let beryl_state::ThemeAppearanceSource::Installed(document) = current.prepared().source()
    else {
        panic!("expected installed appearance");
    };
    assert_eq!(document.theme_id().as_str(), "active");
    assert!(runtime.diagnostics().pages_read() > 0);
    assert_eq!(runtime.diagnostics().last_failure(), None);
}

#[test]
fn confirmed_settings_publication_ends_preview_and_foreign_service_is_rejected() {
    let fixture = RuntimeFixture::new();
    let revision = fixture.state.settings().revision(&fixture.store).unwrap();
    let mut runtime = ThemeRuntime::start(
        &fixture.store,
        fixture.state.themes(),
        revision,
        None,
        config(),
    )
    .unwrap();
    let preview = runtime
        .begin_preview(
            PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(1).unwrap()),
            PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([9; 32])),
        )
        .unwrap();
    let preview_candidate = preview.candidate().clone();
    let preview_prepared = runtime.current().unwrap().prepared().clone();
    runtime
        .publish_preview(
            preview,
            PreparedPreviewAppearance::new(preview_candidate, preview_prepared),
        )
        .unwrap();
    assert!(runtime.current().unwrap().is_preview());

    let service = fixture.state.themes();
    let committed = service.settings_identity(DomainRevision::new(2).unwrap(), None);
    let prepared = beryl_state::PreparedThemeAppearance::fallback(committed);
    let outcome = ConfirmedSettingsTheme::new(
        ThemeDraftIdentity::new(NonZeroU64::new(7).unwrap()),
        ThemeDraftRevision::INITIAL,
        committed,
        None,
        prepared,
    );
    assert_eq!(
        runtime
            .consume_settings_outcome(SettingsThemeOutcome::Committed(outcome))
            .unwrap(),
        SettingsThemeResult::Published
    );
    assert!(!runtime.current().unwrap().is_preview());

    let foreign = beryl_state::ThemeService::acquire(&fixture.store).unwrap();
    let foreign_settings = foreign.settings_identity(DomainRevision::new(3).unwrap(), None);
    let foreign_outcome = ConfirmedSettingsTheme::new(
        ThemeDraftIdentity::new(NonZeroU64::new(8).unwrap()),
        ThemeDraftRevision::INITIAL,
        foreign_settings,
        None,
        beryl_state::PreparedThemeAppearance::fallback(foreign_settings),
    );
    assert_eq!(
        runtime.consume_settings_outcome(SettingsThemeOutcome::ReconciledExactNew(foreign_outcome)),
        Err(ThemeRuntimeFailureClass::Settings)
    );
}

#[test]
fn dynamic_tool_install_and_save_as_share_the_typed_repository_route() {
    let fixture = RuntimeFixture::new();
    let revision = fixture.state.settings().revision(&fixture.store).unwrap();
    let mut runtime = ThemeRuntime::start(
        &fixture.store,
        fixture.state.themes(),
        revision,
        None,
        config(),
    )
    .unwrap();
    let service = fixture.state.themes();
    let empty = service
        .observe_repository(
            &fixture.store,
            config_max_manifest(),
            manifest_limits(),
            None,
        )
        .unwrap();
    let install = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            empty.manifest(),
            InstalledThemeId::new("active").unwrap(),
            ThemeName::new("Active").unwrap(),
            ThemeDocument::parse_bytes(ACTIVE_DOCUMENT, ThemeParseMode::StrictCandidate).unwrap(),
        )
        .unwrap(),
    );
    let request = ThemeRepositoryRequest::new(ThemeRepositoryRequestOrigin::DynamicTool, install);
    assert_eq!(request.origin(), ThemeRepositoryRequestOrigin::DynamicTool);
    assert!(matches!(
        runtime
            .execute_repository_request(&fixture.store, request, &NoReferences)
            .unwrap(),
        ThemeRepositoryRequestResult::Committed { .. }
    ));

    let observed = service
        .observe_repository(
            &fixture.store,
            config_max_manifest(),
            manifest_limits(),
            Some(&empty),
        )
        .unwrap();
    let mut session = service
        .open_manifest(
            &fixture.store,
            &observed,
            config_max_manifest(),
            manifest_limits(),
        )
        .unwrap();
    let page = session
        .read_page(
            ThemeManifestCursor::first(observed.manifest()),
            ThemePageLimits::new(
                NonZeroUsize::new(8).unwrap(),
                NonZeroUsize::new(4096).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let selection = page.selection(0).unwrap();
    let loaded = service
        .load_document(&fixture.store, &observed, &selection, None)
        .unwrap();
    let draft = ThemeDocumentDraft::new(
        ThemeDraftIdentity::new(NonZeroU64::new(11).unwrap()),
        ThemeDraftRevision::INITIAL,
        loaded.identity().clone(),
        loaded.document().clone(),
    )
    .unwrap();
    let save_as = ThemeRepositoryCommand::SaveAs(
        SaveThemeAs::new(
            observed.manifest(),
            draft,
            InstalledThemeId::new("active-copy").unwrap(),
            ThemeName::new("Active Copy").unwrap(),
        )
        .unwrap(),
    );
    assert!(matches!(
        runtime
            .execute_repository_request(
                &fixture.store,
                ThemeRepositoryRequest::new(ThemeRepositoryRequestOrigin::Feature, save_as),
                &NoReferences,
            )
            .unwrap(),
        ThemeRepositoryRequestResult::Committed {
            appearance: RepositoryAppearanceResult::Unchanged,
            ..
        }
    ));
    assert_eq!(runtime.diagnostics().repository_requests(), 2);
    assert_eq!(runtime.diagnostics().state().mutations_committed(), 2);
}
