use super::*;
use beryl_app::main_window::MainWindowConversationComposer;

pub(super) fn current(
    fixture: &MountedSubmissionFixture,
    cx: &gpui::VisualTestContext,
) -> Entity<MainWindowConversationComposer> {
    fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap()
}

pub(super) fn settled(
    composer: &Entity<MainWindowConversationComposer>,
    cx: &mut gpui::VisualTestContext,
) {
    drive_until(cx, 2048, "coherent composer history", |cx| {
        composer.read_with(cx, |composer, app| {
            !composer.test_accepted_input_history_pending()
                && composer.test_edit_position_proof_ready(app)
        })
    });
}

pub(super) fn submit_first(fixture: &MountedSubmissionFixture, cx: &mut gpui::VisualTestContext) {
    prepare_flushed_submission(cx, &fixture.mount, &fixture.service, "first");
    cx.simulate_keystrokes("enter");
    drive_until(cx, 2048, "accepted input and mounted successor", |cx| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|selection| selection.binding().presentation_generation().get() == 2)
            && fixture
                .mount
                .read_with(cx, |mount, _| mount.submission_status())
                == MainWindowConversationComposerSubmissionStatus::Idle
    });
    let composer = current(fixture, cx);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    settled(&composer, cx);
}

#[gpui::test]
fn mounted_history_keys_recall_during_execution_and_restore_unsent_directed_selection(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "mounted-ordinary-recall", 141);
    submit_first(&fixture, cx);
    let composer = current(&fixture, cx);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_text_in_range(None, "unsent second", window, cx)
        })
    });
    settled(&composer, cx);
    cx.simulate_keystrokes("end shift-left shift-left shift-left");
    settled(&composer, cx);
    let original = fixture.service.selected_identity().unwrap();
    let positions = input.read_with(cx, |input, _| {
        let s = input.surface().unwrap();
        (s.caret(), s.selection())
    });
    let owner = fixture.service.accepted_input_history();
    assert_eq!(
        owner.entry_count(original.binding().home_id(), original.claim().thread_id()),
        1
    );
    cx.simulate_keystrokes("alt-up");
    settled(&composer, cx);
    let recalled = fixture.service.selected_identity().unwrap();
    assert_eq!(recalled.binding().logical_extent().logical_utf8_bytes(), 5);
    assert_eq!(
        input.read_with(cx, |input, _| input
            .surface()
            .unwrap()
            .caret()
            .byte_offset
            .get()),
        5
    );
    assert!(input.read_with(cx, |input, _| {
        let selection = input.surface().unwrap().selection();
        selection.anchor == selection.head
    }));
    assert!(recalled.binding().range_history_frontier().undo_available);
    cx.simulate_keystrokes("alt-up");
    drive(cx, 8);
    assert_eq!(fixture.service.selected_identity(), Some(recalled));
    cx.simulate_keystrokes("alt-down");
    settled(&composer, cx);
    let restored = fixture.service.selected_identity().unwrap();
    assert_eq!(restored.binding().root(), original.binding().root());
    assert_eq!(
        input.read_with(cx, |input, _| {
            let s = input.surface().unwrap();
            (s.caret(), s.selection())
        }),
        positions
    );
    cx.simulate_keystrokes("alt-down");
    drive(cx, 8);
    assert_eq!(fixture.service.selected_identity(), Some(restored));
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        None
    );
}

#[gpui::test]
fn mounted_edits_and_undo_keep_the_browse_checkpoint_until_forward_restore(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "mounted-browse-bookmark", 171);
    submit_first(&fixture, cx);
    let composer = current(&fixture, cx);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_text_in_range(None, "save me", window, cx)
        })
    });
    settled(&composer, cx);
    let original = fixture.service.selected_identity().unwrap();
    cx.simulate_keystrokes("alt-up");
    settled(&composer, cx);
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_text_in_range(None, " edited", window, cx)
        })
    });
    settled(&composer, cx);
    assert_eq!(
        fixture
            .service
            .selected_identity()
            .unwrap()
            .binding()
            .logical_extent()
            .logical_utf8_bytes(),
        12
    );
    cx.simulate_keystrokes("ctrl-z");
    settled(&composer, cx);
    assert_eq!(
        fixture
            .service
            .selected_identity()
            .unwrap()
            .binding()
            .logical_extent()
            .logical_utf8_bytes(),
        5
    );
    cx.simulate_keystrokes("alt-down");
    settled(&composer, cx);
    assert_eq!(
        fixture
            .service
            .selected_identity()
            .unwrap()
            .binding()
            .root(),
        original.binding().root()
    );
    cx.simulate_keystrokes("ctrl-z");
    settled(&composer, cx);
    assert_eq!(
        fixture
            .service
            .selected_identity()
            .unwrap()
            .binding()
            .logical_extent()
            .logical_utf8_bytes(),
        5
    );
}

#[gpui::test]
fn empty_history_direction_keys_leave_the_editor_and_positions_unchanged(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "mounted-empty-history", 201);
    let composer = current(&fixture, cx);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    settled(&composer, cx);
    let original = fixture.service.selected_identity();
    let positions = input.read_with(cx, |input, _| {
        let s = input.surface().unwrap();
        (s.caret(), s.selection())
    });
    cx.simulate_keystrokes("alt-up alt-down");
    drive(cx, 16);
    assert_eq!(fixture.service.selected_identity(), original);
    assert_eq!(
        input.read_with(cx, |input, _| {
            let s = input.surface().unwrap();
            (s.caret(), s.selection())
        }),
        positions
    );
    assert!(!composer.read_with(cx, |composer, _| {
        composer.test_accepted_input_history_pending()
    }));
}
