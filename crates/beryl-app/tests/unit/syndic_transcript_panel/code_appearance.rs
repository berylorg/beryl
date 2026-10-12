use super::super::code_appearance::CodeAppearance;
use beryl_state::{ThemeDocument, ThemeParseMode, ThemeResolver};
use gpui::rgb;

#[test]
fn code_colors_consume_resolved_inheritance_and_independent_part_overrides() {
    let document = ThemeDocument::parse_bytes(
        br##"schema = 1
id = "code-colors"
name = "Code Colors"

[[role]]
id = "surface.inset"
background = "#112233"

[[role]]
id = "text.code"
foreground = "#ddeeff"

[[role]]
id = "code_panel.container"
background = "static_parent"

[[role]]
id = "code_panel.body"
background = "static_parent"

[[role]]
id = "code_panel.body.text"
foreground = "static_parent"
font_family = "Consolas"
font_size = 36
font_weight = 700

[[role]]
id = "code_panel.header"
background = "#223344"

[[role]]
id = "code_panel.header.text"
foreground = "#bbccdd"
font_family = ".SystemUIFont"
font_size = 12
font_weight = 500

[[role]]
id = "code_panel.border"
color = "#334455"

[[role]]
id = "focus.ring"
color = "#445566"

[[role]]
id = "scrollbar.thumb.normal"
color = "#556677"
"##,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap();
    let resolved = ThemeResolver::new(document.definition()).unwrap().resolve();
    let appearance = CodeAppearance::prepare(&resolved);
    assert_eq!(appearance.container, rgb(0x112233));
    assert_eq!(appearance.body, rgb(0x112233));
    assert_eq!(appearance.body_text, rgb(0xddeeff));
    assert_eq!(appearance.header, rgb(0x223344));
    assert_eq!(appearance.header_text, rgb(0xbbccdd));
    assert_eq!(appearance.border, rgb(0x334455));
    assert_eq!(appearance.selected_border, rgb(0x445566));
    assert_eq!(appearance.scrollbar.thumb_color, 0x556677);
    assert_eq!(appearance.body_font.font.family.as_ref(), "Consolas");
    assert_eq!(appearance.body_font.size, 36.0);
    assert_eq!(appearance.body_font.font.weight, gpui::FontWeight(700.0));
    assert_eq!(appearance.header_font.font.family.as_ref(), ".SystemUIFont");
    assert_eq!(appearance.header_font.size, 12.0);
    assert_eq!(appearance.header_font.font.weight, gpui::FontWeight(500.0));
}

#[gpui::test]
fn code_row_metric_covers_large_font_and_keeps_small_font_realization_bounded(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|app| {
        let mut appearance = CodeAppearance::default();
        appearance.body_font.size = 48.0;
        let (height, key) = appearance.line_height(app).unwrap();
        let system = app.text_system();
        assert_eq!(system.font_environment_key(), Some(key));
        let id = system.resolve_font(&appearance.body_font.font);
        let size = gpui::px(48.0);
        assert!(
            height
                >= f32::from(system.ascent(id, size)).abs()
                    + f32::from(system.descent(id, size)).abs()
        );
        assert!(height > super::super::code_viewport::LINE_HEIGHT);
        appearance.body_font.size = 1.0;
        assert_eq!(
            appearance.line_height(app).unwrap().0,
            super::super::code_viewport::LINE_HEIGHT
        );
    });
}
