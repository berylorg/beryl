use super::{Fixture, config};
use beryl_app::composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits};
use beryl_app::main_window::{
    MainWindowComposerSubmissionRequestSource, MainWindowInitialComposer, MainWindowShellPrepared,
};
use beryl_app::theme_runtime::{AppearanceCoordinator, AppearanceCoordinatorConfig};
use std::num::NonZeroUsize;

pub fn prepared_shell(
    fixture: &Fixture,
    custody: MainWindowInitialComposer,
) -> MainWindowShellPrepared {
    let prepared = custody
        .prepare(&mut config)
        .unwrap_or_else(|failure| panic!("{}", failure.error));
    let appearance = AppearanceCoordinator::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
        beryl_state::PreparedThemeAppearance::fallback(
            fixture
                .state
                .themes()
                .settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None),
        ),
    )
    .current();
    let seals = DraftMarkerSealService::test_new(
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
    let submission = MainWindowComposerSubmissionRequestSource::new(
        beryl_app::cas_projection::SubmissionExecutionWake::storage_only_for_test(),
        beryl_app::cas_projection::ProjectionServiceConfig::try_new(
            1,
            4,
            beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap()
        .turn_start_admission_requirement(),
    );
    prepared
        .into_shell(Box::new(config), seals, submission, appearance)
        .unwrap_or_else(|failure| panic!("{}", failure.error))
}
