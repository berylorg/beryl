use super::*;

pub(crate) fn shell_for_recovery(
    cx: &mut gpui::TestAppContext,
) -> (
    Fixture,
    MainWindowShell,
    RestoredWindowPreparationAttempt,
    RestoredWindowServiceTestLifetime,
) {
    shell_with_text(cx, SAVED_TEXT)
}

pub(crate) fn shell_for_empty_recovery(
    cx: &mut gpui::TestAppContext,
) -> (
    Fixture,
    MainWindowShell,
    RestoredWindowPreparationAttempt,
    RestoredWindowServiceTestLifetime,
) {
    shell_with_text(cx, "")
}

fn shell_with_text(
    cx: &mut gpui::TestAppContext,
    text: &'static str,
) -> (
    Fixture,
    MainWindowShell,
    RestoredWindowPreparationAttempt,
    RestoredWindowServiceTestLifetime,
) {
    let (fixture, prepared, attempt, service, appearance) = home_support::join(
        home_support::worker(move || {
            let PreparedFixture {
                prepared,
                attempt,
                _service,
                fixture,
                appearance,
                seals,
                ..
            } = prepared_fixture_with_text(141, None, text);
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
