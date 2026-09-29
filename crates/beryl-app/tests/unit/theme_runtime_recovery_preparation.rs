use super::*;

pub(super) fn recovered() -> (
    tempfile::TempDir,
    HomeRecoveryCandidate,
    BerylState,
    BerylState,
    FaultController,
) {
    let (directory, candidate, stale, faults) = candidate();
    let store = candidate.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let candidate = store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&candidate).unwrap();
    (directory, candidate, stale, fresh, faults)
}

#[test]
fn recovery_watcher_waits_for_publication_and_release_then_joins() {
    let (_directory, mut candidate, _, state, faults) = recovered();
    let observer = state.themes();
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let prepared = PreparedThemeRuntime::prepare_recovery(
        &mut candidate,
        state.themes(),
        &state.settings(),
        config(),
    )
    .unwrap();
    assert_eq!(observer.diagnostics().active_subscriptions(), 1);
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    let store = candidate.publish().unwrap();
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    let mut runtime = prepared.release_published(&store).unwrap();
    assert!(observation.wait_until_reached(Duration::from_secs(3)));
    observation.release();
    assert!(runtime.current().is_some());
    runtime.retire();
    assert_eq!(observer.diagnostics().active_subscriptions(), 0);
    store.close().unwrap();
}

#[test]
fn recovery_rejects_stale_foreign_services_and_returns_failed_spawn_capacity() {
    let (_directory, mut candidate, stale, state, faults) = recovered();
    let (_foreign_directory, foreign, _, foreign_state, _) = recovered();
    for service in [stale.themes(), foreign_state.themes()] {
        let error = PreparedThemeRuntime::prepare_recovery(
            &mut candidate,
            service.clone(),
            &state.settings(),
            config(),
        )
        .err()
        .unwrap();
        assert_eq!(error.class(), ThemeRuntimeFailureClass::Subscription);
        assert_eq!(service.diagnostics().active_subscriptions(), 0);
    }
    faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
    assert!(
        PreparedThemeRuntime::prepare_recovery(
            &mut candidate,
            state.themes(),
            &state.settings(),
            config()
        )
        .is_err()
    );
    assert_eq!(state.themes().diagnostics().active_subscriptions(), 0);
    for _ in 0..2 {
        drop(
            PreparedThemeRuntime::prepare_recovery(
                &mut candidate,
                state.themes(),
                &state.settings(),
                config(),
            )
            .unwrap(),
        );
        assert_eq!(state.themes().diagnostics().active_subscriptions(), 0);
    }
    candidate.abort().close().unwrap();
    foreign.abort().close().unwrap();
}

#[test]
fn recovery_release_before_publication_or_after_failure_joins_before_abort() {
    for fail in [false, true] {
        let (_directory, mut candidate, _, state, faults) = recovered();
        let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
        let prepared = PreparedThemeRuntime::prepare_recovery(
            &mut candidate,
            state.themes(),
            &state.settings(),
            config(),
        )
        .unwrap();
        if fail {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(
                candidate
                    .recovery_access()
                    .unwrap()
                    .home_revision()
                    .is_err()
            );
        }
        let reference = candidate.service_reference();
        let error = prepared.release_published(&reference).err().unwrap();
        assert!(matches!(
            error.class(),
            ThemeRuntimeFailureClass::Subscription | ThemeRuntimeFailureClass::Identity
        ));
        assert_eq!(state.themes().diagnostics().active_subscriptions(), 0);
        assert!(!observation.wait_until_reached(Duration::from_millis(60)));
        observation.release();
        candidate.abort().close().unwrap();
    }
}
