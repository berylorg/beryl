use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::BerylState;

use super::*;

#[path = "theme_runtime_recovery_preparation.rs"]
mod recovery;

fn candidate() -> (
    tempfile::TempDir,
    HomeOpenPublication,
    BerylState,
    FaultController,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap();
    (directory, candidate, state, faults)
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

#[test]
fn preparation_stays_dormant_until_published_loading_and_retirement_joins() {
    let (_directory, mut candidate, state, faults) = candidate();
    let observer = state.themes();
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let prepared = PreparedThemeRuntime::prepare(&mut candidate, state.themes(), config()).unwrap();
    assert_eq!(observer.diagnostics().active_subscriptions(), 1);
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    let store = candidate.publish().unwrap();
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    let revision = state.settings().revision(&store).unwrap();
    let mut runtime = prepared.load_published(&store, revision, None).unwrap();
    assert!(observation.wait_until_reached(Duration::from_secs(3)));
    observation.release();
    assert!(runtime.current().is_some());
    assert_eq!(runtime.diagnostics().last_failure(), None);
    assert!(matches!(
        runtime.current().unwrap().prepared().source(),
        beryl_state::ThemeAppearanceSource::BuiltinFallback(_)
    ));
    runtime.retire();
    assert!(runtime.current().is_none());
    assert_eq!(observer.diagnostics().active_subscriptions(), 0);
    store.close().unwrap();
}

#[test]
fn spawn_failure_and_abandonment_return_candidate_watcher_capacity() {
    let (_directory, mut candidate, state, faults) = candidate();
    let observer = state.themes();
    faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
    let error = PreparedThemeRuntime::prepare(&mut candidate, state.themes(), config())
        .err()
        .unwrap();
    assert_eq!(error.class(), ThemeRuntimeFailureClass::Subscription);
    assert_eq!(observer.diagnostics().active_subscriptions(), 0);
    for _ in 0..2 {
        drop(PreparedThemeRuntime::prepare(&mut candidate, state.themes(), config()).unwrap());
        assert_eq!(observer.diagnostics().active_subscriptions(), 0);
    }
    candidate.close().unwrap();
}

#[test]
fn unpublished_home_cannot_load_and_failed_release_joins() {
    let (_directory, mut candidate, state, _) = candidate();
    let reference = candidate.service_reference();
    let observer = state.themes();
    let prepared = PreparedThemeRuntime::prepare(&mut candidate, state.themes(), config()).unwrap();
    let error = prepared
        .load_published(&reference, DomainRevision::new(1).unwrap(), None)
        .err()
        .unwrap();
    assert_eq!(error.class(), ThemeRuntimeFailureClass::Subscription);
    assert_eq!(observer.diagnostics().active_subscriptions(), 0);
    candidate.close().unwrap();
}

#[test]
fn foreign_home_is_rejected_before_worker_release_or_loading() {
    let (_directory, mut original, state, faults) = candidate();
    let (_other_directory, other, _, _) = candidate();
    let other = other.publish().unwrap();
    let observer = state.themes();
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let prepared = PreparedThemeRuntime::prepare(&mut original, state.themes(), config()).unwrap();
    let original = original.publish().unwrap();
    let error = prepared
        .load_published(&other, DomainRevision::new(1).unwrap(), None)
        .err()
        .unwrap();
    assert_eq!(error.class(), ThemeRuntimeFailureClass::Identity);
    assert_eq!(observer.diagnostics().active_subscriptions(), 0);
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    observation.release();
    original.close().unwrap();
    other.close().unwrap();
}

#[test]
fn invalid_watcher_bounds_fail_before_allocating_activity() {
    let (_directory, mut candidate, state, _) = candidate();
    let observer = state.themes();
    let mut config = config();
    config.watch_interval = Duration::ZERO;
    let error = PreparedThemeRuntime::prepare(&mut candidate, state.themes(), config)
        .err()
        .unwrap();
    assert_eq!(error.class(), ThemeRuntimeFailureClass::Subscription);
    assert_eq!(observer.diagnostics().active_subscriptions(), 0);
    candidate.close().unwrap();
}

#[test]
fn recovered_generation_cannot_release_old_runtime() {
    let (_directory, mut candidate, state, faults) = candidate();
    let observer = state.themes();
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let prepared = PreparedThemeRuntime::prepare(&mut candidate, state.themes(), config()).unwrap();
    let store = candidate.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let recovered = store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovered).unwrap();
    let store = recovered.publish().unwrap();
    let revision = fresh.settings().revision(&store).unwrap();
    let error = prepared
        .load_published(&store, revision, None)
        .err()
        .unwrap();
    assert_eq!(error.class(), ThemeRuntimeFailureClass::Identity);
    assert_eq!(observer.diagnostics().active_subscriptions(), 0);
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    observation.release();
    store.close().unwrap();
}

#[test]
fn published_loading_preserves_missing_document_fallback_provenance() {
    use beryl_home_store::{CommandOutcome, HomeCommand};
    use beryl_state::{
        ApplySettings, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
    };

    let (_directory, mut candidate, state, _) = candidate();
    let prepared = PreparedThemeRuntime::prepare(&mut candidate, state.themes(), config()).unwrap();
    let store = candidate.publish().unwrap();
    let revision = state.settings().revision(&store).unwrap();
    let contribution = state.settings().apply(
        revision,
        ApplySettings::new(vec![SettingUpdate::new(
            SettingKey::ActiveThemeId,
            ExpectedSettingRevision::Absent,
            SettingValue::active_theme_id("missing").unwrap(),
        )])
        .unwrap(),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    let revision = state.settings().revision(&store).unwrap();
    let record = state
        .settings()
        .setting(&store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    let mut runtime = prepared
        .load_published(&store, revision, Some(&record))
        .unwrap();
    assert_eq!(
        runtime.diagnostics().last_failure(),
        Some(ThemeRuntimeFailureClass::DocumentMissing)
    );
    assert!(matches!(
        runtime.current().unwrap().prepared().source(),
        beryl_state::ThemeAppearanceSource::BuiltinFallback(_)
    ));
    runtime.retire();
    assert_eq!(state.themes().diagnostics().active_subscriptions(), 0);
    store.close().unwrap();
}
