use super::support::*;
use beryl_app::main_window::{
    MainWindowNoticeControlFocus, MainWindowNoticeWidgetEvent, NoticeCommand, NoticeCommandId,
    NoticeContent, NoticeDismissal, NoticeVariant,
};
use gpui::{
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent,
    point, px,
};
use std::time::Duration;

fn command_content(dismissal: NoticeDismissal, id: u64, detail: &str) -> NoticeContent {
    NoticeContent::new(NoticeVariant::Warning, dismissal, "commands", detail)
        .with_commands(&[
            NoticeCommand::enabled(NoticeCommandId::new(id), "Proceed"),
            NoticeCommand::loading(NoticeCommandId::new(id + 1), "Working"),
            NoticeCommand::disabled(
                NoticeCommandId::new(id + 2),
                "Blocked",
                "Backend is unavailable",
            ),
        ])
        .expect("bounded commands")
}

fn complete_keyboard_activation(visual: &mut gpui::VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).expect("activation keystroke");
    visual.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
    });
    visual.simulate_event(KeyUpEvent { keystroke });
}

#[gpui::test]
fn command_and_close_activate_once_for_complete_keyboard_sequences(cx: &mut gpui::TestAppContext) {
    let mounted = Mounted::new(cx);
    let source = NoticeSource::new(
        5,
        command_content(NoticeDismissal::Dismissible, 41, "detail"),
    );
    let expected_token = source.record().token().clone();
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let enabled_selector = mounted.command_selector(0, cx);
    let enabled = visual
        .debug_bounds(enabled_selector)
        .expect("enabled command");
    visual.simulate_click(enabled.center(), Modifiers::none());
    mounted.events.borrow_mut().clear();
    complete_keyboard_activation(&mut visual, "enter");
    let events = mounted.events.borrow();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        MainWindowNoticeWidgetEvent::Command { token, command }
            if token == &expected_token && *command == NoticeCommandId::new(41)
    ));
    drop(events);

    let close = visual.debug_bounds(CLOSE).expect("dismissible close");
    visual.simulate_mouse_down(close.center(), MouseButton::Left, Modifiers::none());
    assert_eq!(
        mounted.diagnostics(cx).focus,
        MainWindowNoticeControlFocus::Close
    );
    mounted.events.borrow_mut().clear();
    complete_keyboard_activation(&mut visual, "space");
    let events = mounted.events.borrow();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        MainWindowNoticeWidgetEvent::Dismiss(token) if token == &expected_token
    ));
}

#[gpui::test]
fn release_after_identity_replacement_cannot_activate_close_or_same_index_command(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let first = NoticeSource::new(
        11,
        command_content(NoticeDismissal::Dismissible, 61, "detail"),
    );
    mounted.replace(Some(first.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let close = visual.debug_bounds(CLOSE).expect("first close");
    visual.simulate_mouse_down(close.center(), MouseButton::Left, Modifiers::none());
    let second = NoticeSource::new(
        12,
        command_content(NoticeDismissal::Dismissible, 71, "detail"),
    );
    mounted.replace(Some(second.record()), cx);
    visual.simulate_mouse_up(close.center(), MouseButton::Left, Modifiers::none());
    assert!(
        mounted.events.borrow().is_empty(),
        "stale close press must not dismiss replacement"
    );

    let command_selector = mounted.command_selector(0, cx);
    let command = visual
        .debug_bounds(command_selector)
        .expect("replacement command");
    visual.simulate_mouse_down(command.center(), MouseButton::Left, Modifiers::none());
    let third = NoticeSource::new(
        13,
        command_content(NoticeDismissal::Dismissible, 81, "detail"),
    );
    mounted.replace(Some(third.record()), cx);
    visual.simulate_mouse_up(command.center(), MouseButton::Left, Modifiers::none());
    assert!(
        mounted.events.borrow().is_empty(),
        "stale same-index command press must not invoke the replacement command"
    );
}

#[gpui::test]
fn key_release_after_identity_replacement_cannot_activate_a_retained_command_focus(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let first = NoticeSource::new(
        14,
        command_content(NoticeDismissal::Dismissible, 101, "first detail"),
    );
    mounted.replace(Some(first.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let command_selector = mounted.command_selector(0, cx);
    let command = visual
        .debug_bounds(command_selector)
        .expect("enabled command");
    visual.simulate_click(command.center(), Modifiers::none());
    mounted.events.borrow_mut().clear();
    assert_eq!(
        mounted.diagnostics(cx).focus,
        MainWindowNoticeControlFocus::Command
    );
    let keystroke = Keystroke::parse("enter").expect("Enter keystroke");
    visual.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
    });
    let replacement = NoticeSource::new(
        15,
        command_content(NoticeDismissal::Dismissible, 101, "replacement detail"),
    );
    mounted.replace(Some(replacement.record()), cx);
    visual.simulate_event(KeyUpEvent { keystroke });
    assert!(
        mounted.events.borrow().is_empty(),
        "a key release that began on the replaced identity must not invoke its retained focus control"
    );
}

#[gpui::test]
fn close_and_disabled_command_expose_their_required_tooltips(cx: &mut gpui::TestAppContext) {
    let mounted = Mounted::new(cx);
    let source = NoticeSource::new(
        16,
        command_content(NoticeDismissal::Dismissible, 111, "tooltip detail"),
    );
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let close = visual.debug_bounds(CLOSE).expect("dismissible close");
    visual.simulate_mouse_move(close.center(), None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(501));
    mounted.draw(cx);
    assert!(
        visual
            .debug_bounds("main-window-notice-close-tooltip")
            .is_some(),
        "close exposes the ordinary Dismiss notice tooltip"
    );
    let disabled_selector = mounted.command_selector(2, cx);
    let disabled = visual
        .debug_bounds(disabled_selector)
        .expect("disabled command");
    visual.simulate_mouse_move(disabled.center(), None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(501));
    mounted.draw(cx);
    assert!(
        visual
            .debug_bounds("main-window-notice-command-disabled-tooltip")
            .is_some(),
        "disabled command explains why it cannot run"
    );
}

#[gpui::test]
fn inert_close_and_disabled_controls_do_not_create_tooltips(cx: &mut gpui::TestAppContext) {
    let mounted = Mounted::new(cx);
    let source = NoticeSource::new(
        17,
        command_content(NoticeDismissal::Dismissible, 121, "inert tooltip detail"),
    );
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let close = visual.debug_bounds(CLOSE).expect("dismissible close");
    let disabled = visual
        .debug_bounds(mounted.command_selector(2, cx))
        .expect("disabled command");

    mounted.inert(true, cx);
    visual.simulate_mouse_move(close.center(), None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(501));
    mounted.draw(cx);
    assert!(
        visual
            .debug_bounds("main-window-notice-close-tooltip")
            .is_none(),
        "inert close control cannot create a tooltip"
    );
    visual.simulate_mouse_move(disabled.center(), None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(501));
    mounted.draw(cx);
    assert!(
        visual
            .debug_bounds("main-window-notice-command-disabled-tooltip")
            .is_none(),
        "inert disabled command cannot create a tooltip"
    );
}

#[gpui::test]
fn persistent_and_inert_notice_reject_each_rendered_input_path_and_hold_safe_focus(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let detail_text = (0..160)
        .map(|line| format!("long inert detail {line}\n"))
        .collect::<String>();
    let source = NoticeSource::new(
        6,
        command_content(NoticeDismissal::Persistent, 91, &detail_text),
    );
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    assert!(visual.debug_bounds(CLOSE).is_none());
    visual.simulate_keystrokes("escape");
    assert!(mounted.events.borrow().is_empty());

    let detail = visual.debug_bounds(DETAIL).expect("detail");
    visual.simulate_mouse_move(detail.center(), None, Modifiers::none());
    visual.simulate_event(ScrollWheelEvent {
        position: detail.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-48.))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
    assert!(mounted.diagnostics(cx).scroll_offset > 0);
    visual.simulate_click(detail.center(), Modifiers::none());
    assert_eq!(
        mounted.diagnostics(cx).focus,
        MainWindowNoticeControlFocus::Detail
    );
    let scrollbar_thumb = point(
        detail.right() - px(3.),
        detail.top()
            + gpui_scrollbar::ScrollbarStyle::default()
                .geometry
                .track_inset
            + px(2.),
    );
    visual.simulate_mouse_down(scrollbar_thumb, MouseButton::Left, Modifiers::none());
    let scroll_before_inert = mounted.diagnostics(cx).scroll_offset;
    mounted.inert(true, cx);
    assert_eq!(
        mounted.diagnostics(cx).focus,
        MainWindowNoticeControlFocus::SafeTarget
    );
    visual.simulate_mouse_move(
        point(scrollbar_thumb.x, detail.bottom() - px(2.)),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    visual.simulate_mouse_up(scrollbar_thumb, MouseButton::Left, Modifiers::none());
    let command_selector = mounted.command_selector(0, cx);
    let command = visual.debug_bounds(command_selector).expect("command");
    visual.simulate_click(detail.center(), Modifiers::none());
    assert_eq!(
        mounted.diagnostics(cx).focus,
        MainWindowNoticeControlFocus::SafeTarget
    );
    visual.simulate_click(command.center(), Modifiers::none());
    assert_eq!(
        mounted.diagnostics(cx).focus,
        MainWindowNoticeControlFocus::SafeTarget
    );
    visual.simulate_keystrokes("tab secondary-a secondary-c enter space escape shift-down");
    visual.simulate_event(ScrollWheelEvent {
        position: detail.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-48.))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
    mounted.draw(cx);
    assert!(mounted.events.borrow().is_empty());
    assert_eq!(
        mounted.diagnostics(cx).focus,
        MainWindowNoticeControlFocus::SafeTarget
    );
    assert_eq!(mounted.diagnostics(cx).scroll_offset, scroll_before_inert);
    assert!(!mounted.diagnostics(cx).selection_present);
}
