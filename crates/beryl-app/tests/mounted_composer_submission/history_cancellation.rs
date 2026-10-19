use super::accepted_input_history::{current, settled, submit_first};
use super::*;

#[gpui::test]
fn escape_cancels_pending_history_and_repeated_keys_do_not_start_another_flight(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "mounted-recall-cancel", 111);
    submit_first(&fixture, cx);
    let composer = current(&fixture, cx);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_text_in_range(None, "keep unsent", window, cx)
        })
    });
    settled(&composer, cx);
    let original = fixture.service.selected_identity().unwrap();
    let gate = fixture.service.test_block_next_accepted_input_recall();
    cx.simulate_keystrokes("alt-up");
    drive_until(cx, 256, "held recall flight", |_| gate.is_blocked());
    cx.simulate_keystrokes("alt-up alt-down enter");
    drive(cx, 16);
    assert_eq!(fixture.service.selected_identity(), Some(original));
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.submission_status()),
        MainWindowConversationComposerSubmissionStatus::Idle
    );
    cx.simulate_keystrokes("escape");
    gate.release();
    settled(&composer, cx);
    assert_eq!(fixture.service.selected_identity(), Some(original));
    cx.simulate_keystrokes("alt-down");
    drive(cx, 8);
    assert_eq!(fixture.service.selected_identity(), Some(original));
}
