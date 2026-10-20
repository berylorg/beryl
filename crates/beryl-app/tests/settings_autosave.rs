use beryl_app::{settings_autosave, settings_service::*};
use beryl_home_store::{
    CommandCancellation, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_state::{BerylState, SettingsState};
use gpui::TestAppContext;

#[cfg(feature = "test-faults")]
#[path = "syndic_composer_history/support.rs"]
mod composer;
#[cfg(feature = "test-faults")]
#[path = "resident_close_flush/support.rs"]
mod mounted_support;
#[cfg(feature = "test-faults")]
#[path = "pending_composer_activation/support.rs"]
mod widget_support;

struct Fixture {
    _directory: tempfile::TempDir,
    store: HomeStore,
    settings: SettingsState,
    owner: SettingsProcessOwner,
    service: SettingsService,
    #[cfg(feature = "test-faults")]
    faults: beryl_home_store::test_faults::FaultController,
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
}

fn prepare_interval(
    service: &SettingsService,
    seconds: &str,
    operation: u64,
) -> PreparedSettingsApply {
    let mut draft = SettingsDraft::new(service.snapshot().unwrap(), 1, operation);
    draft
        .set(SettingsDraftValue::DraftAutosaveInterval(seconds.into()))
        .unwrap();
    service
        .prepare_apply(&draft, SettingsApplyAction::Apply, operation)
        .unwrap()
}

fn commit_interval(service: &SettingsService, seconds: &str, operation: u64) {
    let prepared = prepare_interval(service, seconds, operation);
    assert!(matches!(
        service
            .execute(prepared, &CommandCancellation::new())
            .unwrap(),
        SettingsApplyObservation::Committed { .. }
    ));
}

#[gpui::test]
fn exact_applied_source_replacement_keeps_a_process_monotonic_token(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let old = fixture.service.applied_reader();
    assert!(
        cx.update(|app| settings_autosave::publish_applied(&old, app))
            .unwrap()
    );
    let initial = cx.read(settings_autosave::current).unwrap();
    assert_eq!(initial.interval().unwrap().seconds(), 30);
    assert!(
        !cx.update(|app| settings_autosave::publish_applied(&old, app))
            .unwrap()
    );
    commit_interval(&fixture.service, "45", 1);
    assert!(
        cx.read(settings_autosave::current)
            .unwrap()
            .interval()
            .is_none(),
        "an unprojected publication cannot authorize a timer with the previous interval"
    );
    assert!(
        cx.update(|app| settings_autosave::publish_applied(&old, app))
            .unwrap()
    );
    let changed = cx.read(settings_autosave::current).unwrap();
    assert!(changed.publication() > initial.publication());
    assert_eq!(changed.interval().unwrap().seconds(), 45);
    let replacement = SettingsService::prepare(
        fixture.store.service_reference(),
        fixture.settings.clone(),
        fixture.owner.clone(),
        2,
    )
    .unwrap();
    let fresh = replacement.applied_reader();
    assert!(
        cx.update(|app| settings_autosave::publish_applied(&fresh, app))
            .unwrap()
    );
    let replaced = cx.read(settings_autosave::current).unwrap();
    assert!(replaced.publication() > changed.publication());
    assert_eq!(
        replaced.source(),
        replacement.snapshot().unwrap().identity()
    );
    assert!(
        cx.update(|app| settings_autosave::publish_applied(&old, app))
            .is_err()
    );
    let foreign = Fixture::new();
    assert!(
        cx.update(|app| settings_autosave::publish_applied(&foreign.service.applied_reader(), app))
            .is_err()
    );
    assert_eq!(cx.read(settings_autosave::current), Some(replaced));
}

#[cfg(feature = "test-faults")]
#[gpui::test]
fn pending_keeps_the_proven_interval_and_scoped_uncertainty_excludes_only_timer_admission(
    cx: &mut TestAppContext,
) {
    use beryl_home_store::test_faults::FaultPoint;
    let fixture = Fixture::new();
    let reader = fixture.service.applied_reader();
    cx.update(|app| settings_autosave::publish_applied(&reader, app))
        .unwrap();
    let before = cx.read(settings_autosave::current).unwrap();
    let prepared = prepare_interval(&fixture.service, "50", 1);
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
    assert!(
        !cx.update(|app| settings_autosave::publish_applied(&reader, app))
            .unwrap()
    );
    assert_eq!(cx.read(settings_autosave::current), Some(before));
    fixture
        .faults
        .fail_next(FaultPoint::BeforeReconciliationSnapshot);
    assert!(matches!(
        fixture.service.reconcile_original(1).unwrap(),
        SettingsApplyObservation::ScopedUnresolved { .. }
    ));
    assert!(
        cx.update(|app| settings_autosave::publish_applied(&reader, app))
            .unwrap()
    );
    let excluded = cx.read(settings_autosave::current).unwrap();
    assert!(excluded.publication() > before.publication());
    assert!(excluded.interval().is_none());
    let applied = reader.snapshot().unwrap();
    assert!(applied.draft_autosave_interval().is_err());
    assert!(applied.compaction_timeout().is_ok());
    let replacement = SettingsService::prepare(
        fixture.store.service_reference(),
        fixture.settings.clone(),
        fixture.owner.clone(),
        2,
    )
    .unwrap();
    cx.update(|app| settings_autosave::publish_applied(&replacement.applied_reader(), app))
        .unwrap();
    let reopened = cx.read(settings_autosave::current).unwrap();
    assert!(reopened.publication() > excluded.publication());
    assert!(
        reopened.interval().is_none(),
        "same-process reopen must not recover excluded raw bytes"
    );
}

#[cfg(feature = "test-faults")]
#[gpui::test]
fn same_home_recovery_uses_a_new_exact_source_without_resetting_the_timer_token(
    cx: &mut TestAppContext,
) {
    use beryl_home_store::test_faults::FaultPoint;
    let fixture = Fixture::new();
    commit_interval(&fixture.service, "55", 1);
    let reader = fixture.service.applied_reader();
    cx.update(|app| settings_autosave::publish_applied(&reader, app))
        .unwrap();
    let before = cx.read(settings_autosave::current).unwrap();
    fixture.service.retire().unwrap();
    assert!(
        cx.read(settings_autosave::current)
            .unwrap()
            .interval()
            .is_none()
    );
    let Fixture {
        _directory,
        store,
        settings,
        owner,
        service,
        faults,
    } = fixture;
    drop((settings, service));
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut candidate = store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let prepared = PreparedSettingsService::prepare_candidate(
        &candidate.recovery_access().unwrap(),
        &state.settings(),
        owner,
        2,
    )
    .unwrap();
    let fresh_reader = prepared.applied_reader();
    assert!(
        cx.update(|app| settings_autosave::publish_applied(&fresh_reader, app))
            .is_err()
    );
    let store = candidate.publish().unwrap();
    let service = prepared
        .release_published(store.service_reference())
        .unwrap();
    cx.update(|app| settings_autosave::publish_applied(&fresh_reader, app))
        .unwrap();
    let recovered = cx.read(settings_autosave::current).unwrap();
    assert!(recovered.publication() > before.publication());
    assert!(recovered.source().home_generation > before.source().home_generation);
    assert_eq!(recovered.interval().unwrap().seconds(), 55);
    assert!(
        cx.update(|app| settings_autosave::publish_applied(&reader, app))
            .is_err()
    );
    assert_eq!(cx.read(settings_autosave::current), Some(recovered));
    service.retire().unwrap();
}

#[cfg(feature = "test-faults")]
#[gpui::test]
fn mounted_timer_checks_applied_identity_and_unavailable_interval_preserves_explicit_flush(
    cx: &mut TestAppContext,
) {
    use beryl_app::main_window::{
        MainWindowConversationComposerAutosavePhase as Phase,
        MainWindowConversationComposerCloseAdvance as CloseAdvance,
    };
    use gpui::EntityInputHandler;
    use std::time::Duration;

    let (fixture, cx) = mounted_support::mounted(cx, "applied-autosave", 125);
    let state = BerylState::reacquire(&fixture.store).unwrap();
    let owner = SettingsProcessOwner::new(fixture.store.home_id());
    let service = SettingsService::prepare(
        fixture.store.service_reference(),
        state.settings(),
        owner.clone(),
        1,
    )
    .unwrap();
    commit_interval(&service, "5", 1);
    let reader = service.applied_reader();
    cx.update(|_, app| settings_autosave::publish_applied(&reader, app))
        .unwrap();
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(
                None,
                "kept through unavailable autosave",
                None,
                window,
                cx,
            );
        })
    });
    mounted_support::drive_until(cx, "applied interval timer", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
            && fixture.mount.read_with(cx, |mount, _| {
                mount.autosave_diagnostics().phase() == Phase::Waiting
            })
    });
    let first_timer = fixture.service.test_with_selected_host(|host| {
        assert_eq!(host.autosave_interval().seconds(), 5);
        host.autosave_timer().unwrap()
    });
    assert_eq!(
        cx.update(|window, app| fixture.mount.update(app, |mount, cx| {
            mount.publish_autosave_interval(
                u64::MAX,
                beryl_app::composer_host::ComposerHostAutosaveInterval::new(5).unwrap(),
                window,
                cx,
            )
        }))
        .unwrap(),
        beryl_app::composer_host::ComposerHostAutosaveSettingsCompletion::Stale,
    );
    assert_eq!(
        fixture
            .service
            .test_with_selected_host(|host| host.autosave_timer()),
        Some(first_timer)
    );
    commit_interval(&service, "10", 2);
    cx.update(|_, app| settings_autosave::publish_applied(&reader, app))
        .unwrap();
    mounted_support::drive(cx, 8);
    fixture.service.test_with_selected_host(|host| {
        let current = host.autosave_timer().unwrap();
        assert!(current.settings_generation() > first_timer.settings_generation());
        assert_eq!(host.autosave_interval().seconds(), 10);
    });
    cx.executor().advance_clock(Duration::from_secs(5));
    mounted_support::drive(cx, 8);
    assert!(
        fixture
            .service
            .test_with_selected_host(|host| host.is_dirty())
    );
    service.retire().unwrap();
    cx.executor().advance_clock(Duration::from_secs(5));
    mounted_support::drive_until(cx, "retired source rejects timer capture", |cx| {
        fixture.mount.read_with(cx, |mount, _| {
            mount.autosave_diagnostics().phase() == Phase::Idle
        })
    });
    fixture.service.test_with_selected_host(|host| {
        assert!(host.is_dirty());
        assert!(
            host.lifecycle_diagnostics()
                .joined_publication_ticket()
                .is_none()
        );
    });
    let close = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    mounted_support::drive_until(
        cx,
        "explicit flush while timer Settings unavailable",
        |cx| {
            cx.update(|window, app| {
                fixture.mount.update(app, |mount, cx| {
                    mount.advance_window_close(close.ticket, window, cx)
                })
            })
            .unwrap()
                == CloseAdvance::Ready
        },
    );
    assert!(
        !fixture
            .service
            .test_with_selected_host(|host| host.is_dirty())
    );
    drop((input, composer, service, reader, owner, state));
    mounted_support::finish(fixture, cx);
}
