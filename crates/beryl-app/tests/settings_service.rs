use std::sync::Arc;
use std::time::Duration;

use beryl_app::settings_service::*;
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingUpdate, SettingValue, SettingsState,
};

#[cfg(feature = "test-faults")]
#[path = "settings_service/faults.rs"]
mod faults;

struct Fixture {
    _directory: tempfile::TempDir,
    store: HomeStore,
    settings: SettingsState,
    owner: SettingsProcessOwner,
    service: SettingsService,
    #[cfg(feature = "test-faults")]
    faults: beryl_home_store::test_faults::FaultController,
}

#[test]
fn abandoned_preparation_retains_exact_noncommit_observation_for_the_window() {
    for retire_first in [false, true] {
        let fixture = Fixture::new();
        let mut draft = fixture.draft();
        draft
            .set(SettingsDraftValue::DeveloperInstructions(
                "pending draft".into(),
            ))
            .unwrap();
        let binding = draft.binding();
        let prepared = fixture
            .service
            .prepare_apply(&draft, SettingsApplyAction::Ok, 1)
            .unwrap();
        if retire_first {
            fixture.service.retire().unwrap();
        }
        drop(prepared);
        let observation = fixture
            .owner
            .observation(1)
            .unwrap()
            .expect("original terminal observation");
        assert_eq!(observation.binding(), binding);
        assert_eq!(observation.operation(), 1);
        assert_eq!(observation.action(), SettingsApplyAction::Ok);
        assert!(matches!(
            observation,
            SettingsApplyObservation::NotCommitted { .. }
        ));
        assert!(fixture.owner.pending_operation().is_none());
        assert!(
            fixture
                .settings
                .setting(&fixture.store, SettingKey::DeveloperInstructions)
                .unwrap()
                .is_none()
        );
    }
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let options = HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT);
        #[cfg(feature = "test-faults")]
        let faults = beryl_home_store::test_faults::FaultController::new();
        #[cfg(feature = "test-faults")]
        let mut candidate = HomeOpenCandidate::open_with_faults(options, faults.clone()).unwrap();
        #[cfg(not(feature = "test-faults"))]
        let mut candidate = HomeOpenCandidate::open(options).unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let settings = state.settings();
        let owner = SettingsProcessOwner::new(store.home_id());
        let service = SettingsService::prepare(
            store.service_reference(),
            settings.clone(),
            owner.clone(),
            1,
        )
        .unwrap();
        Self {
            _directory: directory,
            store,
            settings,
            owner,
            service,
            #[cfg(feature = "test-faults")]
            faults,
        }
    }
    fn draft(&self) -> SettingsDraft {
        SettingsDraft::new(self.service.snapshot().unwrap(), 1, 1)
    }
}

#[test]
fn absent_defaults_are_coherent_clean_and_bound_to_the_exact_home() {
    let fixture = Fixture::new();
    let draft = fixture.draft();
    assert!(draft.modified().is_empty());
    assert!(!draft.can_apply());
    let applied = fixture.service.applied_reader().snapshot().unwrap();
    assert_eq!(
        applied.compaction_timeout().unwrap(),
        Duration::from_secs(180)
    );
    assert_eq!(
        applied.draft_autosave_interval().unwrap(),
        Duration::from_secs(30)
    );
    assert_eq!(applied.developer_instructions().unwrap(), "");
    assert!(applied.active_theme().unwrap().is_none());
    assert!(applied.end_turn_sound().unwrap().is_none());
    assert!(applied.matches_home(&fixture.store));
    assert!(!applied.matches_home(&Fixture::new().store));
    for key in SETTINGS_KEYS {
        assert_eq!(applied.origin(key), AppliedSettingOrigin::AbsentDefault);
    }
}

#[test]
fn complete_invalid_draft_prevents_every_write_and_preserves_exact_text() {
    let fixture = Fixture::new();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::DraftAutosaveInterval("45".into()))
        .unwrap();
    draft
        .set(SettingsDraftValue::DeveloperInstructions(
            "x".repeat(60 * 1024 + 1),
        ))
        .unwrap();
    assert!(!draft.can_apply());
    assert!(matches!(
        fixture
            .service
            .prepare_apply(&draft, SettingsApplyAction::Apply, 1),
        Err(SettingsServiceError::Invalid {
            key: SettingKey::DeveloperInstructions,
            ..
        })
    ));
    assert_eq!(
        draft.value(SettingKey::DeveloperInstructions).len(),
        60 * 1024 + 1
    );
    assert!(!fixture.service.is_reconciling());
    for key in SETTINGS_KEYS {
        assert!(
            fixture
                .settings
                .setting(&fixture.store, key)
                .unwrap()
                .is_none()
        );
    }
    let binding = draft.binding();
    assert!(
        draft
            .set(SettingsDraftValue::DeveloperInstructions(
                "x".repeat(64 * 1024 + 1)
            ))
            .is_err()
    );
    assert_eq!(draft.binding(), binding);
}

#[test]
fn apply_commits_the_complete_batch_before_invalidation_and_cleans_only_matching_draft() {
    let fixture = Fixture::new();
    let reader = fixture.service.applied_reader();
    let mut draft = fixture.draft();
    let mut stale = draft.clone();
    let previous = draft.binding().source;
    draft
        .set(SettingsDraftValue::ContextCompactionTimeout("7".into()))
        .unwrap();
    draft
        .set(SettingsDraftValue::DraftAutosaveInterval("60".into()))
        .unwrap();
    draft
        .set(SettingsDraftValue::DeveloperInstructions(
            "exact\ntext".into(),
        ))
        .unwrap();
    let prepared = fixture
        .service
        .prepare_apply(&draft, SettingsApplyAction::Ok, 1)
        .unwrap();
    assert_eq!(fixture.service.pending_operation(), Some(1));
    let observation = fixture
        .service
        .execute(prepared, &CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        observation,
        SettingsApplyObservation::Committed {
            action: SettingsApplyAction::Ok,
            ..
        }
    ));
    assert!(reader.invalidation_after(previous).unwrap().is_some());
    let applied = reader.snapshot().unwrap();
    assert_eq!(
        applied.compaction_timeout().unwrap(),
        Duration::from_secs(7)
    );
    assert_eq!(
        applied.draft_autosave_interval().unwrap(),
        Duration::from_secs(60)
    );
    assert_eq!(applied.developer_instructions().unwrap(), "exact\ntext");
    assert!(stale.accept_committed(&observation).is_err());
    assert!(draft.accept_committed(&observation).unwrap());
    assert!(draft.modified().is_empty());
    assert!(!fixture.service.is_reconciling());
    assert!(matches!(
        fixture.service.observation(1).unwrap(),
        Some(SettingsApplyObservation::Committed { .. })
    ));
}

#[test]
fn abandoned_preparation_releases_only_the_unexecuted_flight() {
    let fixture = Fixture::new();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::DraftAutosaveInterval("45".into()))
        .unwrap();
    let prepared = fixture
        .service
        .prepare_apply(&draft, SettingsApplyAction::Apply, 1)
        .unwrap();
    assert!(matches!(
        fixture
            .service
            .prepare_apply(&draft, SettingsApplyAction::Apply, 2),
        Err(SettingsServiceError::Busy)
    ));
    drop(prepared);
    assert_eq!(fixture.service.pending_operation(), None);
    let prepared = fixture
        .service
        .prepare_apply(&draft, SettingsApplyAction::Apply, 2)
        .unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        fixture.service.execute(prepared, &cancellation).unwrap(),
        SettingsApplyObservation::NotCommitted { .. }
    ));
    assert!(
        fixture
            .settings
            .setting(&fixture.store, SettingKey::DraftAutosaveInterval)
            .unwrap()
            .is_none()
    );
}

#[test]
fn reopen_discard_advances_window_identity_and_reset_only_changes_one_row() {
    let fixture = Fixture::new();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::DraftAutosaveInterval("45".into()))
        .unwrap();
    draft
        .set(SettingsDraftValue::DeveloperInstructions("staged".into()))
        .unwrap();
    draft.reset(SettingKey::DraftAutosaveInterval).unwrap();
    assert_eq!(draft.modified(), vec![SettingKey::DeveloperInstructions]);
    let before = draft.binding();
    draft
        .discard_to_baseline(fixture.service.snapshot().unwrap())
        .unwrap();
    assert!(draft.binding().draft_revision > before.draft_revision);
    assert!(draft.modified().is_empty());
}

#[test]
fn rejected_saved_value_uses_feature_default_with_visible_row_feedback() {
    let fixture = Fixture::new();
    let apply = ApplySettings::new(vec![SettingUpdate::new(
        SettingKey::DraftAutosaveInterval,
        ExpectedSettingRevision::Absent,
        SettingValue::draft_autosave_interval_seconds(0),
    )])
    .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(
            fixture
                .settings
                .apply(fixture.settings.revision(&fixture.store).unwrap(), apply),
        )
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    let fresh = SettingsService::prepare(
        fixture.store.service_reference(),
        fixture.settings.clone(),
        fixture.owner.clone(),
        2,
    )
    .unwrap();
    let baseline = fresh.snapshot().unwrap();
    assert_eq!(
        baseline.row(SettingKey::DraftAutosaveInterval).availability,
        SettingsRowAvailability::RejectedSaved
    );
    assert_eq!(
        baseline.applied().draft_autosave_interval().unwrap(),
        Duration::from_secs(30)
    );
    assert_eq!(
        baseline.applied().origin(SettingKey::DraftAutosaveInterval),
        AppliedSettingOrigin::RejectedSaved
    );
}

#[test]
fn retired_service_and_weak_readers_cannot_execute_or_observe_even_with_live_gui_arc() {
    let fixture = Fixture::new();
    let old_gui = Arc::new(fixture.service.clone());
    let reader = old_gui.applied_reader();
    let published = old_gui.reader();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::DraftAutosaveInterval("45".into()))
        .unwrap();
    let prepared = old_gui
        .prepare_apply(&draft, SettingsApplyAction::Apply, 1)
        .unwrap();
    old_gui.retire().unwrap();
    assert!(matches!(
        reader.snapshot(),
        Err(SettingsServiceError::Retired)
    ));
    assert!(matches!(
        published.snapshot(),
        Err(SettingsServiceError::Retired)
    ));
    assert!(matches!(
        old_gui.execute(prepared, &CommandCancellation::new()),
        Err(SettingsServiceError::Retired)
    ));
    assert!(fixture.owner.pending_operation().is_none());
}

#[test]
fn missing_theme_preparation_refuses_the_complete_batch_without_writing_valid_rows() {
    let fixture = Fixture::new();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::ActiveThemeId("missing-theme".into()))
        .unwrap();
    draft
        .set(SettingsDraftValue::DeveloperInstructions(
            "otherwise valid".into(),
        ))
        .unwrap();
    let prepared = fixture
        .service
        .prepare_apply(&draft, SettingsApplyAction::Apply, 1)
        .unwrap();
    assert!(matches!(
        fixture
            .service
            .execute(prepared, &CommandCancellation::new()),
        Err(SettingsServiceError::Theme)
    ));
    assert!(fixture.owner.pending_operation().is_none());
    for key in SETTINGS_KEYS {
        assert!(
            fixture
                .settings
                .setting(&fixture.store, key)
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(
        draft.value(SettingKey::DeveloperInstructions),
        "otherwise valid"
    );
}

#[test]
fn theme_choice_qualification_refuses_missing_source_without_claiming_command_custody() {
    let fixture = Fixture::new();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::ActiveThemeId("missing-theme".into()))
        .unwrap();
    let before = fixture.service.snapshot().unwrap();
    assert!(matches!(
        fixture.service.qualify_active_theme(&draft),
        Err(SettingsServiceError::Theme)
    ));
    assert!(fixture.owner.pending_operation().is_none());
    assert_eq!(
        fixture.service.snapshot().unwrap().identity(),
        before.identity()
    );
    let operation = fixture.service.next_operation_id().unwrap();
    assert_eq!(operation, 1);
    let prepared = fixture
        .service
        .prepare_apply(&draft, SettingsApplyAction::Apply, operation)
        .unwrap();
    drop(prepared);
    assert!(fixture.owner.pending_operation().is_none());
}

#[test]
fn prepared_batch_is_immutable_and_late_result_cannot_clean_a_newer_draft() {
    let fixture = Fixture::new();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::DeveloperInstructions(
            "original prepared text".into(),
        ))
        .unwrap();
    let prepared = fixture
        .service
        .prepare_apply(&draft, SettingsApplyAction::Apply, 1)
        .unwrap();
    draft
        .set(SettingsDraftValue::DeveloperInstructions(
            "newer staged text".into(),
        ))
        .unwrap();
    let observation = fixture
        .service
        .execute(prepared, &CommandCancellation::new())
        .unwrap();
    assert_eq!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .developer_instructions()
            .unwrap(),
        "original prepared text"
    );
    assert!(matches!(
        draft.accept_committed(&observation),
        Err(SettingsServiceError::Stale)
    ));
    assert_eq!(
        draft.value(SettingKey::DeveloperInstructions),
        "newer staged text"
    );
    assert!(matches!(
        fixture
            .service
            .prepare_apply(&draft, SettingsApplyAction::Apply, 2),
        Err(SettingsServiceError::Stale)
    ));
}

#[test]
fn abandoned_prepublication_reader_cannot_revive_after_home_publication() {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap();
    let owner = SettingsProcessOwner::new(publication.home_id());
    let prepared = PreparedSettingsService::prepare_candidate(
        &publication.recovery_access().unwrap(),
        &state.settings(),
        owner,
        1,
    )
    .unwrap();
    let reader = prepared.applied_reader();
    assert!(matches!(
        reader.snapshot(),
        Err(SettingsServiceError::Retired)
    ));
    drop(prepared);
    let _store = publication.publish().unwrap();
    assert!(matches!(
        reader.snapshot(),
        Err(SettingsServiceError::Retired)
    ));
}
