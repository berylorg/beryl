use super::*;
use beryl_home_store::{HomeHealthState, test_faults::FaultPoint};

#[test]
fn original_indeterminate_is_installed_before_observation_and_survives_lost_observer() {
    let fixture = Fixture::new();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::DeveloperInstructions(
            "accepted exact text".into(),
        ))
        .unwrap();
    let before = fixture.service.applied_reader().snapshot().unwrap();
    let prepared = fixture
        .service
        .prepare_apply(&draft, SettingsApplyAction::Apply, 1)
        .unwrap();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let abandoned_observation = fixture
        .service
        .execute(prepared, &CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        abandoned_observation,
        SettingsApplyObservation::Reconciling { .. }
    ));
    drop(abandoned_observation);
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    assert_eq!(fixture.service.pending_operation(), Some(1));
    assert_eq!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .identity,
        before.identity
    );
    assert_eq!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .developer_instructions()
            .unwrap(),
        ""
    );
    let final_observation = fixture.service.reconcile_original(1).unwrap();
    assert!(matches!(
        final_observation,
        SettingsApplyObservation::Committed { .. }
    ));
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert_eq!(fixture.service.pending_operation(), None);
    assert_eq!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .developer_instructions()
            .unwrap(),
        "accepted exact text"
    );
    assert!(draft.accept_committed(&final_observation).unwrap());
}

#[test]
fn healthy_unresolved_original_excludes_only_its_keys_and_disjoint_batch_can_commit() {
    let fixture = Fixture::new();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::DeveloperInstructions(
            "uncertain exact text".into(),
        ))
        .unwrap();
    let prepared = fixture
        .service
        .prepare_apply(&draft, SettingsApplyAction::Apply, 1)
        .unwrap();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        fixture
            .service
            .execute(prepared, &CommandCancellation::new())
            .unwrap(),
        SettingsApplyObservation::Reconciling { .. }
    ));
    fixture
        .faults
        .fail_next(FaultPoint::BeforeReconciliationSnapshot);
    let observation = fixture.service.reconcile_original(1).unwrap();
    assert_eq!(fixture.store.health().state(), HomeHealthState::Healthy);
    assert!(matches!(
        observation,
        SettingsApplyObservation::ScopedUnresolved { .. }
    ));
    assert!(draft.mark_scoped_unresolved(&observation).unwrap());
    assert_eq!(
        draft.value(SettingKey::DeveloperInstructions),
        "uncertain exact text"
    );
    assert!(!draft.can_apply());
    assert_eq!(
        fixture.owner.excluded_keys().unwrap(),
        vec![SettingKey::DeveloperInstructions]
    );
    assert!(!fixture.service.is_reconciling());
    assert!(matches!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .developer_instructions(),
        Err(SettingsServiceError::Unavailable(
            SettingKey::DeveloperInstructions
        ))
    ));
    let mut unrelated = SettingsDraft::new(fixture.service.snapshot().unwrap(), 2, 1);
    unrelated
        .set(SettingsDraftValue::DraftAutosaveInterval("50".into()))
        .unwrap();
    let prepared = fixture
        .service
        .prepare_apply(&unrelated, SettingsApplyAction::Apply, 2)
        .unwrap();
    assert!(matches!(
        fixture
            .service
            .execute(prepared, &CommandCancellation::new())
            .unwrap(),
        SettingsApplyObservation::Committed { .. }
    ));
    assert_eq!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .draft_autosave_interval()
            .unwrap(),
        Duration::from_secs(50)
    );
    assert!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .developer_instructions()
            .is_err()
    );
    let fresh = SettingsService::prepare(
        fixture.store.service_reference(),
        fixture.settings.clone(),
        fixture.owner.clone(),
        2,
    )
    .unwrap();
    assert!(
        fresh
            .applied_reader()
            .snapshot()
            .unwrap()
            .developer_instructions()
            .is_err()
    );
    assert!(matches!(
        fixture.service.reconcile_original(1).unwrap(),
        SettingsApplyObservation::ScopedUnresolved { .. }
    ));
}

#[test]
fn candidate_joins_original_registry_then_releases_fenced_reader_without_reexecuting() {
    let fixture = Fixture::new();
    let mut draft = fixture.draft();
    draft
        .set(SettingsDraftValue::DraftAutosaveInterval("55".into()))
        .unwrap();
    let prepared = fixture
        .service
        .prepare_apply(&draft, SettingsApplyAction::Apply, 1)
        .unwrap();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        fixture
            .service
            .execute(prepared, &CommandCancellation::new())
            .unwrap(),
        SettingsApplyObservation::Reconciling { .. }
    ));
    fixture.service.retire().unwrap();
    let Fixture {
        _directory,
        store,
        settings,
        owner,
        service,
        faults,
    } = fixture;
    drop(settings);
    drop(service);
    drop(draft);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    let mut candidate = store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let settings = state.settings();
    let prepared = PreparedSettingsService::prepare_candidate(
        &candidate.recovery_access().unwrap(),
        &settings,
        owner.clone(),
        2,
    )
    .unwrap();
    let reader = prepared.applied_reader();
    assert!(matches!(
        reader.snapshot(),
        Err(SettingsServiceError::Retired)
    ));
    assert_eq!(owner.pending_operation(), None);
    let store = candidate.publish().unwrap();
    let service = prepared
        .release_published(store.service_reference())
        .unwrap();
    assert_eq!(
        reader
            .snapshot()
            .unwrap()
            .draft_autosave_interval()
            .unwrap(),
        Duration::from_secs(55)
    );
    assert!(reader.snapshot().unwrap().matches_home(&store));
    assert!(matches!(
        owner.observation(1).unwrap(),
        Some(SettingsApplyObservation::Committed { .. })
    ));
    assert_eq!(
        settings
            .setting(&store, SettingKey::DraftAutosaveInterval)
            .unwrap()
            .unwrap()
            .revision()
            .get(),
        1
    );
    service.retire().unwrap();
}
