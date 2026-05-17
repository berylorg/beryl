#[test]
fn phase5_chrome_sources_do_not_use_literal_visible_colors() {
    for (path, source) in PHASE5_CHROME_SOURCES {
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
fn phase5_chrome_sources_use_role_helpers_for_stateful_surfaces() {
    let common = include_str!("../src/shell/render/common.rs");
    let conversation = include_str!("../src/shell/render/conversation.rs");
    let workspace_picker = include_str!("../src/shell/render/workspace_picker.rs");
    let thread_selector = include_str!("../src/shell/render/thread_selector.rs");
    let status_operation = include_str!("../src/shell/render/status_operation.rs");

    assert!(common.contains("inline_notice("));
    assert!(common.contains("BerylThemeRole"));
    assert!(conversation.contains("last_turn_state_color(shell"));
    assert!(conversation.contains("BerylThemeRole::StatusValueWorking"));
    assert!(conversation.contains("BerylThemeRole::StatusValueCompacting"));
    assert!(conversation.contains("BerylThemeRole::StatusValueOk"));
    assert!(conversation.contains("BerylThemeRole::StatusValueError"));
    assert!(conversation.contains("tool_activity_status_disc(shell"));
    assert!(conversation.contains("BerylThemeRole::MediaPlaceholder"));
    assert!(!workspace_picker.contains("CODE_FONT_FAMILY"));
    assert!(workspace_picker.contains("role_font_family("));
    assert!(thread_selector.contains("BerylThemeRole::ThreadSelectorRowSelected"));
    assert!(status_operation.contains("BerylThemeRole::StatusValueError"));
}

#[test]
fn phase5_scrollbar_entrypoints_are_theme_aware() {
    let scrollbars = include_str!("../src/shell/render/scrollbars.rs");

    assert!(scrollbars.contains("themed_beryl_scrollbar_style(style"));
    assert!(scrollbars.contains("style.scrollbar_thumb_color()"));
    assert!(scrollbars.contains("style: &ShellRenderStyleSnapshot"));
    assert!(!scrollbars.contains("pub(super) fn render_div_scrollbar("));

    for (path, source) in THEMED_SCROLLBAR_CALLERS {
        assert!(
            source.contains("render_themed_div_scrollbar("),
            "{path} should use the active-theme scrollbar entrypoint"
        );
    }
}

const PHASE5_CHROME_SOURCES: &[(&str, &str)] = &[
    (
        "src/shell/render/common.rs",
        include_str!("../src/shell/render/common.rs"),
    ),
    (
        "src/shell/render/conversation.rs",
        include_str!("../src/shell/render/conversation.rs"),
    ),
    (
        "src/shell/render/startup.rs",
        include_str!("../src/shell/render/startup.rs"),
    ),
    (
        "src/shell/render/workspace_picker.rs",
        include_str!("../src/shell/render/workspace_picker.rs"),
    ),
    (
        "src/shell/render/workspace_picker_row_menu.rs",
        include_str!("../src/shell/render/workspace_picker_row_menu.rs"),
    ),
    (
        "src/shell/render/thread_selector.rs",
        include_str!("../src/shell/render/thread_selector.rs"),
    ),
    (
        "src/shell/render/status_operation.rs",
        include_str!("../src/shell/render/status_operation.rs"),
    ),
];

const THEMED_SCROLLBAR_CALLERS: &[(&str, &str)] = &[
    (
        "src/shell/render/common.rs",
        include_str!("../src/shell/render/common.rs"),
    ),
    (
        "src/shell/render/conversation.rs",
        include_str!("../src/shell/render/conversation.rs"),
    ),
    (
        "src/shell/render/workspace_picker.rs",
        include_str!("../src/shell/render/workspace_picker.rs"),
    ),
    (
        "src/shell/render/thread_selector.rs",
        include_str!("../src/shell/render/thread_selector.rs"),
    ),
    (
        "src/shell/render/status_operation.rs",
        include_str!("../src/shell/render/status_operation.rs"),
    ),
    (
        "src/shell/render/column_selector.rs",
        include_str!("../src/shell/render/column_selector.rs"),
    ),
    (
        "src/shell/render/graph_overlay.rs",
        include_str!("../src/shell/render/graph_overlay.rs"),
    ),
    (
        "src/shell/render/graph_link_menu.rs",
        include_str!("../src/shell/render/graph_link_menu.rs"),
    ),
    (
        "src/shell/render/checklist_sidebar.rs",
        include_str!("../src/shell/render/checklist_sidebar.rs"),
    ),
];
