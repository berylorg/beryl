#[path = "../src/shell/layout.rs"]
pub(crate) mod layout;

#[path = "../src/shell/syntax_highlighting.rs"]
pub(crate) mod syntax_highlighting;

mod shell {
    pub(crate) use crate::layout;
    pub(crate) use crate::syntax_highlighting;

    pub(crate) struct ShellRenderStyleSnapshot;

    impl ShellRenderStyleSnapshot {
        pub(crate) fn scrollbar_thumb_color(&self) -> u32 {
            0x000000
        }
    }

    pub(crate) struct ShellView;

    impl ShellView {
        pub(crate) fn scrollbar_thumb_color(&self) -> u32 {
            0x000000
        }
    }
}

#[path = "../src/shell/render/code_panel.rs"]
mod code_panel;

#[path = "../src/shell/render/scrollbars.rs"]
mod scrollbars;

use gpui::{Overflow, point, px, size};

#[test]
fn smart_wrap_for_columns_falls_back_to_forced_breaks() {
    assert_eq!(
        code_panel::smart_wrap_for_columns("abcdef", 2),
        "ab\ncd\nef"
    );
}

#[test]
fn smart_wrap_for_columns_preserves_existing_newlines() {
    assert_eq!(
        code_panel::smart_wrap_for_columns("abcd\nef", 3),
        "abc\nd\nef"
    );
}

#[test]
fn smart_wrap_for_columns_prefers_spaces_commas_and_semicolons() {
    assert_eq!(
        code_panel::smart_wrap_for_columns("alpha,beta gamma;delta", 12),
        "alpha,beta \ngamma;delta"
    );
}

#[test]
fn code_panel_display_lines_mark_soft_wrap_segments_without_raw_breaks() {
    let lines = code_panel::code_panel_display_lines(
        "abcdef\nghij",
        code_panel::CodePanelWrapMode::Smart { columns: 2 },
    );

    assert_eq!(
        lines,
        vec![
            code_panel::CodePanelDisplayLine {
                display_text: "ab".to_string(),
                raw_text: "ab".to_string(),
                break_before: 1,
                source_range: 0..2,
            },
            code_panel::CodePanelDisplayLine {
                display_text: "cd".to_string(),
                raw_text: "cd".to_string(),
                break_before: 0,
                source_range: 2..4,
            },
            code_panel::CodePanelDisplayLine {
                display_text: "ef".to_string(),
                raw_text: "ef".to_string(),
                break_before: 0,
                source_range: 4..6,
            },
            code_panel::CodePanelDisplayLine {
                display_text: "gh".to_string(),
                raw_text: "gh".to_string(),
                break_before: 1,
                source_range: 7..9,
            },
            code_panel::CodePanelDisplayLine {
                display_text: "ij".to_string(),
                raw_text: "ij".to_string(),
                break_before: 0,
                source_range: 9..11,
            },
        ]
    );
}

#[test]
fn code_panel_display_lines_preserve_no_wrap_logical_lines() {
    let lines = code_panel::code_panel_display_lines(
        "alpha beta\ngamma",
        code_panel::CodePanelWrapMode::NoWrap,
    );

    assert_eq!(
        lines,
        vec![
            code_panel::CodePanelDisplayLine {
                display_text: "alpha beta".to_string(),
                raw_text: "alpha beta".to_string(),
                break_before: 1,
                source_range: 0..10,
            },
            code_panel::CodePanelDisplayLine {
                display_text: "gamma".to_string(),
                raw_text: "gamma".to_string(),
                break_before: 1,
                source_range: 11..16,
            },
        ]
    );
}

#[test]
fn code_panel_wrap_modes_preserve_source_ranges_for_selection_payloads() {
    let source = "alpha **bold** gamma";
    let nowrap =
        code_panel::code_panel_display_lines(source, code_panel::CodePanelWrapMode::NoWrap);
    let smart = code_panel::code_panel_display_lines(
        source,
        code_panel::CodePanelWrapMode::Smart { columns: 8 },
    );

    assert_eq!(nowrap.len(), 1);
    assert_eq!(nowrap[0].raw_text, source);
    assert_eq!(nowrap[0].source_range, 0..source.len());
    assert_eq!(
        smart
            .iter()
            .map(|line| (
                line.raw_text.as_str(),
                line.break_before,
                line.source_range.clone()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("alpha ", 1, 0..6),
            ("**bold**", 0, 6..14),
            (" gamma", 0, 14..20),
        ]
    );
}

#[test]
fn estimated_resizable_code_panel_height_respects_minimum_height() {
    assert_eq!(
        code_panel::estimated_resizable_code_panel_height("one line", px(80.0), Some(px(240.0))),
        px(80.0)
    );
}

#[test]
fn estimated_resizable_code_panel_height_respects_maximum_height() {
    let tall_text = (0..20)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        code_panel::estimated_resizable_code_panel_height(
            tall_text.as_str(),
            px(64.0),
            Some(px(180.0)),
        ),
        px(180.0)
    );
}

#[test]
fn clamp_resizable_code_panel_height_preserves_resize_state_bounds() {
    assert_eq!(
        code_panel::clamp_resizable_code_panel_height(px(24.0), px(80.0), Some(px(180.0))),
        px(80.0)
    );
    assert_eq!(
        code_panel::clamp_resizable_code_panel_height(px(220.0), px(80.0), Some(px(180.0))),
        px(180.0)
    );
    assert_eq!(
        code_panel::clamp_resizable_code_panel_height(px(140.0), px(80.0), None),
        px(140.0)
    );
}

#[test]
fn code_panel_scroll_overflow_allows_owned_vertical_wheel() {
    assert_eq!(
        code_panel::code_panel_scroll_overflow(
            code_panel::ScrollbarAxes {
                horizontal: true,
                vertical: true,
            },
            code_panel::CodePanelVerticalWheelOwnership::Panel,
        ),
        code_panel::CodePanelScrollOverflow {
            horizontal: Overflow::Scroll,
            vertical: Overflow::Scroll,
        }
    );
}

#[test]
fn code_panel_scroll_overflow_hides_unowned_vertical_wheel() {
    assert_eq!(
        code_panel::code_panel_scroll_overflow(
            code_panel::ScrollbarAxes {
                horizontal: true,
                vertical: true,
            },
            code_panel::CodePanelVerticalWheelOwnership::Parent,
        ),
        code_panel::CodePanelScrollOverflow {
            horizontal: Overflow::Scroll,
            vertical: Overflow::Hidden,
        }
    );
}

#[test]
fn code_panel_scroll_overflow_preserves_horizontal_no_wrap_scrolling() {
    assert_eq!(
        code_panel::code_panel_scroll_overflow(
            code_panel::ScrollbarAxes {
                horizontal: true,
                vertical: false,
            },
            code_panel::CodePanelVerticalWheelOwnership::Parent,
        ),
        code_panel::CodePanelScrollOverflow {
            horizontal: Overflow::Scroll,
            vertical: Overflow::Visible,
        }
    );
}

#[test]
fn owned_vertical_wheel_stops_propagation_to_transcript() {
    assert!(code_panel::code_panel_stops_scroll_wheel_propagation(
        code_panel::ScrollbarAxes {
            horizontal: true,
            vertical: true,
        },
        code_panel::CodePanelVerticalWheelOwnership::Panel,
    ));
}

#[test]
fn unowned_vertical_wheel_propagates_to_transcript() {
    assert!(!code_panel::code_panel_stops_scroll_wheel_propagation(
        code_panel::ScrollbarAxes {
            horizontal: true,
            vertical: true,
        },
        code_panel::CodePanelVerticalWheelOwnership::Parent,
    ));
}

#[test]
fn horizontal_only_panel_does_not_stop_vertical_transcript_scroll() {
    assert!(!code_panel::code_panel_stops_scroll_wheel_propagation(
        code_panel::ScrollbarAxes {
            horizontal: true,
            vertical: false,
        },
        code_panel::CodePanelVerticalWheelOwnership::Panel,
    ));
}

#[test]
fn code_panel_scroll_delta_clamps_vertical_offset() {
    assert_eq!(
        code_panel::code_panel_offset_after_scroll_delta(
            point(px(0.0), px(-95.0)),
            size(px(0.0), px(100.0)),
            point(px(0.0), px(-20.0)),
        ),
        point(px(0.0), px(-100.0))
    );
}

#[test]
fn code_panel_scroll_delta_clamps_horizontal_offset() {
    assert_eq!(
        code_panel::code_panel_offset_after_scroll_delta(
            point(px(-95.0), px(0.0)),
            size(px(100.0), px(0.0)),
            point(px(-20.0), px(0.0)),
        ),
        point(px(-100.0), px(0.0))
    );
}

#[test]
fn code_panel_scroll_delta_keeps_dominant_diagonal_axis() {
    assert_eq!(
        code_panel::code_panel_offset_after_scroll_delta(
            point(px(-20.0), px(-20.0)),
            size(px(100.0), px(100.0)),
            point(px(-8.0), px(-16.0)),
        ),
        point(px(-20.0), px(-36.0))
    );
}
