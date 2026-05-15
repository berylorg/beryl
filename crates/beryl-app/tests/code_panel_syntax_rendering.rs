#[path = "../src/shell/layout.rs"]
pub(crate) mod layout;

#[path = "../src/shell/syntax_highlighting.rs"]
pub(crate) mod syntax_highlighting;

mod shell {
    pub(crate) use crate::layout;
    pub(crate) use crate::syntax_highlighting;
}

#[path = "../src/shell/render/code_panel.rs"]
mod code_panel;

#[path = "../src/shell/render/scrollbars.rs"]
mod scrollbars;

use code_panel::{CodePanelSyntaxTheme, CodePanelWrapMode};
use gpui::{Rgba, TextRun, px, rgb};
use syntax_highlighting::{SyntaxHighlight, SyntaxHighlightCache, highlight_syntax};

fn theme() -> CodePanelSyntaxTheme {
    CodePanelSyntaxTheme {
        plain_foreground: rgb(0x101010),
        structural_foreground: rgb(0x202020),
        heading_foreground: rgb(0x303030),
        emphasis_foreground: rgb(0x404040),
        strong_emphasis_foreground: rgb(0x505050),
        code_foreground: rgb(0x606060),
        link_foreground: rgb(0x707070),
        escape_foreground: rgb(0x808080),
    }
}

fn styled_line_parts(
    source: &str,
    label: Option<&str>,
    theme: CodePanelSyntaxTheme,
) -> (String, Vec<TextRun>) {
    let highlight = highlight_syntax(source, label);

    styled_line_parts_for_highlight(source, &highlight, theme)
}

fn styled_line_parts_for_highlight(
    source: &str,
    highlight: &SyntaxHighlight,
    theme: CodePanelSyntaxTheme,
) -> (String, Vec<TextRun>) {
    let display_lines = code_panel::code_panel_display_lines(source, CodePanelWrapMode::NoWrap);
    let spans =
        code_panel::code_panel_display_line_syntax_spans(&display_lines, highlight.tokens());

    code_panel::code_panel_styled_text_parts(
        display_lines[0].display_text.as_str(),
        spans[0].as_slice(),
        theme,
    )
}

fn assert_run(run: &TextRun, len: usize, color: Rgba) {
    assert_eq!(run.len, len);
    assert_eq!(run.color, color.into());
}

#[test]
fn highlighted_markdown_code_panels_use_token_role_colors() {
    let (text, runs) = styled_line_parts("# heading **bold**", Some("markdown"), theme());

    assert_eq!(text, "# heading **bold**");
    assert_eq!(runs.len(), 5);
    assert_run(&runs[0], 1, rgb(0x303030));
    assert_run(&runs[1], 9, rgb(0x101010));
    assert_run(&runs[2], 2, rgb(0x505050));
    assert_run(&runs[3], 4, rgb(0x101010));
    assert_run(&runs[4], 2, rgb(0x505050));
}

#[test]
fn unsupported_and_empty_labels_render_plain_code_runs() {
    for label in [Some("rust"), Some("not a language"), None] {
        let (text, runs) = styled_line_parts("# heading **bold**", label, theme());

        assert_eq!(text, "# heading **bold**");
        assert_eq!(runs.len(), 1);
        assert_run(&runs[0], text.len(), rgb(0x101010));
    }
}

#[test]
fn unstyled_ranges_fall_back_to_plain_code_appearance() {
    let (text, runs) = styled_line_parts("`code` plain", Some("md"), theme());

    assert_eq!(text, "`code` plain");
    assert_eq!(runs.len(), 4);
    assert_run(&runs[0], 1, rgb(0x202020));
    assert_run(&runs[1], 4, rgb(0x606060));
    assert_run(&runs[2], 1, rgb(0x202020));
    assert_run(&runs[3], 6, rgb(0x101010));
}

#[test]
fn theme_changes_affect_rendered_token_styles() {
    let first_theme = theme();
    let mut second_theme = theme();
    second_theme.heading_foreground = rgb(0xa0a0a0);
    second_theme.plain_foreground = rgb(0xb0b0b0);

    let (_, first_runs) = styled_line_parts("# heading", Some("markdown"), first_theme);
    let (_, second_runs) = styled_line_parts("# heading", Some("markdown"), second_theme);

    assert_ne!(first_runs[0].color, second_runs[0].color);
    assert_ne!(first_runs[1].color, second_runs[1].color);
    assert_run(&second_runs[0], 1, rgb(0xa0a0a0));
    assert_run(&second_runs[1], 8, rgb(0xb0b0b0));
}

#[test]
fn cached_highlight_token_roles_repaint_with_current_theme() {
    let mut cache = SyntaxHighlightCache::new(8, 4096);
    let lookup = cache.lookup("panel:1", "# heading", Some("markdown"));
    let request = lookup
        .highlight_request
        .expect("first Markdown lookup should schedule tokenization");
    assert!(
        cache
            .complete_highlight(request.highlight())
            .display_changed
    );
    let ready = cache.lookup("panel:1", "# heading", Some("markdown"));
    assert!(ready.highlight_request.is_none());

    let first_theme = theme();
    let mut second_theme = theme();
    second_theme.heading_foreground = rgb(0x111111);
    second_theme.plain_foreground = rgb(0x222222);

    let (_, first_runs) =
        styled_line_parts_for_highlight("# heading", ready.highlight.as_ref(), first_theme);
    let (_, second_runs) =
        styled_line_parts_for_highlight("# heading", ready.highlight.as_ref(), second_theme);

    assert_ne!(first_runs[0].color, second_runs[0].color);
    assert_ne!(first_runs[1].color, second_runs[1].color);
    assert_run(&second_runs[0], 1, rgb(0x111111));
    assert_run(&second_runs[1], 8, rgb(0x222222));
}

#[test]
fn large_plain_panels_use_plain_syntax_span_storage() {
    let source = "plain line\n".repeat(10_000);
    let display_lines =
        code_panel::code_panel_display_lines(source.as_str(), CodePanelWrapMode::NoWrap);
    let syntax_spans = code_panel::CodePanelDisplaySyntaxSpans::new(&display_lines, &[]);

    assert!(syntax_spans.is_plain());
    assert_eq!(display_lines.len(), 10_001);
}

#[test]
fn code_panel_renderer_does_not_parse_syntax_without_supplied_highlight() {
    let source = include_str!("../src/shell/render/code_panel.rs");

    assert!(!source.contains("highlight_syntax("));
}

#[test]
fn large_precomputed_projection_materializes_only_visible_display_rows() {
    let source = "plain line\n".repeat(10_000);
    let projection =
        code_panel::CodePanelDisplayProjection::new(source.as_str(), CodePanelWrapMode::NoWrap);
    let window = code_panel::code_panel_display_window(
        projection.display_line_count(),
        Some(px(80.0)),
        None,
        2,
    );
    let visible_lines = projection.display_lines_for_window(window.range.clone());

    assert_eq!(projection.display_line_count(), 10_001);
    assert_eq!(window.range, 0..6);
    assert_eq!(visible_lines.len(), 6);
    assert!(visible_lines.len() * 1_000 < projection.display_line_count());
    assert!(window.bottom_spacer_height > px(100_000.0));
}
