use super::support::{self, drive_until};
use beryl_app::main_window::MainWindowConversationComposerCloseAdvance as Advance;
use gpui::TestAppContext;

#[gpui::test]
fn changed_widget_environment_refuses_recovery_handoff_and_keeps_presentation(
    cx: &mut TestAppContext,
) {
    let (fixture, cx) = support::mounted(cx, "resident-protection-environment", 178);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let close = cx.update(|window, app| {
        fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .unwrap()
    });
    drive_until(cx, "protected close settles", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
            && input.read_with(cx, |input, _| input.is_quiescent())
    });
    composer
        .update(cx, |composer, cx| {
            composer.test_set_shutdown_interaction_gated(true, cx)
        })
        .unwrap();
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| mount
                .fence_interrupted_exit_resident(close.ticket, cx))
            .unwrap()
    );
    let (protection, seed) = composer.read_with(cx, |composer, _| {
        let snapshot = composer.recovery_snapshot().unwrap();
        (snapshot.protection(), *snapshot.restoration())
    });
    input.update(cx, |input, cx| {
        assert!(input.resident_protection_is_current(protection));
        assert!(
            input
                .set_presentation_generation(gpui_text_input::PresentationGeneration::new(99), cx)
                .is_err()
        );
        assert!(!input.resident_protection_is_current(protection));
        input.set_enabled(true, cx);
        assert!(!input.is_enabled());
        assert_eq!(input.export_restoration(seed.history).unwrap(), seed);
    });
    fixture.mount.update(cx, |mount, cx| {
        assert!(
            mount
                .fence_interrupted_exit_resident(close.ticket, cx)
                .is_err()
        );
        assert!(
            mount
                .detach_interrupted_exit_resources(close.ticket, cx)
                .is_err()
        );
        assert!(
            mount
                .take_interrupted_exit_retirement(close.ticket, cx)
                .is_err()
        );
    });
    composer.read_with(cx, |composer, _| {
        assert_eq!(
            composer.recovery_snapshot().unwrap().protection(),
            protection
        );
        assert_eq!(*composer.recovery_snapshot().unwrap().restoration(), seed);
        assert_eq!(composer.gpui_input().entity_id(), input.entity_id());
    });
    drop((composer, input));
    support::finish(fixture, cx);
}
