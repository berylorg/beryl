use super::support::*;
use beryl_app::main_window::{
    MainWindowNoticeOverlayAllocation, MainWindowNoticePaintPhase, MainWindowNoticeVisibility,
    NoticeContent, NoticeDismissal, NoticeVariant,
};

#[gpui::test]
fn bounded_overlay_keeps_header_commands_and_scrollbar_reachable_for_long_detail(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let detail = (0..300)
        .map(|line| format!("wrapped detail line {line}\n"))
        .collect::<String>();
    let source = NoticeSource::new(
        7,
        NoticeContent::new(
            NoticeVariant::Error,
            NoticeDismissal::Dismissible,
            "bounded",
            &detail,
        )
        .with_commands(&[
            beryl_app::main_window::NoticeCommand::enabled(
                beryl_app::main_window::NoticeCommandId::new(51),
                "Recover",
            ),
            beryl_app::main_window::NoticeCommand::enabled(
                beryl_app::main_window::NoticeCommandId::new(52),
                "Retry",
            ),
            beryl_app::main_window::NoticeCommand::enabled(
                beryl_app::main_window::NoticeCommandId::new(53),
                "Dismiss",
            ),
        ])
        .expect("three commands"),
    )
    .with_allocation(MainWindowNoticeOverlayAllocation::new(
        64., 12., 16., 320., 188.,
    ));
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let root = visual.debug_bounds(ROOT).expect("notice root");
    let header = visual
        .debug_bounds("main-window-notice-header")
        .expect("header");
    let detail = visual.debug_bounds(DETAIL).expect("detail");
    let commands = (0..3)
        .map(|index| {
            visual
                .debug_bounds(mounted.command_selector(index, cx))
                .expect("command")
        })
        .collect::<Vec<_>>();
    visual.simulate_mouse_move(detail.center(), None, gpui::Modifiers::none());
    assert!(root.size.width <= gpui::px(320.));
    assert!(root.size.height <= gpui::px(188.));
    assert!(root.contains(&header.center()));
    assert!(root.contains(&detail.center()));
    for command in &commands {
        assert!(root.contains(&command.center()));
        assert!(
            detail.bottom() <= command.top(),
            "detail viewport overlaps commands: root={root:?}, header={header:?}, detail={detail:?}, commands={commands:?}"
        );
    }
    assert!(
        detail.bottom() <= root.bottom(),
        "detail viewport escapes the allocated frame: root={root:?}, header={header:?}, detail={detail:?}, commands={commands:?}"
    );
    assert!(mounted.diagnostics(cx).overflow);
}

#[gpui::test]
fn paint_uses_the_current_notice_variant_roles_for_each_severity(cx: &mut gpui::TestAppContext) {
    let mounted = Mounted::new(cx);
    for (seed, variant, background, _border, marker, title, detail_and_close) in [
        (
            8,
            NoticeVariant::Warning,
            0x2b2110,
            0xa16207,
            0xf59e0b,
            0xfef3c7,
            0xfde68a,
        ),
        (
            9,
            NoticeVariant::Error,
            0x2b1518,
            0xb91c1c,
            0xef4444,
            0xfee2e2,
            0xfecaca,
        ),
        (
            10,
            NoticeVariant::Info,
            0x10243a,
            0x0369a1,
            0x38bdf8,
            0xe0f2fe,
            0xbae6fd,
        ),
    ] {
        let source = NoticeSource::new(
            seed,
            NoticeContent::new(variant, NoticeDismissal::Dismissible, "paint", "role paint"),
        );
        mounted.replace(Some(source.record()), cx);
        let snapshot = mounted.paint(cx);
        assert!(
            snapshot
                .backgrounds
                .iter()
                .any(|(_, color)| *color == gpui::rgb(background).into()),
            "missing frame background for {variant:?}"
        );
        assert!(
            snapshot
                .backgrounds
                .iter()
                .any(|(_, color)| *color == gpui::rgb(marker).into()),
            "missing severity marker for {variant:?}"
        );
        for (part, expected) in [("title", title), ("detail and close", detail_and_close)] {
            assert!(
                snapshot
                    .glyphs
                    .iter()
                    .any(|(_, color)| *color == gpui::rgb(expected).into()),
                "missing {part} foreground role for {variant:?}"
            );
        }
        assert!(
            snapshot
                .glyphs
                .iter()
                .filter(|(_, color)| *color == gpui::rgb(detail_and_close).into())
                .count()
                >= 2,
            "detail and close must both consume the severity foreground role for {variant:?}"
        );
    }
}

#[gpui::test]
fn title_uses_the_active_severity_role_font_override(cx: &mut gpui::TestAppContext) {
    const DOCUMENT: &[u8] = br##"schema = 1

[[role]]
id = "main-window-notice.title.warning"
foreground = "#c0ffee"
font_size = 29
font_weight = 700
"##;

    let default = Mounted::new(cx);
    let custom = Mounted::with_theme_document(cx, DOCUMENT);
    let source = NoticeSource::new(
        18,
        NoticeContent::new(
            NoticeVariant::Warning,
            NoticeDismissal::Persistent,
            "MMMMMMMM",
            "detail",
        ),
    );
    default.replace(Some(source.record()), cx);
    custom.replace(Some(source.record()), cx);

    let default_title_height = default
        .paint(cx)
        .glyphs
        .iter()
        .filter(|(_, color)| *color == gpui::rgb(0xfef3c7).into())
        .map(|(bounds, _)| bounds.size.height)
        .max_by(|left, right| left.partial_cmp(right).expect("finite glyph height"))
        .expect("default title glyph");
    let custom_title_height = custom
        .paint(cx)
        .glyphs
        .iter()
        .filter(|(_, color)| *color == gpui::rgb(0xc0ffee).into())
        .map(|(bounds, _)| bounds.size.height)
        .max_by(|left, right| left.partial_cmp(right).expect("finite glyph height"))
        .expect("custom title glyph");
    assert!(
        custom_title_height > default_title_height,
        "the title must use the active severity role's font size"
    );
}

#[gpui::test]
fn diagnostics_report_the_measured_capped_frame_size(cx: &mut gpui::TestAppContext) {
    let mounted = Mounted::new(cx);
    let detail = (0..300)
        .map(|line| format!("measured detail line {line} wraps\n"))
        .collect::<String>();
    let source = NoticeSource::new(
        20,
        NoticeContent::new(
            NoticeVariant::Error,
            NoticeDismissal::Dismissible,
            "measured frame",
            &detail,
        ),
    )
    .with_allocation(MainWindowNoticeOverlayAllocation::new(
        64., 12., 16., 320., 188.,
    ));
    mounted.replace(Some(source.record()), cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let root = visual.debug_bounds(ROOT).expect("measured notice root");
    let diagnostic = mounted.diagnostics(cx);
    assert_eq!(
        diagnostic.allocated_inline,
        f32::from(root.size.width).round() as u16
    );
    assert_eq!(
        diagnostic.allocated_block,
        f32::from(root.size.height).round() as u16
    );
    assert!(diagnostic.allocated_inline <= 320);
    assert!(diagnostic.allocated_block <= 188);
}

#[gpui::test]
fn active_appearance_repaints_notice_text_backgrounds(cx: &mut gpui::TestAppContext) {
    const DOCUMENT: &[u8] = br##"schema = 1

[[role]]
id = "main-window-notice.title.warning"
foreground = "#c0ffee"
text_background = "#102030"

[[role]]
id = "main-window-notice.detail-viewport.warning"
foreground = "#d0e0f0"
text_background = "#304050"
"##;

    let mounted = Mounted::new(cx);
    let source = NoticeSource::new(
        21,
        NoticeContent::new(
            NoticeVariant::Warning,
            NoticeDismissal::Persistent,
            "updated title",
            "updated detail",
        ),
    );
    mounted.replace(Some(source.record()), cx);
    mounted.set_theme_document(DOCUMENT, cx);
    let snapshot = mounted.paint(cx);
    for expected in [0x102030, 0x304050] {
        assert!(
            snapshot
                .backgrounds
                .iter()
                .any(|(_, color)| *color == gpui::rgb(expected).into()),
            "the active appearance must repaint the text background role {expected:#x}"
        );
    }
    assert!(
        snapshot
            .glyphs
            .iter()
            .any(|(_, color)| *color == gpui::rgb(0xc0ffee).into()),
        "the active appearance must repaint the title foreground role"
    );
}

#[gpui::test]
fn paint_phase_projects_entering_visible_and_leaving_without_removing_the_record(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = Mounted::new(cx);
    let source = NoticeSource::new(
        22,
        NoticeContent::new(
            NoticeVariant::Info,
            NoticeDismissal::Persistent,
            "phase",
            "detail",
        ),
    );
    mounted.replace(Some(source.record()), cx);
    for (phase, visibility) in [
        (
            MainWindowNoticePaintPhase::Entering,
            MainWindowNoticeVisibility::Entering,
        ),
        (
            MainWindowNoticePaintPhase::Visible,
            MainWindowNoticeVisibility::Visible,
        ),
        (
            MainWindowNoticePaintPhase::Leaving,
            MainWindowNoticeVisibility::Leaving,
        ),
    ] {
        mounted.set_paint_phase(phase, cx);
        let diagnostic = mounted.diagnostics(cx);
        assert!(diagnostic.visible);
        assert_eq!(diagnostic.visibility, visibility);
    }
}
