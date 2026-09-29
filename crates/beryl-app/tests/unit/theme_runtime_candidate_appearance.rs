use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_state::{
    ApplySettings, ExpectedSettingRevision, SettingUpdate, SettingValue, ThemeAppearanceSource,
    ThemeDocumentDigest,
};

const DOCUMENT: &str = "schema = 1\nid = \"active\"\nname = \"Active\"\n\n[[role]]\nid = \"app.window\"\nbackground = \"#102030\"\n";

fn seeded(document: Option<&str>) -> (tempfile::TempDir, HomeStore, BerylState, FaultController) {
    let (directory, candidate, state, faults) = candidate();
    let store = candidate.publish().unwrap();
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
        CommandOutcome::Committed { .. }
    ));
    if let Some(document) = document {
        std::fs::create_dir_all(directory.path().join("themes/installed")).unwrap();
        std::fs::write(
            directory.path().join("themes/manifest.toml"),
            "schema_version = 1\ngeneration = 2\n\n[[theme]]\nid = \"active\"\nname = \"Active\"\n",
        )
        .unwrap();
        std::fs::write(
            directory.path().join("themes/installed/active.toml"),
            document,
        )
        .unwrap();
    }
    (directory, store, state, faults)
}

fn assert_appearance(prepared: &PreparedThemeRuntime, document: Option<&str>) {
    let appearance = prepared.current();
    match document {
        Some(DOCUMENT) => {
            let ThemeAppearanceSource::Installed(identity) = appearance.prepared().source() else {
                panic!("installed appearance expected");
            };
            assert_eq!(
                identity.digest(),
                ThemeDocumentDigest::of_bytes(DOCUMENT.as_bytes())
            );
            assert_eq!(prepared.runtime.diagnostics().last_failure(), None);
            assert_eq!(prepared.runtime.diagnostics().pages_read(), 1);
        }
        _ => {
            assert!(matches!(
                appearance.prepared().source(),
                ThemeAppearanceSource::BuiltinFallback(_)
            ));
            assert_eq!(
                prepared.runtime.diagnostics().last_failure(),
                Some(if document.is_some() {
                    ThemeRuntimeFailureClass::DocumentInvalid
                } else {
                    ThemeRuntimeFailureClass::DocumentMissing
                })
            );
        }
    }
}

#[test]
fn initial_candidate_resolves_persisted_selection_and_fallback_before_publication() {
    for document in [Some(DOCUMENT), Some("schema = invalid"), None] {
        let (directory, store, _, _) = seeded(document);
        store.close().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let mut candidate = candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap();
        let prepared = PreparedThemeRuntime::prepare(
            &mut candidate,
            state.themes(),
            &state.settings(),
            config(),
        )
        .unwrap();
        assert_appearance(&prepared, document);
        assert!(
            state
                .settings()
                .revision(&candidate.service_reference())
                .is_err()
        );
        let appearance = prepared.current();
        let store = candidate.publish().unwrap();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        let mut runtime = prepared.release_published(&store).unwrap();
        assert!(Arc::ptr_eq(&appearance, &runtime.current().unwrap()));
        runtime.retire();
        assert!(
            store.home_revision().is_err(),
            "release must leave the read fault unconsumed"
        );
        assert_eq!(state.themes().diagnostics().active_subscriptions(), 0);
        store.close().unwrap();
    }
}

#[test]
fn recovered_candidate_resolves_selection_and_fallback_with_fresh_settings() {
    for document in [Some(DOCUMENT), Some("schema = invalid"), None] {
        let (_directory, store, stale, faults) = seeded(document);
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut candidate = store.recover_same_home().unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let error = PreparedThemeRuntime::prepare_recovery(
            &mut candidate,
            state.themes(),
            &stale.settings(),
            config(),
        )
        .err()
        .unwrap();
        assert_eq!(error.class(), ThemeRuntimeFailureClass::Settings);
        assert_eq!(state.themes().diagnostics().active_subscriptions(), 0);
        let prepared = PreparedThemeRuntime::prepare_recovery(
            &mut candidate,
            state.themes(),
            &state.settings(),
            config(),
        )
        .unwrap();
        assert_appearance(&prepared, document);
        assert!(
            state
                .settings()
                .revision(&candidate.service_reference())
                .is_err()
        );
        let appearance = prepared.current();
        let store = candidate.publish().unwrap();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        let mut runtime = prepared.release_published(&store).unwrap();
        assert!(Arc::ptr_eq(&appearance, &runtime.current().unwrap()));
        runtime.retire();
        assert!(store.home_revision().is_err());
        store.close().unwrap();
    }
}

#[test]
fn failed_candidate_settings_reads_join_the_dormant_watcher() {
    let (_directory, mut candidate, state, faults) = candidate();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    let error =
        PreparedThemeRuntime::prepare(&mut candidate, state.themes(), &state.settings(), config())
            .err()
            .unwrap();
    assert_eq!(error.class(), ThemeRuntimeFailureClass::Settings);
    assert_eq!(state.themes().diagnostics().active_subscriptions(), 0);
    candidate.close().unwrap();

    let (_directory, mut candidate, _, state, faults) = recovery::recovered();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    let error = PreparedThemeRuntime::prepare_recovery(
        &mut candidate,
        state.themes(),
        &state.settings(),
        config(),
    )
    .err()
    .unwrap();
    assert_eq!(error.class(), ThemeRuntimeFailureClass::Settings);
    assert_eq!(state.themes().diagnostics().active_subscriptions(), 0);
    candidate.abort().close().unwrap();
}

#[test]
fn foreign_settings_cannot_supply_candidate_appearance() {
    let (_directory, mut candidate, state, _) = candidate();
    let (_foreign_directory, foreign, foreign_state, _) = super::candidate();
    let error = PreparedThemeRuntime::prepare(
        &mut candidate,
        state.themes(),
        &foreign_state.settings(),
        config(),
    )
    .err()
    .unwrap();
    assert_eq!(error.class(), ThemeRuntimeFailureClass::Settings);
    assert_eq!(state.themes().diagnostics().active_subscriptions(), 0);
    candidate.close().unwrap();
    foreign.close().unwrap();
}
