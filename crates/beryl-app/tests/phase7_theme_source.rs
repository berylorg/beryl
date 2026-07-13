#[test]
fn retained_render_sources_do_not_use_appearance_settings_or_literal_colors() {
    for (path, source) in RETAINED_RENDER_SOURCES {
        let source_without_functional_transparency =
            source.replace("rgba(0x00000000)", "FUNCTIONAL_TRANSPARENT_TEXT");

        assert!(
            !source.contains("AppearanceSettings"),
            "{path} should not depend on flat appearance settings"
        );
        assert!(
            !source.contains("appearance."),
            "{path} should not read appearance fields directly"
        );
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

const RETAINED_RENDER_SOURCES: &[(&str, &str)] = &[
    (
        "src/shell/render/common.rs",
        include_str!("../src/shell/render/common.rs"),
    ),
    (
        "src/shell/render/status_operation.rs",
        include_str!("../src/shell/render/status_operation.rs"),
    ),
];
