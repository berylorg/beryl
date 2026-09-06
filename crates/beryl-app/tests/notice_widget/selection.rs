use super::support::*;
use beryl_app::main_window::{
    MainWindowNoticeControlFocus, MainWindowNoticeOverlayAllocation, NoticeContent,
    NoticeDismissal, NoticeVariant,
};
use gpui::{ClipboardItem, Modifiers, MouseButton, point, px, rgba};

#[gpui::test]
fn pointer_drag_copies_exact_utf8_bytes_and_paints_one_selection_quad_per_wrapped_row(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let detail = "Aé漢字 wrapped detail travels across enough words to wrap into a later visual line, with a final short row.";
    let source = NoticeSource::new(
        1,
        NoticeContent::new(
            NoticeVariant::Warning,
            NoticeDismissal::Dismissible,
            "UTF8",
            detail,
        ),
    );
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let bounds = visual.debug_bounds(DETAIL).expect("detail viewport");
    let start = point(bounds.left() + px(12.), bounds.top() + px(4.));
    let end = point(bounds.right() - px(12.), bounds.bottom() - px(4.));
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    mounted.draw(cx);
    assert!(mounted.diagnostics(cx).selection_present);
    visual.simulate_keystrokes("secondary-c");
    assert_eq!(
        visual
            .read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(detail),
        "the pointer range must retain every UTF-8 byte boundary"
    );
    let selection_color = rgba(0x38bdf84d).into();
    let selection_rows = mounted
        .paint(cx)
        .backgrounds
        .iter()
        .filter(|(_, background)| *background == selection_color)
        .map(|(bounds, _)| bounds.origin.y)
        .collect::<Vec<_>>();
    assert!(
        selection_rows.len() >= 2,
        "wrapped selection must paint multiple visual rows"
    );
    assert!(
        selection_rows.windows(2).any(|pair| pair[0] != pair[1]),
        "selection quads must split at wrapped-row boundaries"
    );
    assert_eq!(
        mounted.diagnostics(cx).focus,
        MainWindowNoticeControlFocus::Detail
    );
}

#[gpui::test]
fn shift_down_and_shift_end_copy_exact_utf8_ranges(cx: &mut gpui::TestAppContext) {
    let mounted = Mounted::new(cx);
    let detail = "Aé\n漢字\nlast";
    let source = NoticeSource::new(
        2,
        NoticeContent::new(
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "keys",
            detail,
        ),
    );
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let bounds = visual.debug_bounds(DETAIL).expect("detail viewport");
    visual.simulate_click(
        point(bounds.left() + px(12.), bounds.top() + px(4.)),
        Modifiers::none(),
    );
    visual.simulate_keystrokes("home shift-end secondary-c");
    assert_eq!(
        visual
            .read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("Aé"),
        "Shift+End must stop before the newline and retain the whole UTF-8 grapheme"
    );
    visual.write_to_clipboard(ClipboardItem::new_string("unchanged".to_owned()));
    visual.simulate_keystrokes("home shift-down secondary-c");
    assert_eq!(
        visual
            .read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("Aé\n"),
        "Shift+Down must select the intervening newline without splitting UTF-8"
    );
}

#[gpui::test]
fn home_and_end_select_the_entire_physical_row_from_mid_row_positions(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let detail = "first UTF8 é row\nsecond 漢字 row\nthird row";
    let source = NoticeSource::new(
        3,
        NoticeContent::new(
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "row edges",
            detail,
        ),
    );
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let bounds = visual.debug_bounds(DETAIL).expect("detail viewport");
    let middle_of_first = point(bounds.left() + px(60.), bounds.top() + px(4.));
    visual.simulate_click(middle_of_first, Modifiers::none());
    visual.simulate_keystrokes("home shift-end secondary-c");
    assert_eq!(
        visual
            .read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("first UTF8 é row"),
        "Home and Shift+End select the full first physical row from its middle"
    );
    let middle_of_second = point(bounds.left() + px(60.), bounds.top() + px(44.));
    visual.simulate_click(middle_of_second, Modifiers::none());
    visual.simulate_keystrokes("home shift-end secondary-c");
    assert_eq!(
        visual
            .read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("second 漢字 row"),
        "Home and Shift+End select the full second physical row from its middle"
    );
}

#[gpui::test]
fn home_and_end_use_wrapped_visual_rows_without_splitting_utf8(cx: &mut gpui::TestAppContext) {
    let mounted = Mounted::new(cx);
    let detail = "first alpha é beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau second 漢字 upsilon phi chi psi omega";
    let source = NoticeSource::new(
        19,
        NoticeContent::new(
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "wrapped row edges",
            detail,
        ),
    )
    .with_allocation(MainWindowNoticeOverlayAllocation::new(
        48., 12., 12., 180., 220.,
    ));
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let bounds = visual
        .debug_bounds(DETAIL)
        .expect("wrapped detail viewport");

    visual.simulate_click(
        point(bounds.left() + px(48.), bounds.top() + px(4.)),
        Modifiers::none(),
    );
    visual.simulate_keystrokes("home shift-end secondary-c");
    let first = visual
        .read_from_clipboard()
        .and_then(|item| item.text())
        .expect("first visual-row selection");

    visual.simulate_click(
        point(bounds.left() + px(48.), bounds.top() + px(28.)),
        Modifiers::none(),
    );
    visual.simulate_keystrokes("home shift-end secondary-c");
    let second = visual
        .read_from_clipboard()
        .and_then(|item| item.text())
        .expect("second visual-row selection");

    for selection in [&first, &second] {
        assert!(detail.contains(selection.as_str()));
        assert!(selection.len() < detail.len());
        assert!(
            std::str::from_utf8(selection.as_bytes()).is_ok(),
            "selection endpoints must remain valid UTF-8 boundaries"
        );
    }
    assert_ne!(
        first, second,
        "Home and End must distinguish wrapped visual rows"
    );
}
