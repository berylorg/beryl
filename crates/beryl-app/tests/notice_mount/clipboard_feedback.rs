use super::*;

#[gpui::test]
fn clipboard_write_failure_is_dismissible_and_repeated_cut_preserves_editor(
    cx: &mut gpui::TestAppContext,
) {
    let (mounted, _service) = composer_feedback::configured_mount(
        cx,
        201,
        syndic_storage::DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
        |_| {},
    );
    let composer = composer_feedback::composer(&mounted, cx);
    composer_feedback::prepare_editor(&mounted, &composer, cx);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    mounted
        .window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                let binding = input.surface().unwrap().binding();
                let start = gpui_text_input::SourcePosition::new(
                    gpui_text_input::ByteOffset::new(0),
                    gpui_text_input::InlineObjectGap::NoObjects,
                );
                let end = gpui_text_input::SourcePosition::new(
                    gpui_text_input::ByteOffset::new(5),
                    gpui_text_input::InlineObjectGap::NoObjects,
                );
                input
                    .rebind(
                        binding,
                        Some(gpui_text_input::RangeSourceSelection {
                            anchor: start,
                            head: end,
                        }),
                        window,
                        cx,
                    )
                    .unwrap();
            });
            composer.update(app, |composer, _| {
                composer.test_set_checked_clipboard_writer(Box::new(|_, _, _| {
                    gpui_text_input::ClipboardWriteOutcome::Failed
                }))
            });
        })
        .unwrap();
    for _ in 0..16 {
        support::draw(cx);
    }
    let before = composer_feedback::editor_state(&composer, cx);
    cx.simulate_keystrokes(mounted.window.into(), "ctrl-x");
    for _ in 0..64 {
        support::draw(cx);
    }
    assert_eq!(composer_feedback::editor_state(&composer, cx), before);
    let feedback = composer.read_with(cx, |composer, _| composer.clipboard_feedback().unwrap());
    assert_eq!(
        feedback.kind,
        MainWindowComposerClipboardFeedbackKind::Failed
    );
    let (first, content) = composer_feedback::notice(&mounted, cx);
    assert_eq!(content.dismissal, NoticeDismissal::Dismissible);
    assert_eq!(content.commands().count(), 0);
    let ingress = support::ingress(mounted.window, cx);
    cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(first.clone()), app))
        .unwrap();
    for _ in 0..4 {
        support::draw(cx);
    }
    assert!(projection(mounted.window, cx).is_none());
    cx.simulate_keystrokes(mounted.window.into(), "ctrl-x");
    for _ in 0..64 {
        support::draw(cx);
    }
    let repeated = composer.read_with(cx, |composer, _| composer.clipboard_feedback().unwrap());
    assert_ne!(feedback.operation, repeated.operation);
    assert_eq!(composer_feedback::editor_state(&composer, cx), before);
    let (second, _) = composer_feedback::notice(&mounted, cx);
    assert!(!first.record().same_identity(second.record()));
    drop(composer);
    drop(input);
    drop(_service);
    support::finish(mounted, cx);
}
