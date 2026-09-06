use super::*;
use beryl_app::composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits};
use beryl_app::theme_runtime::{
    AppearanceCoordinator, AppearanceCoordinatorConfig, AppearanceGeneration,
};
use gpui::AppContext;
use std::num::NonZeroUsize;

pub fn target(fixture: &Fixture) -> RememberedTarget {
    RememberedTarget::new(
        RuntimeId::from_bytes([fixture.seed; 16]),
        RootId::from_bytes([fixture.seed.wrapping_add(1); 16]),
    )
}

pub fn services(fixture: &Fixture) -> (Arc<MainWindowCreationServices>, Arc<AppearanceGeneration>) {
    let coordinator = AppearanceCoordinator::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(256).unwrap()),
        beryl_state::PreparedThemeAppearance::fallback(
            fixture
                .state
                .themes()
                .settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None),
        ),
    );
    let execution = fixture.execution();
    let request_source = Arc::new(move |window: WindowId, target| {
        let mut draft = *window.as_bytes();
        draft[0] ^= 0x80;
        RuntimeBackedWindowAcquisitionRequest::new(
            window,
            target,
            placement(),
            SyndicThreadId::from_bytes(*window.as_bytes()),
            SyndicDraftId::from_bytes(draft),
            execution.clone(),
            SyndicTimestamp::from_unix_millis(1000),
            DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
        )
        .map_err(|error| format!("{error:?}"))
    });
    let seals = DraftMarkerSealService::new(
        &fixture.store,
        fixture.store.health().generation().unwrap(),
        fixture.storage.clone(),
        fixture.state.assets(),
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let services = MainWindowCreationServices {
        acquisition: fixture.service.clone(),
        store: fixture.store.clone(),
        state: fixture.state.clone(),
        storage: fixture.storage.clone(),
        request_source,
        activation_source: Arc::new(|acquisition| {
            let seed = acquisition.window_id().as_bytes()[0];
            Ok((
                composer_support::activation(
                    acquisition.thread_id(),
                    seed,
                    seed.wrapping_add(1),
                    1,
                    0,
                ),
                composer_support::fixture::operation_id(seed.wrapping_add(2)),
            ))
        }),
        configurator_source: Arc::new(|| Box::new(config)),
        marker_seals: seals,
        turn_start_requirement: beryl_app::cas_projection::ProjectionServiceConfig::try_new(
            1,
            4,
            beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap()
        .turn_start_admission_requirement(),
        test_before_initial_advance: None,
    };
    (Arc::new(services), coordinator.current())
}

pub fn settle(mut work: MainWindowCreation, appearance: Arc<AppearanceGeneration>) {
    for _ in 0..16 {
        match work.advance(appearance.clone()) {
            MainWindowCreationOutcome::Pending(next) => work = next,
            MainWindowCreationOutcome::Settled { .. } => return,
            MainWindowCreationOutcome::Prepared { .. } => panic!("expected exact abandonment"),
        }
    }
    panic!("creation custody did not settle within the test bound");
}

pub fn cleanup(fixture: Fixture) {
    let Fixture {
        directory,
        store,
        state,
        storage,
        faults,
        process,
        service,
        ..
    } = fixture;
    drop((service, state, storage, faults, process, store));
    directory.close().unwrap();
}

pub fn draw(cx: &mut gpui::TestAppContext) {
    cx.run_until_parked();
    let windows = cx.windows();
    for window in windows {
        cx.update(|app| {
            let _ = app.update_window(window, |_, window, app| window.draw(app).clear());
        });
    }
    cx.run_until_parked();
}

pub fn drive_until(
    cx: &mut gpui::TestAppContext,
    mut ready: impl FnMut(&mut gpui::TestAppContext) -> bool,
) {
    for _ in 0..512 {
        draw(cx);
        if ready(cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("creation did not reach its bounded test milestone");
}
