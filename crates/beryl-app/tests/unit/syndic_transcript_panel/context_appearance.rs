use super::super::context_appearance::ContextAppearance;
use beryl_state::{ThemeDocument, ThemeParseMode, ThemeResolver};
use gpui::rgb;

#[test]
fn context_adapter_consumes_active_colors_typography_and_geometry_derivations() {
    let document = ThemeDocument::parse_bytes(
        br##"schema = 1
id = "context"
name = "Context"
[[role]]
id = "transcript-view.context-group"
background = "#112233"
border = "#223344"
foreground = "#334455"
[[role]]
id = "transcript-view.context-body"
font_family = ".SystemUIFont"
font_size = 28
font_weight = 500
foreground = "#445566"
[[role]]
id = "transcript-view.context-heading"
font_size = 20
foreground = "#556677"
[[role]]
id = "transcript-view.context-provenance"
font_size = 22
foreground = "#667788"
"##,
        ThemeParseMode::StrictCandidate,
    )
    .unwrap();
    let appearance =
        ContextAppearance::prepare(&ThemeResolver::new(document.definition()).unwrap().resolve());
    assert_eq!(appearance.background, rgb(0x112233));
    assert_eq!(appearance.border, rgb(0x223344));
    assert_eq!(appearance.foreground, rgb(0x334455));
    assert_eq!(appearance.body.color, rgb(0x445566));
    assert_eq!(appearance.body.font.family.as_ref(), ".SystemUIFont");
    assert_eq!(appearance.body.font.weight.0, 500.0);
    assert_eq!(appearance.scale(), 2.0);
    assert_eq!(appearance.heading.size, 20.0);
    assert_eq!(appearance.provenance.size, 22.0);
}
