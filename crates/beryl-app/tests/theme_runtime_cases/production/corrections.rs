use super::*;

fn start_active_runtime(fixture: &RuntimeFixture) -> ThemeRuntime {
    install_active(fixture);
    execute_settings(fixture, SettingValue::active_theme_id("active").unwrap());
    let revision = fixture.state.settings().revision(&fixture.store).unwrap();
    let record = fixture.active_record();
    ThemeRuntime::start(
        &fixture.store,
        fixture.state.themes(),
        revision,
        Some(&record),
        config(),
    )
    .unwrap()
}

fn active_document_hint() -> ThemeChangeHint {
    ThemeChangeHint::DocumentChanged(InstalledThemeId::new("active").unwrap())
}

#[test]
fn confirmed_settings_durable_base_survives_adapter_rejection_and_retries_exactly() {
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
    let adapter = TestAdapter::new(41);
    runtime.register_adapter(adapter.clone()).unwrap();
    let initial = runtime.current().unwrap();
    let committed = fixture
        .state
        .themes()
        .settings_identity(DomainRevision::new(2).unwrap(), None);
    let confirmed = ConfirmedSettingsTheme::new(
        ThemeDraftIdentity::new(NonZeroU64::new(42).unwrap()),
        ThemeDraftRevision::INITIAL,
        committed,
        None,
        beryl_state::PreparedThemeAppearance::fallback(committed),
    );

    adapter.state.reject(true);
    assert_eq!(
        runtime.consume_settings_outcome(SettingsThemeOutcome::Committed(confirmed)),
        Err(ThemeRuntimeFailureClass::Publication)
    );
    assert_eq!(runtime.current().unwrap().number(), initial.number());
    let failed = runtime.diagnostics().appearance().unwrap();
    assert!(failed.pending_durable_application());
    assert_ne!(failed.durable_generation(), failed.current_generation());
    assert_eq!(
        runtime.diagnostics().last_failure(),
        Some(ThemeRuntimeFailureClass::Publication)
    );

    adapter.state.reject(false);
    assert_eq!(
        runtime.retry_current_durable().unwrap(),
        RepositoryAppearanceResult::Published
    );
    let applied = runtime.current().unwrap();
    assert_eq!(applied.prepared().settings(), committed);
    assert_eq!(adapter.state.current().number(), applied.number());
    let retried = runtime.diagnostics().appearance().unwrap();
    assert!(!retried.pending_durable_application());
    assert_eq!(retried.durable_generation(), retried.current_generation());
    assert_eq!(runtime.diagnostics().last_failure(), None);
}

#[test]
fn external_invalid_edit_retains_prior_and_later_valid_edit_publishes() {
    let fixture = RuntimeFixture::new();
    let mut runtime = start_active_runtime(&fixture);
    let initial = runtime.current().unwrap();

    replace_active_bytes(&fixture, INVALID_DOCUMENT);
    assert_eq!(
        await_appearance(&mut runtime, &fixture),
        RepositoryAppearanceResult::Retained(ThemeRuntimeFailureClass::DocumentInvalid)
    );
    assert_eq!(runtime.current().unwrap().number(), initial.number());

    replace_active_bytes(&fixture, UPDATED_DOCUMENT);
    assert_eq!(
        await_appearance(&mut runtime, &fixture),
        RepositoryAppearanceResult::Published
    );
    assert!(runtime.current().unwrap().number() > initial.number());
    let diagnostic = runtime.diagnostics();
    assert!(diagnostic.watch_hints() >= 2);
    assert!(diagnostic.state().change_hints() >= 2);
}

#[test]
fn invalid_active_candidate_cannot_advance_repository_before_coherent_retry() {
    let fixture = RuntimeFixture::new();
    let mut runtime = start_active_runtime(&fixture);
    let initial = runtime.current().unwrap();
    install_theme(&fixture, "other", "Other", OTHER_DOCUMENT);
    replace_active_bytes(&fixture, INVALID_DOCUMENT);

    let invalid = runtime
        .consume_change_hints(&fixture.store, &[ThemeChangeHint::ManifestChanged])
        .unwrap();
    assert_eq!(
        invalid.appearance(),
        RepositoryAppearanceResult::Retained(ThemeRuntimeFailureClass::DocumentInvalid)
    );
    assert_eq!(runtime.current().unwrap().number(), initial.number());

    replace_active_bytes(&fixture, UPDATED_DOCUMENT);
    let stale_document_only = runtime
        .consume_change_hints(&fixture.store, &[active_document_hint()])
        .unwrap();
    assert!(matches!(
        stale_document_only.appearance(),
        RepositoryAppearanceResult::Retained(_)
    ));
    assert_eq!(runtime.current().unwrap().number(), initial.number());

    assert_eq!(
        runtime
            .consume_change_hints(&fixture.store, &[ThemeChangeHint::ManifestChanged])
            .unwrap()
            .appearance(),
        RepositoryAppearanceResult::Published
    );
    assert!(runtime.current().unwrap().number() > initial.number());
}

#[test]
fn missing_active_candidate_and_stale_restore_cannot_form_a_mixed_snapshot() {
    let fixture = RuntimeFixture::new();
    let mut runtime = start_active_runtime(&fixture);
    let initial = runtime.current().unwrap();
    install_theme(&fixture, "other", "Other", OTHER_DOCUMENT);
    let active_path = fixture
        .directory
        .path()
        .join("themes/installed/active.toml");
    fs::remove_file(&active_path).unwrap();

    let missing = runtime
        .consume_change_hints(&fixture.store, &[ThemeChangeHint::ManifestChanged])
        .unwrap();
    assert_eq!(
        missing.appearance(),
        RepositoryAppearanceResult::Retained(ThemeRuntimeFailureClass::DocumentUnreadable)
    );
    assert_eq!(runtime.current().unwrap().number(), initial.number());

    fs::write(&active_path, UPDATED_DOCUMENT).unwrap();
    let stale_document_only = runtime
        .consume_change_hints(&fixture.store, &[active_document_hint()])
        .unwrap();
    assert!(matches!(
        stale_document_only.appearance(),
        RepositoryAppearanceResult::Retained(_)
    ));
    assert_eq!(runtime.current().unwrap().number(), initial.number());

    assert_eq!(
        runtime
            .consume_change_hints(&fixture.store, &[ThemeChangeHint::ManifestChanged])
            .unwrap()
            .appearance(),
        RepositoryAppearanceResult::Published
    );
    assert!(runtime.current().unwrap().number() > initial.number());
}

#[test]
fn overflow_and_duplicate_hints_coalesce_into_one_exact_refresh() {
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
    let result = runtime
        .consume_change_hints(
            &fixture.store,
            &[
                ThemeChangeHint::ManifestChanged,
                ThemeChangeHint::ManifestChanged,
                ThemeChangeHint::Overflow,
                ThemeChangeHint::Overflow,
            ],
        )
        .unwrap();
    assert_eq!(result.received(), 4);
    assert!(!result.more_pending());
    assert_eq!(result.appearance(), RepositoryAppearanceResult::Unchanged);
    let diagnostics = runtime.diagnostics();
    assert_eq!(diagnostics.watch_batches(), 1);
    assert_eq!(diagnostics.watch_hints(), 4);
    assert_eq!(diagnostics.app_coalesced_hints(), 3);
    assert_eq!(diagnostics.app_overflow_hints(), 2);
}

#[test]
fn rejected_full_refresh_discards_candidate_and_requires_fresh_reread() {
    let fixture = RuntimeFixture::new();
    let mut runtime = start_active_runtime(&fixture);
    let adapter = TestAdapter::new(51);
    runtime.register_adapter(adapter.clone()).unwrap();
    let prior_current = runtime.current().unwrap();
    let prior = runtime.diagnostics();
    install_theme(&fixture, "other", "Other", OTHER_DOCUMENT);
    replace_active_bytes(&fixture, UPDATED_DOCUMENT);

    adapter.state.reject(true);
    let rejected = runtime
        .consume_change_hints(&fixture.store, &[ThemeChangeHint::ManifestChanged])
        .unwrap();
    assert_eq!(
        rejected.appearance(),
        RepositoryAppearanceResult::Retained(ThemeRuntimeFailureClass::Publication)
    );
    let retained = runtime.diagnostics();
    assert_eq!(
        retained.repository_generation(),
        prior.repository_generation()
    );
    assert_eq!(
        retained.active_document_revision(),
        prior.active_document_revision()
    );
    assert_eq!(
        retained.appearance().unwrap().durable_generation(),
        prior.appearance().unwrap().durable_generation()
    );
    assert!(!retained.appearance().unwrap().pending_durable_application());
    assert!(retained.refresh_reread_needed());
    assert_eq!(runtime.current().unwrap().number(), prior_current.number());
    assert_eq!(
        runtime.retry_current_durable().unwrap(),
        RepositoryAppearanceResult::Unchanged
    );

    adapter.state.reject(false);
    assert_eq!(
        runtime
            .consume_change_hints(&fixture.store, &[ThemeChangeHint::ManifestChanged])
            .unwrap()
            .appearance(),
        RepositoryAppearanceResult::Published
    );
    let applied = runtime.diagnostics();
    assert_ne!(
        applied.repository_generation(),
        prior.repository_generation()
    );
    assert_ne!(
        applied.active_document_revision(),
        prior.active_document_revision()
    );
    assert!(!applied.refresh_reread_needed());
}

#[test]
fn rejected_live_refresh_discards_candidate_and_requires_fresh_reread() {
    let fixture = RuntimeFixture::new();
    let mut runtime = start_active_runtime(&fixture);
    let adapter = TestAdapter::new(52);
    runtime.register_adapter(adapter.clone()).unwrap();
    let prior_current = runtime.current().unwrap();
    let prior = runtime.diagnostics();
    replace_active_bytes(&fixture, UPDATED_DOCUMENT);

    adapter.state.reject(true);
    let rejected = runtime
        .consume_change_hints(&fixture.store, &[active_document_hint()])
        .unwrap();
    assert_eq!(
        rejected.appearance(),
        RepositoryAppearanceResult::Retained(ThemeRuntimeFailureClass::Publication)
    );
    let retained = runtime.diagnostics();
    assert_eq!(
        retained.repository_generation(),
        prior.repository_generation()
    );
    assert_eq!(
        retained.active_document_revision(),
        prior.active_document_revision()
    );
    assert_eq!(
        retained.appearance().unwrap().durable_generation(),
        prior.appearance().unwrap().durable_generation()
    );
    assert!(!retained.appearance().unwrap().pending_durable_application());
    assert!(retained.refresh_reread_needed());
    assert_eq!(runtime.current().unwrap().number(), prior_current.number());
    assert_eq!(
        runtime.retry_current_durable().unwrap(),
        RepositoryAppearanceResult::Unchanged
    );

    adapter.state.reject(false);
    assert_eq!(
        runtime
            .consume_change_hints(&fixture.store, &[active_document_hint()])
            .unwrap()
            .appearance(),
        RepositoryAppearanceResult::Published
    );
    let applied = runtime.diagnostics();
    assert_eq!(
        applied.repository_generation(),
        prior.repository_generation()
    );
    assert_ne!(
        applied.active_document_revision(),
        prior.active_document_revision()
    );
    assert!(!applied.refresh_reread_needed());
}
