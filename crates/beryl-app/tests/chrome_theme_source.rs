#[test]
fn retained_chrome_sources_do_not_use_literal_visible_colors() {
    for (path, source) in RETAINED_CHROME_SOURCES {
        let source_without_functional_transparency =
            source.replace("rgba(0x00000000)", "FUNCTIONAL_TRANSPARENT_TEXT");

        assert!(
            !source_without_functional_transparency.contains("rgb("),
            "{path} should resolve visible colors through theme roles"
        );
        assert!(
            !source_without_functional_transparency.contains("rgba("),
            "{path} should not embed visible rgba colors"
        );
    }
}

#[test]
fn retained_chrome_sources_use_role_helpers_for_stateful_surfaces() {
    let common = include_str!("../src/shell/render/common.rs");
    let status_operation = include_str!("../src/shell/render/status_operation.rs");

    assert!(common.contains("inline_notice("));
    assert!(common.contains("BerylThemeRole"));
    assert!(common.contains("BerylThemeRole::ControlListHeader"));
    assert!(status_operation.contains("BerylThemeRole::StatusValueOk"));
    assert!(status_operation.contains("BerylThemeRole::ControlPopupHeader"));
    assert!(status_operation.contains("BerylThemeRole::SemanticError"));
}

#[test]
fn phase5_scrollbar_entrypoints_are_theme_aware() {
    let scrollbars = include_str!("../src/shell/render/scrollbars.rs");

    assert!(scrollbars.contains("themed_beryl_scrollbar_style(style"));
    assert!(scrollbars.contains("style.scrollbar_thumb_color()"));
    assert!(scrollbars.contains("style: &ShellRenderStyleSnapshot"));
    assert!(!scrollbars.contains("pub(super) fn render_div_scrollbar("));
}

const RETAINED_CHROME_SOURCES: &[(&str, &str)] = &[
    (
        "src/shell/render/common.rs",
        include_str!("../src/shell/render/common.rs"),
    ),
    (
        "src/shell/render/status_operation.rs",
        include_str!("../src/shell/render/status_operation.rs"),
    ),
];
