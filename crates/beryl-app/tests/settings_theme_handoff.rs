#![cfg(feature = "test-faults")]

#[path = "theme_runtime_cases/support.rs"]
mod support;

use std::{
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};

use beryl_app::settings_service::{
    SettingsApplyAction, SettingsApplyObservation, SettingsDraft, SettingsDraftValue,
    SettingsProcessOwner, SettingsService,
};
use beryl_app::theme_runtime::{
    AppearanceCoordinatorConfig, AppearancePublication, AppearancePublicationTarget,
    ConfirmedSettingsTheme, GpuiAppearanceWindowSet, SettingsThemeOutcome, SettingsThemeResult,
    ThemeRuntime, ThemeRuntimeConfig, ThemeRuntimeFailureClass, ThemeSettingsSource,
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, InstallTheme, InstalledThemeId,
    PreparedThemeAppearance, SettingKey, SettingUpdate, SettingValue, ThemeAppearanceSource,
    ThemeDocument, ThemeDraftIdentity, ThemeDraftRevision, ThemeManifestReadLimits, ThemeName,
    ThemePageLimits, ThemeParseMode, ThemeReferenceSnapshot, ThemeReferenceSnapshotProvider,
    ThemeReferenceSnapshotUnavailable, ThemeRepositoryCommand, ThemeRepositoryOperationOutcome,
    ThemeService, ThemeSettingsIdentity,
};

const DOCUMENT: &[u8] = br##"schema = 1
id = "active"
name = "Active"
[[role]]
id = "app.window"
background = "#102030"
"##;

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

struct NoReferences;
impl ThemeReferenceSnapshotProvider for NoReferences {
    fn current_theme_references(
        &self,
    ) -> Result<ThemeReferenceSnapshot, ThemeReferenceSnapshotUnavailable> {
        Err(ThemeReferenceSnapshotUnavailable)
    }
}

struct Recovered {
    _directory: tempfile::TempDir,
    store: HomeStore,
    state: BerylState,
    service: ThemeService,
    runtime: ThemeRuntime,
    old_source: ThemeSettingsSource,
    previous: Arc<beryl_app::theme_runtime::AppearanceGeneration>,
    faults: FaultController,
}

fn recovered() -> Recovered {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let service = state.themes();
    let observation = service
        .observe_repository(
            &store,
            NonZeroU64::new(1024 * 1024).unwrap(),
            ThemeManifestReadLimits::new(
                NonZeroUsize::new(4096).unwrap(),
                NonZeroUsize::new(16 * 1024).unwrap(),
                NonZeroUsize::new(256 * 1024).unwrap(),
            )
            .unwrap(),
            None,
        )
        .unwrap();
    let command = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            observation.manifest(),
            InstalledThemeId::new("active").unwrap(),
            ThemeName::new("Active").unwrap(),
            ThemeDocument::parse_bytes(DOCUMENT, ThemeParseMode::StrictCandidate).unwrap(),
        )
        .unwrap(),
    );
    assert!(matches!(
        service
            .execute_command(
                &store,
                &observation,
                &command,
                NonZeroU64::new(1024 * 1024).unwrap(),
                &NoReferences,
            )
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let settings = state.settings();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            settings.apply(
                settings.revision(&store).unwrap(),
                ApplySettings::new(vec![SettingUpdate::new(
                    SettingKey::ActiveThemeId,
                    ExpectedSettingRevision::Absent,
                    SettingValue::active_theme_id("active").unwrap(),
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let record = settings
        .setting(&store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    let mut old = ThemeRuntime::start(
        &store,
        service,
        settings.revision(&store).unwrap(),
        Some(&record),
        config(),
    )
    .unwrap();
    let previous = old.current().unwrap();
    assert!(matches!(
        previous.prepared().source(),
        ThemeAppearanceSource::Installed(_)
    ));
    assert_eq!(previous.home(), previous.prepared().home());
    let context = old.recovery_context().unwrap();
    let old_source = old.settings_source().unwrap();
    old.retire();
    drop((old, settings, state, record, observation));
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let candidate = store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let service = state.themes();
    let store = candidate.publish().unwrap();
    let runtime =
        ThemeRuntime::start_unavailable_context(&store, service.clone(), context, config())
            .unwrap();
    Recovered {
        _directory: directory,
        store,
        state,
        service,
        runtime,
        old_source,
        previous,
        faults,
    }
}

fn confirmed(prepared: PreparedThemeAppearance) -> ConfirmedSettingsTheme {
    ConfirmedSettingsTheme::new(
        ThemeDraftIdentity::new(NonZeroU64::new(1).unwrap()),
        ThemeDraftRevision::INITIAL,
        prepared.settings(),
        Some(InstalledThemeId::new("active").unwrap()),
        prepared,
    )
}

fn running_recovered() -> Recovered {
    let mut fixture = recovered();
    fixture.runtime.retire();
    let settings = fixture.state.settings();
    let record = settings
        .setting(&fixture.store, SettingKey::ActiveThemeId)
        .unwrap();
    fixture.runtime = ThemeRuntime::start(
        &fixture.store,
        fixture.service.clone(),
        settings.revision(&fixture.store).unwrap(),
        record.as_ref(),
        config(),
    )
    .unwrap();
    fixture
}

#[test]
fn changed_document_after_scalar_commit_retains_preview_and_retry_freshly_reads_latest_bytes() {
    changed_document_after_scalar_commit_completes_pending_activation(
        PendingActivationCompletion::Retry,
    );
}

#[test]
fn active_document_watch_completes_pending_settings_activation_and_ends_preview() {
    changed_document_after_scalar_commit_completes_pending_activation(
        PendingActivationCompletion::Watch(beryl_state::ThemeChangeHint::DocumentChanged(
            InstalledThemeId::new("active").unwrap(),
        )),
    );
}

#[test]
fn manifest_watch_completes_pending_settings_activation_and_ends_preview() {
    changed_document_after_scalar_commit_completes_pending_activation(
        PendingActivationCompletion::Watch(beryl_state::ThemeChangeHint::ManifestChanged),
    );
}

#[test]
fn fresh_stop_completes_pending_settings_activation_after_invalid_document_retains_preview() {
    changed_document_after_scalar_commit_completes_pending_activation(
        PendingActivationCompletion::Stop,
    );
}

#[test]
fn active_save_completes_pending_settings_activation_and_ends_preview() {
    changed_document_after_scalar_commit_completes_pending_activation(
        PendingActivationCompletion::Save,
    );
}

enum PendingActivationCompletion {
    Retry,
    Watch(beryl_state::ThemeChangeHint),
    Stop,
    Save,
}

fn changed_document_after_scalar_commit_completes_pending_activation(
    completion: PendingActivationCompletion,
) {
    use beryl_app::theme_runtime::{
        PreparedPreviewAppearance, PreviewCandidateIdentity, PreviewSource, PreviewSourceIdentity,
        RepositoryAppearanceResult,
    };
    let mut fixture = running_recovered();
    support::attach_test_adapter(&mut fixture.runtime, support::TestAdapter::new(31));
    let before = fixture.runtime.proven_selection().unwrap();
    let candidate =
        PreviewCandidateIdentity::Digest(beryl_state::ThemeDocumentDigest::of_bytes(b"preview"));
    let request = fixture
        .runtime
        .begin_preview(
            PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(31).unwrap()),
            candidate.clone(),
        )
        .unwrap();
    fixture
        .runtime
        .publish_preview(
            request,
            PreparedPreviewAppearance::new(candidate, PreparedThemeAppearance::fallback(before.0)),
        )
        .unwrap();
    let prior = fixture.runtime.current().unwrap();
    assert!(prior.is_preview());
    let intended = ThemeSettingsIdentity::new(
        before.0.home(),
        before.0.domain_revision().checked_next().unwrap(),
        before.0.active_record_revision(),
    );
    let prepared = fixture
        .runtime
        .prepare_settings_selection(
            &fixture.store,
            ThemeDraftIdentity::new(NonZeroU64::new(32).unwrap()),
            ThemeDraftRevision::INITIAL,
            intended,
            before.1.clone(),
        )
        .unwrap();
    assert_eq!(
        scalar_commit(&fixture.store, &fixture.state, &fixture.service),
        intended
    );
    let path = fixture
        .store
        .canonical_path()
        .join("themes/installed/active.toml");
    std::fs::write(&path, b"schema = invalid\n").unwrap();
    assert_eq!(
        fixture
            .runtime
            .consume_settings_outcome(&fixture.store, SettingsThemeOutcome::Committed(prepared))
            .unwrap(),
        SettingsThemeResult::Retained
    );
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &prior));
    let late_candidate = PreviewCandidateIdentity::Digest(
        beryl_state::ThemeDocumentDigest::of_bytes(b"late-preview"),
    );
    let late_request = fixture
        .runtime
        .begin_preview(
            PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(33).unwrap()),
            late_candidate.clone(),
        )
        .unwrap();
    assert!(matches!(
        fixture.runtime.stop_preview(&fixture.store),
        Err(beryl_app::theme_runtime::PreviewPublicationError::CandidateMismatch)
    ));
    assert!(matches!(
        fixture.runtime.publish_preview(
            late_request,
            PreparedPreviewAppearance::new(
                late_candidate,
                PreparedThemeAppearance::fallback(intended)
            )
        ),
        Err(beryl_app::theme_runtime::PreviewPublicationError::Stale(
            beryl_app::theme_runtime::StalePublicationReason::PreviewSequence
        ))
    ));
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &prior));
    assert!(fixture.runtime.diagnostics().refresh_reread_needed());
    assert!(
        !fixture
            .runtime
            .diagnostics()
            .appearance()
            .unwrap()
            .pending_durable_application()
    );
    assert_eq!(
        fixture.runtime.proven_selection(),
        Some((intended, before.1))
    );
    let rejected = match &completion {
        PendingActivationCompletion::Watch(hint) => fixture
            .runtime
            .consume_change_hints(&fixture.store, std::slice::from_ref(hint))
            .unwrap()
            .appearance(),
        _ => fixture
            .runtime
            .retry_current_durable(&fixture.store)
            .unwrap(),
    };
    assert_eq!(
        rejected,
        RepositoryAppearanceResult::Retained(ThemeRuntimeFailureClass::DocumentInvalid)
    );
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &prior));
    let latest = String::from_utf8(DOCUMENT.to_vec())
        .unwrap()
        .replace("#102030", "#789abc");
    std::fs::write(&path, latest.as_bytes()).unwrap();
    let result = match &completion {
        PendingActivationCompletion::Watch(hint) => fixture
            .runtime
            .consume_change_hints(&fixture.store, std::slice::from_ref(hint))
            .unwrap()
            .appearance(),
        PendingActivationCompletion::Stop => {
            assert!(matches!(
                fixture.runtime.stop_preview(&fixture.store).unwrap(),
                beryl_app::theme_runtime::StopPreviewResult::Restored(_)
            ));
            RepositoryAppearanceResult::Published
        }
        PendingActivationCompletion::Save => save_active_through_broker(&mut fixture),
        PendingActivationCompletion::Retry => fixture
            .runtime
            .retry_current_durable(&fixture.store)
            .unwrap(),
    };
    assert_eq!(result, RepositoryAppearanceResult::Published);
    let published = fixture.runtime.current().unwrap();
    assert!(!published.is_preview());
    assert_eq!(published.prepared().settings(), intended);
    assert!(!fixture.runtime.diagnostics().refresh_reread_needed());
    let ThemeAppearanceSource::Installed(document) = published.prepared().source() else {
        panic!("installed current appearance");
    };
    assert_eq!(
        document.digest(),
        beryl_state::ThemeDocumentDigest::of_bytes(&std::fs::read(&path).unwrap())
    );
    assert_eq!(
        fixture.state.settings().revision(&fixture.store).unwrap(),
        intended.domain_revision()
    );
    assert_eq!(
        fixture
            .state
            .settings()
            .setting(&fixture.store, SettingKey::DraftAutosaveInterval)
            .unwrap()
            .unwrap()
            .revision()
            .get(),
        1
    );
    fixture.runtime.retire();
}

fn broker_call(
    reader: &beryl_app::theme_runtime::SettingsThemeReader,
    work: beryl_app::theme_runtime::ThemeSettingsWork,
) -> (
    beryl_app::theme_runtime::ThemeSettingsTicket,
    Result<
        beryl_app::theme_runtime::ThemeSettingsResult,
        beryl_app::theme_runtime::ThemeSettingsBridgeError,
    >,
) {
    let ticket = reader.submit(work).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match reader.poll(ticket) {
            Ok(Some(result)) => return (ticket, Ok(result)),
            Err(error) => return (ticket, Err(error)),
            Ok(None) => {}
        }
        assert!(Instant::now() < deadline, "broker request did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn active_save_completion(
    fixture: &mut Recovered,
) -> beryl_app::theme_runtime::ThemeSettingsCompletion {
    use beryl_app::theme_runtime::{SettingsThemeBroker, ThemeSettingsResult, ThemeSettingsWork};
    let mut broker = SettingsThemeBroker::start(
        fixture.store.service_reference(),
        fixture.runtime.settings_source().unwrap(),
    )
    .unwrap();
    broker
        .set_active_selection(fixture.runtime.proven_selection())
        .unwrap();
    let reader = broker.reader();
    let ThemeSettingsResult::Catalog(catalog) =
        broker_call(&reader, ThemeSettingsWork::Observe).1.unwrap()
    else {
        panic!("catalog result");
    };
    let ThemeSettingsResult::Page(page) = broker_call(
        &reader,
        ThemeSettingsWork::PageRange {
            observation: catalog.observation.clone(),
            range: 0..1,
            focus: None,
        },
    )
    .1
    .unwrap() else {
        panic!("page result");
    };
    let ThemeSettingsResult::Document(document) = broker_call(
        &reader,
        ThemeSettingsWork::LoadDocument {
            observation: catalog.observation.clone(),
            selection: page.selections[0].clone(),
        },
    )
    .1
    .unwrap() else {
        panic!("document result");
    };
    let draft = beryl_state::ThemeDocumentDraft::new(
        ThemeDraftIdentity::new(NonZeroU64::new(78).unwrap()),
        ThemeDraftRevision::INITIAL,
        document.identity().clone(),
        document.document().clone(),
    )
    .unwrap();
    let (ticket, result) = broker_call(
        &reader,
        ThemeSettingsWork::Execute {
            observation: catalog.observation,
            command: ThemeRepositoryCommand::Save(beryl_state::SaveTheme::new(draft)),
            references: Arc::new(NoReferences),
        },
    );
    assert!(
        matches!(result.unwrap(), ThemeSettingsResult::Repository(outcome) if matches!(outcome.as_ref(), ThemeRepositoryOperationOutcome::Committed { .. }))
    );
    let completion = broker.take_completion().unwrap();
    assert_eq!(completion.ticket, ticket);
    assert!(completion.prepared.is_some());
    assert!(completion.preparation_failure.is_none());
    broker.retire();
    completion
}

fn save_active_through_broker(
    fixture: &mut Recovered,
) -> beryl_app::theme_runtime::RepositoryAppearanceResult {
    let completion = active_save_completion(fixture);
    fixture
        .runtime
        .consume_repository_completion(&fixture.store, completion)
        .unwrap()
}

#[test]
fn stop_preview_after_active_save_freshness_failure_preserves_preview_until_valid_current_document()
{
    use beryl_app::theme_runtime::{
        PreparedPreviewAppearance, PreviewCandidateIdentity, PreviewPublicationError,
        PreviewSource, PreviewSourceIdentity, RepositoryAppearanceResult, StopPreviewResult,
    };
    let mut fixture = running_recovered();
    support::attach_test_adapter(&mut fixture.runtime, support::TestAdapter::new(79));
    let settings = fixture.runtime.proven_selection().unwrap().0;
    let candidate = PreviewCandidateIdentity::Digest(beryl_state::ThemeDocumentDigest::of_bytes(
        b"save-preview",
    ));
    let request = fixture
        .runtime
        .begin_preview(
            PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(79).unwrap()),
            candidate.clone(),
        )
        .unwrap();
    fixture
        .runtime
        .publish_preview(
            request,
            PreparedPreviewAppearance::new(candidate, PreparedThemeAppearance::fallback(settings)),
        )
        .unwrap();
    let preview = fixture.runtime.current().unwrap();
    let completion = active_save_completion(&mut fixture);
    let path = fixture
        .store
        .canonical_path()
        .join("themes/installed/active.toml");
    std::fs::write(&path, b"schema = invalid\n").unwrap();
    assert!(matches!(
        fixture
            .runtime
            .consume_repository_completion(&fixture.store, completion)
            .unwrap(),
        RepositoryAppearanceResult::Retained(_)
    ));
    assert!(matches!(
        fixture.runtime.stop_preview(&fixture.store),
        Err(PreviewPublicationError::CandidateMismatch)
    ));
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &preview));
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(
        fixture.runtime.stop_preview(&fixture.store),
        Err(PreviewPublicationError::CandidateMismatch)
    ));
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &preview));
    let latest = String::from_utf8(DOCUMENT.to_vec())
        .unwrap()
        .replace("#102030", "#fedcba");
    std::fs::write(&path, latest.as_bytes()).unwrap();
    assert!(matches!(
        fixture.runtime.stop_preview(&fixture.store).unwrap(),
        StopPreviewResult::Restored(_)
    ));
    let current = fixture.runtime.current().unwrap();
    assert!(!current.is_preview());
    let ThemeAppearanceSource::Installed(document) = current.prepared().source() else {
        panic!("installed current appearance");
    };
    assert_eq!(
        document.digest(),
        beryl_state::ThemeDocumentDigest::of_bytes(latest.as_bytes())
    );
    assert_eq!(current.prepared().settings(), settings);
    assert!(fixture.runtime.diagnostics().last_failure().is_none());
    fixture.runtime.retire();
}

#[test]
fn save_as_and_its_manifest_hint_leave_appearance_unchanged_and_capacity_refusal_is_not_started() {
    use beryl_app::theme_runtime::{
        RepositoryAppearanceResult, SettingsThemeBroker, ThemeSettingsBridgeError,
        ThemeSettingsResult, ThemeSettingsWork,
    };
    let mut fixture = running_recovered();
    support::attach_test_adapter(&mut fixture.runtime, support::TestAdapter::new(41));
    let prior = fixture.runtime.current().unwrap();
    let ThemeAppearanceSource::Installed(binding) = prior.prepared().source() else {
        panic!("installed active");
    };
    let observation = fixture
        .service
        .observe_repository(
            &fixture.store,
            NonZeroU64::new(1024 * 1024).unwrap(),
            ThemeManifestReadLimits::new(
                NonZeroUsize::new(4096).unwrap(),
                NonZeroUsize::new(16 * 1024).unwrap(),
                NonZeroUsize::new(256 * 1024).unwrap(),
            )
            .unwrap(),
            None,
        )
        .unwrap();
    let draft = beryl_state::ThemeDocumentDraft::new(
        ThemeDraftIdentity::new(NonZeroU64::new(41).unwrap()),
        ThemeDraftRevision::INITIAL,
        binding.clone(),
        ThemeDocument::parse_bytes(DOCUMENT, ThemeParseMode::StrictCandidate).unwrap(),
    )
    .unwrap();
    let command = ThemeRepositoryCommand::SaveAs(
        beryl_state::SaveThemeAs::new(
            observation.manifest(),
            draft,
            InstalledThemeId::new("copy").unwrap(),
            ThemeName::new("Copy").unwrap(),
        )
        .unwrap(),
    );
    let mut broker = SettingsThemeBroker::start(
        fixture.store.service_reference(),
        fixture.runtime.settings_source().unwrap(),
    )
    .unwrap();
    broker
        .set_active_selection(fixture.runtime.proven_selection())
        .unwrap();
    let reader = broker.reader();
    let (ticket, result) = broker_call(
        &reader,
        ThemeSettingsWork::Execute {
            observation,
            command,
            references: Arc::new(NoReferences),
        },
    );
    assert!(
        matches!(result.unwrap(), ThemeSettingsResult::Repository(outcome) if matches!(outcome.as_ref(), ThemeRepositoryOperationOutcome::Committed { .. }))
    );
    let completion = broker.take_completion().unwrap();
    assert_eq!(completion.ticket, ticket);
    assert!(completion.appearance_target.is_none());
    assert!(completion.prepared.is_none());
    assert!(completion.preparation_failure.is_none());
    assert_eq!(
        fixture
            .runtime
            .consume_repository_completion(&fixture.store, completion)
            .unwrap(),
        RepositoryAppearanceResult::Unchanged
    );
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &prior));
    assert_eq!(
        fixture
            .runtime
            .consume_change_hints(
                &fixture.store,
                &[beryl_state::ThemeChangeHint::ManifestChanged]
            )
            .unwrap()
            .appearance(),
        RepositoryAppearanceResult::Unchanged
    );
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &prior));
    let mut manifest = String::from(
        "schema_version = 1\ngeneration = 3\n\n[[theme]]\nid = \"active\"\nname = \"Active\"\n\n[[theme]]\nid = \"copy\"\nname = \"Copy\"\n\n",
    );
    for index in 0..1022 {
        manifest.push_str(&format!(
            "[[theme]]\nid = \"entry-{index}\"\nname = \"Entry {index}\"\n\n"
        ));
    }
    std::fs::write(
        fixture.store.canonical_path().join("themes/manifest.toml"),
        manifest,
    )
    .unwrap();
    let (_, catalog) = broker_call(&reader, ThemeSettingsWork::Observe);
    let ThemeSettingsResult::Catalog(catalog) = catalog.unwrap() else {
        panic!("catalog");
    };
    assert_eq!(catalog.logical_item_count, 1024);
    let document = String::from_utf8(DOCUMENT.to_vec())
        .unwrap()
        .replace("id = \"active\"", "id = \"overflow\"");
    let command = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            catalog.observation.manifest(),
            InstalledThemeId::new("overflow").unwrap(),
            ThemeName::new("Overflow").unwrap(),
            ThemeDocument::parse_bytes(document.as_bytes(), ThemeParseMode::StrictCandidate)
                .unwrap(),
        )
        .unwrap(),
    );
    let (ticket, result) = broker_call(
        &reader,
        ThemeSettingsWork::Execute {
            observation: catalog.observation,
            command,
            references: Arc::new(NoReferences),
        },
    );
    assert!(matches!(
        result,
        Err(ThemeSettingsBridgeError::NotStarted(
            ThemeRuntimeFailureClass::InstalledEntryLimit
        ))
    ));
    let completion = broker.take_completion().unwrap();
    assert_eq!(completion.ticket, ticket);
    assert_eq!(
        completion.not_started,
        Some(ThemeRuntimeFailureClass::InstalledEntryLimit)
    );
    assert!(completion.repository.is_none());
    assert!(completion.unresolved.is_none());
    assert!(
        !fixture
            .store
            .canonical_path()
            .join("themes/installed/overflow.toml")
            .exists()
    );
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &prior));
    broker.retire();
    fixture.runtime.retire();
}

#[test]
fn retirement_of_queued_execute_retains_exact_not_started_ticket_without_writing() {
    use beryl_app::theme_runtime::{
        SettingsThemeBroker, ThemeSettingsBridgeError, ThemeSettingsWork,
    };
    let mut fixture = running_recovered();
    let observation = fixture
        .service
        .observe_repository(
            &fixture.store,
            NonZeroU64::new(1024 * 1024).unwrap(),
            ThemeManifestReadLimits::new(
                NonZeroUsize::new(4096).unwrap(),
                NonZeroUsize::new(16 * 1024).unwrap(),
                NonZeroUsize::new(256 * 1024).unwrap(),
            )
            .unwrap(),
            None,
        )
        .unwrap();
    let document = String::from_utf8(DOCUMENT.to_vec())
        .unwrap()
        .replace("id = \"active\"", "id = \"queued\"");
    let command = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            observation.manifest(),
            InstalledThemeId::new("queued").unwrap(),
            ThemeName::new("Queued").unwrap(),
            ThemeDocument::parse_bytes(document.as_bytes(), ThemeParseMode::StrictCandidate)
                .unwrap(),
        )
        .unwrap(),
    );
    let mut broker = SettingsThemeBroker::start(
        fixture.store.service_reference(),
        fixture.runtime.settings_source().unwrap(),
    )
    .unwrap();
    let reader = broker.reader();
    let block = fixture.faults.block_next(FaultPoint::BeforeThemeRead);
    let _read = reader.submit(ThemeSettingsWork::Observe).unwrap();
    assert!(block.wait_until_reached(Duration::from_secs(10)));
    let ticket = reader
        .submit(ThemeSettingsWork::Execute {
            observation,
            command,
            references: Arc::new(NoReferences),
        })
        .unwrap();
    let retired = std::thread::spawn(move || broker.retire());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match reader.submit(ThemeSettingsWork::Observe) {
            Err(ThemeSettingsBridgeError::Retired) => break,
            Ok(read) => reader.cancel(read).unwrap(),
            other => panic!("unexpected queued observer: {other:?}"),
        }
        assert!(Instant::now() < deadline, "broker did not close admission");
        std::thread::yield_now();
    }
    block.release();
    let retirement = retired.join().unwrap();
    assert!(retirement.unresolved.is_none());
    assert!(!retirement.worker_failed);
    let completion = retirement.completion.unwrap();
    assert_eq!(completion.ticket, ticket);
    assert_eq!(
        completion.not_started,
        Some(ThemeRuntimeFailureClass::Retired)
    );
    assert!(completion.repository.is_none());
    assert!(completion.reconciliation.is_none());
    assert!(
        !fixture
            .store
            .canonical_path()
            .join("themes/installed/queued.toml")
            .exists()
    );
    fixture.runtime.retire();
}

#[test]
fn unavailable_context_has_fresh_owner_and_preserves_prior_proof_without_adopting_it() {
    let mut fixture = recovered();
    let context = fixture.runtime.current().unwrap();
    assert_eq!(context.home(), fixture.service.home());
    assert_eq!(context.home().home_id(), fixture.previous.home().home_id());
    assert!(context.home().home_generation() > fixture.previous.home().home_generation());
    assert_eq!(context.prepared(), fixture.previous.prepared());
    assert_eq!(context.prepared().home(), fixture.previous.home());
    assert_ne!(context.home(), context.prepared().home());
    assert_eq!(
        context.publication(),
        &AppearancePublication::UnavailableContext
    );
    assert!(!context.is_preview());
    assert!(!fixture.runtime.selection_available());
    assert_eq!(
        fixture.runtime.sync_applied_selection(
            fixture.service.settings_identity(
                fixture.state.settings().revision(&fixture.store).unwrap(),
                None
            ),
            None,
        ),
        Err(ThemeRuntimeFailureClass::Settings)
    );
    assert!(fixture.runtime.proven_selection().is_none());
    assert_eq!(
        fixture.runtime.consume_settings_outcome(
            &fixture.store,
            SettingsThemeOutcome::Committed(confirmed(fixture.previous.prepared().clone()),)
        ),
        Err(ThemeRuntimeFailureClass::Settings)
    );
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &context));
    assert!(!fixture.runtime.selection_available());
    let settings = fixture.service.settings_identity(
        fixture.state.settings().revision(&fixture.store).unwrap(),
        None,
    );
    assert!(matches!(
        fixture.old_source.prepare_selection(
            &fixture.store,
            ThemeDraftIdentity::new(NonZeroU64::new(2).unwrap()),
            ThemeDraftRevision::INITIAL,
            settings,
            None,
        ),
        Err(ThemeRuntimeFailureClass::Retired)
    ));
    assert!(matches!(
        fixture
            .runtime
            .settings_source()
            .unwrap()
            .prepare_selection(
                &fixture.store,
                ThemeDraftIdentity::new(NonZeroU64::new(2).unwrap()),
                ThemeDraftRevision::INITIAL,
                settings,
                None,
            ),
        Err(ThemeRuntimeFailureClass::Reconciliation)
    ));
    fixture.runtime.retire();
}

fn scalar_commit(
    home: &HomeStore,
    state: &BerylState,
    theme: &ThemeService,
) -> ThemeSettingsIdentity {
    let service = SettingsService::prepare(
        home.service_reference(),
        state.settings(),
        SettingsProcessOwner::new(home.home_id()),
        1,
    )
    .unwrap();
    let mut draft = SettingsDraft::new(service.snapshot().unwrap(), 1, 1);
    draft
        .set(SettingsDraftValue::DraftAutosaveInterval("45".into()))
        .unwrap();
    let prepared = service
        .prepare_apply(&draft, SettingsApplyAction::Apply, 1)
        .unwrap();
    assert!(matches!(
        service
            .execute(prepared, &beryl_home_store::CommandCancellation::new())
            .unwrap(),
        SettingsApplyObservation::Committed { .. }
    ));
    let snapshot = service.snapshot().unwrap();
    let identity = ThemeSettingsIdentity::new(
        theme.home(),
        snapshot.identity().settings_revision,
        snapshot.record_revision(SettingKey::ActiveThemeId),
    );
    service.retire().unwrap();
    identity
}

#[test]
fn scalar_commit_advances_only_known_selection_metadata_without_io_or_appearance_retagging() {
    let mut fixture = recovered();
    fixture.runtime.retire();
    let settings = fixture.state.settings();
    let record = settings
        .setting(&fixture.store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    let mut runtime = ThemeRuntime::start(
        &fixture.store,
        fixture.service.clone(),
        settings.revision(&fixture.store).unwrap(),
        Some(&record),
        config(),
    )
    .unwrap();
    let before = runtime.proven_selection().unwrap();
    let appearance = runtime.current().unwrap();
    let updated = scalar_commit(&fixture.store, &fixture.state, &fixture.service);
    assert!(updated.domain_revision() > before.0.domain_revision());
    assert_eq!(
        updated.active_record_revision(),
        before.0.active_record_revision()
    );
    let home_revision = fixture.store.home_revision().unwrap();
    assert_eq!(
        runtime.sync_applied_selection(updated, before.1.clone()),
        Ok(())
    );
    assert_eq!(
        runtime.sync_applied_selection(updated, before.1.clone()),
        Ok(())
    );
    assert_eq!(
        runtime.proven_selection(),
        Some((updated, before.1.clone()))
    );
    assert!(Arc::ptr_eq(&runtime.current().unwrap(), &appearance));
    assert_eq!(appearance.prepared().settings(), before.0);
    assert_eq!(fixture.store.home_revision().unwrap(), home_revision);
    for (identity, active, expected) in [
        (
            before.0,
            before.1.clone(),
            ThemeRuntimeFailureClass::Settings,
        ),
        (
            updated,
            Some(InstalledThemeId::new("other").unwrap()),
            ThemeRuntimeFailureClass::Settings,
        ),
        (
            ThemeSettingsIdentity::new(
                updated.home(),
                updated.domain_revision(),
                Some(
                    beryl_state::RecordRevision::new(
                        updated.active_record_revision().unwrap().get() + 1,
                    )
                    .unwrap(),
                ),
            ),
            before.1.clone(),
            ThemeRuntimeFailureClass::Settings,
        ),
        (
            ThemeSettingsIdentity::new(
                ThemeService::acquire(&fixture.store).unwrap().home(),
                updated.domain_revision(),
                updated.active_record_revision(),
            ),
            before.1.clone(),
            ThemeRuntimeFailureClass::Identity,
        ),
    ] {
        assert_eq!(
            runtime.sync_applied_selection(identity, active),
            Err(expected)
        );
        assert_eq!(
            runtime.proven_selection(),
            Some((updated, before.1.clone()))
        );
        assert!(Arc::ptr_eq(&runtime.current().unwrap(), &appearance));
    }
    runtime.mark_selection_unavailable();
    runtime.mark_selection_unavailable();
    assert!(!runtime.selection_available());
    assert!(runtime.proven_selection().is_none());
    assert_eq!(
        runtime.sync_applied_selection(updated, before.1),
        Err(ThemeRuntimeFailureClass::Settings)
    );
    assert!(Arc::ptr_eq(&runtime.current().unwrap(), &appearance));
    runtime.retire();
    assert_eq!(
        runtime.sync_applied_selection(updated, None),
        Err(ThemeRuntimeFailureClass::Retired)
    );
}

#[test]
fn proven_absent_active_selection_accepts_scalar_revision_without_becoming_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let service = state.themes();
    let mut runtime = ThemeRuntime::start(
        &store,
        service.clone(),
        state.settings().revision(&store).unwrap(),
        None,
        config(),
    )
    .unwrap();
    let appearance = runtime.current().unwrap();
    assert!(runtime.proven_selection().unwrap().1.is_none());
    let updated = scalar_commit(&store, &state, &service);
    assert_eq!(runtime.sync_applied_selection(updated, None), Ok(()));
    assert_eq!(runtime.proven_selection(), Some((updated, None)));
    assert!(runtime.selection_available());
    assert!(Arc::ptr_eq(&runtime.current().unwrap(), &appearance));
    runtime.retire();
}

#[test]
fn retained_preparation_source_observes_unavailable_gate_before_any_home_read() {
    let mut fixture = recovered();
    fixture.runtime.retire();
    let settings = fixture.state.settings();
    let record = settings
        .setting(&fixture.store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    let mut runtime = ThemeRuntime::start(
        &fixture.store,
        fixture.service.clone(),
        settings.revision(&fixture.store).unwrap(),
        Some(&record),
        config(),
    )
    .unwrap();
    let source = runtime.settings_source().unwrap();
    let retained = source.clone();
    let proven = runtime.proven_selection().unwrap();
    let appearance = runtime.current().unwrap();
    assert!(
        source
            .prepare_selection(
                &fixture.store,
                ThemeDraftIdentity::new(NonZeroU64::new(3).unwrap()),
                ThemeDraftRevision::INITIAL,
                proven.0,
                proven.1.clone(),
            )
            .is_ok()
    );
    runtime.mark_selection_unavailable();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    for reader in [source, retained, runtime.settings_source().unwrap()] {
        assert!(matches!(
            reader.prepare_selection(
                &fixture.store,
                ThemeDraftIdentity::new(NonZeroU64::new(3).unwrap()),
                ThemeDraftRevision::INITIAL,
                proven.0,
                proven.1.clone(),
            ),
            Err(ThemeRuntimeFailureClass::Reconciliation)
        ));
    }
    assert!(
        fixture.store.home_revision().is_err(),
        "closed preparation readers must leave the injected read fault unconsumed"
    );
    assert!(runtime.proven_selection().is_none());
    assert!(Arc::ptr_eq(&runtime.current().unwrap(), &appearance));
    runtime.retire();
    fixture.runtime.retire();
}

fn on_worker<T: Send + 'static>(
    cx: &mut gpui::TestAppContext,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let worker = std::thread::Builder::new()
        .name("appearance-handoff".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(work)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "appearance handoff did not settle"
        );
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap()
}

#[gpui::test]
fn gpui_publication_accepts_fresh_proof_after_prior_context_without_retagging_prior_proof(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, next) = on_worker(cx, || {
        let mut fixture = recovered();
        let context = fixture.runtime.recovery_context().unwrap();
        fixture.runtime.retire();
        let settings = fixture.state.settings();
        let record = settings
            .setting(&fixture.store, SettingKey::ActiveThemeId)
            .unwrap()
            .unwrap();
        let mut proven = ThemeRuntime::start(
            &fixture.store,
            fixture.service.clone(),
            settings.revision(&fixture.store).unwrap(),
            Some(&record),
            config(),
        )
        .unwrap();
        let next = proven.current().unwrap().prepared().clone();
        proven.retire();
        fixture.runtime = ThemeRuntime::start_unavailable_context(
            &fixture.store,
            fixture.service.clone(),
            context,
            config(),
        )
        .unwrap();
        (fixture, next)
    });
    let context = fixture.runtime.current().unwrap();
    let owner = cx.update(|app| {
        GpuiAppearanceWindowSet::new(context.clone(), NonZeroUsize::new(4).unwrap(), app)
    });
    let target = owner.read_with(cx, |owner, _| owner.target());
    let prepared = next.clone();
    let fixture = on_worker(cx, {
        let target = target.clone();
        move || {
            let mut fixture = fixture;
            fixture.runtime.attach_publication_target(target).unwrap();
            assert_eq!(
                fixture
                    .runtime
                    .consume_settings_outcome(
                        &fixture.store,
                        SettingsThemeOutcome::ReconciledExactNew(confirmed(prepared))
                    )
                    .unwrap(),
                SettingsThemeResult::Published
            );
            fixture
        }
    });
    let published = fixture.runtime.current().unwrap();
    assert_eq!(published.home(), fixture.service.home());
    assert_eq!(published.prepared(), &next);
    assert_eq!(published.prepared().home(), published.home());
    assert_eq!(published.publication(), &AppearancePublication::Durable);
    assert!(Arc::ptr_eq(&target.snapshot().current, &published));
    assert_eq!(context.prepared(), fixture.previous.prepared());
    assert_ne!(context.prepared().home(), published.home());
    assert!(fixture.runtime.selection_available());
    let (mut fixture, scalar) = on_worker(cx, move || {
        let scalar = scalar_commit(&fixture.store, &fixture.state, &fixture.service);
        (fixture, scalar)
    });
    let active = fixture.runtime.proven_selection().unwrap().1;
    assert_eq!(
        cx.update(|_| fixture.runtime.sync_applied_selection(scalar, active)),
        Ok(())
    );
    assert_eq!(fixture.runtime.proven_selection().unwrap().0, scalar);
    assert!(Arc::ptr_eq(&fixture.runtime.current().unwrap(), &published));
    fixture = on_worker(cx, move || {
        fixture.runtime.retire();
        fixture
    });
    drop((fixture, owner, target));
}
