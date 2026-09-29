use super::*;

pub(crate) fn shell_for_recovery(
    cx: &mut gpui::TestAppContext,
) -> (
    Fixture,
    MainWindowShell,
    RestoredWindowPreparationAttempt,
    RestoredWindowServiceTestLifetime,
) {
    let (fixture, prepared, attempt, service, appearance) = home_support::join(
        home_support::worker(|| {
            let PreparedFixture {
                prepared,
                attempt,
                _service,
                fixture,
                appearance,
                seals,
                ..
            } = prepared_fixture(141);
            let prepared = RestoredWindowShellPrepared::prepare(
                prepared,
                &attempt,
                &fixture.process,
                Box::new(config),
                seals,
                submission(),
                appearance.clone(),
            )
            .unwrap_or_else(|failure| panic!("{}", failure.error));
            (fixture, prepared, attempt, _service, appearance)
        }),
        cx,
    );
    let placement = placement_support::prepare(prepared.window_id(), prepared.placement());
    let shell = cx.update(|app| {
        let appearance =
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app);
        placement_support::attach(GpuiMainWindowShellHost::new(app, appearance), placement)
            .construct_restored_hidden(prepared)
            .unwrap_or_else(|_| panic!("restored recovery shell"))
    });
    (fixture, shell, attempt, service)
}
