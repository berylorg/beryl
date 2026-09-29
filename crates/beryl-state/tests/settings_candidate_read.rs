use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeHealthState, HomeOpenCandidate, HomeOpenOptions,
    HomeOpenPublication, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};

fn candidate(
    directory: &tempfile::TempDir,
    faults: FaultController,
) -> (HomeOpenPublication, BerylState) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    (
        candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap(),
        state,
    )
}

#[test]
fn initial_candidate_reads_absent_and_persisted_settings_without_ordinary_admission() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let (mut candidate, state) = candidate(&directory, FaultController::new());
    let (foreign, foreign_state) = self::candidate(&foreign_directory, FaultController::new());
    let settings = state.settings();
    let reference = candidate.service_reference();
    let access = candidate.recovery_access().unwrap();
    let initial = settings.revision_candidate(&access).unwrap();
    assert_eq!(
        settings
            .setting_candidate(&access, SettingKey::ActiveThemeId)
            .unwrap(),
        None
    );
    assert!(
        foreign_state
            .settings()
            .revision_candidate(&access)
            .is_err()
    );
    assert!(
        foreign_state
            .settings()
            .setting_candidate(&access, SettingKey::ActiveThemeId)
            .is_err()
    );
    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(settings.revision(&reference).is_err());
    assert!(
        settings
            .setting(&reference, SettingKey::ActiveThemeId)
            .is_err()
    );
    let store = candidate.publish().unwrap();
    assert_eq!(settings.revision(&store).unwrap(), initial);
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            settings.apply(
                initial,
                ApplySettings::new(vec![SettingUpdate::new(
                    SettingKey::ActiveThemeId,
                    ExpectedSettingRevision::Absent,
                    SettingValue::active_theme_id("installed-theme").unwrap(),
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
    let revision = settings.revision(&store).unwrap();
    let record = settings
        .setting(&store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    assert_ne!(revision, initial);
    store.close().unwrap();

    let (mut reopened, fresh) = self::candidate(&directory, FaultController::new());
    let access = reopened.recovery_access().unwrap();
    assert_eq!(
        fresh.settings().revision_candidate(&access).unwrap(),
        revision
    );
    assert_eq!(
        fresh
            .settings()
            .setting_candidate(&access, SettingKey::ActiveThemeId)
            .unwrap(),
        Some(record)
    );
    assert!(settings.revision_candidate(&access).is_err());
    assert!(
        settings
            .setting_candidate(&access, SettingKey::ActiveThemeId)
            .is_err()
    );
    reopened.close().unwrap();
    foreign.close().unwrap();
}

#[test]
fn recovered_candidate_preserves_settings_and_rejects_old_and_foreign_handles() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (candidate, state) = candidate(&directory, faults.clone());
    let (foreign, foreign_state) = self::candidate(&foreign_directory, FaultController::new());
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
                    SettingValue::active_theme_id("recovered-theme").unwrap(),
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
    let revision = settings.revision(&store).unwrap();
    let record = settings
        .setting(&store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovered)
        .unwrap()
        .settings();
    let reference = recovered.service_reference();
    let access = recovered.recovery_access().unwrap();
    assert_eq!(reference.health().state(), HomeHealthState::Reopening);
    assert!(fresh.revision(&reference).is_err());
    assert!(
        fresh
            .setting(&reference, SettingKey::ActiveThemeId)
            .is_err()
    );
    for invalid in [settings, foreign_state.settings()] {
        assert!(invalid.revision_candidate(&access).is_err());
        assert!(
            invalid
                .setting_candidate(&access, SettingKey::ActiveThemeId)
                .is_err()
        );
    }
    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
    assert_eq!(
        fresh
            .setting_candidate(&access, SettingKey::ActiveThemeId)
            .unwrap(),
        Some(record.clone())
    );
    assert_eq!(
        fresh
            .setting_candidate(&access, SettingKey::DeveloperInstructions)
            .unwrap(),
        None
    );
    let store = recovered.publish().unwrap();
    assert_eq!(fresh.revision(&store).unwrap(), revision);
    assert_eq!(
        fresh.setting(&store, SettingKey::ActiveThemeId).unwrap(),
        Some(record)
    );
    store.close().unwrap();
    foreign.close().unwrap();
}

#[test]
fn confirmation_failure_closes_both_candidate_settings_reads() {
    for recovering in [false, true] {
        for fail_revision in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let faults = FaultController::new();
            let (mut candidate, state) = candidate(&directory, faults.clone());
            let check =
                |settings: beryl_state::SettingsState,
                 access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>| {
                    faults.fail_next(FaultPoint::BeforeReadConfirmation);
                    if fail_revision {
                        assert!(settings.revision_candidate(access).is_err());
                    } else {
                        assert!(
                            settings
                                .setting_candidate(access, SettingKey::ActiveThemeId)
                                .is_err()
                        );
                    }
                    assert!(settings.revision_candidate(access).is_err());
                    assert!(
                        settings
                            .setting_candidate(access, SettingKey::ActiveThemeId)
                            .is_err()
                    );
                };
            if recovering {
                let store = candidate.publish().unwrap();
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
                assert!(store.home_revision().is_err());
                let mut recovered = store.recover_same_home().unwrap();
                let settings = BerylState::reacquire_candidate(&recovered)
                    .unwrap()
                    .settings();
                check(settings, &recovered.recovery_access().unwrap());
                recovered.abort().close().unwrap();
            } else {
                check(state.settings(), &candidate.recovery_access().unwrap());
                candidate.close().unwrap();
            }
        }
    }
}
