use super::*;
use gpui::{EntityInputHandler, Focusable};

pub(super) fn search_state(
    input: &gpui::Entity<gpui_text_input::TextInput>,
    cx: &mut TestAppContext,
) -> (
    String,
    std::ops::Range<usize>,
    usize,
    Option<std::ops::Range<usize>>,
    gpui_text_input::TextInputRetainedCounts,
    gpui::Point<gpui::Pixels>,
) {
    input.read_with(cx, |input, _| {
        (
            input.text().to_owned(),
            input.selection(),
            input.cursor_offset(),
            input.state().marked_range(),
            input.state().retained_counts(),
            input.scroll_offset(),
        )
    })
}

#[gpui::test]
fn fenced_picker_rejects_real_search_edits_and_paste_before_mutation_but_allows_inspection(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let input = cx.update(|app| picker.read(app).search_input());
    window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| input.focus(window, cx));
        })
        .unwrap();
    window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                input.replace_text_in_range(None, "001", window, cx);
            });
        })
        .unwrap();
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_owned()),
        "001"
    );
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("2".into()));
    cx.dispatch_action(window.into(), gpui_text_input::Paste);
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_owned()),
        "0012"
    );
    cx.dispatch_action(window.into(), gpui_text_input::Undo);
    catalog::settled(&picker, 1, cx);
    window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                input.replace_text_in_range(Some(0..1), "1", window, cx);
                assert_eq!(input.text(), "101");
                input.replace_text_in_range(Some(0..1), "0", window, cx);
                input.set_selection(0..1, false, cx);
                input.replace_and_mark_text_in_range(Some(0..1), "0", Some(0..1), window, cx);
                assert!(input.has_marked_text());
                input.unmark_text(window, cx);
                assert!(!input.has_marked_text());
                input.replace_and_mark_text_in_range(Some(0..1), "0", Some(0..1), window, cx);
            });
        })
        .unwrap();
    let before = search_state(&input, cx);
    assert_eq!(before.0, "001");
    assert_eq!(before.1, 0..1);
    assert!(before.3.is_some());
    assert!(before.4.undo_snapshot_count > 0);
    let reason = "New Thread is waiting for this window's original request.";
    picker.update(cx, |picker, cx| {
        assert!(picker.set_external_command_reason(Some(reason.into()), cx));
    });
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    picker.update(cx, |picker, cx| picker.clear_search(cx));
    assert_eq!(search_state(&input, cx), before);
    window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                input.replace_text_in_range(Some(0..1), "7", window, cx);
                input.replace_and_mark_text_in_range(Some(0..1), "8", Some(0..1), window, cx);
                input.unmark_text(window, cx);
            });
        })
        .unwrap();
    assert_eq!(search_state(&input, cx), before);
    cx.dispatch_action(window.into(), gpui_text_input::Paste);
    cx.dispatch_action(window.into(), gpui_text_input::Cut);
    cx.dispatch_action(window.into(), gpui_text_input::Undo);
    cx.dispatch_action(window.into(), gpui_text_input::Redo);
    assert_eq!(search_state(&input, cx), before);
    assert_eq!(search_state(&input, cx), before);
    assert_eq!(
        cx.update(|app| picker.read(app).query_text().to_owned()),
        "001"
    );
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_keystrokes("tab");
    window
        .update(cx, |_, window, app| {
            assert!(!input.read(app).focus_handle(app).is_focused(window));
            assert!(picker.read(app).diagnostics().focused_key.is_some());
        })
        .unwrap();
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_keystrokes("shift-tab");
    window
        .update(cx, |_, window, app| {
            assert!(input.read(app).focus_handle(app).is_focused(window));
        })
        .unwrap();
    let command = PickerCommand::AddRuntime;
    let bounds = gpui::VisualTestContext::from_window(window.into(), cx)
        .debug_bounds("thread-root-picker-add-runtime")
        .unwrap();
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_mouse_down(
        bounds.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    window
        .update(cx, |_, window, app| {
            let picker = picker.read(app);
            assert!(
                picker
                    .command_focus_handle(&command)
                    .unwrap()
                    .is_focused(window)
            );
            assert_eq!(
                picker
                    .command_state(&command)
                    .unwrap()
                    .unavailable_reason
                    .as_deref(),
                Some(reason)
            );
        })
        .unwrap();
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_mouse_up(
        bounds.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_keystrokes("tab");
    window
        .update(cx, |_, window, app| {
            assert!(
                !picker
                    .read(app)
                    .command_focus_handle(&command)
                    .unwrap()
                    .is_focused(window)
            );
        })
        .unwrap();
    let search_bounds = gpui::VisualTestContext::from_window(window.into(), cx)
        .debug_bounds("thread-root-picker-search")
        .unwrap();
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_mouse_down(
        search_bounds.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    window
        .update(cx, |_, window, app| {
            assert!(input.read(app).focus_handle(app).is_focused(window));
        })
        .unwrap();
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_mouse_up(
        search_bounds.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    assert_eq!(search_state(&input, cx), before);
    picker.update(cx, |picker, cx| {
        assert!(picker.set_external_command_reason(None, cx));
    });
    assert!(input.read_with(cx, |input, _| input.is_enabled()));
    assert_eq!(search_state(&input, cx), before);
    window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| input.focus(window, cx));
        })
        .unwrap();
    window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                input.replace_text_in_range(None, "7", window, cx);
            });
        })
        .unwrap();
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_owned()),
        "701"
    );
    picker.update(cx, |picker, cx| {
        input.update(cx, |input, cx| input.set_enabled(false, cx));
        assert!(picker.set_external_command_reason(Some(reason.into()), cx));
        assert!(picker.set_external_command_reason(None, cx));
    });
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    dispose_recovered(owner, window, cx);
    drop(directory);
}
