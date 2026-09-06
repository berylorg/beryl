use super::support::*;
use beryl_app::main_window::{
    MainWindowNoticeControlFocus, MainWindowNoticeOverlayAllocation, NoticeContent,
    NoticeDismissal, NoticeVariant,
};
use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, point, px};

#[gpui::test]
fn revision_clears_selection_and_preserves_only_valid_scroll_geometry(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let long_detail = (0..180)
        .map(|index| format!("line {index}: repeatable wrapped detail\n"))
        .collect::<String>();
    let mut source = NoticeSource::new(
        2,
        NoticeContent::new(
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "revision",
            &long_detail,
        ),
    );
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let detail = visual.debug_bounds(DETAIL).expect("overflow detail");
    visual.simulate_click(
        point(detail.left() + px(18.), detail.top() + px(8.)),
        Modifiers::none(),
    );
    visual.simulate_event(ScrollWheelEvent {
        position: detail.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-160.))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
    mounted.draw(cx);
    let initial_scroll = mounted.diagnostics(cx).scroll_offset;
    assert!(
        initial_scroll > 0,
        "wheel scroll must establish the continuity anchor"
    );
    let appended = format!("{long_detail}later text below the established visible geometry");
    mounted.replace(
        Some(source.update(NoticeContent::new(
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "revision",
            &appended,
        ))),
        cx,
    );
    let after_append = mounted.diagnostics(cx);
    assert!(!after_append.selection_present);
    assert_eq!(after_append.scroll_offset, initial_scroll);
    mounted.replace(
        Some(source.update(NoticeContent::new(
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "revision",
            &format!("new prefix changes wrapped geometry\n{appended}"),
        ))),
        cx,
    );
    assert_eq!(mounted.diagnostics(cx).scroll_offset, 0);
}

#[gpui::test]
fn identity_replacement_resets_detail_and_routes_focus_to_retained_controls_or_safe_target(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let original = NoticeSource::new(
        3,
        NoticeContent::new(
            NoticeVariant::Warning,
            NoticeDismissal::Dismissible,
            "first",
            "select and replace this detail",
        ),
    );
    mounted.replace(Some(original.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let detail = visual.debug_bounds(DETAIL).expect("detail");
    visual.simulate_click(detail.center(), gpui::Modifiers::none());
    visual.simulate_keystrokes("shift-right shift-right");
    assert!(mounted.diagnostics(cx).selection_present);
    let replacement = NoticeSource::new(
        4,
        NoticeContent::new(
            NoticeVariant::Error,
            NoticeDismissal::Persistent,
            "second",
            "",
        ),
    );
    mounted.replace(Some(replacement.record()), cx);
    let diagnostic = mounted.diagnostics(cx);
    assert!(!diagnostic.selection_present);
    assert_eq!(diagnostic.scroll_offset, 0);
    assert_eq!(diagnostic.focus, MainWindowNoticeControlFocus::SafeTarget);
    mounted.replace(None, cx);
    assert_eq!(
        mounted.diagnostics(cx).focus,
        MainWindowNoticeControlFocus::SafeTarget
    );
}

#[gpui::test]
fn same_identity_revision_revalidates_a_nonzero_scroll_anchor_after_width_reflow(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let detail = (0..220)
        .map(|line| format!("line {line}: repeated words make visual wrapping deterministic\n"))
        .collect::<String>();
    let mut source = NoticeSource::new(
        22,
        NoticeContent::new(
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "reflow",
            &detail,
        ),
    )
    .with_allocation(MainWindowNoticeOverlayAllocation::new(
        48., 12., 12., 420., 240.,
    ));
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let viewport = visual.debug_bounds(DETAIL).expect("detail viewport");
    visual.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-180.))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
    mounted.draw(cx);
    assert!(mounted.diagnostics(cx).scroll_offset > 0);
    source.set_allocation(MainWindowNoticeOverlayAllocation::new(
        48., 12., 12., 240., 240.,
    ));
    mounted.replace(
        Some(source.update(NoticeContent::new(
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "reflow",
            &detail,
        ))),
        cx,
    );
    assert_eq!(
        mounted.diagnostics(cx).scroll_offset,
        0,
        "a changed wrap geometry invalidates the saved visual-line anchor"
    );
}
